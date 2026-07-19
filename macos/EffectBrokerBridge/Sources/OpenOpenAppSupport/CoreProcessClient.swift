import Darwin
import EffectBrokerBridge
import Foundation

enum CoreExecutableResolver {
  static func resolve(bundleURL: URL = Bundle.main.bundleURL) throws -> URL {
    guard bundleURL.pathExtension == "app" else {
      throw CoreClientError.invalidBundleLayout
    }
    let standardizedBundle = bundleURL.standardizedFileURL
    guard standardizedBundle.resolvingSymlinksInPath() == standardizedBundle else {
      throw CoreClientError.invalidBundleLayout
    }
    let executable =
      standardizedBundle
      .appendingPathComponent("Contents", isDirectory: true)
      .appendingPathComponent("MacOS", isDirectory: true)
      .appendingPathComponent("OpenOpenCore", isDirectory: false)
      .standardizedFileURL
    let values = try executable.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
    guard values.isRegularFile == true, values.isSymbolicLink != true,
      executable.resolvingSymlinksInPath() == executable
    else {
      throw CoreClientError.invalidBundleLayout
    }
    return executable
  }
}

enum CoreExecutableAuthenticator {
  static func validateStatic(_ executable: URL) throws {
    let identity = try currentHostIdentity()
    try StaticCodeSigningValidator.validate(
      executableURL: executable,
      expectedSigningIdentifier: EffectBrokerConstants.coreSigningIdentifier,
      teamIdentifier: identity.teamIdentifier
    )
  }

  static func validateRunning(auditTokenHex: String) throws {
    let identity = try currentHostIdentity()
    try StaticCodeSigningValidator.validateRunningProcess(
      auditTokenHex: auditTokenHex,
      expectedSigningIdentifier: EffectBrokerConstants.coreSigningIdentifier,
      teamIdentifier: identity.teamIdentifier
    )
  }

  private static func currentHostIdentity() throws -> CodeSigningIdentity {
    let identity = try SecurityCodeSigningIdentityProvider().currentIdentity()
    guard identity.signingIdentifier == EffectBrokerConstants.hostSigningIdentifier else {
      throw CodeSigningIdentityError.unexpectedSigningIdentifier(
        expected: EffectBrokerConstants.hostSigningIdentifier,
        actual: identity.signingIdentifier
      )
    }
    return identity
  }
}

enum IMessageRuntimeAuthenticator {
  private static let signingIdentifier = "com.thesongzhu.OpenOpen.imsg"

  static func executable(bundleURL: URL = Bundle.main.bundleURL) throws -> URL {
    let executable =
      bundleURL.standardizedFileURL
      .appendingPathComponent("Contents/Resources/iMessage/0.13.0/bin/imsg")
      .standardizedFileURL
    let values = try executable.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
    guard values.isRegularFile == true, values.isSymbolicLink != true,
      executable.resolvingSymlinksInPath() == executable
    else { throw CoreClientError.invalidBundleLayout }
    return executable
  }

  static func validateStatic(_ executable: URL) throws {
    let identity = try SecurityCodeSigningIdentityProvider().currentIdentity()
    try StaticCodeSigningValidator.validate(
      executableURL: executable,
      expectedSigningIdentifier: signingIdentifier,
      teamIdentifier: identity.teamIdentifier
    )
  }

  static func validateRunning(_ processIdentifier: Int32) throws {
    let identity = try SecurityCodeSigningIdentityProvider().currentIdentity()
    try StaticCodeSigningValidator.validateRunningProcessIdentifier(
      processIdentifier,
      expectedSigningIdentifier: signingIdentifier,
      teamIdentifier: identity.teamIdentifier
    )
  }
}

public final class CoreProcessClient: CoreLifecycleMonitoring, @unchecked Sendable {
  private enum CallStartPolicy {
    case startIfNeeded
    case requireRunning
  }

  private enum LaunchStateError: Error {
    case priorGenerationStopping
  }

  private static let bootstrapMagic = Data("OPENOPEN_BOOTSTRAP_V1\0".utf8)
  private static let maximumFrameBytes = 8 * 1024 * 1024
  private static let maximumAbandonedRequests = 1_024
  private static let loginDeadline: Duration = .seconds(660)

  private typealias Completion = @Sendable (Result<Data, CoreClientError>) -> Void
  private struct PendingRequest {
    let generation: UInt64
    let completion: Completion
  }

  private enum FrameWriteDisposition {
    case written
    case revoked(Completion?)
  }

  private let stateLock = NSLock()
  private let writeLock = NSLock()
  private let readerQueue = DispatchQueue(label: "com.thesongzhu.OpenOpen.core-reader")
  private let errorQueue = DispatchQueue(label: "com.thesongzhu.OpenOpen.core-stderr")
  private let executableResolver: @Sendable () throws -> URL
  private let staticCodeValidator: @Sendable (URL) throws -> Void
  private let runningCodeValidator: @Sendable (String) throws -> Void
  private let exactProcessTerminator: @Sendable (String) -> Bool
  private let masterKeyLoader: @Sendable () throws -> Data
  private let childEnvironmentLoader: @Sendable () -> [String: String]
  private let requestInstalledBeforeWriteHook: @Sendable () -> Void
  private let inputRevokedHook: @Sendable () -> Void
  private var process: Process?
  private var processAuditTokenHex: String?
  private var quarantinedGeneration: UInt64?
  private var activeGenerationFence: CoreGenerationFence?
  private var input: FileHandle?
  private var pending: [UInt64: PendingRequest] = [:]
  private var abandoned: [UInt64: UInt64] = [:]
  private var shutdownReasons: [UInt64: CoreTerminationReason] = [:]
  private var lifecycleObservers: [UUID: AsyncStream<CoreTerminationEvent>.Continuation] = [:]
  private var nextIdentifier: UInt64 = 1
  private var generation: UInt64 = 0

