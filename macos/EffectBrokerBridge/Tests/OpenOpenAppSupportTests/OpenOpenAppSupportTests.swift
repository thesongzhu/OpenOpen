import CryptoKit
import EffectBrokerBridge
import Foundation
import Testing

@testable import OpenOpenAppSupport

private func testConfirmedMission(
  missionId: String = "mission-1",
  title: String = "Plan the day",
  workItems: [MissionWorkItem] = [
    MissionWorkItem(id: "work-1", title: "Pick one priority")
  ],
  validAuthorization: Bool = true,
  writeDisposition: ReminderWriteDisposition = .createOnce,
  reminderDispatch: [ConfirmedReminderDispatch] = [],
  reminderLinks: [ReminderLink] = []
) -> ConfirmedMission {
  let target = ReminderTarget(
    sourceIdentifier: "source-1", calendarIdentifier: "calendar-1"
  )
  let payloadSha256 = ReminderWriteAuthorization.payloadSha256(
    missionId: missionId, target: target, workItems: workItems
  )
  return ConfirmedMission(
    missionId: missionId,
    title: title,
    workItems: workItems,
    reminderAuthorization: ReminderWriteAuthorization(
      missionId: missionId,
      listId: ReminderWriteAuthorization.logicalListId,
      payloadSha256: validAuthorization ? payloadSha256 : String(repeating: "0", count: 64),
      approvalId: "approval-reminders-1",
      approvalDigest: String(repeating: "a", count: 64),
      target: target,
      writeDisposition: writeDisposition
    ),
    reminderDispatch: reminderDispatch,
    reminderLinks: reminderLinks
  )
}

private struct TestChannelSend: Equatable, Sendable {
  let missionId: String
  let kind: ChannelMessageKind
  let content: String
  let approvedAtMs: Int64
}

