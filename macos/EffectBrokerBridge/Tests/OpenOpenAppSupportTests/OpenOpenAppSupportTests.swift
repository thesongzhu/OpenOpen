import CryptoKit
import EffectBrokerBridge
import Foundation
import Testing

@testable import OpenOpenAppSupport

private actor MockCore: CoreServing {
  var control = RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0)
  let dashboardDelay: Duration
  let dashboardFails: Bool
  var challengesIssued = 0
  var proofNonces: [String] = []
  var proposalCount = 0
  var leaseInstalled = false
  var codexInitialized = false
  var codexInitializeCount = 0
  var codexAbortCount = 0
  var offPrepareCount = 0
  var activeOperation = false
  var coreInstanceNonce = String(repeating: "a", count: 64)
  var rejectNextOffPreparation = false
  var mismatchRecoveryTimestamp = false

  init(dashboardDelay: Duration = .zero, dashboardFails: Bool = false) {
    self.dashboardDelay = dashboardDelay
    self.dashboardFails = dashboardFails
  }

  func runtime() -> RuntimeControl { control }
  func effectIdentity() -> CoreEffectIdentity {
    testCoreIdentity(coreInstanceNonce: coreInstanceNonce)
  }
  func signBrokerEnrollment(_: EnrolledBrokerTrustAnchor) -> Data { Data("{}".utf8) }
  func runtimeChallenge() -> String {
    challengesIssued += 1
    let suffix = String(challengesIssued, radix: 16)
    return String(repeating: "0", count: 64 - suffix.count) + suffix
  }

  func prepareRuntime(_ enabled: Bool) throws -> RuntimeControlAuthorization {
    if !enabled {
      if rejectNextOffPreparation {
        rejectNextOffPreparation = false
        throw CoreClientError.contractViolation("Core Off preparation failed.")
      }
      offPrepareCount += 1
      activeOperation = false
    }
    return RuntimeControlAuthorization(
      protocolVersion: 1,
      enabled: enabled,
      revision: control.revision + 1,
      updatedAtMs: control.updatedAtMs + 1,
      coreKeyId: String(repeating: "1", count: 64),
      authorizationSignatureHex: String(repeating: "2", count: 128)
    )
  }

  func commitRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt _: RuntimeControlReceipt
  ) -> RuntimeControl {
    control = RuntimeControl(
      enabled: authorization.enabled,
      revision: authorization.revision,
      updatedAtMs: authorization.updatedAtMs
    )
    return control
  }

  func recoverRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt _: RuntimeControlReceipt
  ) -> RuntimeControl {
    control = RuntimeControl(
      enabled: authorization.enabled,
      revision: authorization.revision,
      updatedAtMs: authorization.updatedAtMs
    )
    if mismatchRecoveryTimestamp {
      return RuntimeControl(
        enabled: control.enabled,
        revision: control.revision,
        updatedAtMs: control.updatedAtMs + 1
      )
    }
    return control
  }

  func installBrokerEnrollment(_: Data) {}

  func startActiveOperation() { activeOperation = true }
  func forceRuntime(_ enabled: Bool) {
    control = RuntimeControl(
      enabled: enabled,
      revision: control.revision + 1,
      updatedAtMs: control.updatedAtMs + 1
    )
  }
  func rotateCoreInstance() { coreInstanceNonce = String(repeating: "b", count: 64) }
  func failNextOffPreparation() { rejectNextOffPreparation = true }
  func returnMismatchedRecoveryTimestamp() { mismatchRecoveryTimestamp = true }

  func dashboard() async throws -> DashboardState {
    if dashboardDelay > .zero { try await Task.sleep(for: dashboardDelay) }
    if dashboardFails {
      throw CoreClientError.contractViolation("Delayed dashboard failure.")
    }
    return DashboardState(
      activeCards: [],
      microphone: MicrophoneState(
        available: false,
        reason: "Microphone unavailable until Voice setup"
      ),
      runtime: control,
      suggestion: nil
    )
  }

  func account(proof: BrokerRuntimeState) -> AccountState {
    proofNonces.append(proof.receipt.requestNonce ?? "")
    return .notConnected
  }

  func beginLogin(proof _: BrokerRuntimeState) -> ChatGptLogin {
    ChatGptLogin(authUrl: "https://example.invalid", loginId: "login-1")
  }

  func awaitLogin(identifier _: String, proof _: BrokerRuntimeState) -> AccountState {
    .notConnected
  }

  func models(proof: BrokerRuntimeState) -> [GptModel] {
    proofNonces.append(proof.receipt.requestNonce ?? "")
    return []
  }

  func propose(prompt _: String, proof _: BrokerRuntimeState) -> OutcomeSuggestion {
    proposalCount += 1
    return OutcomeSuggestion(
      id: "suggestion-1",
      title: "Plan the day",
      whyNow: "It is morning",
      proposedSteps: ["Pick one priority"],
      sourceRefs: []
    )
  }
}