  public init() {
    executableResolver = { try CoreExecutableResolver.resolve() }
    staticCodeValidator = { try CoreExecutableAuthenticator.validateStatic($0) }
    runningCodeValidator = { try CoreExecutableAuthenticator.validateRunning(auditTokenHex: $0) }
    exactProcessTerminator = { Self.terminateExactProcess($0) }
    masterKeyLoader = { try KeychainMasterKey.loadOrCreate() }
    childEnvironmentLoader = {
      ["HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]
    }
    requestInstalledBeforeWriteHook = {}
    inputRevokedHook = {}
  }

  init(
    executableResolver: @escaping @Sendable () throws -> URL,
    staticCodeValidator: @escaping @Sendable (URL) throws -> Void,
    runningCodeValidator: @escaping @Sendable (String) throws -> Void,
    masterKeyLoader: @escaping @Sendable () throws -> Data,
    childEnvironmentLoader: @escaping @Sendable () -> [String: String] = {
      ["HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]
    },
    requestInstalledBeforeWriteHook: @escaping @Sendable () -> Void = {},
    inputRevokedHook: @escaping @Sendable () -> Void = {},
    exactProcessTerminator: @escaping @Sendable (String) -> Bool = {
      CoreProcessClient.terminateExactProcess($0)
    }
  ) {
    self.executableResolver = executableResolver
    self.staticCodeValidator = staticCodeValidator
    self.runningCodeValidator = runningCodeValidator
    self.masterKeyLoader = masterKeyLoader
    self.childEnvironmentLoader = childEnvironmentLoader
    self.requestInstalledBeforeWriteHook = requestInstalledBeforeWriteHook
    self.inputRevokedHook = inputRevokedHook
    self.exactProcessTerminator = exactProcessTerminator
  }

  deinit {
    finishLifecycleObservers()
    shutdown()
  }

  public func terminationEvents() -> AsyncStream<CoreTerminationEvent> {
    let identifier = UUID()
    return AsyncStream(bufferingPolicy: .bufferingNewest(8)) { [weak self] continuation in
      guard let self else {
        continuation.finish()
        return
      }
      stateLock.withLock {
        lifecycleObservers[identifier] = continuation
      }
      continuation.onTermination = { [weak self] _ in
        _ = self?.stateLock.withLock {
          self?.lifecycleObservers.removeValue(forKey: identifier)
        }
      }
    }
  }

  public func beginCoreGenerationFence() async throws -> CoreGenerationFence {
    guard stateLock.withLock({ activeGenerationFence == nil }) else {
      throw CoreClientError.contractViolation("A Core generation fence is already active.")
    }
    let (fencedGeneration, handle) = try await startForCall()
    return try stateLock.withLock {
      guard activeGenerationFence == nil, generation == fencedGeneration,
        process?.isRunning == true, input === handle
      else {
        throw CoreClientError.processUnavailable
      }
      let fence = CoreGenerationFence(identifier: UUID(), generation: fencedGeneration)
      activeGenerationFence = fence
      return fence
    }
  }

  public func closeCoreGenerationFence(_ fence: CoreGenerationFence) async -> Bool {
    stateLock.withLock {
      guard activeGenerationFence == fence else { return false }
      activeGenerationFence = nil
      return generation == fence.generation && process?.isRunning == true && input != nil
    }
  }

  public func runtime() async throws -> RuntimeControl {
    try await call(method: "mission.runtime.read", parameters: EmptyParameters())
  }

  public func effectIdentity() async throws -> CoreEffectIdentity {
    let response: CoreEffectIdentityResponse = try await call(
      method: "broker.identity.read",
      parameters: EmptyParameters()
    )
    return CoreEffectIdentity(
      coreKeyID: response.coreKeyId,
      coreVerifyingKeyHex: response.coreVerifyingKeyHex,
      coreProcessIdentifier: response.corePid,
      coreInstanceNonce: response.coreInstanceNonce
    )
  }

  public func signBrokerEnrollment(_ anchor: EnrolledBrokerTrustAnchor) async throws -> Data {
    let record: BrokerEnrollmentRecord = try await call(
      method: "broker.enrollment.sign",
      parameters: SignBrokerEnrollmentParameters(anchor)
    )
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    return try encoder.encode(record)
  }

  public func prepareCodexRuntime() async throws -> Int32 {
    let response: CodexRuntimeIdentityResponse = try await call(
      method: "broker.codex.prepare", parameters: EmptyParameters()
    )
    guard response.codexPid > 0 else {
      throw CoreClientError.contractViolation("Core returned an invalid Codex process.")
    }
    return response.codexPid
  }

  public func prepareCodexLoginRuntime() async throws -> Int32 {
    let response: CodexRuntimeIdentityResponse = try await call(
      method: "broker.codex.login.prepare", parameters: EmptyParameters()
    )
    guard response.codexPid > 0 else {
      throw CoreClientError.contractViolation("Core returned an invalid login-only Codex process.")
    }
    return response.codexPid
  }

  public func bindCodexCandidateForBroker() async throws {
    let result: InstallBrokerResult = try await call(
      method: "broker.codex.candidate.bind", parameters: EmptyParameters()
    )
    guard result.status == "bound" else {
      throw CoreClientError.contractViolation("Core rejected Codex broker handoff.")
    }
  }

  public func initializeCodexRuntime() async throws {
    let result: InstallBrokerResult = try await call(
      method: "broker.codex.initialize", parameters: EmptyParameters()
    )
    guard result.status == "initialized" else {
      throw CoreClientError.contractViolation("Core rejected Codex initialization.")
    }
  }

  public func abortCodexCandidate() async throws {
    let result: InstallBrokerResult = try await call(
      method: "broker.codex.abort", parameters: EmptyParameters()
    )
    guard result.status == "aborted" else {
      throw CoreClientError.contractViolation("Core rejected Codex candidate cleanup.")
    }
  }

  public func runtimeChallenge() async throws -> String {
    let result: RuntimeChallenge = try await call(
      method: "mission.runtime.challenge",
      parameters: EmptyParameters()
    )
    guard result.challenge.count == 64,
      result.challenge.allSatisfy({ $0.isHexDigit && !$0.isUppercase })
    else {
      throw CoreClientError.contractViolation("Core returned an invalid runtime challenge.")
    }
    return result.challenge
  }

  public func prepareRuntime(_ enabled: Bool) async throws -> RuntimeControlAuthorization {
    try await call(
      method: "mission.runtime.prepare",
      parameters: SetEnabledParameters(enabled: enabled)
    )
  }

  public func commitRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt: RuntimeControlReceipt
  ) async throws -> RuntimeControl {
    try await call(
      method: "mission.runtime.commit",
      parameters: CommitRuntimeParameters(
        authorization: authorization,
        brokerReceipt: brokerReceipt
      )
    )
  }

  public func recoverRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt: RuntimeControlReceipt
  ) async throws -> RuntimeControl {
    try await call(
      method: "mission.runtime.recover",
      parameters: CommitRuntimeParameters(
        authorization: authorization,
        brokerReceipt: brokerReceipt
      )
    )
  }