private actor MockCore: CoreServing {
  var control = RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0)
  let dashboardDelay: Duration
  let dashboardFails: Bool
  var challengesIssued = 0
  var proofNonces: [String] = []
  var proposalCount = 0
  var confirmationCount = 0
  var dispatchBeginCount = 0
  var reminderCompletionPayloads: [[ReminderCompletionInput]] = []
  var receiptReturnApprovals: [Int64?] = []
  var leaseInstalled = false
  var codexInitialized = false
  var codexInitializeCount = 0
  var codexAbortCount = 0
  var offPrepareCount = 0
  var activeOperation = false
  var coreInstanceNonce = String(repeating: "a", count: 64)
  var rejectNextOffPreparation = false
  var mismatchRecoveryTimestamp = false
  var invalidReminderAuthorization = false
  var dashboardConfirmedMission: ConfirmedMission?
  var dashboardReceipt: MissionReceipt?
  var dashboardChannelOrigin: ChannelMissionOrigin?
  var dashboardNeedsYou: MissionNeedsYou?
  var dispatchedMissions: [String: ConfirmedMission] = [:]
  var loseNextDispatchResponse = false
  var channelPairings: [ChannelKind: ChannelPairing] = [:]
  var pairChannelCount = 0
  var discordSetupStartCount = 0
  var discordSetupConfirmCount = 0
  var discordStartTokens: [String] = []
  var stoppedChannels: [ChannelKind] = []
  var rejectNextIMessageActivation = false
  var rejectNextIMessagePrepareAfterCommit = false
  var iMessagePrepareCount = 0
  var iMessageDiscoveryPrepareCount = 0
  var iMessageChatsListCount = 0
  var iMessageDiscoveryPrepared = false
  var iMessageStartCount = 0
  var channelPollCount = 0
  var queuedChannelSuggestion: OutcomeSuggestion?
  var channelSends: [TestChannelSend] = []

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
      codexInitialized = false
      leaseInstalled = false
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
  func returnInvalidReminderAuthorization() { invalidReminderAuthorization = true }
  func restoreFromDashboard(
    mission: ConfirmedMission?, receipt: MissionReceipt?,
    channelOrigin: ChannelMissionOrigin? = nil,
    needsYou: MissionNeedsYou? = nil
  ) {
    dashboardConfirmedMission = mission
    dashboardReceipt = receipt
    dashboardChannelOrigin = channelOrigin
    dashboardNeedsYou = needsYou
    if let mission, !mission.reminderDispatch.isEmpty {
      dispatchedMissions[mission.missionId] = mission
    }
  }

  func loseNextReminderDispatchResponse() { loseNextDispatchResponse = true }

  func setChannelPairing(_ pairing: ChannelPairing) {
    channelPairings[pairing.channel] = pairing
  }

  func pairedDiscordApplicationId() -> String? {
    channelPairings[.discord]?.discord?.applicationId
  }

  func queueChannelSuggestion(_ suggestion: OutcomeSuggestion) {
    queuedChannelSuggestion = suggestion
  }

  func dashboard() async throws -> DashboardState {
    if dashboardDelay > .zero { try await Task.sleep(for: dashboardDelay) }
    if dashboardFails {
      throw CoreClientError.contractViolation("Delayed dashboard failure.")
    }
    return DashboardState(
      activeCards: [],
      channelOrigin: dashboardChannelOrigin,
      microphone: MicrophoneState(
        available: false,
        reason: "Microphone unavailable until Voice setup"
      ),
      runtime: control,
      suggestion: nil,
      confirmedMission: dashboardConfirmedMission,
      needsYou: dashboardNeedsYou,
      receipt: dashboardReceipt
    )
  }

  func pairChannel(_ pairing: ChannelPairing, proof _: BrokerRuntimeState) {
    pairChannelCount += 1
    channelPairings[pairing.channel] = pairing
  }

  func channelPairing(_ channel: ChannelKind) -> ChannelPairing? {
    channelPairings[channel]
  }

  func startDiscordSetup(
    token _: String, proof _: BrokerRuntimeState
  ) -> DiscordSetupStart {
    discordSetupStartCount += 1
    return DiscordSetupStart(
      identity: DiscordBotIdentity(botUserId: 3003, applicationId: 4004, botName: "OpenOpen"),
      installUrl:
        "https://discord.com/api/oauth2/authorize?client_id=4004&scope=bot&permissions=101376",
      pairingCode: String(repeating: "a", count: 32),
      status: "connecting"
    )
  }

  func pollDiscordSetup(proof _: BrokerRuntimeState) -> DiscordSetupPollResponse {
    DiscordSetupPollResponse(
      status: "connected",
      candidate: DiscordPairingCandidate(
        candidateId: "discord-pair-" + String(repeating: "b", count: 64),
        sourceMessageId: "5005",
        guildId: "6006",
        guildName: "OpenOpen Test",
        channelId: "2002",
        channelName: "outcomes",
        ownerUserId: "1001",
        ownerName: "Owner",
        botUserId: "3003",
        applicationId: "4004",
        receivedAtMs: 1,
        messageContentIntentReady: true,
        permissions: DiscordPermissionProbe(
          viewChannel: "passed",
          sendMessages: "passed",
          readMessageHistory: "passed",
          attachFiles: "passed",
          historyReadback: "passed",
          effectivePermissionBits: 101_376
        )
      )
    )
  }

  func confirmDiscordSetup(
    candidateId: String, confirmedAtMs: Int64, proof _: BrokerRuntimeState
  ) {
    discordSetupConfirmCount += 1
    channelPairings[.discord] = ChannelPairing(
      channel: .discord,
      ownerSenderId: "1001",
      conversationId: "2002",
      discord: DiscordPairingMetadata(
        guildId: "6006",
        botUserId: "3003",
        applicationId: "4004",
        setupSourceMessageId: "5005",
        setupCandidateId: candidateId
      ),
      pairedAtMs: confirmedAtMs
    )
  }

  func startDiscord(
    token: String, proof _: BrokerRuntimeState
  ) -> ChannelStatusResponse {
    discordStartTokens.append(token)
    return ChannelStatusResponse(status: "connected")
  }

  func failNextIMessageActivation() { rejectNextIMessageActivation = true }

  func loseNextIMessagePrepareResponse() { rejectNextIMessagePrepareAfterCommit = true }

  func prepareIMessage(proof _: BrokerRuntimeState) throws {
    iMessagePrepareCount += 1
    if rejectNextIMessagePrepareAfterCommit {
      rejectNextIMessagePrepareAfterCommit = false
      throw CoreClientError.contractViolation("iMessage prepare response was lost.")
    }
  }

  func prepareIMessageChatDiscovery(proof _: BrokerRuntimeState) {
    iMessageDiscoveryPrepareCount += 1
    iMessageDiscoveryPrepared = true
  }

  func listPreparedIMessageChats(proof _: BrokerRuntimeState) throws -> [IMessageChat] {
    guard iMessageDiscoveryPrepared else {
      throw CoreClientError.contractViolation("iMessage discovery was not prepared.")
    }
    iMessageChatsListCount += 1
    iMessageDiscoveryPrepared = false
    return [
      IMessageChat(
        chatId: "42",
        name: "Owner",
        service: "iMessage",
        participants: ["owner@example.invalid"]
      ),
      IMessageChat(
        chatId: "84",
        name: "Family",
        service: "iMessage",
        participants: ["owner@example.invalid", "family@example.invalid"]
      ),
    ]
  }

  func activateIMessage(proof _: BrokerRuntimeState) throws -> ChannelStatusResponse {
    if rejectNextIMessageActivation {
      rejectNextIMessageActivation = false
      throw CoreClientError.contractViolation("iMessage activation failed.")
    }
    iMessageStartCount += 1
    return ChannelStatusResponse(status: "connected")
  }

  func channelStatus(_ channel: ChannelKind) -> ChannelStatusResponse {
    ChannelStatusResponse(status: channelPairings[channel] == nil ? "disconnected" : "connected")
  }

  func stopChannel(_ channel: ChannelKind) -> ChannelStatusResponse {
    stoppedChannels.append(channel)
    if channel == .iMessage { iMessageDiscoveryPrepared = false }
    return ChannelStatusResponse(status: "disconnected")
  }

  func pollChannel(
    _: ChannelKind, proof _: BrokerRuntimeState
  ) -> ChannelPollResponse {
    channelPollCount += 1
    defer { queuedChannelSuggestion = nil }
    return ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "ready", suggestion: queuedChannelSuggestion)
  }

  func sendChannelMessage(
    missionId: String,
    kind: ChannelMessageKind,
    content: String,
    approvedAtMs: Int64,
    proof _: BrokerRuntimeState
  ) -> ChannelSendResponse {
    channelSends.append(
      TestChannelSend(
        missionId: missionId,
        kind: kind,
        content: content,
        approvedAtMs: approvedAtMs
      ))
    return ChannelSendResponse(status: "sent", providerMessageId: "provider-message-1")
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
    let isFirst = proposalCount == 1
    return OutcomeSuggestion(
      id: isFirst ? "suggestion-1" : "suggestion-2",
      title: isFirst ? "Plan the day" : "Plan tomorrow",
      whyNow: "It is morning",
      proposedSteps: [isFirst ? "Pick one priority" : "Choose tomorrow's priority"],
      sourceRefs: []
    )
  }

  func confirmSuggestion(
    identifier: String, reminderTarget: ReminderTarget
  ) throws -> ConfirmedMission {
    guard identifier == "suggestion-1" || identifier == "suggestion-2" else {
      throw CoreClientError.contractViolation("Unexpected suggestion identifier.")
    }
    confirmationCount += 1
    guard
      reminderTarget
        == ReminderTarget(sourceIdentifier: "source-1", calendarIdentifier: "calendar-1")
    else {
      throw CoreClientError.contractViolation("Unexpected Reminder target.")
    }
    let isFirst = identifier == "suggestion-1"
    return testConfirmedMission(
      missionId: isFirst ? "mission-1" : "mission-2",
      title: isFirst ? "Plan the day" : "Plan tomorrow",
      workItems: [
        MissionWorkItem(
          id: isFirst ? "work-1" : "work-2",
          title: isFirst ? "Pick one priority" : "Choose tomorrow's priority"
        )
      ],
      validAuthorization: !invalidReminderAuthorization
    )
  }

  func completeReminderMission(
    identifier: String,
    completions: [ReminderCompletionInput],
    receiptReturnApprovedAtMs: Int64?
  ) throws -> MissionReceipt {
    guard identifier == "mission-1" || identifier == "mission-2" else {
      throw CoreClientError.contractViolation("Unexpected Mission identifier.")
    }
    reminderCompletionPayloads.append(completions)
    receiptReturnApprovals.append(receiptReturnApprovedAtMs)
    return MissionReceipt(
      id: identifier == "mission-1" ? "receipt-1" : "receipt-2",
      missionId: identifier,
      summary: "Completed Plan the day",
      actualModel: "gpt-5.6-sol",
      evidenceIds: completions.map { "evidence-\($0.workItemId)" },
      outputHashes: [],
      completedAtMs: 10
    )
  }

  func beginReminderDispatch(identifier: String) throws -> ReminderDispatchStart {
    dispatchBeginCount += 1
    if let mission = dispatchedMissions[identifier] {
      return ReminderDispatchStart(mission: mission, executeNow: false)
    }
    let base: ConfirmedMission
    if let dashboardConfirmedMission, dashboardConfirmedMission.missionId == identifier {
      base = dashboardConfirmedMission
    } else {
      let isFirst = identifier == "mission-1"
      base = testConfirmedMission(
        missionId: identifier,
        title: isFirst ? "Plan the day" : "Plan tomorrow",
        workItems: [
          MissionWorkItem(
            id: isFirst ? "work-1" : "work-2",
            title: isFirst ? "Pick one priority" : "Choose tomorrow's priority"
          )
        ]
      )
    }
    let dispatch = base.workItems.map {
      ConfirmedReminderDispatch(
        workItemId: $0.id,
        token: "dispatch-\($0.id)"
      )
    }
    let mission = ConfirmedMission(
      missionId: base.missionId,
      title: base.title,
      workItems: base.workItems,
      reminderAuthorization: base.recoveryOnly().reminderAuthorization,
      reminderDispatch: dispatch,
      reminderLinks: base.reminderLinks
    )
    dispatchedMissions[identifier] = mission
    dashboardConfirmedMission = mission
    if loseNextDispatchResponse {
      loseNextDispatchResponse = false
      throw CoreClientError.contractViolation("Core dispatch response was lost.")
    }
    return ReminderDispatchStart(mission: mission, executeNow: true)
  }

  func recordReminderMirror(
    identifier: String, links: [ReminderLink]
  ) throws -> ConfirmedMission {
    guard let dispatched = dispatchedMissions[identifier] else {
      throw CoreClientError.contractViolation("Reminder dispatch was not persisted.")
    }
    let mission = ConfirmedMission(
      missionId: dispatched.missionId,
      title: dispatched.title,
      workItems: dispatched.workItems,
      reminderAuthorization: dispatched.reminderAuthorization,
      reminderDispatch: dispatched.reminderDispatch,
      reminderLinks: links
    )
    dispatchedMissions[identifier] = mission
    dashboardConfirmedMission = mission
    return mission
  }
}