private actor FailClosedOffCore: CoreServing {
  var control = RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0)
  var rejectOffRecovery = true

  func allowOffRecovery() { rejectOffRecovery = false }

  func runtime() -> RuntimeControl { control }
  func effectIdentity() -> CoreEffectIdentity { testCoreIdentity() }
  func signBrokerEnrollment(_: EnrolledBrokerTrustAnchor) -> Data { Data("{}".utf8) }
  func runtimeChallenge() -> String { String(repeating: "a", count: 64) }
  func prepareRuntime(_ enabled: Bool) -> RuntimeControlAuthorization {
    RuntimeControlAuthorization(
      protocolVersion: 1,
      enabled: enabled,
      revision: control.revision + 1,
      updatedAtMs: control.updatedAtMs + 1,
      coreKeyId: String(repeating: "1", count: 64),
      authorizationSignatureHex: String(repeating: "2", count: 128)
    )
  }
  func commitRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt _: RuntimeControlReceipt
  ) throws -> RuntimeControl {
    if !authorization.enabled, rejectOffRecovery {
      throw CoreClientError.contractViolation("Core commit failed after protected Off.")
    }
    control = RuntimeControl(
      enabled: authorization.enabled,
      revision: authorization.revision,
      updatedAtMs: authorization.updatedAtMs
    )
    return control
  }
  func recoverRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt _: RuntimeControlReceipt
  ) throws -> RuntimeControl {
    if !authorization.enabled, rejectOffRecovery {
      throw CoreClientError.contractViolation("Core recovery is temporarily unavailable.")
    }
    control = RuntimeControl(
      enabled: authorization.enabled,
      revision: authorization.revision,
      updatedAtMs: authorization.updatedAtMs
    )
    return control
  }
  func installBrokerEnrollment(_: Data) {}
  func dashboard() -> DashboardState {
    DashboardState(
      activeCards: [],
      microphone: MicrophoneState(available: false, reason: "Unavailable"),
      runtime: control,
      suggestion: nil
    )
  }
  func account(proof _: BrokerRuntimeState) -> AccountState { .notConnected }
  func beginLogin(proof _: BrokerRuntimeState) -> ChatGptLogin {
    ChatGptLogin(authUrl: "https://example.invalid", loginId: "login-1")
  }
  func awaitLogin(identifier _: String, proof _: BrokerRuntimeState) -> AccountState {
    .notConnected
  }
  func models(proof _: BrokerRuntimeState) -> [GptModel] { [] }
  func propose(prompt _: String, proof _: BrokerRuntimeState) -> OutcomeSuggestion {
    OutcomeSuggestion(
      id: "suggestion-1",
      title: "Plan",
      whyNow: "Now",
      proposedSteps: ["One"],
      sourceRefs: []
    )
  }
}

