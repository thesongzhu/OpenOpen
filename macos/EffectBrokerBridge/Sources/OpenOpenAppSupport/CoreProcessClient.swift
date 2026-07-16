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

  static func validateRunning(_ processIdentifier: Int32) throws {
    let identity = try currentHostIdentity()
    try StaticCodeSigningValidator.validateRunningProcessIdentifier(
      processIdentifier,
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

public final class CoreProcessClient: @unchecked Sendable {
  private static let bootstrapMagic = Data("OPENOPEN_BOOTSTRAP_V1\0".utf8)
  private static let maximumFrameBytes = 8 * 1024 * 1024
  private static let loginDeadline: Duration = .seconds(660)

  private typealias Completion = @Sendable (Result<Data, CoreClientError>) -> Void
  private struct PendingRequest {
    let generation: UInt64
    let completion: Completion
  }

  private let stateLock = NSLock()
  private let writeLock = NSLock()
  private let readerQueue = DispatchQueue(label: "com.thesongzhu.OpenOpen.core-reader")
  private let errorQueue = DispatchQueue(label: "com.thesongzhu.OpenOpen.core-stderr")
  private let executableResolver: @Sendable () throws -> URL
  private let staticCodeValidator: @Sendable (URL) throws -> Void
  private let runningCodeValidator: @Sendable (Int32) throws -> Void
  private let masterKeyLoader: @Sendable () throws -> Data
  private let childEnvironmentLoader: @Sendable () -> [String: String]
  private var process: Process?
  private var input: FileHandle?
  private var pending: [UInt64: PendingRequest] = [:]
  private var nextIdentifier: UInt64 = 1
  private var generation: UInt64 = 0

  public init() {
    executableResolver = { try CoreExecutableResolver.resolve() }
    staticCodeValidator = { try CoreExecutableAuthenticator.validateStatic($0) }
    runningCodeValidator = { try CoreExecutableAuthenticator.validateRunning($0) }
    masterKeyLoader = { try KeychainMasterKey.loadOrCreate() }
    childEnvironmentLoader = {
      ["HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]
    }
  }

  init(
    executableResolver: @escaping @Sendable () throws -> URL,
    staticCodeValidator: @escaping @Sendable (URL) throws -> Void,
    runningCodeValidator: @escaping @Sendable (Int32) throws -> Void,
    masterKeyLoader: @escaping @Sendable () throws -> Data,
    childEnvironmentLoader: @escaping @Sendable () -> [String: String] = {
      ["HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]
    }
  ) {
    self.executableResolver = executableResolver
    self.staticCodeValidator = staticCodeValidator
    self.runningCodeValidator = runningCodeValidator
    self.masterKeyLoader = masterKeyLoader
    self.childEnvironmentLoader = childEnvironmentLoader
  }

  deinit {
    shutdown()
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
    let result: InstallBrokerResult = try await call(
      method: "channel.pair",
      parameters: PairChannelParameters(pairing: pairing, proof: proof)
    )
    guard result.status == "paired" else {
      throw CoreClientError.contractViolation("Core rejected the exact channel pairing.")
    }
  }

  public func channelPairing(_ channel: ChannelKind) async throws -> ChannelPairing? {
    try await call(
      method: "channel.pairing.read",
      parameters: ChannelSelectionParameters(channel: channel)
    )
  }

  public func startDiscordSetup(
    token: String, proof: BrokerRuntimeState
  ) async throws -> DiscordSetupStart {
    try await call(
      method: "channel.discord.setup.start",
      parameters: StartDiscordParameters(botToken: token, proof: proof),
      deadline: .seconds(30)
    )
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
    try await call(
      method: "channel.discord.start",
      parameters: StartDiscordParameters(botToken: token, proof: proof)
    )
  }

  public func prepareIMessage(proof: BrokerRuntimeState) async throws {
    let executable = try IMessageRuntimeAuthenticator.executable()
    try IMessageRuntimeAuthenticator.validateStatic(executable)
    let response: IMessagePrepareResponse = try await call(
      method: "channel.imessage.prepare", parameters: RuntimeProofParameters(proof)
    )
    do {
      try IMessageRuntimeAuthenticator.validateRunning(response.processIdentifier)
    } catch {
      _ = try? await stopChannel(.iMessage)
      throw error
    }
  }

  public func prepareIMessageChatDiscovery(proof: BrokerRuntimeState) async throws {
    let executable = try IMessageRuntimeAuthenticator.executable()
    try IMessageRuntimeAuthenticator.validateStatic(executable)
    let response: IMessagePrepareResponse = try await call(
      method: "channel.imessage.chats.prepare", parameters: RuntimeProofParameters(proof)
    )
    do {
      try IMessageRuntimeAuthenticator.validateRunning(response.processIdentifier)
    } catch {
      _ = try? await stopChannel(.iMessage)
      throw error
    }
  }

  public func listPreparedIMessageChats(proof: BrokerRuntimeState) async throws -> [IMessageChat] {
    let result: IMessageChatsResponse = try await call(
      method: "channel.imessage.chats.list", parameters: RuntimeProofParameters(proof)
    )
    guard result.chats.count <= 200,
      Set(result.chats.map(\.chatId)).count == result.chats.count,
      result.chats.allSatisfy({ chat in
        guard let chatId = Int64(chat.chatId), chatId > 0 else { return false }
        return !chat.service.isEmpty
          && !chat.participants.isEmpty
          && Set(chat.participants).count == chat.participants.count
          && chat.participants.allSatisfy({
            !$0.isEmpty && $0 == $0.trimmingCharacters(in: .whitespacesAndNewlines)
          })
      })
    else {
      throw CoreClientError.contractViolation("Core returned invalid iMessage conversations.")
    }
    return result.chats
  }

  public func activateIMessage(proof: BrokerRuntimeState) async throws -> ChannelStatusResponse {
    try await call(
      method: "channel.imessage.activate", parameters: RuntimeProofParameters(proof)
    )
  }

  public func channelStatus(_ channel: ChannelKind) async throws -> ChannelStatusResponse {
    try await call(
      method: "channel.status",
      parameters: ChannelSelectionParameters(channel: channel)
    )
  }

  public func stopChannel(_ channel: ChannelKind) async throws -> ChannelStatusResponse {
    try await call(
      method: "channel.stop",
      parameters: ChannelSelectionParameters(channel: channel)
    )
  }

  public func pollChannel(
    _ channel: ChannelKind, proof: BrokerRuntimeState
  ) async throws -> ChannelPollResponse {
    try await call(
      method: "channel.poll",
      parameters: PollChannelParameters(channel: channel, proof: proof),
      deadline: .seconds(210)
    )
  }

  public func sendChannelMessage(
    missionId: String,
    kind: ChannelMessageKind,
    content: String,
    approvedAtMs: Int64,
    proof: BrokerRuntimeState
  ) async throws -> ChannelSendResponse {
    try await call(
      method: "channel.outbound.send",
      parameters: SendChannelMessageParameters(
        missionId: missionId,
        kind: kind,
        content: content,
        approvedAtMs: approvedAtMs,
        proof: proof
      )
    )
  }

  public func account(proof: BrokerRuntimeState) async throws -> AccountState {
    try await call(method: "account.read", parameters: RuntimeProofParameters(proof))
  }

  public func beginLogin(proof: BrokerRuntimeState) async throws -> ChatGptLogin {
    try await call(method: "account.login.start", parameters: RuntimeProofParameters(proof))
  }

  public func awaitLogin(identifier: String, proof: BrokerRuntimeState) async throws -> AccountState
  {
    try await call(
      method: "account.login.await",
      parameters: AwaitLoginParameters(
        loginId: identifier,
        authorization: proof.authorization,
        brokerReceipt: proof.receipt
      ),
      deadline: Self.loginDeadline
    )
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
    receiptReturnApprovedAtMs: Int64?
  ) async throws -> MissionReceipt {
    try await call(
      method: "mission.reminders.complete",
      parameters: CompleteReminderMissionParameters(
        missionId: identifier,
        completions: completions,
        receiptReturnApprovedAtMs: receiptReturnApprovedAtMs
      )
    )
  }

  public func shutdown() {
    shutdown(generation: nil, error: .processTerminated)
  }

  private func call<Parameters, ResultValue>(
    method: String,
    parameters: Parameters,
    deadline: Duration = .seconds(30)
  ) async throws -> ResultValue
  where Parameters: Encodable & Sendable, ResultValue: Decodable & Sendable {
    let (requestGeneration, handle) = try startIfNeeded()
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
        if Task.isCancelled {
          cancel(identifier: identifier, generation: requestGeneration)
          return
        }
        do {
          try writeLock.withLock {
            try handle.write(contentsOf: encoded)
          }
        } catch {
          finish(
            identifier: identifier,
            generation: requestGeneration,
            with: .failure(.processTerminated)
          )
          shutdown(generation: requestGeneration, error: .processTerminated)
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

  private func startIfNeeded() throws -> (UInt64, FileHandle) {
    stateLock.lock()
    defer { stateLock.unlock() }
    if process?.isRunning == true, let input {
      return (generation, input)
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
    child.terminationHandler = { [weak self] _ in
      self?.processTerminated(generation: nextGeneration)
    }
    var master = Data()
    do {
      try child.run()
      try runningCodeValidator(child.processIdentifier)
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
      DispatchQueue.global(qos: .utility).async {
        child.waitUntilExit()
      }
      throw CoreClientError.processUnavailable
    }
    generation = nextGeneration
    process = child
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
          self.shutdown(generation: generation, error: .oversizedFrame)
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
            self.shutdown(generation: generation, error: .oversizedFrame)
            return
          }
          self.consume(frame: frame, generation: generation)
        }
      }
      self.failGeneration(generation, with: .processTerminated)
      self.processTerminated(generation: generation)
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
      shutdown(generation: generation, error: .malformedResponse)
      return
    }
    guard let identifier = Self.exactResponseIdentifier(number) else {
      failGeneration(generation, with: .malformedResponse)
      shutdown(generation: generation, error: .malformedResponse)
      return
    }

    if let error = response["error"] as? [String: Any] {
      guard response["result"] == nil,
        let code = error["code"] as? Int,
        let message = error["message"] as? String
      else {
        failGeneration(generation, with: .malformedResponse)
        shutdown(generation: generation, error: .malformedResponse)
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
      shutdown(generation: generation, error: .malformedResponse)
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
    let completion = stateLock.withLock { () -> Completion? in
      guard pending[identifier]?.generation == generation else { return nil }
      return pending.removeValue(forKey: identifier)?.completion
    }
    guard let completion else {
      failGeneration(generation, with: .unknownResponseIdentifier)
      shutdown(generation: generation, error: .unknownResponseIdentifier)
      return
    }
    completion(result)
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
    let exists = stateLock.withLock { pending[identifier]?.generation == generation }
    guard exists else { return }
    finish(identifier: identifier, generation: generation, with: .failure(.requestTimedOut))
    shutdown(generation: generation, error: .requestTimedOut)
  }

  private func cancel(identifier: UInt64, generation: UInt64) {
    let exists = stateLock.withLock { pending[identifier]?.generation == generation }
    guard exists else { return }
    finish(identifier: identifier, generation: generation, with: .failure(.requestCancelled))
    shutdown(generation: generation, error: .requestCancelled)
  }

  private func processTerminated(generation terminatedGeneration: UInt64) {
    stateLock.withLock {
      guard generation == terminatedGeneration else { return }
      process = nil
      input = nil
    }
  }

  private func shutdown(generation targetGeneration: UInt64?, error: CoreClientError) {
    let state = stateLock.withLock { () -> (Process, FileHandle?, UInt64)? in
      guard let running = process,
        targetGeneration == nil || targetGeneration == generation
      else {
        return nil
      }
      let stoppedGeneration = generation
      let stoppedInput = input
      process = nil
      input = nil
      return (running, stoppedInput, stoppedGeneration)
    }
    guard let (running, stoppedInput, stoppedGeneration) = state else { return }
    failGeneration(stoppedGeneration, with: error)
    try? stoppedInput?.close()
    DispatchQueue.global(qos: .utility).async {
      running.waitUntilExit()
    }
  }
}