private actor FailClosedOffCore: CoreServing {
  var control = RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0)
  var rejectOffRecovery = true
  var coreInstanceNonce = String(repeating: "a", count: 64)
  var brokerEnrollmentInstallCount = 0
  var brokerEnrollmentInstalled = false
  var terminateOnNextOffCommit = false

  func allowOffRecovery() { rejectOffRecovery = false }
  func terminateCoreOnNextOffCommit() { terminateOnNextOffCommit = true }

  func runtime() -> RuntimeControl { control }
  func effectIdentity() -> CoreEffectIdentity {
    testCoreIdentity(coreInstanceNonce: coreInstanceNonce)
  }
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
    if !authorization.enabled, terminateOnNextOffCommit {
      terminateOnNextOffCommit = false
      coreInstanceNonce = String(repeating: "b", count: 64)
      brokerEnrollmentInstalled = false
      throw CoreClientError.processTerminated
    }
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
    guard brokerEnrollmentInstalled else {
      throw CoreClientError.contractViolation("Replacement Core has no broker enrollment.")
    }
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
  func installBrokerEnrollment(_: Data) {
    brokerEnrollmentInstallCount += 1
    brokerEnrollmentInstalled = true
  }
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
  func confirmSuggestion(
    identifier _: String, reminderTarget _: ReminderTarget
  ) throws -> ConfirmedMission {
    throw CoreClientError.contractViolation("Unexpected Mission confirmation.")
  }
  func completeReminderMission(
    identifier _: String,
    completions _: [ReminderCompletionInput],
    receiptReturnApprovedAtMs _: Int64?
  ) throws -> MissionReceipt {
    throw CoreClientError.contractViolation("Unexpected Mission completion.")
  }
  func recordReminderMirror(
    identifier _: String, links _: [ReminderLink]
  ) throws -> ConfirmedMission {
    throw CoreClientError.contractViolation("Unexpected Reminder mirror.")
  }
  func beginReminderDispatch(identifier _: String) throws -> ReminderDispatchStart {
    throw CoreClientError.contractViolation("Unexpected Reminder dispatch.")
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
      ),
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
  func confirmSuggestion(
    identifier _: String, reminderTarget _: ReminderTarget
  ) throws -> ConfirmedMission {
    throw CoreClientError.contractViolation("Unexpected Mission confirmation.")
  }
  func completeReminderMission(
    identifier _: String,
    completions _: [ReminderCompletionInput],
    receiptReturnApprovedAtMs _: Int64?
  ) throws -> MissionReceipt {
    throw CoreClientError.contractViolation("Unexpected Mission completion.")
  }
  func recordReminderMirror(
    identifier _: String, links _: [ReminderLink]
  ) throws -> ConfirmedMission {
    throw CoreClientError.contractViolation("Unexpected Reminder mirror.")
  }
  func beginReminderDispatch(identifier _: String) throws -> ReminderDispatchStart {
    throw CoreClientError.contractViolation("Unexpected Reminder dispatch.")
  }
}