private actor DelayedSwitchCore: CoreServing {
  var control = RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0)
  var writes: [Bool] = []

  func runtime() -> RuntimeControl { control }
  func effectIdentity() -> CoreEffectIdentity { testCoreIdentity() }
  func signBrokerEnrollment(_: EnrolledBrokerTrustAnchor) -> Data { Data("{}".utf8) }
  func runtimeChallenge() -> String { String(repeating: "a", count: 64) }

  func prepareRuntime(_ enabled: Bool) async throws -> RuntimeControlAuthorization {
    writes.append(enabled)
    if enabled {
      try await Task.sleep(for: .milliseconds(75))
    }
    return RuntimeControlAuthorization(
      protocolVersion: 1,
      enabled: enabled,
      revision: control.revision + 1,
      updatedAtMs: control.updatedAtMs + 1,
      coreKeyId: String(repeating: "1", count: 64),
      authorizationSignatureHex: String(repeating: "2", count: 128)
    )
  }

  func commitRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt _: RuntimeControlReceipt
  ) -> RuntimeControl {
    control = RuntimeControl(
      enabled: authorization.enabled,
      revision: authorization.revision,
      updatedAtMs: authorization.updatedAtMs
    )
    return control
  }

  func recoverRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt _: RuntimeControlReceipt
  ) -> RuntimeControl {
    control = RuntimeControl(
      enabled: authorization.enabled,
      revision: authorization.revision,
      updatedAtMs: authorization.updatedAtMs
    )
    return control
  }

  func installBrokerEnrollment(_: Data) {}

  func dashboard() -> DashboardState {
    DashboardState(
      activeCards: [],
      microphone: MicrophoneState(available: false, reason: "Unavailable"),
      runtime: control,
      suggestion: nil
    )
  }

  func account(proof _: BrokerRuntimeState) -> AccountState { .notConnected }
  func beginLogin(proof _: BrokerRuntimeState) -> ChatGptLogin {
    ChatGptLogin(authUrl: "https://example.invalid", loginId: "login-1")
  }
  func awaitLogin(identifier _: String, proof _: BrokerRuntimeState) -> AccountState {
    .notConnected
  }
  func models(proof _: BrokerRuntimeState) -> [GptModel] { [] }
  func propose(prompt _: String, proof _: BrokerRuntimeState) -> OutcomeSuggestion {
    OutcomeSuggestion(
      id: "suggestion-1",
      title: "Plan",
      whyNow: "Now",
      proposedSteps: ["One"],
      sourceRefs: []
    )
  }
}

private actor MockBroker: BrokerRuntimeServing {
  var control: RuntimeControlAuthorization?
  var receipt: RuntimeControlReceipt?
  var appliedValues: [Bool] = []
  var leaseAcquireCount = 0
  var rejectFurtherLeaseAcquisition = false
  var rejectNextProvision = false
  var rejectNextOffBeforePersistence = false
  var loseNextOffResponseAfterPersistence = false
  var delayAndRejectNextOnBeforePersistence = false

  func rejectSubsequentLeaseAcquisition() { rejectFurtherLeaseAcquisition = true }
  func failNextProvision() { rejectNextProvision = true }
  func failNextOffBeforePersistence() { rejectNextOffBeforePersistence = true }
  func loseNextOffResponse() { loseNextOffResponseAfterPersistence = true }
  func delayAndFailNextOn() { delayAndRejectNextOnBeforePersistence = true }

  func provision(coreIdentity _: CoreEffectIdentity) throws -> EnrolledBrokerTrustAnchor {
    if rejectNextProvision {
      rejectNextProvision = false
      throw CoreClientError.contractViolation("Broker provisioning failed.")
    }
    return try testBrokerAnchor()
  }

  func status(challenge: String) -> BrokerRuntimeState? {
    guard let control, let receipt else { return nil }
    let challenged = RuntimeControlReceipt(
      protocolVersion: receipt.protocolVersion,
      authorizationHash: receipt.authorizationHash,
      checkpointNonce: receipt.checkpointNonce,
      requestNonce: challenge,
      brokerKeyId: receipt.brokerKeyId,
      brokerSignatureHex: receipt.brokerSignatureHex
    )
    return BrokerRuntimeState(authorization: control, receipt: challenged)
  }

  func apply(_ authorization: RuntimeControlAuthorization) async throws -> RuntimeControlReceipt {
    if authorization.enabled, delayAndRejectNextOnBeforePersistence {
      delayAndRejectNextOnBeforePersistence = false
      try await Task.sleep(for: .milliseconds(150))
      throw CoreClientError.contractViolation("Delayed broker On rejection.")
    }
    if !authorization.enabled, rejectNextOffBeforePersistence {
      rejectNextOffBeforePersistence = false
      throw CoreClientError.contractViolation("Broker rejected Off before persistence.")
    }
    appliedValues.append(authorization.enabled)
    control = authorization
    let value = RuntimeControlReceipt(
      protocolVersion: 1,
      authorizationHash: String(repeating: "3", count: 64),
      checkpointNonce: String(repeating: "6", count: 64),
      requestNonce: nil,
      brokerKeyId: String(repeating: "4", count: 64),
      brokerSignatureHex: String(repeating: "5", count: 128)
    )
    receipt = value
    if !authorization.enabled, loseNextOffResponseAfterPersistence {
      loseNextOffResponseAfterPersistence = false
      throw CoreClientError.contractViolation("Broker Off response was lost.")
    }
    return value
  }
}