  public func installBrokerEnrollment(_ recordJSON: Data) async throws {
    let record = try JSONDecoder().decode(BrokerEnrollmentRecord.self, from: recordJSON)
    let result: InstallBrokerResult = try await call(
      method: "broker.enrollment.install",
      parameters: InstallBrokerParameters(record: record)
    )
    guard result.status == "installed" else {
      throw CoreClientError.contractViolation("Core rejected the protected broker enrollment.")
    }
  }

  public func installCoreLease(_ leaseJSON: Data) async throws {
    let lease = try JSONDecoder().decode(CoreInstanceLease.self, from: leaseJSON)
    let result: InstallBrokerResult = try await call(
      method: "broker.lease.install",
      parameters: InstallCoreLeaseParameters(lease: lease)
    )
    guard result.status == "installed" else {
      throw CoreClientError.contractViolation("Core rejected the protected instance lease.")
    }
  }

  public func dashboard() async throws -> DashboardState {
    let state: DashboardState = try await call(
      method: "mission.dashboard.read",
      parameters: EmptyParameters()
    )
    return try state.validated()
  }

  public func pairChannel(_ pairing: ChannelPairing, proof: BrokerRuntimeState) async throws {
    _ = try pairing.validated(expectedChannel: pairing.channel)
    let result: InstallBrokerResult = try await call(
      method: "channel.pair",
      parameters: PairChannelParameters(pairing: pairing, proof: proof)
    )
    guard result.status == "paired" else {
      throw CoreClientError.contractViolation("Core rejected the exact channel pairing.")
    }
  }

  public func channelPairing(_ channel: ChannelKind) async throws -> ChannelPairing? {
    let pairing: ChannelPairing? = try await call(
      method: "channel.pairing.read",
      parameters: ChannelSelectionParameters(channel: channel)
    )
    return try pairing?.validated(expectedChannel: channel)
  }

  public func startDiscordSetup(
    token: String, proof: BrokerRuntimeState
  ) async throws -> DiscordSetupStart {
    let response: DiscordSetupStart = try await call(
      method: "channel.discord.setup.start",
      parameters: StartDiscordParameters(botToken: token, proof: proof),
      deadline: .seconds(30)
    )
    return try response.validated()
  }

  public func pollDiscordSetup(proof: BrokerRuntimeState) async throws -> DiscordSetupPollResponse {
    try await call(
      method: "channel.discord.setup.poll", parameters: RuntimeProofParameters(proof))
  }

  public func confirmDiscordSetup(
    candidateId: String, confirmedAtMs: Int64, proof: BrokerRuntimeState
  ) async throws {
    let result: InstallBrokerResult = try await call(
      method: "channel.discord.setup.confirm",
      parameters: ConfirmDiscordSetupParameters(
        candidateId: candidateId, confirmedAtMs: confirmedAtMs, proof: proof)
    )
    guard result.status == "paired" else {
      throw CoreClientError.contractViolation("Core rejected Discord setup confirmation.")
    }
  }

  public func startDiscord(
    token: String, proof: BrokerRuntimeState
  ) async throws -> ChannelStatusResponse {
    let response: ChannelStatusResponse = try await call(
      method: "channel.discord.start",
      parameters: StartDiscordParameters(botToken: token, proof: proof)
    )
    return try response.validated()
  }

  public func prepareIMessage(proof: BrokerRuntimeState) async throws {
    let executable = try IMessageRuntimeAuthenticator.executable()
    try IMessageRuntimeAuthenticator.validateStatic(executable)
    let response = try
      (await call(
        method: "channel.imessage.prepare", parameters: RuntimeProofParameters(proof)
      ) as IMessagePrepareResponse).validated()
    do {
      try IMessageRuntimeAuthenticator.validateRunning(response.processIdentifier)
    } catch {
      _ = try? await stopChannelIfRunning(.iMessage)
      throw error
    }
  }