@MainActor
private final class MockReminders: RemindersServing {
  enum Mode {
    case complete
    case partial
    case failBeforeCommit
    case commitThenFailReadback
    case commitThenDelay
  }

  var mode: Mode
  private(set) var executeCount = 0
  private(set) var recoverCount = 0
  private var storedLinks: [String: [ReminderLink]] = [:]

  init(mode: Mode = .complete) {
    self.mode = mode
  }

  func prepareTarget() async throws -> ReminderTarget {
    ReminderTarget(sourceIdentifier: "source-1", calendarIdentifier: "calendar-1")
  }

  func executeInitialMirror(_ start: ReminderDispatchStart) async throws -> [ReminderLink] {
    let mission = start.mission
    executeCount += 1
    if mode == .failBeforeCommit {
      throw CoreClientError.contractViolation("Reminders access was denied.")
    }
    let dispatchByWorkItem = Dictionary(
      uniqueKeysWithValues: mission.reminderDispatch.map { ($0.workItemId, $0.token) }
    )
    let links = mission.workItems.map {
      ReminderLink(
        missionId: mission.missionId,
        workItemId: $0.id,
        sourceIdentifier: mission.reminderAuthorization.target.sourceIdentifier,
        calendarIdentifier: mission.reminderAuthorization.target.calendarIdentifier,
        calendarItemIdentifier: "reminder-\($0.id)",
        dispatchToken: dispatchByWorkItem[$0.id] ?? "",
        title: $0.title
      )
    }
    storedLinks[mission.missionId] = links
    if mode == .commitThenFailReadback {
      throw CoreClientError.contractViolation("Reminders readback failed after commit.")
    }
    if mode == .commitThenDelay {
      try await Task.sleep(for: .milliseconds(150))
    }
    return links
  }

  func recoverMirror(for mission: ConfirmedMission) async throws -> [ReminderLink] {
    recoverCount += 1
    guard let links = storedLinks[mission.missionId] else {
      throw RemindersClientError.incompleteMirror(mission.title)
    }
    return links
  }

  func completedReminders(
    for links: [ReminderLink]
  ) async throws -> [ReminderCompletionInput] {
    let completed = mode == .partial ? links.dropLast() : links[...]
    return completed.map {
      ReminderCompletionInput(
        workItemId: $0.workItemId,
        sourceId: $0.calendarItemIdentifier,
        completedAtMs: 9
      )
    }
  }
}

@Test
func connectedAccountDecodesTheRustCamelCaseContract() throws {
  let payload = Data(
    #"{"email":"owner@example.invalid","planType":"pro","state":"chatGpt"}"#.utf8
  )

  let account = try JSONDecoder().decode(AccountState.self, from: payload)

  #expect(account == .chatGpt(email: "owner@example.invalid", planType: "pro"))
}

@Test
func heroAInitialEventKitAuthorityIsConsumedBeforeAnyExternalBoundary() throws {
  let mission = testConfirmedMission(
    writeDisposition: .recoverOnly,
    reminderDispatch: [
      ConfirmedReminderDispatch(workItemId: "work-1", token: "dispatch-work-1")
    ]
  )
  let start = ReminderDispatchStart(mission: mission, executeNow: true)
  var claims = ReminderExecutionClaims()

  try claims.consume(start)
  #expect(throws: RemindersClientError.incompleteMirror("Plan the day")) {
    try claims.consume(start)
  }
  var noAuthority = ReminderExecutionClaims()
  #expect(
    throws: RemindersClientError.invalidMission(
      "Core did not issue initial execution authority"
    )
  ) {
    try noAuthority.consume(
      ReminderDispatchStart(mission: mission, executeNow: false)
    )
  }
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

private final class MockDiscordTokenStore: DiscordTokenStoring, @unchecked Sendable {
  private let token = LockIsolated<String?>(nil)

  func save(_ value: String) throws {
    guard !value.isEmpty, value == value.trimmingCharacters(in: .whitespacesAndNewlines) else {
      throw CoreClientError.contractViolation("Discord rejected an invalid bot token.")
    }
    token.withLock { $0 = value }
  }

  func load() throws -> String? { token.value }

  func delete() throws { token.withLock { $0 = nil } }
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
func protectedOffReenrollsAReplacementCoreBeforeCheckpointRecovery() async {
  let broker = MockBroker()
  let core = FailClosedOffCore()
  await core.allowOffRecovery()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  #expect(model.enabled)
  #expect(await core.brokerEnrollmentInstallCount == 1)

  await core.terminateCoreOnNextOffCommit()
  await model.updateEnabled(false)
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.errorMessage == nil)
  #expect(await core.brokerEnrollmentInstallCount == 2)
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
func discordConnectionUsesKeychainTokenAndAnExistingExactPairing() async {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let pairing = ChannelPairing(
    channel: .discord,
    ownerSenderId: "1001",
    conversationId: "2002",
    discord: DiscordPairingMetadata(
      guildId: "6006",
      botUserId: "3003",
      applicationId: "4004",
      setupSourceMessageId: "5005",
      setupCandidateId: "discord-pair-" + String(repeating: "b", count: 64)
    ),
    pairedAtMs: 1
  )
  await core.setChannelPairing(pairing)
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore
  ) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()