private actor DelayedProofBroker: BrokerRuntimeServing {
  var control: RuntimeControlAuthorization?
  var receipt: RuntimeControlReceipt?
  var shouldDelayNextStatus = false

  func delayNextStatus() { shouldDelayNextStatus = true }

  func provision(coreIdentity _: CoreEffectIdentity) throws -> EnrolledBrokerTrustAnchor {
    try testBrokerAnchor()
  }

  func status(challenge: String) async throws -> BrokerRuntimeState? {
    let capturedControl = control
    let capturedReceipt = receipt
    let shouldDelay = shouldDelayNextStatus
    shouldDelayNextStatus = false
    if shouldDelay { try await Task.sleep(for: .milliseconds(150)) }
    guard let capturedControl, let capturedReceipt else { return nil }
    return BrokerRuntimeState(
      authorization: capturedControl,
      receipt: RuntimeControlReceipt(
        protocolVersion: capturedReceipt.protocolVersion,
        authorizationHash: capturedReceipt.authorizationHash,
        checkpointNonce: capturedReceipt.checkpointNonce,
        requestNonce: challenge,
        brokerKeyId: capturedReceipt.brokerKeyId,
        brokerSignatureHex: capturedReceipt.brokerSignatureHex
      )
    )
  }

  func apply(_ authorization: RuntimeControlAuthorization) -> RuntimeControlReceipt {
    control = authorization
    let value = RuntimeControlReceipt(
      protocolVersion: 1,
      authorizationHash: String(repeating: "3", count: 64),
      checkpointNonce: String(repeating: "6", count: 64),
      requestNonce: nil,
      brokerKeyId: String(repeating: "4", count: 64),
      brokerSignatureHex: String(repeating: "5", count: 128)
    )
    receipt = value
    return value
  }
}

private actor FailingLeaseBroker: BrokerRuntimeServing {
  func provision(coreIdentity _: CoreEffectIdentity) throws -> EnrolledBrokerTrustAnchor {
    try testBrokerAnchor()
  }

  func acquireCoreLease(
    coreIdentity _: CoreEffectIdentity, codexProcessIdentifier _: Int32
  ) throws -> Data {
    throw CoreClientError.contractViolation("Lease acquisition failed.")
  }

  func status(challenge _: String) -> BrokerRuntimeState? { nil }

  func apply(_: RuntimeControlAuthorization) throws -> RuntimeControlReceipt {
    throw CoreClientError.contractViolation("Unexpected runtime apply.")
  }
}

extension MockCore {
  func prepareCodexRuntime() -> Int32 { 99 }
  func initializeCodexRuntime() throws {
    guard leaseInstalled else {
      throw CoreClientError.contractViolation("Codex initialized before lease installation.")
    }
    guard !codexInitialized else {
      throw CoreClientError.contractViolation("Codex initialized more than once.")
    }
    codexInitializeCount += 1
    codexInitialized = true
  }
  func abortCodexCandidate() { codexAbortCount += 1 }
  func installCoreLease(_: Data) { leaseInstalled = true }
}

extension FailClosedOffCore {
  func prepareCodexRuntime() -> Int32 { 99 }
  func initializeCodexRuntime() {}
  func abortCodexCandidate() {}
  func installCoreLease(_: Data) {}
}

extension DelayedSwitchCore {
  func prepareCodexRuntime() -> Int32 { 99 }
  func initializeCodexRuntime() {}
  func abortCodexCandidate() {}
  func installCoreLease(_: Data) {}
}

extension MockBroker {
  func acquireCoreLease(
    coreIdentity _: CoreEffectIdentity, codexProcessIdentifier _: Int32
  ) throws -> Data {
    leaseAcquireCount += 1
    if rejectFurtherLeaseAcquisition {
      throw CoreClientError.contractViolation("The existing Codex process is unavailable.")
    }
    return Data("lease".utf8)
  }
}

extension DelayedProofBroker {
  func acquireCoreLease(
    coreIdentity _: CoreEffectIdentity, codexProcessIdentifier _: Int32
  ) -> Data { Data("lease".utf8) }
}