  public func prepareIMessageChatDiscovery(proof: BrokerRuntimeState) async throws {
    let executable = try IMessageRuntimeAuthenticator.executable()
    try IMessageRuntimeAuthenticator.validateStatic(executable)
    let response = try
      (await call(
        method: "channel.imessage.chats.prepare", parameters: RuntimeProofParameters(proof)
      ) as IMessagePrepareResponse).validated()
    do {
      try IMessageRuntimeAuthenticator.validateRunning(response.processIdentifier)
    } catch {
      _ = try? await stopChannelIfRunning(.iMessage)
      throw error
    }
  }

  public func listPreparedIMessageChats(proof: BrokerRuntimeState) async throws -> [IMessageChat] {
    let result = try
      (await call(
        method: "channel.imessage.chats.list", parameters: RuntimeProofParameters(proof)
      ) as IMessageChatsResponse).validated()
    return result.chats
  }

  public func activateIMessage(proof: BrokerRuntimeState) async throws -> ChannelStatusResponse {
    let response: ChannelStatusResponse = try await call(
      method: "channel.imessage.activate", parameters: RuntimeProofParameters(proof)
    )
    return try response.validated()
  }

  public func channelStatus(_ channel: ChannelKind) async throws -> ChannelStatusResponse {
    let response: ChannelStatusResponse = try await call(
      method: "channel.status",
      parameters: ChannelSelectionParameters(channel: channel)
    )
    return try response.validated()
  }

  public func stopChannel(_ channel: ChannelKind) async throws -> ChannelStatusResponse {
    let response: ChannelStatusResponse = try await call(
      method: "channel.stop",
      parameters: ChannelSelectionParameters(channel: channel)
    )
    return try response.validated()
  }

  public func stopChannelIfRunning(_ channel: ChannelKind) async throws
    -> ChannelStatusResponse
  {
    let response: ChannelStatusResponse = try await call(
      method: "channel.stop",
      parameters: ChannelSelectionParameters(channel: channel),
      startPolicy: .requireRunning
    )
    return try response.validated()
  }

  public func pollChannel(
    _ channel: ChannelKind,
    modelWorkAllowed: Bool,
    proof: BrokerRuntimeState
  ) async throws -> ChannelPollResponse {
    let response: ChannelPollResponse = try await call(
      method: "channel.poll",
      parameters: PollChannelParameters(
        channel: channel,
        modelWorkAllowed: modelWorkAllowed,
        proof: proof
      ),
      deadline: .seconds(210)
    )
    return try response.validated(for: channel)
  }

  public func acknowledgeChannelFailure(
    _ incident: ChannelFailureIncident,
    acknowledgedAtMs: Int64,
    proof: BrokerRuntimeState
  ) async throws -> ChannelFailureIncident {
    let response: ChannelFailureIncident = try await call(
      method: "channel.failure.acknowledge",
      parameters: AcknowledgeChannelFailureParameters(
        incident: incident,
        acknowledgedAtMs: acknowledgedAtMs,
        proof: proof
      )
    )
    return try response.validatedAcknowledgementResponse(for: incident)
  }

  public func sendChannelMessage(
    missionId: String,
    routeId: String,
    kind: ChannelMessageKind,
    content: String,
    approvedAtMs: Int64,
    proof: BrokerRuntimeState
  ) async throws -> ChannelSendResponse {
    let response: ChannelSendResponse = try await call(
      method: "channel.outbound.send",
      parameters: SendChannelMessageParameters(
        missionId: missionId,
        routeId: routeId,
        kind: kind,
        content: content,
        approvedAtMs: approvedAtMs,
        proof: proof
      )
    )
    return try response.validated()
  }

  public func bindChannelRoute(
    _ approval: ChannelRouteApproval, proof: BrokerRuntimeState
  ) async throws -> ChannelRouteSet {
    let response: ChannelRouteSet = try await call(
      method: "channel.route.bind",
      parameters: BindChannelRouteParameters(approval: approval, proof: proof)
    )
    return try response.validated(expectedMissionId: approval.missionId)
  }

  public func account(proof: BrokerRuntimeState) async throws -> AccountState {
    try await call(method: "account.read", parameters: RuntimeProofParameters(proof))
  }

  public func beginLogin(proof: BrokerRuntimeState) async throws -> ChatGptLogin {
    try await call(method: "account.login.start", parameters: RuntimeProofParameters(proof))
  }

  public func awaitLogin(identifier: String, proof: BrokerRuntimeState) async throws {
    let result: InstallBrokerResult = try await call(
      method: "account.login.await",
      parameters: AwaitLoginParameters(
        loginId: identifier,
        authorization: proof.authorization,
        brokerReceipt: proof.receipt
      ),
      deadline: Self.loginDeadline
    )
    guard result.status == "completed" else {
      throw CoreClientError.contractViolation("Core rejected managed login completion.")
    }
  }

  public func models(proof: BrokerRuntimeState) async throws -> [GptModel] {
    try await call(method: "models.list", parameters: RuntimeProofParameters(proof))
  }

  public func propose(prompt: String, proof: BrokerRuntimeState) async throws -> OutcomeSuggestion {
    try await call(
      method: "outcome.propose",
      parameters: OutcomeRequest(prompt: prompt, proof: proof),
      deadline: .seconds(210)
    )
  }

  public func confirmSuggestion(
    identifier: String, reminderTarget: ReminderTarget
  ) async throws -> ConfirmedMission {
    try await call(
      method: "mission.confirm",
      parameters: ConfirmSuggestionParameters(
        suggestionId: identifier, reminderTarget: reminderTarget
      )
    )
  }