  #expect(model.discordTokenDraft.isEmpty)
  #expect(model.discordStatus == "connected")
  #expect(model.errorMessage == nil)
  #expect(await core.pairChannelCount == 0)
  #expect(await core.discordStartTokens == ["test-only-discord-token"])
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordWizardInfersIdsProbesPermissionsAndRequiresCandidateConfirmation() async {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore
  ) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()
  #expect(model.discordSetup?.identity.botUserId == 3003)
  #expect(model.discordSetup?.installUrl.contains("permissions=101376") == true)
  #expect(model.discordPairingCandidate == nil)
  #expect(await core.pairedDiscordApplicationId() == nil)
  #expect(await core.stoppedChannels == [.discord])

  await model.checkDiscordPairingMessage()
  #expect(model.discordPairingCandidate?.ownerUserId == "1001")
  #expect(model.discordPairingCandidate?.channelId == "2002")
  #expect(await core.pairedDiscordApplicationId() == nil)

  await model.confirmDiscordPairing()

  #expect(model.discordStatus == "connected")
  #expect(model.errorMessage == nil)
  #expect(await core.discordSetupStartCount == 1)
  #expect(await core.discordSetupConfirmCount == 1)
  #expect(await core.pairedDiscordApplicationId() == "4004")
  #expect(await core.discordStartTokens == ["test-only-discord-token"])
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordWizardRestartStopsThePriorSetupBeforeStartingAnother() async {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore
  ) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()
  await model.connectDiscord()

  #expect(model.discordStatus == "connecting")
  #expect(model.errorMessage == nil)
  #expect(await core.discordSetupStartCount == 2)
  #expect(await core.stoppedChannels == [.discord, .discord])
  await model.updateEnabled(false)
}

@MainActor
@Test
func iMessageDiscoveryListsApprovedChatsWithoutManualDatabaseIds() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)

  await model.refreshIMessageChats()

  #expect(model.errorMessage == nil)
  #expect(model.iMessageChats.map(\.chatId) == ["42", "84"])
  #expect(model.iMessageChatId.isEmpty)
  #expect(model.iMessageOwnerOptions.isEmpty)
  #expect(await core.iMessageDiscoveryPrepareCount == 1)
  #expect(await core.iMessageChatsListCount == 1)
  #expect(await core.stoppedChannels == [.iMessage])

  model.selectIMessageChat("84")
  #expect(model.iMessageOwnerOptions == ["owner@example.invalid", "family@example.invalid"])
  model.iMessageOwnerSender = "family@example.invalid"
  #expect(model.iMessageOwnerSender == "family@example.invalid")
  await model.updateEnabled(false)
}

@MainActor
@Test
func iMessageConnectionPairsPollsOneSuggestionAndOffStopsPolling() async {
  let core = MockCore()
  await core.queueChannelSuggestion(
    OutcomeSuggestion(
      id: "channel-suggestion-1",
      title: "Prepare the update",
      whyNow: "The owner explicitly asked in Messages",
      proposedSteps: ["Draft the concise update"],
      sourceRefs: ["imessage:message-1"]
    ))
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.iMessageChatId = "42"
  model.iMessageOwnerSender = "owner@example.invalid"

  await model.connectIMessage()
  for _ in 0..<50 where model.suggestion == nil {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.iMessageStatus == "connected")
  #expect(model.suggestion?.id == "channel-suggestion-1")
  #expect(await core.pairChannelCount == 1)
  #expect(await core.iMessageStartCount == 1)
  #expect(await core.channelPollCount >= 1)
  await model.updateEnabled(false)
  let pollsAfterOff = await core.channelPollCount
  try? await Task.sleep(for: .milliseconds(1_100))
  #expect(await core.channelPollCount == pollsAfterOff)
  #expect(model.iMessageStatus == "disconnected")
}

@MainActor
@Test
func iMessageActivationFailureStopsPreparedChildAndRetryConnects() async {
  let core = MockCore()
  await core.failNextIMessageActivation()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.iMessageChatId = "42"
  model.iMessageOwnerSender = "owner@example.invalid"

  await model.connectIMessage()
  #expect(model.iMessageStatus == "faulted")
  #expect(await core.stoppedChannels == [.iMessage, .iMessage])

  await model.connectIMessage()
  #expect(model.iMessageStatus == "connected")
  #expect(model.errorMessage == nil)
  #expect(await core.stoppedChannels == [.iMessage, .iMessage, .iMessage])
  #expect(await core.iMessagePrepareCount == 2)
  #expect(await core.iMessageStartCount == 1)
  await model.updateEnabled(false)
}

@MainActor
@Test
func iMessagePrepareResponseLossStopsPreparedChildAndRetryConnects() async {
  let core = MockCore()
  await core.loseNextIMessagePrepareResponse()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.iMessageChatId = "42"
  model.iMessageOwnerSender = "owner@example.invalid"

  await model.connectIMessage()
  #expect(model.iMessageStatus == "faulted")
  #expect(await core.stoppedChannels == [.iMessage, .iMessage])

  await model.connectIMessage()
  #expect(model.iMessageStatus == "connected")
  #expect(model.errorMessage == nil)
  #expect(await core.stoppedChannels == [.iMessage, .iMessage, .iMessage])
  #expect(await core.iMessagePrepareCount == 2)
  #expect(await core.iMessageStartCount == 1)
  await model.updateEnabled(false)
}