private func testCoreIdentity(
  coreInstanceNonce: String = String(repeating: "a", count: 64)
) -> CoreEffectIdentity {
  CoreEffectIdentity(
    coreKeyID: String(repeating: "1", count: 64),
    coreVerifyingKeyHex: String(repeating: "2", count: 64),
    coreInstanceNonce: coreInstanceNonce
  )
}

private func testBrokerAnchor() throws -> EnrolledBrokerTrustAnchor {
  let verifyingKey = Data(repeating: 0x55, count: 32)
  let verifyingKeyHex = verifyingKey.map { String(format: "%02x", $0) }.joined()
  let keyID = Data(SHA256.hash(data: verifyingKey)).map { String(format: "%02x", $0) }.joined()
  return try EnrolledBrokerTrustAnchor(
    persistedBrokerKeyID: keyID,
    persistedBrokerVerifyingKeyHex: verifyingKeyHex,
    helperDesignatedRequirementDigest: String(repeating: "6", count: 64),
    installedAtMilliseconds: 1
  )
}

@MainActor
@Test
func globalSwitchPersistsThroughCoreAndRegistersOnlyWhenOn() async {
  let core = MockCore()
  let broker = MockBroker()
  let registrations = LockIsolated(0)
  let model = AppModel(core: core, broker: broker) {
    registrations.withLock { $0 += 1 }
  }
  await model.updateEnabled(true)
  #expect(model.enabled)
  #expect(await core.leaseInstalled)
  #expect(await core.codexInitialized)
  #expect(await core.codexInitializeCount == 1)
  #expect(registrations.value == 1)
  await core.startActiveOperation()
  await model.updateEnabled(false)
  #expect(!model.enabled)
  #expect(await core.codexInitializeCount == 1)
  #expect(await core.offPrepareCount == 1)
  #expect(!(await core.activeOperation))
  #expect(await broker.appliedValues == [true, false])
  #expect(registrations.value == 1)
}

@MainActor
@Test
func initialStateIsUnknownAndOnlyExactCoreDefaultMayDisplayOffWithoutBrokerHistory() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  #expect(model.runtimeDisplayState == .unknown)
  await model.refreshDashboard()
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)

  await core.forceRuntime(false)
  let replayed = AppModel(core: core, broker: MockBroker()) {}
  await replayed.refreshDashboard()
  #expect(!replayed.enabled)
  #expect(replayed.runtimeDisplayState == .unknown)
  #expect(!replayed.modelEntryEnabled)
}

@MainActor
@Test
func deadCodexOrLeaseReacquireFailureCannotBlockProtectedOff() async {
  let core = MockCore()
  let broker = MockBroker()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  #expect(model.enabled)
  await broker.rejectSubsequentLeaseAcquisition()
  await core.startActiveOperation()
  await model.updateEnabled(false)
  #expect(!model.enabled)
  #expect(await broker.leaseAcquireCount == 1)
  #expect(await broker.appliedValues == [true, false])
  #expect(await core.offPrepareCount == 1)
  #expect(!(await core.activeOperation))
}

@MainActor
@Test
func offProvisionFailureCancelsFirstAndNeverDisplaysFalseOff() async {
  let core = MockCore()
  let broker = MockBroker()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  await core.rotateCoreInstance()
  await core.startActiveOperation()
  await broker.failNextProvision()
  await model.updateEnabled(false)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .on)
  #expect(!model.modelEntryEnabled)
  #expect(await core.offPrepareCount == 1)
  #expect(!(await core.activeOperation))
  #expect(await broker.appliedValues == [true])
  model.prompt = "must remain blocked"
  await model.submitPrompt()
  #expect(await core.proposalCount == 0)
}

@MainActor
@Test
func offPrepareFailureKeepsAuthoritativeOnVisibleAndBlocksNewModelEntry() async {
  let core = MockCore()
  let broker = MockBroker()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  await core.startActiveOperation()
  await core.failNextOffPreparation()
  await model.updateEnabled(false)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .on)
  #expect(!model.modelEntryEnabled)
  #expect(await core.activeOperation)
  #expect(await broker.appliedValues == [true])
}