  public func cancelMission(
    identifier: String, proof: BrokerRuntimeState
  ) async throws -> MissionCancellation {
    let response: MissionCancellation = try await call(
      method: "mission.cancel",
      parameters: CancelMissionParameters(missionId: identifier, proof: proof),
      startPolicy: .requireRunning
    )
    return try response.validated(expectedMissionId: identifier)
  }

  public func beginReminderDispatch(identifier: String) async throws -> ReminderDispatchStart {
    try await call(
      method: "mission.reminders.begin",
      parameters: BeginReminderDispatchParameters(missionId: identifier)
    )
  }

  public func recordReminderMirror(
    identifier: String, links: [ReminderLink]
  ) async throws -> ConfirmedMission {
    try await call(
      method: "mission.reminders.record",
      parameters: RecordReminderMirrorParameters(missionId: identifier, links: links)
    )
  }

  public func completeReminderMission(
    identifier: String,
    completions: [ReminderCompletionInput],
    receiptReturnApprovedAtMs: Int64?,
    receiptReturnRouteId: String?
  ) async throws -> MissionReceipt {
    try await call(
      method: "mission.reminders.complete",
      parameters: CompleteReminderMissionParameters(
        missionId: identifier,
        completions: completions,
        receiptReturnApprovedAtMs: receiptReturnApprovedAtMs,
        receiptReturnRouteId: receiptReturnRouteId
      )
    )
  }

  @discardableResult
  public func shutdown() -> Bool {
    shutdown(
      generation: nil,
      error: .processTerminated,
      reason: .explicitShutdown
    )
  }