@MainActor
@Test
func channelProgressSendBindsTheRestoredMissionAndFreshApprovalTime() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await core.restoreFromDashboard(
    mission: testConfirmedMission(),
    receipt: nil,
    channelOrigin: ChannelMissionOrigin(
      missionId: "mission-1",
      channel: .discord,
      conversationId: "2002",
      ownerSenderId: "1001",
      sourceMessageId: "source-message-1",
      boundAtMs: 1
    )
  )
  await model.refreshDashboard()
  model.channelMessageDraft = "Working on it — the bounded Mission is active."
  let earliestApproval = Int64((Date().timeIntervalSince1970 * 1_000).rounded(.down))

  await model.sendChannelProgress()

  let latestApproval = Int64((Date().timeIntervalSince1970 * 1_000).rounded(.down))
  let sends = await core.channelSends
  #expect(sends.count == 1)
  #expect(sends.first?.missionId == "mission-1")
  #expect(sends.first?.kind == .progress)
  #expect(sends.first?.content == "Working on it — the bounded Mission is active.")
  #expect((sends.first?.approvedAtMs ?? 0) >= earliestApproval)
  #expect((sends.first?.approvedAtMs ?? 0) <= latestApproval)
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func channelNeedYouSendUsesOnlyTheExactRestoredPrompt() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelOrigin: ChannelMissionOrigin(
      missionId: "mission-1",
      channel: .discord,
      conversationId: "2002",
      ownerSenderId: "1001",
      sourceMessageId: "source-message-1",
      boundAtMs: 1
    ),
    needsYou: MissionNeedsYou(
      missionId: "mission-1",
      title: "Plan the day",
      prompt: "Choose the one approved destination.",
      createdAtMs: 2
    )
  )
  await model.refreshDashboard()

  await model.sendChannelNeedYou()

  let sends = await core.channelSends
  #expect(sends.count == 1)
  #expect(sends.first?.kind == .needYou)
  #expect(sends.first?.content == "Need you: Choose the one approved destination.")
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func channelMissionCompletionAuthorizesAndReturnsTheExactEvidenceReceipt() async {
  let core = MockCore()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelOrigin: ChannelMissionOrigin(
      missionId: "mission-1",
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid",
      sourceMessageId: "source-message-1",
      boundAtMs: 1
    )
  )
  await model.updateEnabled(true)
  model.prompt = "Help me plan today"
  await model.submitPrompt()
  await model.confirmSuggestion()
  await model.checkMissionProgress()

  let approvals = await core.receiptReturnApprovals
  let sends = await core.channelSends
  #expect(approvals.count == 1)
  #expect(approvals[0] != nil)
  #expect(sends.count == 1)
  #expect(sends.first?.kind == .receipt)
  #expect(
    sends.first?.content
      == "Done: Completed Plan the day\nEvidence: 1 verified completion\nModel: gpt-5.6-sol"
  )
  #expect(model.receipt?.missionId == "mission-1")
  #expect(model.channelOrigin?.missionId == "mission-1")
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
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

@MainActor
@Test
func heroAConfirmCreatesRemindersAndCompletedReadbackProducesReceipt() async {
  let core = MockCore()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  model.prompt = "Help me plan today"
  await model.submitPrompt()
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 1)
  #expect(model.suggestion == nil)
  #expect(model.confirmedMission?.missionId == "mission-1")
  #expect(model.reminderLinks.map(\.calendarItemIdentifier) == ["reminder-work-1"])
  #expect(model.activeCards.count == 1)

  await model.checkMissionProgress()
  #expect(model.receipt?.missionId == "mission-1")
  #expect(model.receipt?.actualModel == "gpt-5.6-sol")
  #expect(model.activeCards.isEmpty)
  #expect(model.confirmedMission == nil)
  #expect(model.reminderLinks.isEmpty)
  let payloads = await core.reminderCompletionPayloads
  #expect(payloads.count == 1)
  #expect(
    payloads[0] == [
      ReminderCompletionInput(
        workItemId: "work-1",
        sourceId: "reminder-work-1",
        completedAtMs: 9
      )
    ])
}

@MainActor
@Test
func heroASecondOutcomeCannotReuseTheCompletedMission() async {
  let core = MockCore()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)

  model.prompt = "Help me plan today"
  await model.submitPrompt()
  await model.confirmSuggestion()
  await model.checkMissionProgress()

  model.prompt = "Help me plan tomorrow"
  await model.submitPrompt()
  #expect(model.suggestion?.id == "suggestion-2")
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 2)
  #expect(model.confirmedMission?.missionId == "mission-2")
  #expect(model.reminderLinks.map(\.workItemId) == ["work-2"])
  #expect(model.receipt == nil)
}

@MainActor
@Test
func heroAInvalidCoreReminderAuthorizationCannotReachTheExternalWriter() async {
  let core = MockCore()
  await core.returnInvalidReminderAuthorization()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  model.prompt = "Help me plan today"
  await model.submitPrompt()
  await model.confirmSuggestion()

  #expect(reminders.executeCount == 0)
  #expect(model.reminderLinks.isEmpty)
  #expect(model.errorMessage?.contains("exact Reminder write") == true)
}

@Test
func heroAReminderAuthorizationPayloadHasTheRustContractVector() {
  #expect(
    ReminderWriteAuthorization.payloadSha256(
      missionId: "mission-1",
      target: ReminderTarget(sourceIdentifier: "source-1", calendarIdentifier: "calendar-1"),
      workItems: [MissionWorkItem(id: "work-1", title: "Pick one priority")]
    ) == "188605fc48e5a3bc42efee3820582cb016a84685869bfbb6688daf79b055fab0"
  )
}