@MainActor
@Test
func brokerOffRejectionShowsUnknownNotOffAfterCoreCancellation() async {
  let core = MockCore()
  let broker = MockBroker()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  await core.startActiveOperation()
  await broker.failNextOffBeforePersistence()
  await model.updateEnabled(false)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(!model.modelEntryEnabled)
  #expect(!(await core.activeOperation))
  #expect(await broker.appliedValues == [true])
}

@MainActor
@Test
func refreshPreservesExplicitOffIntentAfterBrokerRejection() async {
  let core = MockCore()
  let broker = MockBroker()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  await broker.failNextOffBeforePersistence()
  await model.updateEnabled(false)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(!model.modelEntryEnabled)

  await model.refreshDashboard()
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .turningOff)
  #expect(!model.modelEntryEnabled)
  #expect(await core.codexInitializeCount == 1)
  #expect(await broker.appliedValues == [true])
}

@MainActor
@Test
func staleAwaitFailureContinuesUntilTheLatestToggleIntentConverges() async {
  let core = MockCore()
  let broker = MockBroker()
  let model = AppModel(core: core, broker: broker) {}
  await broker.delayAndFailNextOn()
  let on = Task { await model.updateEnabled(true) }
  try? await Task.sleep(for: .milliseconds(20))
  let off = Task { await model.updateEnabled(false) }
  await on.value
  await off.value
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(!model.modelEntryEnabled)
  #expect(await broker.appliedValues == [false])
}

@MainActor
@Test
func lostBrokerOffResponseNeedsFreshStatusProofBeforeDisplayingOff() async {
  let core = MockCore()
  let broker = MockBroker()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  await core.startActiveOperation()
  await broker.loseNextOffResponse()
  await model.updateEnabled(false)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(!model.modelEntryEnabled)
  #expect(!(await core.activeOperation))
  #expect(await broker.appliedValues == [true, false])
  await model.refreshDashboard()
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
}

@MainActor
@Test
func dashboardFailureShowsUnknownWithoutInventingOff() async {
  let core = MockCore(dashboardFails: true)
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  #expect(model.enabled)
  await model.refreshDashboard()
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(!model.modelEntryEnabled)
}

@MainActor
@Test
func missingProtectedStateCannotTurnACoreOnSnapshotIntoDisplayedOff() async {
  let core = MockCore()
  await core.forceRuntime(true)
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(!model.modelEntryEnabled)
}

@MainActor
@Test
func leaseAcquireFailureAbortsTheUninitializedCodexCandidate() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: FailingLeaseBroker()) {}
  await model.updateEnabled(true)
  #expect(!model.enabled)
  #expect(await core.codexAbortCount == 1)
  #expect(!(await core.codexInitialized))
}

@Test
func coreClientContainsNoNumericProcessSignalAuthority() throws {
  let source = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/OpenOpenAppSupport/CoreProcessClient.swift")
  let text = try String(contentsOf: source, encoding: .utf8)
  #expect(!text.contains("Darwin.kill("))
  #expect(!text.contains("Darwin.getpgid("))
  #expect(!text.contains("child.terminate()"))
}

@MainActor
@Test
func loginItemFailureDoesNotMisreportAnAcceptedOnState() async {
  let model = AppModel(core: MockCore(), broker: MockBroker()) {
    throw CoreClientError.contractViolation("Login item registration failed.")
  }
  await model.updateEnabled(true)
  #expect(model.enabled)
  #expect(model.errorMessage == "Login item registration failed.")
}

@MainActor
@Test
func rapidOnThenOffIsSerializedAndPreservesLastIntent() async {
  let core = DelayedSwitchCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let on = Task { await model.updateEnabled(true) }
  try? await Task.sleep(for: .milliseconds(10))
  let off = Task { await model.updateEnabled(false) }
  await on.value
  await off.value
  #expect(!model.enabled)
  #expect(await core.writes == [true, false])
}

@MainActor
@Test
func brokerAcceptedOffNeverRevertsUIOnWhenCoreCommitAndRecoveryFail() async {
  let broker = MockBroker()
  let core = FailClosedOffCore()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  #expect(model.enabled)
  await model.updateEnabled(false)
  #expect(!model.enabled)
  #expect(
    await broker.status(challenge: String(repeating: "a", count: 64))?.authorization.enabled
      == false)
  #expect(model.errorMessage == "Core recovery is temporarily unavailable.")
  await core.allowOffRecovery()
  await model.updateEnabled(true)
  #expect(model.enabled)
  #expect(model.errorMessage == nil)
}