  private func call<Parameters, ResultValue>(
    method: String,
    parameters: Parameters,
    deadline: Duration = .seconds(30),
    startPolicy: CallStartPolicy = .startIfNeeded
  ) async throws -> ResultValue
  where Parameters: Encodable & Sendable, ResultValue: Decodable & Sendable {
    let processState: (UInt64, FileHandle)
    if let fencedGeneration = stateLock.withLock({ activeGenerationFence?.generation }) {
      processState = try runningForCall(expectedGeneration: fencedGeneration)
    } else {
      switch startPolicy {
      case .startIfNeeded:
        processState = try await startForCall()
      case .requireRunning:
        processState = try runningForCall()
      }
    }
    let (requestGeneration, handle) = processState
    let identifier = stateLock.withLock { () -> UInt64 in
      let value = nextIdentifier
      nextIdentifier &+= 1
      return value
    }
    guard identifier != 0 else {
      throw CoreClientError.processUnavailable
    }
    let request = RpcRequest(id: identifier, method: method, params: parameters)
    guard var encoded = try? JSONEncoder().encode(request) else {
      throw CoreClientError.malformedResponse
    }
    guard encoded.count <= Self.maximumFrameBytes else {
      throw CoreClientError.oversizedRequest
    }
    encoded.append(0x0A)

    let responseData = try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation {
        (continuation: CheckedContinuation<Data, Error>) in
        let completion: Completion = { result in
          continuation.resume(with: result.mapError { $0 as Error })
        }
        let installed = stateLock.withLock { () -> Bool in
          guard generation == requestGeneration, process?.isRunning == true,
            input === handle
          else {
            return false
          }
          pending[identifier] = PendingRequest(
            generation: requestGeneration,
            completion: completion
          )
          return true
        }
        guard installed else {
          completion(.failure(.processUnavailable))
          return
        }
        let timeout = max(1, deadline.components.seconds)
        DispatchQueue.global(qos: .utility).asyncAfter(
          deadline: .now() + .seconds(Int(timeout))
        ) { [weak self] in
          self?.timeout(identifier: identifier, generation: requestGeneration)
        }
        requestInstalledBeforeWriteHook()
        if Task.isCancelled {
          cancelBeforeWrite(identifier: identifier, generation: requestGeneration)
          return
        }
        do {
          switch try writeFrame(
            encoded,
            identifier: identifier,
            generation: requestGeneration,
            handle: handle
          ) {
          case .written:
            break
          case .revoked(let completion):
            completion?(.failure(.processUnavailable))
          }
        } catch {
          finish(
            identifier: identifier,
            generation: requestGeneration,
            with: .failure(.processTerminated)
          )
          shutdown(
            generation: requestGeneration,
            error: .processTerminated,
            reason: .transportFailure
          )
        }
      }
    } onCancel: { [weak self] in
      self?.cancel(identifier: identifier, generation: requestGeneration)
    }
    guard let decoded = try? JSONDecoder().decode(ResultValue.self, from: responseData) else {
      throw CoreClientError.malformedResponse
    }
    return decoded
  }

  private func writeFrame(
    _ frame: Data,
    identifier: UInt64,
    generation requestGeneration: UInt64,
    handle: FileHandle
  ) throws -> FrameWriteDisposition {
    try writeLock.withLock {
      let revokedCompletion = stateLock.withLock { () -> Completion?? in
        guard generation == requestGeneration, process?.isRunning == true,
          input === handle, pending[identifier]?.generation == requestGeneration
        else {
          let completion =
            pending[identifier]?.generation == requestGeneration
            ? pending.removeValue(forKey: identifier)?.completion
            : nil
          return .some(completion)
        }
        return nil
      }
      if let revokedCompletion {
        return .revoked(revokedCompletion)
      }
      try handle.write(contentsOf: frame)
      return .written
    }
  }

  private func startForCall() async throws -> (UInt64, FileHandle) {
    for delay in [
      Duration.zero, .milliseconds(25), .milliseconds(50), .milliseconds(100),
      .milliseconds(200), .milliseconds(400), .milliseconds(800),
    ] {
      if delay > .zero {
        try await Task.sleep(for: delay)
      }
      do {
        return try startIfNeeded()
      } catch LaunchStateError.priorGenerationStopping {
        continue
      }
    }
    throw CoreClientError.processUnavailable
  }

  private func runningForCall() throws -> (UInt64, FileHandle) {
    try stateLock.withLock {
      guard process?.isRunning == true, let input else {
        throw CoreClientError.processUnavailable
      }
      return (generation, input)
    }
  }

  private func runningForCall(expectedGeneration: UInt64) throws -> (UInt64, FileHandle) {
    try stateLock.withLock {
      guard generation == expectedGeneration, process?.isRunning == true, let input else {
        throw CoreClientError.processUnavailable
      }
      return (generation, input)
    }
  }

  private func startIfNeeded() throws -> (UInt64, FileHandle) {
    stateLock.lock()
    defer { stateLock.unlock() }
    if process?.isRunning == true, let input, quarantinedGeneration == nil {
      return (generation, input)
    }
    if process != nil {
      throw LaunchStateError.priorGenerationStopping
    }
    let executable = try executableResolver()
    try staticCodeValidator(executable)

    let standardInput = Pipe()
    let standardOutput = Pipe()
    let standardError = Pipe()
    let child = Process()
    child.executableURL = executable
    child.arguments = []
    child.currentDirectoryURL = executable.deletingLastPathComponent()
    child.environment = childEnvironmentLoader()
    child.standardInput = standardInput
    child.standardOutput = standardOutput
    child.standardError = standardError
    let nextGeneration = generation &+ 1
    guard nextGeneration != 0 else { throw CoreClientError.processUnavailable }
    child.terminationHandler = { [weak self] terminated in
      self?.processTerminated(generation: nextGeneration, process: terminated)
    }
    var master = Data()
    var auditTokenHex: String?
    var childWasLaunched = false
    do {
      try child.run()
      childWasLaunched = true
      let capturedAuditTokenHex = try Self.captureAuditTokenHex(
        for: child.processIdentifier)
      auditTokenHex = capturedAuditTokenHex
      try runningCodeValidator(capturedAuditTokenHex)
      guard child.isRunning else { throw CoreClientError.processUnavailable }
      master = try masterKeyLoader()
      guard master.count == 32 else {
        throw CoreClientError.keychain(errSecDecode)
      }
      var bootstrap = Self.bootstrapMagic
      bootstrap.append(master)
      bootstrap.append(0x0A)
      master.resetBytes(in: master.indices)
      defer { bootstrap.resetBytes(in: bootstrap.indices) }
      try standardInput.fileHandleForWriting.write(contentsOf: bootstrap)
    } catch {
      master.resetBytes(in: master.indices)
      try? standardInput.fileHandleForWriting.close()
      try? standardOutput.fileHandleForReading.close()
      try? standardError.fileHandleForReading.close()
      guard childWasLaunched else { throw CoreClientError.processUnavailable }
      let terminationAccepted = auditTokenHex.map(exactProcessTerminator) ?? false
      if terminationAccepted, Self.waitForExactProcessExit(child) {
        throw CoreClientError.processUnavailable
      }
      // A launched child is never forgotten. If exact termination was rejected
      // or its exit could not be proven, retain a quarantined generation with
      // no writable transport. Subsequent calls fail closed until the exact
      // Process exits; they can never launch an overlapping replacement.
      generation = nextGeneration
      process = child
      processAuditTokenHex = auditTokenHex
      input = nil
      quarantinedGeneration = nextGeneration
      shutdownReasons[nextGeneration] = .transportFailure
      throw CoreClientError.processUnavailable
    }
    generation = nextGeneration
    process = child
    processAuditTokenHex = auditTokenHex
    quarantinedGeneration = nil
    input = standardInput.fileHandleForWriting
    beginReading(output: standardOutput.fileHandleForReading, generation: nextGeneration)
    beginDraining(error: standardError.fileHandleForReading)
    return (nextGeneration, standardInput.fileHandleForWriting)
  }

  private func beginReading(output: FileHandle, generation: UInt64) {
    readerQueue.async { [weak self] in
      guard let self else { return }
      var buffer = Data()
      while true {
        // `read(upToCount:)` may wait for the requested byte count or EOF.
        // Core is intentionally persistent and its line responses are much
        // smaller than the read cap, so consume bytes as soon as the pipe is
        // readable instead of requiring Core to exit after every response.
        let chunk = output.availableData
        guard !chunk.isEmpty else { break }
        buffer.append(chunk)
        if buffer.count > Self.maximumFrameBytes, !buffer.contains(0x0A) {
          self.failGeneration(generation, with: .oversizedFrame)
          self.shutdown(
            generation: generation,
            error: .oversizedFrame,
            reason: .protocolViolation
          )
          return
        }
        while let newline = buffer.firstIndex(of: 0x0A) {
          var frame = Data(buffer[..<newline])
          buffer.removeSubrange(...newline)
          if frame.last == 0x0D {
            frame.removeLast()
          }
          guard frame.count <= Self.maximumFrameBytes else {
            self.failGeneration(generation, with: .oversizedFrame)
            self.shutdown(
              generation: generation,
              error: .oversizedFrame,
              reason: .protocolViolation
            )
            return
          }
          self.consume(frame: frame, generation: generation)
        }
      }
      // EOF is already a terminal transport fact even if Process has not yet
      // delivered its termination callback. Close launch authority for this
      // generation before waking callers so a retry cannot attach a request to
      // the same dying child during that callback window.
      let isCurrentGeneration = self.writeLock.withLock { () -> Bool in
        self.stateLock.withLock { () -> Bool in
          guard self.generation == generation else { return false }
          self.input = nil
          return true
        }
      }
      if isCurrentGeneration {
        self.inputRevokedHook()
        self.failGeneration(generation, with: .processTerminated)
      }
    }
  }

  private func beginDraining(error: FileHandle) {
    errorQueue.async {
      while !error.availableData.isEmpty {}
    }
  }

  private func consume(frame: Data, generation: UInt64) {
    guard
      let object = try? JSONSerialization.jsonObject(with: frame),
      let response = object as? [String: Any],
      response["jsonrpc"] as? String == "2.0",
      let number = response["id"] as? NSNumber,
      CFGetTypeID(number) != CFBooleanGetTypeID()
    else {
      failGeneration(generation, with: .malformedResponse)
      shutdown(
        generation: generation,
        error: .malformedResponse,
        reason: .protocolViolation
      )
      return
    }
    guard let identifier = Self.exactResponseIdentifier(number) else {
      failGeneration(generation, with: .malformedResponse)
      shutdown(
        generation: generation,
        error: .malformedResponse,
        reason: .protocolViolation
      )
      return
    }

    if let error = response["error"] as? [String: Any] {
      guard response["result"] == nil,
        let code = error["code"] as? Int,
        let message = error["message"] as? String
      else {
        failGeneration(generation, with: .malformedResponse)
        shutdown(
          generation: generation,
          error: .malformedResponse,
          reason: .protocolViolation
        )
        return
      }
      finish(
        identifier: identifier,
        generation: generation,
        with: .failure(.remote(code: code, message: message))
      )
      return
    }
    guard let result = response["result"],
      let data = try? JSONSerialization.data(withJSONObject: result, options: [.fragmentsAllowed])
    else {
      failGeneration(generation, with: .malformedResponse)
      shutdown(
        generation: generation,
        error: .malformedResponse,
        reason: .protocolViolation
      )
      return
    }
    finish(identifier: identifier, generation: generation, with: .success(data))
  }

  static func exactResponseIdentifier(_ number: NSNumber) -> UInt64? {
    guard CFGetTypeID(number) != CFBooleanGetTypeID(), !CFNumberIsFloatType(number),
      number.int64Value > 0
    else {
      return nil
    }
    return UInt64(number.int64Value)
  }

  private func finish(
    identifier: UInt64,
    generation: UInt64,
    with result: Result<Data, CoreClientError>
  ) {
    enum Resolution {
      case completion(Completion)
      case abandoned
      case unknown
    }
    let resolution = stateLock.withLock { () -> Resolution in
      if pending[identifier]?.generation == generation,
        let completion = pending.removeValue(forKey: identifier)?.completion
      {
        return .completion(completion)
      }
      if abandoned[identifier] == generation {
        abandoned.removeValue(forKey: identifier)
        return .abandoned
      }
      return .unknown
    }
    switch resolution {
    case .completion(let completion):
      completion(result)
    case .abandoned:
      return
    case .unknown:
      failGeneration(generation, with: .unknownResponseIdentifier)
      shutdown(
        generation: generation,
        error: .unknownResponseIdentifier,
        reason: .protocolViolation
      )
    }
  }

  private func failGeneration(_ generation: UInt64, with error: CoreClientError) {
    let completions: [Completion] = stateLock.withLock {
      let identifiers = pending.compactMap { identifier, request in
        request.generation == generation ? identifier : nil
      }
      let values = identifiers.compactMap { pending.removeValue(forKey: $0)?.completion }
      return values
    }
    for completion in completions {
      completion(.failure(error))
    }
  }

  private func timeout(identifier: UInt64, generation: UInt64) {
    let completion = stateLock.withLock { () -> Completion? in
      guard pending[identifier]?.generation == generation else { return nil }
      abandoned[identifier] = generation
      return pending.removeValue(forKey: identifier)?.completion
    }
    guard let completion else { return }
    completion(.failure(.requestTimedOut))
    shutdown(
      generation: generation,
      error: .requestTimedOut,
      reason: .requestTimedOut
    )
  }

  private func cancel(identifier: UInt64, generation: UInt64) {
    let result = stateLock.withLock { () -> (Completion?, Bool)? in
      guard pending[identifier]?.generation == generation else { return nil }
      let overflow = abandoned.count >= Self.maximumAbandonedRequests
      if !overflow {
        // A SwiftUI view task may disappear while Core is still completing a
        // valid shared-process RPC. Resume the cancelled caller now, then
        // consume exactly that late response without terminating Core.
        abandoned[identifier] = generation
      }
      return (pending.removeValue(forKey: identifier)?.completion, overflow)
    }
    guard let (completion, overflow) = result else { return }
    completion?(.failure(.requestCancelled))
    if overflow {
      shutdown(
        generation: generation,
        error: .processTerminated,
        reason: .transportFailure
      )
    }
  }

  private func cancelBeforeWrite(identifier: UInt64, generation: UInt64) {
    let completion = stateLock.withLock { () -> Completion? in
      // `onCancel` may already have moved this request to `abandoned`. Because
      // the caller has proven that no frame was written, remove that tombstone:
      // no Core response can ever arrive for it.
      if abandoned[identifier] == generation {
        abandoned.removeValue(forKey: identifier)
        return nil
      }
      guard pending[identifier]?.generation == generation else { return nil }
      return pending.removeValue(forKey: identifier)?.completion
    }
    completion?(.failure(.requestCancelled))
  }

  private func processTerminated(generation terminatedGeneration: UInt64, process child: Process) {
    let result = stateLock.withLock {
      () -> (
        CoreTerminationEvent, [AsyncStream<CoreTerminationEvent>.Continuation]
      )? in
      guard generation == terminatedGeneration, process === child else { return nil }
      let requestedReason = shutdownReasons.removeValue(forKey: terminatedGeneration)
      let reason: CoreTerminationReason
      if let requestedReason {
        reason = requestedReason
      } else if child.terminationReason == .uncaughtSignal {
        reason = .uncaughtSignal
      } else {
        reason = .exited
      }
      process = nil
      processAuditTokenHex = nil
      if quarantinedGeneration == terminatedGeneration {
        quarantinedGeneration = nil
      }
      input = nil
      abandoned = abandoned.filter { $0.value != terminatedGeneration }
      return (
        CoreTerminationEvent(
          generation: terminatedGeneration,
          reason: reason,
          exitStatus: child.terminationStatus
        ),
        Array(lifecycleObservers.values)
      )
    }
    guard let (event, observers) = result else { return }
    failGeneration(terminatedGeneration, with: .processTerminated)
    for observer in observers {
      observer.yield(event)
    }
  }

  @discardableResult
  private func shutdown(
    generation targetGeneration: UInt64?,
    error: CoreClientError,
    reason: CoreTerminationReason
  ) -> Bool {
    let decision = stateLock.withLock {
      () -> ((Process, String, FileHandle?, UInt64)?, Bool) in
      if targetGeneration == nil {
        activeGenerationFence = nil
      }
      guard targetGeneration == nil || targetGeneration == generation else {
        return (nil, true)
      }
      guard let running = process else { return (nil, true) }
      guard let auditTokenHex = processAuditTokenHex else { return (nil, false) }
      let stoppedGeneration = generation
      let stoppedInput = input
      input = nil
      // Once a shutdown attempt revokes transport, retain this exact process
      // generation as quarantined until Process termination is positively
      // observed. A refused exact terminator may then be retried against the
      // same captured audit token; it must never degrade into waiting for an
      // exit that no caller requested or allow an overlapping replacement.
      quarantinedGeneration = stoppedGeneration
      shutdownReasons[stoppedGeneration] = reason
      return ((running, auditTokenHex, stoppedInput, stoppedGeneration), true)
    }
    guard
      let (running, auditTokenHex, stoppedInput, stoppedGeneration) = decision.0
    else {
      return decision.1
    }
    failGeneration(stoppedGeneration, with: error)
    let terminationAccepted = exactProcessTerminator(auditTokenHex)
    try? stoppedInput?.close()
    guard terminationAccepted else { return false }
    return Self.waitForExactProcessExit(running)
  }

  private static func captureAuditTokenHex(for processIdentifier: Int32) throws -> String {
    guard processIdentifier > 0 else { throw CoreClientError.processUnavailable }
    var task = mach_port_name_t(MACH_PORT_NULL)
    guard task_name_for_pid(mach_task_self_, processIdentifier, &task) == KERN_SUCCESS else {
      throw CoreClientError.processUnavailable
    }
    defer { mach_port_deallocate(mach_task_self_, task) }
    var token = audit_token_t()
    let expectedCount = mach_msg_type_number_t(
      MemoryLayout<audit_token_t>.size / MemoryLayout<natural_t>.size
    )
    var count = expectedCount
    let status = withUnsafeMutablePointer(to: &token) { pointer in
      pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { words in
        task_info(task, task_flavor_t(TASK_AUDIT_TOKEN), words, &count)
      }
    }
    guard status == KERN_SUCCESS, count == expectedCount,
      audit_token_to_pid(token) == processIdentifier
    else {
      throw CoreClientError.processUnavailable
    }
    return withUnsafeBytes(of: token) { bytes in
      bytes.map { String(format: "%02x", $0) }.joined()
    }
  }

  private static func terminateExactProcess(_ capturedTokenHex: String) -> Bool {
    guard capturedTokenHex.count == MemoryLayout<audit_token_t>.size * 2 else { return false }
    var bytes = [UInt8]()
    bytes.reserveCapacity(MemoryLayout<audit_token_t>.size)
    var index = capturedTokenHex.startIndex
    while index < capturedTokenHex.endIndex {
      let next = capturedTokenHex.index(index, offsetBy: 2)
      guard let byte = UInt8(capturedTokenHex[index..<next], radix: 16) else { return false }
      bytes.append(byte)
      index = next
    }
    var token = audit_token_t()
    withUnsafeMutableBytes(of: &token) { destination in
      destination.copyBytes(from: bytes)
    }
    var signal = SIGKILL
    if proc_terminate_with_audittoken(&token, &signal) == 0 { return true }
    return errno == ESRCH
  }

  private static func waitForExactProcessExit(_ process: Process) -> Bool {
    let exited = DispatchSemaphore(value: 0)
    DispatchQueue.global(qos: .utility).async {
      process.waitUntilExit()
      exited.signal()
    }
    guard exited.wait(timeout: .now() + .seconds(2)) == .success else { return false }
    return !process.isRunning
  }

  private func finishLifecycleObservers() {
    let observers = stateLock.withLock { () -> [AsyncStream<CoreTerminationEvent>.Continuation] in
      let values = Array(lifecycleObservers.values)
      lifecycleObservers.removeAll()
      return values
    }
    for observer in observers {
      observer.finish()
    }
  }
}