@Test
func heroARenamedOwnedCalendarBeatsDefaultAccountDrift() throws {
  let target = try selectReminderTarget(
    candidates: [
      ReminderCalendarCandidate(
        sourceIdentifier: "original-source",
        calendarIdentifier: "original-calendar",
        title: "Renamed by the owner",
        containsOpenOpenMarker: true
      ),
      ReminderCalendarCandidate(
        sourceIdentifier: "new-default-source",
        calendarIdentifier: "unrelated-calendar",
        title: "Personal",
        containsOpenOpenMarker: false
      ),
    ]
  )
  #expect(
    target
      == ReminderTarget(
        sourceIdentifier: "original-source", calendarIdentifier: "original-calendar"
      ))
}

@Test
func heroAAmbiguousPhysicalReminderTargetsFailClosed() {
  #expect(throws: RemindersClientError.ambiguousCalendar) {
    try selectReminderTarget(
      candidates: [
        ReminderCalendarCandidate(
          sourceIdentifier: "source-1",
          calendarIdentifier: "calendar-1",
          title: "OpenOpen",
          containsOpenOpenMarker: true
        ),
        ReminderCalendarCandidate(
          sourceIdentifier: "source-2",
          calendarIdentifier: "calendar-2",
          title: "OpenOpen old",
          containsOpenOpenMarker: true
        ),
      ]
    )
  }
}

@Test
func heroARequiresAnExistingPhysicalOpenOpenListBeforeConfirmation() {
  #expect(throws: RemindersClientError.targetUnavailable) {
    try selectReminderTarget(
      candidates: [
        ReminderCalendarCandidate(
          sourceIdentifier: "source-1",
          calendarIdentifier: "calendar-1",
          title: "Personal",
          containsOpenOpenMarker: false
        )
      ]
    )
  }
}

@Test
func heroADashboardDecodesTheExactRustRecoveryShape() throws {
  let data = Data(
    #"{"activeCards":[{"id":"mission-1","state":"working","title":"Plan the day"}],"confirmedMission":{"missionId":"mission-1","reminderAuthorization":{"approvalDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","approvalId":"approval-reminders-1","listId":"openopen.default-reminders","missionId":"mission-1","payloadSha256":"188605fc48e5a3bc42efee3820582cb016a84685869bfbb6688daf79b055fab0","target":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"},"writeDisposition":"recoverOnly"},"reminderDispatch":[{"token":"dispatch-work-1","workItemId":"work-1"}],"reminderLinks":[],"title":"Plan the day","workItems":[{"id":"work-1","title":"Pick one priority"}]},"microphone":{"available":false,"reason":"Microphone unavailable until Voice setup"},"receipt":null,"runtime":{"enabled":true,"revision":1,"updatedAtMs":2},"suggestion":null}"#
      .utf8
  )
  let dashboard = try JSONDecoder().decode(DashboardState.self, from: data)
  #expect(try dashboard.validated() == dashboard)
  #expect(
    dashboard.confirmedMission?.reminderAuthorization.validates(
      missionId: "mission-1",
      workItems: [MissionWorkItem(id: "work-1", title: "Pick one priority")]
    ) == true
  )
}

@MainActor
@Test
func heroADashboardRestoresARecoverableMissionOrReceiptAfterRestart() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker(), reminders: MockReminders()) {}
  await model.updateEnabled(true)
  let mission = testConfirmedMission()
  await core.restoreFromDashboard(mission: mission, receipt: nil)
  await model.refreshDashboard()
  #expect(model.confirmedMission == mission)
  #expect(model.receipt == nil)
  await model.confirmSuggestion()
  #expect(await core.confirmationCount == 0)
  #expect(model.reminderLinks.map(\.calendarItemIdentifier) == ["reminder-work-1"])

  let receipt = MissionReceipt(
    id: "receipt-1",
    missionId: "mission-1",
    summary: "Completed Plan the day",
    actualModel: "gpt-5.6-sol",
    evidenceIds: ["evidence-work-1"],
    outputHashes: [],
    completedAtMs: 10
  )
  await core.restoreFromDashboard(mission: nil, receipt: receipt)
  await model.refreshDashboard()
  #expect(model.confirmedMission == nil)
  #expect(model.receipt == receipt)
}

@MainActor
@Test
func heroAPersistedReminderLinksNeverRepeatTheExternalWriteAfterRestart() async {
  let core = MockCore()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  let link = ReminderLink(
    missionId: "mission-1",
    workItemId: "work-1",
    sourceIdentifier: "source-1",
    calendarIdentifier: "calendar-1",
    calendarItemIdentifier: "reminder-work-1",
    dispatchToken: "dispatch-work-1",
    title: "Pick one priority"
  )
  let mission = testConfirmedMission(
    writeDisposition: .recoverOnly,
    reminderDispatch: [
      ConfirmedReminderDispatch(workItemId: "work-1", token: "dispatch-work-1")
    ],
    reminderLinks: [link]
  )
  await core.restoreFromDashboard(mission: mission, receipt: nil)

  await model.refreshDashboard()
  await model.confirmSuggestion()

  #expect(model.confirmedMission == mission)
  #expect(model.reminderLinks == [link])
  #expect(reminders.executeCount == 0)
  #expect(await core.confirmationCount == 0)
}

@MainActor
@Test
func heroAPartialReminderReadbackCannotFabricateReceipt() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .partial)
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  model.prompt = "Help me plan today"
  await model.submitPrompt()
  await model.confirmSuggestion()
  await model.checkMissionProgress()

  #expect(model.receipt == nil)
  #expect(model.errorMessage?.contains("Finish every OpenOpen reminder") == true)
  #expect(await core.reminderCompletionPayloads.isEmpty)
}