@MainActor
@Test
func delayedDashboardRefreshCannotOverwriteNewerToggleGeneration() async {
  let model = AppModel(
    core: MockCore(dashboardDelay: .milliseconds(100)),
    broker: MockBroker()
  ) {}
  let refresh = Task { await model.refreshDashboard() }
  try? await Task.sleep(for: .milliseconds(10))
  await model.updateEnabled(true)
  await refresh.value
  #expect(model.enabled)
}

@MainActor
@Test
func staleDashboardFailureCannotOverwriteANewerSuccessfulToggle() async {
  let model = AppModel(
    core: MockCore(dashboardDelay: .milliseconds(100), dashboardFails: true),
    broker: MockBroker()
  ) {}
  let refresh = Task { await model.refreshDashboard() }
  try? await Task.sleep(for: .milliseconds(10))
  await model.updateEnabled(true)
  await refresh.value
  #expect(model.enabled)
  #expect(model.errorMessage == nil)
}

@MainActor
@Test
func accountAndModelsConsumeDistinctFreshRuntimeChallenges() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  let before = await core.challengesIssued
  await model.refreshAccountAndModels()
  let nonces = await core.proofNonces
  #expect(await core.challengesIssued == before + 2)
  #expect(nonces.count == 2)
  #expect(Set(nonces).count == 2)
  #expect(await core.codexInitializeCount == 1)
}

@MainActor
@Test
func delayedOnProofCannotCrossANewerOffGenerationOrReachTheModel() async {
  let core = MockCore()
  let broker = DelayedProofBroker()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  model.prompt = "Plan safely"
  await broker.delayNextStatus()
  let submission = Task { await model.submitPrompt() }
  try? await Task.sleep(for: .milliseconds(20))
  await model.updateEnabled(false)
  await submission.value
  #expect(!model.enabled)
  #expect(model.errorMessage == nil)
  #expect(await core.proposalCount == 0)
  let finalStatus = try? await broker.status(challenge: String(repeating: "a", count: 64))
  #expect(finalStatus?.authorization.enabled == false)
}

@MainActor
@Test
func mismatchedRecoveredTimestampCannotAuthorizeModelEntry() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await core.returnMismatchedRecoveryTimestamp()
  model.prompt = "must stay local"
  await model.submitPrompt()
  #expect(await core.proposalCount == 0)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(!model.modelEntryEnabled)
}

@Test
func dashboardRejectsMoreThanThreeActiveCards() {
  let cards = (0..<4).map {
    ActiveOutcomeCard(id: "card-\($0)", title: "Card \($0)", state: "working")
  }
  let dashboard = DashboardState(
    activeCards: cards,
    microphone: MicrophoneState(available: false, reason: "Unavailable"),
    runtime: RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0),
    suggestion: nil
  )
  #expect(throws: CoreClientError.self) {
    try dashboard.validated()
  }
}

@Test
func executableResolverRejectsASymlinkedCore() throws {
  let root = try TemporaryDirectory()
  let app = root.url.appendingPathComponent("OpenOpen.app", isDirectory: true)
  let macos = app.appendingPathComponent("Contents/MacOS", isDirectory: true)
  try FileManager.default.createDirectory(at: macos, withIntermediateDirectories: true)
  let target = root.url.appendingPathComponent("real-core")
  #expect(FileManager.default.createFile(atPath: target.path, contents: Data()))
  try FileManager.default.createSymbolicLink(
    at: macos.appendingPathComponent("OpenOpenCore"),
    withDestinationURL: target
  )
  #expect(throws: CoreClientError.self) {
    try CoreExecutableResolver.resolve(bundleURL: app)
  }
}