@MainActor
@Test
func heroAPostCommitReadbackFailureRetriesWithReadOnlyRecoveryAndNoSecondWrite() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .commitThenFailReadback)
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  model.prompt = "Help me plan today"
  await model.submitPrompt()
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 1)
  #expect(model.confirmedMission?.missionId == "mission-1")
  #expect(model.suggestion?.id == "suggestion-1")
  #expect(model.receipt == nil)
  #expect(model.reminderLinks.isEmpty)
  #expect(reminders.executeCount == 1)
  #expect(reminders.recoverCount == 0)

  reminders.mode = .complete
  await model.confirmSuggestion()
  #expect(await core.confirmationCount == 1)
  #expect(await core.dispatchBeginCount == 2)
  #expect(model.suggestion == nil)
  #expect(model.reminderLinks.count == 1)
  #expect(reminders.executeCount == 1)
  #expect(reminders.recoverCount == 1)
}

@MainActor
@Test
func heroAPrecommitFailureNeverIssuesASecondExternalWrite() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .failBeforeCommit)
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  model.prompt = "Help me plan today"
  await model.submitPrompt()
  await model.confirmSuggestion()

  reminders.mode = .complete
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 1)
  #expect(await core.dispatchBeginCount == 2)
  #expect(reminders.executeCount == 1)
  #expect(reminders.recoverCount == 1)
  #expect(model.reminderLinks.isEmpty)
  #expect(model.errorMessage?.contains("exactly match") == true)
}

@MainActor
@Test
func heroADispatchResponseLossFailsClosedWithoutAnyExternalWrite() async {
  let core = MockCore()
  await core.loseNextReminderDispatchResponse()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  model.prompt = "Help me plan today"
  await model.submitPrompt()
  await model.confirmSuggestion()
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 1)
  #expect(await core.dispatchBeginCount == 2)
  #expect(reminders.executeCount == 0)
  #expect(reminders.recoverCount == 1)
  #expect(model.reminderLinks.isEmpty)
}

@MainActor
@Test
func heroAOffAfterEventKitCommitStillPermitsOnlyReadOnlyRecovery() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .commitThenDelay)
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  model.prompt = "Help me plan today"
  await model.submitPrompt()
  model.requestSuggestionConfirmation()

  for _ in 0..<20 where reminders.executeCount == 0 {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(reminders.executeCount == 1)
  await model.updateEnabled(false)
  for _ in 0..<30 where model.isBusy {
    try? await Task.sleep(for: .milliseconds(10))
  }

  #expect(model.confirmedMission?.reminderDispatch.count == 1)
  #expect(model.reminderLinks.isEmpty)
  #expect(reminders.executeCount == 1)
  #expect(!model.isBusy)

  reminders.mode = .complete
  await model.updateEnabled(true)
  #expect(model.modelEntryEnabled)
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 1)
  #expect(await core.dispatchBeginCount == 2)
  #expect(reminders.executeCount == 1)
  #expect(reminders.recoverCount == 1)
  #expect(model.reminderLinks.count == 1)
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
func explicitRealCoreClientRoundTripUsesTheProductionPipeProtocol() async throws {
  guard
    let runtimePath = ProcessInfo.processInfo.environment["OPENOPEN_TEST_CORE_RUNTIME"],
    let homePath = ProcessInfo.processInfo.environment["OPENOPEN_TEST_CORE_HOME"]
  else { return }
  guard homePath.hasPrefix("/private/tmp/OpenOpen-CoreClient-") else {
    Issue.record("OPENOPEN_TEST_CORE_RUNTIME requires an isolated CFFIXED_USER_HOME")
    return
  }
  let home = URL(fileURLWithPath: homePath)
  try FileManager.default.createDirectory(
    at: home.appendingPathComponent("Library/Application Support", isDirectory: true),
    withIntermediateDirectories: true
  )
  let runtime = URL(fileURLWithPath: runtimePath).standardizedFileURL
  let stderr = home.appendingPathComponent("core.stderr")
  let executable = home.appendingPathComponent("core-wrapper")
  let wrapper = """
    #!/bin/sh
    exec "\(runtime.path)" 2>"\(stderr.path)"
    """
  try Data(wrapper.utf8).write(to: executable)
  try FileManager.default.setAttributes(
    [.posixPermissions: 0o700],
    ofItemAtPath: executable.path
  )
  let client = CoreProcessClient(
    executableResolver: { executable },
    staticCodeValidator: { _ in },
    runningCodeValidator: { _ in },
    masterKeyLoader: { Data(repeating: 7, count: 32) },
    childEnvironmentLoader: {
      [
        "CFFIXED_USER_HOME": home.path,
        "HOME": home.path,
        "PATH": "/usr/bin:/bin",
      ]
    }
  )
  defer { client.shutdown() }

  let identity: CoreEffectIdentity
  do {
    identity = try await client.effectIdentity()
  } catch {
    let diagnostic = (try? String(contentsOf: stderr, encoding: .utf8)) ?? "<no stderr>"
    Issue.record("real Core stderr: \(diagnostic)")
    throw error
  }

  #expect(identity.coreProcessIdentifier > 0)
  #expect(identity.coreKeyID.count == 64)
  #expect(identity.coreVerifyingKeyHex.count == 64)
  #expect(identity.coreInstanceNonce.count == 64)
}

@Test
func persistentCoreResponseCompletesBeforeTheChildExits() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("persistent-core")
  let script = """
    #!/bin/sh
    /bin/dd bs=55 count=1 of=/dev/null 2>/dev/null
    IFS= read -r request
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"enabled":false,"revision":0,"updatedAtMs":0}}'
    /bin/cat >/dev/null
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
  defer { client.shutdown() }

  let runtime = try await client.runtime()

  #expect(!runtime.enabled)
  #expect(runtime.revision == 0)
  #expect(runtime.updatedAtMs == 0)
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