@Test
func unsignedRegularCoreIsRejectedBeforeTheMasterKeyIsLoaded() async throws {
  let root = try TemporaryDirectory()
  let app = root.url.appendingPathComponent("OpenOpen.app", isDirectory: true)
  let macos = app.appendingPathComponent("Contents/MacOS", isDirectory: true)
  try FileManager.default.createDirectory(at: macos, withIntermediateDirectories: true)
  let executable = macos.appendingPathComponent("OpenOpenCore")
  try Data("#!/bin/sh\nexit 0\n".utf8).write(to: executable)
  try FileManager.default.setAttributes(
    [.posixPermissions: 0o700],
    ofItemAtPath: executable.path
  )
  let masterLoads = LockIsolated(0)
  let client = CoreProcessClient(
    executableResolver: { try CoreExecutableResolver.resolve(bundleURL: app) },
    staticCodeValidator: {
      try StaticCodeSigningValidator.validate(
        executableURL: $0,
        expectedSigningIdentifier: EffectBrokerConstants.coreSigningIdentifier,
        teamIdentifier: "A1B2C3D4E5"
      )
    },
    runningCodeValidator: { _ in },
    masterKeyLoader: {
      masterLoads.withLock { $0 += 1 }
      return Data(repeating: 7, count: 32)
    }
  )
  do {
    _ = try await client.runtime()
    Issue.record("an unsigned regular Core replacement must fail closed")
  } catch {}
  #expect(masterLoads.value == 0)
}

@Test
func runningCoreAuthenticationFailurePrecedesTheMasterKeyLoad() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("fake-core")
  try Data("#!/bin/sh\n/bin/cat >/dev/null\n".utf8).write(to: executable)
  try FileManager.default.setAttributes(
    [.posixPermissions: 0o700],
    ofItemAtPath: executable.path
  )
  let masterLoads = LockIsolated(0)
  let client = CoreProcessClient(
    executableResolver: { executable },
    staticCodeValidator: { _ in },
    runningCodeValidator: { _ in throw CodeSigningIdentityError.invalidIdentity },
    masterKeyLoader: {
      masterLoads.withLock { $0 += 1 }
      return Data(repeating: 7, count: 32)
    }
  )
  do {
    _ = try await client.runtime()
    Issue.record("running Core authentication must fail before bootstrap")
  } catch {}
  #expect(masterLoads.value == 0)
  client.shutdown()
}

@Test
func responseIdentifiersRejectFloatingBooleanZeroAndNegativeJSONNumbers() throws {
  for json in ["1.0", "1e0", "true", "0", "-1"] {
    let value = try JSONSerialization.jsonObject(
      with: Data(json.utf8), options: [.fragmentsAllowed])
    #expect(CoreProcessClient.exactResponseIdentifier(value as! NSNumber) == nil)
  }
  let integer =
    try JSONSerialization.jsonObject(
      with: Data("7".utf8),
      options: [.fragmentsAllowed]
    ) as! NSNumber
  #expect(CoreProcessClient.exactResponseIdentifier(integer) == 7)
}

@Test
func crashedCoreCanRestartWithoutOldGenerationCallbacksFailingTheReplacement() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("fake-core")
  let script = """
    #!/bin/sh
    /bin/dd bs=55 count=1 of=/dev/null 2>/dev/null
    IFS= read -r request
    count_file="$0.count"
    count=0
    if [ -f "$count_file" ]; then count=$(/bin/cat "$count_file"); fi
    count=$((count + 1))
    printf '%s' "$count" > "$count_file"
    if [ "$count" -eq 1 ]; then
      exit 0
    fi
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"enabled":false,"revision":0,"updatedAtMs":0}}'
    """
  try Data(script.utf8).write(to: executable)
  try FileManager.default.setAttributes(
    [.posixPermissions: 0o700],
    ofItemAtPath: executable.path
  )
  let client = CoreProcessClient(
    executableResolver: { executable },
    staticCodeValidator: { _ in },
    runningCodeValidator: { _ in },
    masterKeyLoader: { Data(repeating: 7, count: 32) }
  )
  do {
    _ = try await client.runtime()
    Issue.record("first fake Core invocation should terminate")
  } catch {}
  let replacement = try await client.runtime()
  #expect(replacement == RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0))
  client.shutdown()
}

private final class LockIsolated<Value>: @unchecked Sendable {
  private let lock = NSLock()
  private var stored: Value

  init(_ value: Value) {
    stored = value
  }

  var value: Value { lock.withLock { stored } }

  func withLock(_ body: (inout Value) -> Void) {
    lock.withLock { body(&stored) }
  }
}

private final class TemporaryDirectory {
  let url: URL

  init() throws {
    url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
  }

  deinit {
    try? FileManager.default.removeItem(at: url)
  }
}
