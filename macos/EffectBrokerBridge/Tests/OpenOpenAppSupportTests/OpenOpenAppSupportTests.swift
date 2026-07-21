import AppKit
import CryptoKit
import Darwin
import EffectBrokerBridge
import Foundation
import Security
import SwiftUI
import Testing

@testable import OpenOpenAppSupport

private let testSuggestionOneId =
  "suggestion-1-00000000000000000000000000000001"
private let testSuggestionTwoId =
  "suggestion-2-00000000000000000000000000000002"
private let testChannelSuggestionId =
  "suggestion-3-00000000000000000000000000000003"
private let testOriginalSuggestionId =
  "suggestion-4-00000000000000000000000000000004"
private let testCorrectionSuggestionId =
  "suggestion-5-00000000000000000000000000000005"
private let testRecoveredSuggestionId =
  "suggestion-6-00000000000000000000000000000006"
private let testRetainedSuggestionId =
  "suggestion-7-00000000000000000000000000000007"
private let testInvalidatedSuggestionId =
  "suggestion-8-00000000000000000000000000000008"
private let testRestartSuggestionId =
  "suggestion-9-00000000000000000000000000000009"

@MainActor
private func descendants(of root: NSView) -> [NSView] {
  [root] + root.subviews.flatMap { descendants(of: $0) }
}

@MainActor
private func dashboardOutcomeField(in root: NSView) -> NSTextField? {
  descendants(of: root)
    .compactMap { $0 as? NSTextField }
    .first { $0.placeholderString == "Tell OpenOpen what you want to sort out…" }
}

@MainActor
private func dashboardInteractionAnchor(
  in root: NSView,
  identifier: String
) -> NSView? {
  descendants(of: root).first {
    $0.identifier?.rawValue == identifier
  }
}

@MainActor
private func clickDashboardInteractionAnchor(
  _ anchor: NSView,
  in window: NSWindow
) throws {
  #expect(anchor.window === window)
  #expect(!anchor.bounds.isEmpty)
  let point = anchor.convert(
    NSPoint(x: anchor.bounds.midX, y: anchor.bounds.midY),
    to: nil
  )
  for (type, pressure): (NSEvent.EventType, Float) in [
    (.leftMouseDown, 1), (.leftMouseUp, 0),
  ] {
    let event = try #require(
      NSEvent.mouseEvent(
        with: type,
        location: point,
        modifierFlags: [],
        timestamp: ProcessInfo.processInfo.systemUptime,
        windowNumber: window.windowNumber,
        context: nil,
        eventNumber: 0,
        clickCount: 1,
        pressure: pressure
      )
    )
    window.sendEvent(event)
  }
}

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

private func testChoiceConfirmedMission(
  confirmation: ChoiceConsolidatedConfirmation,
  dispatch: [ConfirmedReminderDispatch]
) -> ConfirmedMission {
  let missionId = "choice-mission-1"
  let target = ReminderTarget(sourceIdentifier: "source-1", calendarIdentifier: "calendar-1")
  let workItems = confirmation.reminderItems.map {
    MissionWorkItem(id: $0.id, title: $0.text)
  }
  var payload = Data("OPENOPEN_REMINDER_WRITE_V2\0".utf8)
  func append(_ value: String) {
    let bytes = Data(value.utf8)
    var count = UInt64(bytes.count).bigEndian
    withUnsafeBytes(of: &count) { payload.append(contentsOf: $0) }
    payload.append(bytes)
  }
  append(missionId)
  append(confirmation.id)
  append(confirmation.payloadDigest)
  append(confirmation.reminderPayloadDigest)
  append(ReminderWriteAuthorization.logicalListId)
  append(target.sourceIdentifier)
  append(target.calendarIdentifier)
  for item in confirmation.reminderItems {
    append(item.id)
    append(item.text)
    var dueAtMs = UInt64(bitPattern: item.dueAtMs).bigEndian
    withUnsafeBytes(of: &dueAtMs) { payload.append(contentsOf: $0) }
    append(item.timeZone)
    append(item.evidenceIntent)
  }
  let digest = SHA256.hash(data: payload).map { String(format: "%02x", $0) }.joined()
  return ConfirmedMission(
    missionId: missionId, title: confirmation.goal, workItems: workItems,
    reminderAuthorization: ReminderWriteAuthorization(
      missionId: missionId, listId: ReminderWriteAuthorization.logicalListId,
      payloadSha256: digest, approvalId: "choice-approval-1",
      approvalDigest: String(repeating: "a", count: 64), target: target,
      writeDisposition: .recoverOnly),
    reminderDispatch: dispatch, reminderLinks: [], choiceConfirmationId: confirmation.id,
    choicePayloadDigest: confirmation.payloadDigest,
    choiceReminderPayloadDigest: confirmation.reminderPayloadDigest,
    choiceReminderItems: confirmation.reminderItems)
}

/// Historical dashboard state may contain an already-durable suggestion from
/// before the Choice loop. Keep recovery coverage explicit: this fixture never
/// drives the retired prompt-to-Outcome UI path.
@MainActor
private func restoreLegacySuggestionForRecovery(
  _ core: MockCore,
  model: AppModel,
  identifier: String = testSuggestionOneId
) async {
  let first = identifier == testSuggestionOneId
  let suggestion = OutcomeSuggestion(
    id: identifier,
    title: first ? "Plan the day" : "Plan tomorrow",
    whyNow: "Recovered durable state.",
    proposedSteps: [first ? "Pick one priority" : "Choose tomorrow's priority"],
    sourceRefs: []
  )
  await core.restoreFromDashboard(mission: nil, receipt: nil, suggestion: suggestion)
  await model.refreshDashboard()
}

private func testChannelRouteSet(
  missionId: String = "mission-1",
  channel: ChannelKind = .discord,
  conversationId: String = "2002",
  ownerSenderId: String = "1001",
  allowedOutboundClasses: [ChannelMessageKind] = [.needYou, .progress, .receipt]
) -> ChannelRouteSet {
  let route = ChannelRoute(
    routeId: "route-primary",
    role: .primary,
    channel: channel,
    conversationId: conversationId,
    ownerSenderId: ownerSenderId,
    providerIdentity: channel == .discord ? "4004" : nil,
    sourceMessageId: channel == .discord ? "5005" : "source-message-1",
    allowedInboundClasses: [.missionParticipation, .needYouResponse],
    allowedOutboundClasses: allowedOutboundClasses,
    revision: 1,
    approvalId: "approval-primary",
    auditId: "audit-primary",
    boundAtMs: 1,
    updatedAtMs: 1
  )
  return ChannelRouteSet(
    missionId: missionId,
    revision: 1,
    primaryRouteId: route.routeId,
    routes: [route]
  )
}

private func testChannelRouteSetWithAdditionalRoute(
  missionId: String = "mission-1"
) -> ChannelRouteSet {
  let primary = testChannelRouteSet(
    missionId: missionId,
    channel: .iMessage,
    conversationId: "42",
    ownerSenderId: "owner@example.invalid"
  ).primaryRoute!
  let additional = ChannelRoute(
    routeId: "route-additional-discord",
    role: .additional,
    channel: .discord,
    conversationId: "2002",
    ownerSenderId: "1001",
    providerIdentity: "4004",
    sourceMessageId: "5005",
    allowedInboundClasses: [.missionParticipation, .needYouResponse],
    allowedOutboundClasses: [],
    revision: 2,
    approvalId: "approval-additional",
    auditId: "audit-additional",
    boundAtMs: 2,
    updatedAtMs: 2
  )
  return ChannelRouteSet(
    missionId: missionId,
    revision: 2,
    primaryRouteId: primary.routeId,
    routes: [primary, additional]
  )
}

private func testDiscordPairing() -> ChannelPairing {
  ChannelPairing(
    channel: .discord,
    ownerSenderId: "1001",
    conversationId: "2002",
    discord: DiscordPairingMetadata(
      guildId: "6006",
      botUserId: "3003",
      applicationId: "4004",
      setupSourceMessageId: "5005",
      setupCandidateId: "discord-pair-\(String(repeating: "a", count: 64))"
    ),
    pairedAtMs: 2
  )
}

private func testIMessagePairing() -> ChannelPairing {
  ChannelPairing(
    channel: .iMessage,
    ownerSenderId: "owner@example.invalid",
    conversationId: "42",
    pairedAtMs: 1
  )
}

@MainActor
private func connectTestIMessage(_ model: AppModel) async {
  if model.iMessageIsConnected { return }
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()
}

private func testChannelFailureIncident(
  channel: ChannelKind = .iMessage,
  seed: String = "a",
  occurredAtMs: Int64 = 10,
  runtimeRevision: UInt64 = 1,
  acknowledged: Bool = false
) -> ChannelFailureIncident {
  let sourceAnchor = ChannelFailureAuditAnchor(
    sequence: 10,
    entryHash: String(repeating: seed, count: 64),
    signatureHex: String(repeating: "1", count: 128)
  )
  let incidentAnchor = ChannelFailureAuditAnchor(
    sequence: 11,
    entryHash: String(repeating: seed == "a" ? "b" : "a", count: 64),
    signatureHex: String(repeating: "2", count: 128)
  )
  return ChannelFailureIncident(
    incidentId: "channel-failure-" + String(repeating: seed, count: 64),
    channel: channel,
    failureClass: .modelResultUnavailable,
    occurredAtMs: occurredAtMs,
    runtimeRevision: runtimeRevision,
    dispatchStateHash: String(repeating: seed, count: 64),
    sourceAuditAnchor: sourceAnchor,
    incidentAuditAnchor: incidentAnchor,
    acknowledgement: acknowledged
      ? ChannelFailureAcknowledgement(
        acknowledgedAtMs: occurredAtMs + 1,
        runtimeRevision: runtimeRevision,
        auditAnchor: ChannelFailureAuditAnchor(
          sequence: 12,
          entryHash: String(repeating: "c", count: 64),
          signatureHex: String(repeating: "3", count: 128)
        )
      ) : nil
  )
}

private func testChannelFailureIncident(
  index: Int,
  channel: ChannelKind,
  acknowledged: Bool = false
) -> ChannelFailureIncident {
  let incidentDigest = String(format: "%064x", index + 1)
  let sourceDigest = String(format: "%064x", index + 10_000)
  let incidentAnchorDigest = String(format: "%064x", index + 20_000)
  let acknowledgementDigest = String(format: "%064x", index + 30_000)
  let baseSequence = Int64(index * 3 + 1)
  return ChannelFailureIncident(
    incidentId: "channel-failure-\(incidentDigest)",
    channel: channel,
    failureClass: .modelResultUnavailable,
    occurredAtMs: Int64(index + 1),
    runtimeRevision: 1,
    dispatchStateHash: incidentDigest,
    sourceAuditAnchor: ChannelFailureAuditAnchor(
      sequence: baseSequence,
      entryHash: sourceDigest,
      signatureHex: String(repeating: "1", count: 128)
    ),
    incidentAuditAnchor: ChannelFailureAuditAnchor(
      sequence: baseSequence + 1,
      entryHash: incidentAnchorDigest,
      signatureHex: String(repeating: "2", count: 128)
    ),
    acknowledgement: acknowledged
      ? ChannelFailureAcknowledgement(
        acknowledgedAtMs: Int64(index + 2),
        runtimeRevision: 1,
        auditAnchor: ChannelFailureAuditAnchor(
          sequence: baseSequence + 2,
          entryHash: acknowledgementDigest,
          signatureHex: String(repeating: "3", count: 128)
        )
      ) : nil
  )
}

private struct TestChannelSend: Equatable, Sendable {
  let missionId: String
  let routeId: String
  let kind: ChannelMessageKind
  let content: String
  let approvedAtMs: Int64
}

private actor MockCore: CoreServing {
  nonisolated var permitsDeferredChannelTestRoutes: Bool { true }
  var control = RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0)
  let dashboardDelay: Duration
  let dashboardFails: Bool
  var dashboardInvocationCount = 0
  var challengesIssued = 0
  var proofNonces: [String] = []
  var proposalCount = 0
  var confirmationCount = 0
  var missionCancellationCount = 0
  var rejectNextMissionCancellation = false
  var loseNextMissionCancellationResponse = false
  var completeBeforeNextMissionCancellation = false
  var dispatchBeginCount = 0
  var reminderCompletionPayloads: [[ReminderCompletionInput]] = []
  var receiptReturnApprovals: [Int64?] = []
  var receiptReturnRouteIds: [String?] = []
  var leaseInstalled = false
  var codexPrepareCount = 0
  var codexLoginPrepareCount = 0
  var codexInitialized = false
  var codexInitializeCount = 0
  var codexAbortCount = 0
  var codexCandidateBindCount = 0
  var brokerEnrollmentInstalled = false
  var brokerEnrollmentInstallCount = 0
  var offRequiresBrokerEnrollment = false
  var candidateBrokerBound = false
  var abortedCandidateBoundStates: [Bool] = []
  var loseNextLeaseInstallResponse = false
  var offPrepareCount = 0
  var activeOperation = false
  var coreInstanceNonce = String(repeating: "a", count: 64)
  var nextEffectIdentityDelay: Duration = .zero
  var nextEffectIdentityGate: NonCooperativeRpcGate?
  var effectIdentityFailuresRemaining = 0
  var effectIdentityAttemptCount = 0
  var effectIdentityFenceStates: [Bool] = []
  var generationFenceSequence: UInt64 = 0
  var activeGenerationFence: CoreGenerationFence?
  var generationFenceBeginCount = 0
  var generationFenceCloseCount = 0
  var invalidateNextGenerationFenceClose = false
  var effectIdentityFailuresAfterFenceInvalidation = 0
  var rejectNextOffPreparation = false
  var mismatchRecoveryTimestamp = false
  var invalidReminderAuthorization = false
  var invalidCompletionReceipt = false
  var dashboardConfirmedMission: ConfirmedMission?
  var dashboardReceipt: MissionReceipt?
  var dashboardChannelRouteSet: ChannelRouteSet?
  var dashboardNeedsYou: MissionNeedsYou?
  var dashboardSuggestion: OutcomeSuggestion?
  var dashboardChannelFailureIncidents: [ChannelFailureIncident] = []
  var dashboardAfterNextCompletion: DashboardState?
  var dashboardAfterNextChannelFailureAcknowledgement: DashboardState?
  var dashboardChannelRouteSetAfterNextConfirmation: ChannelRouteSet?
  var dashboardOverride: DashboardState?
  var rejectNextDashboardRead = false
  var choiceLoopResponse: ChoiceLoopSnapshot?
  var personaStatusResponse: PersonaStatusView?
  var choiceReminderScheduleResponse: ChoiceReminderSchedule?
  var nextChoiceReminderScheduleReadError: CoreClientError?
  var choiceReminderScheduleInputs: [ChoiceReminderScheduleInput] = []
  var choiceConfirmationResponse: ChoiceConsolidatedConfirmation?
  var choiceMarkdownReceiptCleanupAvailable = false
  var choiceMarkdownReceiptCleanupCount = 0
  var choiceCancellationResponse: ChoiceLoopSnapshot?
  var nextChoiceCancellationErrorAfterAcceptance: CoreClientError?
  var choiceBeginAccepted: ChoiceBeginAccepted?
  var choiceBeginParameters: [ChoiceBeginParameters] = []
  var choiceSelections: [ChoiceSelection] = []
  var choiceDInputs: [ChoiceDInput] = []
  var nextChoiceDInputError: CoreClientError?
  var nextChoiceDInputErrorAfterAcceptance: CoreClientError?
  var beforeNextChoiceDInputErrorAfterAcceptance: (@Sendable () -> Void)?
  var nextChoiceDUnexpectedResponseAfterAcceptance: ChoiceLoopSnapshot?
  var choiceResumeCount = 0
  var choiceResumeResponse: ChoiceLoopSnapshot?
  var nextChoiceResumeErrorAfterAcceptance: CoreClientError?
  var choiceCallTrace: [String] = []
  var choiceConfirmationPrepareCount = 0
  var choiceConfirmations: [ChoiceConsolidatedConfirmation] = []
  var choiceReminderMissionResponse: ConfirmedMission?
  var choiceReminderDispatchStartResponse: ReminderDispatchStart?
  var choiceReminderAbortCount = 0
  var loseNextChoiceReminderAbortResponseAfterCommit = false
  var choiceReminderAbortFailuresRemaining = 0
  var rejectNextChoiceLoopRead = false
  var choiceLoopReadFailuresRemaining = 0
  var nextChoiceLoopReadError: CoreClientError?
  var rejectNextChoiceMarkdownReconcile = false
  var nextChoiceLoopGate: NonCooperativeRpcGate?
  var channelFailureAcknowledgementCount = 0
  var rejectNextChannelFailureAcknowledgement = false
  var rejectedChannelFailureIncidentIds = Set<String>()
  var channelFailureAcknowledgementDelay: Duration = .zero
  var nextChannelFailureAcknowledgementGate: NonCooperativeRpcGate?
  var dispatchedMissions: [String: ConfirmedMission] = [:]
  var loseNextDispatchResponse = false
  var channelPairings: [ChannelKind: ChannelPairing] = [:]
  var pairChannelCount = 0
  var discordSetupStartCount = 0
  var discordSetupPollCount = 0
  var discordSetupConfirmCount = 0
  var discordStartTokens: [String] = []
  var discordSessionStartCount = 0
  var discordSessionRunning = false
  var rejectNextDiscordStartUnavailable = false
  var loseNextDiscordConfirmResponse = false
  var loseNextDiscordStartResponse = false
  var discordStartDelay: Duration = .zero
  var nextDiscordStartGate: NonCooperativeRpcGate?
  var stoppedChannels: [ChannelKind] = []
  var rejectNextIMessageActivation = false
  var iMessageActivationDelay: Duration = .zero
  var rejectNextIMessagePrepareAfterCommit = false
  var iMessagePrepareCount = 0
  var iMessageDiscoveryPrepareCount = 0
  var iMessageChatsListCount = 0
  var iMessageDiscoveryPrepared = false
  var iMessageChatsToReturn = [
    IMessageChat(
      chatId: "42", name: "Owner", service: "iMessage",
      participants: ["owner@example.invalid"]),
    IMessageChat(
      chatId: "84", name: "Family", service: "iMessage",
      participants: ["owner@example.invalid", "family@example.invalid"]),
  ]
  var iMessageStartCount = 0
  var iMessageSessionRunning = false
  var channelPollInvocationCount = 0
  var channelPollCount = 0
  var channelPollFenceStates: [Bool] = []
  var channelPollModelWorkAllowances: [Bool] = []
  var discordStatusReadCount = 0
  var channelPollDelay: Duration = .zero
  var channelPollConnectionStatus = "connected"
  var queuedDiscordStatusResponses: [String] = []
  var rejectNextChannelPoll = false
  var nextChannelPollError: CoreClientError?
  var nextChannelPollErrors: [ChannelKind: CoreClientError] = [:]
  var loseNextChannelPollResponse = false
  var queuedChannelSuggestion: OutcomeSuggestion?
  var queuedChannelMissionEvent: ChannelMissionEvent?
  var queuedChannelMissionEventStatus = "missionUpdated"
  var queuedChannelPollResponses: [ChannelPollResponse] = []
  var queuedChannelPollResponsesByChannel: [ChannelKind: [ChannelPollResponse]] = [:]
  var channelSends: [TestChannelSend] = []
  var channelSendAttemptCount = 0
  var loseNextChannelSendResponse = false
  var returnNextChannelSendUncertain = false
  var channelRouteApprovals: [ChannelRouteApproval] = []
  var loginBeginCount = 0
  var loginAwaitCount = 0
  var loginCompleted: Bool
  var modelCatalog: [GptModel]
  var persistedModelSelection: ModelSelection?
  var modelSelectionWriteCount = 0
  var loginAuthURL = "https://example.invalid"
  var rejectNextLoginAwait = false
  var rejectNextCodexPrepare = false

  init(
    dashboardDelay: Duration = .zero,
    dashboardFails: Bool = false,
    loginCompleted: Bool = true,
    modelCatalog: [GptModel] = [
      GptModel(
        id: "gpt-test-model", displayName: "Test model",
        supportedReasoningEfforts: ["high"])
    ]
  ) {
    self.dashboardDelay = dashboardDelay
    self.dashboardFails = dashboardFails
    self.loginCompleted = loginCompleted
    self.modelCatalog = modelCatalog
    persistedModelSelection = loginCompleted ? Self.selection(for: modelCatalog) : nil
  }

  private static func catalogBinding(for models: [GptModel]) -> (
    fingerprint: String, revision: UInt64
  ) {
    let value = models.map { model in
      "\(model.id)\u{0}\(model.displayName)\u{0}\(model.supportedReasoningEfforts.joined(separator: "\u{0}"))"
    }.joined(separator: "\u{1}")
    let fingerprint = SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }
      .joined()
    let revision = UInt64(fingerprint.prefix(16), radix: 16) ?? 1
    return (fingerprint, revision)
  }

  private static func catalogSnapshotId(
    for binding: (fingerprint: String, revision: UInt64)
  ) -> String {
    let value = "mock-catalog-snapshot\u{0}\(binding.fingerprint)\u{0}\(binding.revision)"
    return SHA256.hash(data: Data(value.utf8)).map { String(format: "%02x", $0) }.joined()
  }

  private static func selection(for models: [GptModel]) -> ModelSelection? {
    guard let model = models.first else { return nil }
    let binding = catalogBinding(for: models)
    let effort = model.supportedReasoningEfforts.first ?? "not_applicable"
    return ModelSelection(
      id: "mock-selection-\(model.id)",
      modelId: model.id,
      requestedEffort: effort,
      actualEffort: effort,
      catalogFingerprint: binding.fingerprint,
      catalogRevision: binding.revision,
      accountDisplayClass: "chatgpt:plus",
      protocolSchemaRevision: 1
    )
  }

  func beginCoreGenerationFence() throws -> CoreGenerationFence {
    guard activeGenerationFence == nil else {
      throw CoreClientError.contractViolation("test Core generation fence already active")
    }
    generationFenceSequence += 1
    generationFenceBeginCount += 1
    let fence = CoreGenerationFence(identifier: UUID(), generation: generationFenceSequence)
    activeGenerationFence = fence
    return fence
  }

  func closeCoreGenerationFence(_ fence: CoreGenerationFence) -> Bool {
    generationFenceCloseCount += 1
    guard activeGenerationFence == fence else { return false }
    activeGenerationFence = nil
    if invalidateNextGenerationFenceClose {
      invalidateNextGenerationFenceClose = false
      effectIdentityFailuresRemaining = effectIdentityFailuresAfterFenceInvalidation
      effectIdentityFailuresAfterFenceInvalidation = 0
      return false
    }
    return true
  }

  func invalidateNextFenceAtClose(followedByEffectIdentityFailures: Int = 0) {
    invalidateNextGenerationFenceClose = true
    effectIdentityFailuresAfterFenceInvalidation = followedByEffectIdentityFailures
  }

  func runtime() -> RuntimeControl { control }
  func effectIdentity() async throws -> CoreEffectIdentity {
    effectIdentityAttemptCount += 1
    effectIdentityFenceStates.append(activeGenerationFence != nil)
    if let gate = nextEffectIdentityGate {
      nextEffectIdentityGate = nil
      try await gate.wait()
    }
    if nextEffectIdentityDelay > .zero {
      let delay = nextEffectIdentityDelay
      nextEffectIdentityDelay = .zero
      try await Task.sleep(for: delay)
    }
    if effectIdentityFailuresRemaining > 0 {
      effectIdentityFailuresRemaining -= 1
      throw CoreClientError.processUnavailable
    }
    return testCoreIdentity(coreInstanceNonce: coreInstanceNonce)
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
      if offRequiresBrokerEnrollment, !brokerEnrollmentInstalled {
        throw CoreClientError.remote(code: -32_000, message: "Local operation failed closed")
      }
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

  func installBrokerEnrollment(_: Data) {
    brokerEnrollmentInstalled = true
    brokerEnrollmentInstallCount += 1
  }

  func startActiveOperation() { activeOperation = true }
  func forceRuntime(_ enabled: Bool) {
    control = RuntimeControl(
      enabled: enabled,
      revision: control.revision + 1,
      updatedAtMs: control.updatedAtMs + 1
    )
  }
  func rotateCoreInstance() { coreInstanceNonce = String(repeating: "b", count: 64) }
  func simulateCoreReplacement() {
    coreInstanceNonce = String(repeating: coreInstanceNonce.first == "a" ? "b" : "c", count: 64)
    codexInitialized = false
    leaseInstalled = false
    candidateBrokerBound = false
    brokerEnrollmentInstalled = false
    discordSessionRunning = false
    iMessageSessionRunning = false
  }
  func requireBrokerEnrollmentBeforeOff() { offRequiresBrokerEnrollment = true }
  func delayNextEffectIdentity(by delay: Duration) { nextEffectIdentityDelay = delay }
  func blockNextEffectIdentity(on gate: NonCooperativeRpcGate) {
    nextEffectIdentityGate = gate
  }
  func resetEffectIdentityFenceStates() { effectIdentityFenceStates.removeAll() }
  func blockNextDiscordStart(on gate: NonCooperativeRpcGate) {
    nextDiscordStartGate = gate
  }
  func failNextDiscordStartUnavailable() { rejectNextDiscordStartUnavailable = true }
  func failNextEffectIdentityAttempts(_ count: Int) { effectIdentityFailuresRemaining = count }
  func failNextOffPreparation() { rejectNextOffPreparation = true }
  func returnMismatchedRecoveryTimestamp() { mismatchRecoveryTimestamp = true }
  func returnInvalidReminderAuthorization() { invalidReminderAuthorization = true }
  func returnInvalidCompletionReceipt() { invalidCompletionReceipt = true }
  func restoreFromDashboard(
    mission: ConfirmedMission?, receipt: MissionReceipt?,
    channelRouteSet: ChannelRouteSet? = nil,
    needsYou: MissionNeedsYou? = nil,
    suggestion: OutcomeSuggestion? = nil,
    channelFailureIncidents: [ChannelFailureIncident] = []
  ) {
    dashboardOverride = nil
    dashboardConfirmedMission = mission
    dashboardReceipt = receipt
    dashboardChannelRouteSet = channelRouteSet
    dashboardNeedsYou = needsYou
    dashboardSuggestion = suggestion
    dashboardChannelFailureIncidents = channelFailureIncidents
    if let mission, !mission.reminderDispatch.isEmpty {
      dispatchedMissions[mission.missionId] = mission
    }
  }

  func returnDashboardAfterNextCompletion(_ dashboard: DashboardState) {
    dashboardAfterNextCompletion = dashboard
  }

  func returnDashboardAfterNextChannelFailureAcknowledgement(_ dashboard: DashboardState) {
    dashboardAfterNextChannelFailureAcknowledgement = dashboard
  }

  func returnChannelRouteSetAfterNextConfirmation(_ routeSet: ChannelRouteSet) {
    dashboardChannelRouteSetAfterNextConfirmation = routeSet
  }

  func failNextDashboardRead() {
    rejectNextDashboardRead = true
  }

  func setChoiceLoopSnapshot(_ snapshot: ChoiceLoopSnapshot?) {
    choiceLoopResponse = snapshot
  }

  func setChoiceResumeResponse(_ snapshot: ChoiceLoopSnapshot?) {
    choiceResumeResponse = snapshot
  }

  func failNextChoiceDInput(with error: CoreClientError) {
    nextChoiceDInputError = error
  }

  func failNextChoiceDInputAfterAcceptance(
    with error: CoreClientError, beforeThrow: (@Sendable () -> Void)? = nil
  ) {
    nextChoiceDInputErrorAfterAcceptance = error
    beforeNextChoiceDInputErrorAfterAcceptance = beforeThrow
  }

  func returnUnexpectedChoiceDResponseAfterAcceptance(_ snapshot: ChoiceLoopSnapshot) {
    nextChoiceDUnexpectedResponseAfterAcceptance = snapshot
  }

  func clearChoiceCallTrace() { choiceCallTrace = [] }

  func setChoiceReminderSchedule(_ schedule: ChoiceReminderSchedule?) {
    choiceReminderScheduleResponse = schedule
  }

  func failNextChoiceReminderScheduleRead(with error: CoreClientError) {
    nextChoiceReminderScheduleReadError = error
  }

  func setChoiceConfirmationResponse(_ confirmation: ChoiceConsolidatedConfirmation?) {
    choiceConfirmationResponse = confirmation
  }

  func recordedChoiceReminderScheduleInputs() -> [ChoiceReminderScheduleInput] {
    choiceReminderScheduleInputs
  }

  func setChoiceMarkdownReceiptCleanupAvailable(_ available: Bool) {
    choiceMarkdownReceiptCleanupAvailable = available
  }

  func setChoiceBeginAccepted(_ accepted: ChoiceBeginAccepted?) {
    choiceBeginAccepted = accepted
  }

  func setChoiceCancellationResponse(_ snapshot: ChoiceLoopSnapshot?) {
    choiceCancellationResponse = snapshot
  }

  func failNextChoiceCancellationAfterAcceptance(with error: CoreClientError) {
    nextChoiceCancellationErrorAfterAcceptance = error
  }

  func failNextChoiceResumeAfterAcceptance(with error: CoreClientError) {
    nextChoiceResumeErrorAfterAcceptance = error
  }

  func failNextChoiceLoopRead() {
    rejectNextChoiceLoopRead = true
  }

  func failNextChoiceLoopReadAttempts(_ count: Int) {
    choiceLoopReadFailuresRemaining = max(0, count)
  }

  func failNextChoiceLoopRead(with error: CoreClientError) {
    nextChoiceLoopReadError = error
  }

  func failNextChoiceMarkdownReconcile() {
    rejectNextChoiceMarkdownReconcile = true
  }

  func blockNextChoiceLoopRead(on gate: NonCooperativeRpcGate) {
    nextChoiceLoopGate = gate
  }

  func setLoginCompleted(_ completed: Bool) {
    loginCompleted = completed
  }

  func loseNextReminderDispatchResponse() { loseNextDispatchResponse = true }

  func failNextMissionCancellation() { rejectNextMissionCancellation = true }

  func loseNextMissionCancellationResponseAfterCommit() {
    loseNextMissionCancellationResponse = true
  }

  func completeMissionBeforeNextCancellation() {
    completeBeforeNextMissionCancellation = true
  }

  func loseNextChannelSendResponseAfterCommit() {
    loseNextChannelSendResponse = true
  }

  func returnNextChannelSendUncertainAfterCommit() {
    returnNextChannelSendUncertain = true
  }

  func setChannelPairing(_ pairing: ChannelPairing) {
    channelPairings[pairing.channel] = pairing
  }

  func pairedDiscordApplicationId() -> String? {
    channelPairings[.discord]?.discord?.applicationId
  }

  func queueChannelSuggestion(_ suggestion: OutcomeSuggestion) {
    queuedChannelSuggestion = suggestion
  }

  func queueChannelPollResponses(_ responses: [ChannelPollResponse]) {
    queuedChannelPollResponses.append(contentsOf: responses)
  }

  func queueChannelPollResponses(
    _ responses: [ChannelPollResponse], for channel: ChannelKind
  ) {
    queuedChannelPollResponsesByChannel[channel, default: []].append(contentsOf: responses)
  }

  func queueChannelMissionEvent(
    _ event: ChannelMissionEvent, eventStatus: String = "missionUpdated"
  ) {
    queuedChannelMissionEvent = event
    queuedChannelMissionEventStatus = eventStatus
  }

  func setChannelPollConnectionStatus(_ status: String) {
    channelPollConnectionStatus = status
  }

  func queueDiscordStatusResponses(_ statuses: [String]) {
    queuedDiscordStatusResponses.append(contentsOf: statuses)
  }

  func failNextChannelPoll() {
    rejectNextChannelPoll = true
  }

  func failNextChannelPoll(with error: CoreClientError) {
    nextChannelPollError = error
  }

  func failNextChannelPoll(_ channel: ChannelKind, with error: CoreClientError) {
    nextChannelPollErrors[channel] = error
  }

  func loseNextChannelPollResponseAfterCommit() {
    loseNextChannelPollResponse = true
  }

  func failNextChannelFailureAcknowledgement() {
    rejectNextChannelFailureAcknowledgement = true
  }

  func failChannelFailureAcknowledgement(_ incidentId: String) {
    rejectedChannelFailureIncidentIds.insert(incidentId)
  }

  func delayChannelFailureAcknowledgement(by delay: Duration) {
    channelFailureAcknowledgementDelay = delay
  }

  func blockNextChannelFailureAcknowledgement(on gate: NonCooperativeRpcGate) {
    nextChannelFailureAcknowledgementGate = gate
  }

  func delayDiscordStart(by delay: Duration) {
    discordStartDelay = delay
  }

  func loseDiscordStartResponseAfterCommit() {
    loseNextDiscordStartResponse = true
  }

  func loseDiscordConfirmResponseAfterCommit() {
    loseNextDiscordConfirmResponse = true
  }

  func delayIMessageActivation(by delay: Duration) {
    iMessageActivationDelay = delay
  }

  func delayChannelPoll(by delay: Duration) {
    channelPollDelay = delay
  }

  func dashboard() async throws -> DashboardState {
    dashboardInvocationCount += 1
    if dashboardDelay > .zero { try await Task.sleep(for: dashboardDelay) }
    if rejectNextDashboardRead {
      rejectNextDashboardRead = false
      throw CoreClientError.contractViolation("Dashboard refresh failed closed.")
    }
    if dashboardFails {
      throw CoreClientError.contractViolation("Delayed dashboard failure.")
    }
    if let dashboardOverride { return dashboardOverride }
    var cards =
      dashboardConfirmedMission.map {
        [ActiveOutcomeCard(id: $0.missionId, title: $0.title, state: "working")]
      } ?? []
    if let needsYou = dashboardNeedsYou,
      !cards.contains(where: { $0.id == needsYou.missionId })
    {
      cards.append(
        ActiveOutcomeCard(
          id: needsYou.missionId,
          title: needsYou.title,
          state: "Need you"
        ))
    }
    return DashboardState(
      activeCards: cards,
      channelFailureIncidents: dashboardChannelFailureIncidents,
      channelRouteSet: dashboardChannelRouteSet,
      microphone: MicrophoneState(
        available: false,
        reason: "Microphone unavailable until Voice setup"
      ),
      runtime: control,
      suggestion: dashboardSuggestion,
      confirmedMission: dashboardConfirmedMission,
      needsYou: dashboardNeedsYou,
      receipt: dashboardReceipt
    )
  }

  func choiceLoop() async throws -> ChoiceLoopSnapshot? {
    // Freeze the response at request admission. A noncooperative blocked RPC
    // represents an already-issued Core response; later-generation reads may
    // observe newer Store state without rewriting that older response.
    let response = choiceLoopResponse
    let gate = nextChoiceLoopGate
    nextChoiceLoopGate = nil
    try await gate?.wait()
    if let error = nextChoiceLoopReadError {
      nextChoiceLoopReadError = nil
      throw error
    }
    if choiceLoopReadFailuresRemaining > 0 {
      choiceLoopReadFailuresRemaining -= 1
      throw CoreClientError.contractViolation("Choice continuity refresh failed closed.")
    }
    if rejectNextChoiceLoopRead {
      rejectNextChoiceLoopRead = false
      throw CoreClientError.contractViolation("Choice continuity refresh failed closed.")
    }
    return response
  }

  func personaStatus() -> PersonaStatusView? { personaStatusResponse }

  func setPersonaStatus(_ status: PersonaStatusView?) { personaStatusResponse = status }

  func choiceReminderSchedule() async throws -> ChoiceReminderSchedule? {
    if let error = nextChoiceReminderScheduleReadError {
      nextChoiceReminderScheduleReadError = nil
      throw error
    }
    return choiceReminderScheduleResponse
  }

  func recordChoiceReminderSchedule(
    _ input: ChoiceReminderScheduleInput, proof _: BrokerRuntimeState
  ) async throws -> ChoiceReminderSchedule {
    choiceReminderScheduleInputs.append(input)
    if let current = choiceReminderScheduleResponse, current.input == input { return current }
    let revision = (choiceReminderScheduleResponse?.revision ?? 0) + 1
    let schedule = ChoiceReminderSchedule(
      id: "schedule-\(revision)", input: input, revision: revision,
      acceptedAtMs: Int64(Date().timeIntervalSince1970 * 1_000))
    choiceReminderScheduleResponse = schedule
    return schedule
  }

  func prepareChoiceConfirmation(proof _: BrokerRuntimeState) async throws
    -> ChoiceConsolidatedConfirmation
  {
    choiceConfirmationPrepareCount += 1
    guard let choiceConfirmationResponse else {
      throw CoreClientError.contractViolation("Choice confirmation was not configured.")
    }
    return choiceConfirmationResponse
  }

  func authorizeChoiceReminders(
    confirmationId _: String, reminderTarget _: ReminderTarget, proof _: BrokerRuntimeState
  ) async throws -> ConfirmedMission {
    guard let choiceReminderMissionResponse else {
      throw CoreClientError.contractViolation("Choice Reminder Mission was not configured.")
    }
    dashboardConfirmedMission = choiceReminderMissionResponse
    return choiceReminderMissionResponse
  }

  func setChoiceReminderMission(_ mission: ConfirmedMission, executeNow: Bool) {
    choiceReminderMissionResponse = mission
    choiceReminderDispatchStartResponse = ReminderDispatchStart(
      mission: mission, executeNow: executeNow)
  }

  func loseNextChoiceReminderAbortResponse() {
    loseNextChoiceReminderAbortResponseAfterCommit = true
  }

  func failNextChoiceReminderAbortResponses(_ count: Int) {
    choiceReminderAbortFailuresRemaining = count
  }

  func beginChoiceReminderDispatch(
    confirmationId _: String, proof _: BrokerRuntimeState
  ) async throws -> ReminderDispatchStart {
    guard let response = choiceReminderDispatchStartResponse else {
      throw CoreClientError.contractViolation("Choice Reminder dispatch was not configured.")
    }
    if response.executeNow {
      choiceReminderDispatchStartResponse = ReminderDispatchStart(
        mission: response.mission, executeNow: false)
    }
    return response
  }

  func abortChoiceReminderDispatchBeforeCommit(
    confirmationId _: String, proof _: BrokerRuntimeState
  ) async throws -> ConfirmedMission {
    choiceReminderAbortCount += 1
    guard let choiceReminderMissionResponse else {
      throw CoreClientError.contractViolation("Choice Reminder Mission was not configured.")
    }
    if choiceReminderAbortFailuresRemaining > 0 {
      choiceReminderAbortFailuresRemaining -= 1
      throw CoreClientError.contractViolation(
        "Choice Reminder abort was temporarily unavailable.")
    }
    choiceReminderDispatchStartResponse = ReminderDispatchStart(
      mission: choiceReminderMissionResponse, executeNow: true)
    if loseNextChoiceReminderAbortResponseAfterCommit {
      loseNextChoiceReminderAbortResponseAfterCommit = false
      throw CoreClientError.contractViolation(
        "Choice Reminder abort response was lost after commit.")
    }
    return choiceReminderMissionResponse
  }

  func recordChoiceReminderMirror(
    confirmationId _: String, links: [ReminderLink], proof _: BrokerRuntimeState
  ) async throws -> ConfirmedMission {
    guard let dispatched = choiceReminderMissionResponse else {
      throw CoreClientError.contractViolation("Choice Reminder dispatch was not configured.")
    }
    let mission = ConfirmedMission(
      missionId: dispatched.missionId, title: dispatched.title,
      workItems: dispatched.workItems,
      reminderAuthorization: dispatched.reminderAuthorization,
      reminderDispatch: dispatched.reminderDispatch, reminderLinks: links,
      choiceConfirmationId: dispatched.choiceConfirmationId,
      choicePayloadDigest: dispatched.choicePayloadDigest,
      choiceReminderPayloadDigest: dispatched.choiceReminderPayloadDigest,
      choiceReminderItems: dispatched.choiceReminderItems)
    choiceReminderMissionResponse = mission
    choiceReminderDispatchStartResponse = ReminderDispatchStart(
      mission: mission, executeNow: false)
    dashboardConfirmedMission = mission
    return mission
  }

  func choiceMarkdownReceiptCleanupAvailability() async throws
    -> ChoiceMarkdownReceiptCleanupAvailability
  {
    ChoiceMarkdownReceiptCleanupAvailability(available: choiceMarkdownReceiptCleanupAvailable)
  }

  func cleanupChoiceMarkdownReceipt() async throws -> ChoiceLoopSnapshot {
    choiceMarkdownReceiptCleanupCount += 1
    guard let choiceLoopResponse else {
      throw CoreClientError.contractViolation("Choice Markdown cleanup was not configured.")
    }
    return choiceLoopResponse
  }

  func choiceMarkdownReceiptCleanupInvocations() -> Int {
    choiceMarkdownReceiptCleanupCount
  }

  func reconcileChoiceMarkdown(proof _: BrokerRuntimeState) async throws -> ChoiceLoopSnapshot {
    if rejectNextChoiceMarkdownReconcile {
      rejectNextChoiceMarkdownReconcile = false
      throw CoreClientError.contractViolation("Choice Markdown journal is unavailable.")
    }
    guard let choiceLoopResponse else {
      throw CoreClientError.contractViolation("Choice Markdown journal was not configured.")
    }
    return choiceLoopResponse
  }

  func beginChoice(_ parameters: ChoiceBeginParameters) async throws -> ChoiceBeginAccepted {
    choiceBeginParameters.append(parameters)
    guard let choiceBeginAccepted else {
      throw CoreClientError.contractViolation("Choice begin was not configured.")
    }
    return choiceBeginAccepted
  }

  func selectChoice(_ selection: ChoiceSelection, proof _: BrokerRuntimeState) async throws
    -> ChoiceLoopSnapshot
  {
    choiceSelections.append(selection)
    guard let choiceLoopResponse else {
      throw CoreClientError.contractViolation("Choice selection was not configured.")
    }
    if choiceLoopResponse.session.state == "refining" {
      let session = choiceLoopResponse.session
      return ChoiceLoopSnapshot(
        session: ChoiceSession(
          id: session.id, state: session.state, revision: session.revision,
          modelSelectionState: session.modelSelectionState,
          communicationProfileRevision: session.communicationProfileRevision,
          activeChoiceSetId: session.activeChoiceSetId,
          activeInterpretationRevision: session.activeInterpretationRevision,
          openedAtMs: session.openedAtMs, lastInputAtMs: selection.selectedAtMs,
          softIdleAtMs: selection.selectedAtMs + 1_800_000,
          staleReviewAtMs: selection.selectedAtMs + 86_400_000,
          primaryDeliveryBindingId: session.primaryDeliveryBindingId,
          pendingConfirmationId: session.pendingConfirmationId,
          backgroundMissionIds: session.backgroundMissionIds),
        activeBatch: choiceLoopResponse.activeBatch,
        interpretation: choiceLoopResponse.interpretation,
        activeChoiceSet: choiceLoopResponse.activeChoiceSet, lastSelection: selection,
        pendingRefinementOperation: choiceLoopResponse.pendingRefinementOperation.map {
          ChoiceRefinementOperation(
            id: $0.id, selectionId: selection.id, choiceSessionId: $0.choiceSessionId,
            sourceEnvelopeId: $0.sourceEnvelopeId,
            conversationTurnBatchId: $0.conversationTurnBatchId,
            expectedSessionRevision: $0.expectedSessionRevision,
            expectedGeneration: $0.expectedGeneration, modelProvenance: $0.modelProvenance,
            sourceManifestDigest: $0.sourceManifestDigest, personaRevision: $0.personaRevision,
            dRequestId: $0.dRequestId,
            dInputDigest: $0.dInputDigest, createdAtMs: $0.createdAtMs)
        },
        confirmation: choiceLoopResponse.confirmation,
        documentManifest: choiceLoopResponse.documentManifest)
    }
    return choiceLoopResponse
  }

  func selectChoiceD(
    _ input: ChoiceDInput, proof _: BrokerRuntimeState
  ) async throws -> ChoiceLoopSnapshot {
    choiceDInputs.append(input)
    if let nextChoiceDInputError {
      self.nextChoiceDInputError = nil
      throw nextChoiceDInputError
    }
    if let error = nextChoiceDInputErrorAfterAcceptance {
      nextChoiceDInputErrorAfterAcceptance = nil
      let beforeThrow = beforeNextChoiceDInputErrorAfterAcceptance
      beforeNextChoiceDInputErrorAfterAcceptance = nil
      guard let snapshot = choiceLoopResponse,
        let operation = snapshot.pendingRefinementOperation
      else {
        throw CoreClientError.contractViolation("Choice D response loss was not configured.")
      }
      let acceptedOperation = ChoiceRefinementOperation(
        id: operation.id, selectionId: operation.selectionId,
        choiceSessionId: operation.choiceSessionId,
        sourceEnvelopeId: operation.sourceEnvelopeId,
        conversationTurnBatchId: operation.conversationTurnBatchId,
        expectedSessionRevision: operation.expectedSessionRevision,
        expectedGeneration: operation.expectedGeneration,
        modelProvenance: operation.modelProvenance,
        sourceManifestDigest: operation.sourceManifestDigest,
        personaRevision: operation.personaRevision,
        dRequestId: input.requestId,
        dInputDigest: String(repeating: "a", count: 64), createdAtMs: operation.createdAtMs)
      choiceLoopResponse = ChoiceLoopSnapshot(
        session: snapshot.session, activeBatch: snapshot.activeBatch,
        interpretation: snapshot.interpretation, activeChoiceSet: snapshot.activeChoiceSet,
        lastSelection: snapshot.lastSelection, pendingRefinementOperation: acceptedOperation,
        confirmation: snapshot.confirmation, documentManifest: snapshot.documentManifest)
      beforeThrow?()
      // Let an injected Core-termination event advance AppModel's generation
      // before the old RPC returns its ambiguous transport error.
      await Task.yield()
      throw error
    }
    if let unexpected = nextChoiceDUnexpectedResponseAfterAcceptance {
      nextChoiceDUnexpectedResponseAfterAcceptance = nil
      guard let snapshot = choiceLoopResponse,
        let operation = snapshot.pendingRefinementOperation
      else {
        throw CoreClientError.contractViolation("Choice D unexpected response was not configured.")
      }
      let acceptedOperation = ChoiceRefinementOperation(
        id: operation.id, selectionId: operation.selectionId,
        choiceSessionId: operation.choiceSessionId,
        sourceEnvelopeId: operation.sourceEnvelopeId,
        conversationTurnBatchId: operation.conversationTurnBatchId,
        expectedSessionRevision: operation.expectedSessionRevision,
        expectedGeneration: operation.expectedGeneration,
        modelProvenance: operation.modelProvenance,
        sourceManifestDigest: operation.sourceManifestDigest,
        personaRevision: operation.personaRevision,
        dRequestId: input.requestId,
        dInputDigest: String(repeating: "a", count: 64), createdAtMs: operation.createdAtMs)
      choiceLoopResponse = ChoiceLoopSnapshot(
        session: snapshot.session, activeBatch: snapshot.activeBatch,
        interpretation: snapshot.interpretation, activeChoiceSet: snapshot.activeChoiceSet,
        lastSelection: snapshot.lastSelection, pendingRefinementOperation: acceptedOperation,
        confirmation: snapshot.confirmation, documentManifest: snapshot.documentManifest)
      return unexpected
    }
    guard let choiceLoopResponse else {
      throw CoreClientError.contractViolation("Choice D selection was not configured.")
    }
    return choiceLoopResponse
  }

  func resumeChoice(proof _: BrokerRuntimeState) async throws -> ChoiceLoopSnapshot {
    choiceResumeCount += 1
    choiceCallTrace.append("resume")
    guard let choiceResumeResponse else {
      throw CoreClientError.contractViolation("Choice resume was not configured.")
    }
    if let error = nextChoiceResumeErrorAfterAcceptance {
      nextChoiceResumeErrorAfterAcceptance = nil
      choiceLoopResponse = choiceResumeResponse
      throw error
    }
    return choiceResumeResponse
  }

  func confirmChoice(
    _ confirmation: ChoiceConsolidatedConfirmation, proof _: BrokerRuntimeState
  ) async throws -> ChoiceLoopSnapshot {
    choiceConfirmations.append(confirmation)
    guard let choiceLoopResponse else {
      throw CoreClientError.contractViolation("Choice confirmation was not configured.")
    }
    return choiceLoopResponse
  }

  func cancelChoice(proof _: BrokerRuntimeState) async throws -> ChoiceLoopSnapshot {
    guard let choiceCancellationResponse else {
      throw CoreClientError.contractViolation("Choice cancellation was not configured.")
    }
    if let error = nextChoiceCancellationErrorAfterAcceptance {
      nextChoiceCancellationErrorAfterAcceptance = nil
      choiceLoopResponse = choiceCancellationResponse
      throw error
    }
    return choiceCancellationResponse
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
    discordSetupPollCount += 1
    return DiscordSetupPollResponse(
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
  ) throws {
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
    if loseNextDiscordConfirmResponse {
      loseNextDiscordConfirmResponse = false
      throw CoreClientError.contractViolation("Discord confirmation response was lost.")
    }
  }

  func startDiscord(
    token: String, proof _: BrokerRuntimeState
  ) async throws -> ChannelStatusResponse {
    if let gate = nextDiscordStartGate {
      nextDiscordStartGate = nil
      try await gate.wait()
    }
    if rejectNextDiscordStartUnavailable {
      rejectNextDiscordStartUnavailable = false
      throw CoreClientError.remote(code: -32_020, message: "Channel listener unavailable")
    }
    discordStartTokens.append(token)
    if discordStartDelay > .zero { try await Task.sleep(for: discordStartDelay) }
    if discordSessionRunning,
      channelPollConnectionStatus == "faulted"
        || channelPollConnectionStatus == "disconnected"
    {
      discordSessionRunning = false
      channelPollConnectionStatus = "connecting"
    }
    if !discordSessionRunning {
      discordSessionRunning = true
      discordSessionStartCount += 1
    }
    if loseNextDiscordStartResponse {
      loseNextDiscordStartResponse = false
      throw CoreClientError.contractViolation("Discord start response was lost.")
    }
    return ChannelStatusResponse(status: "connecting")
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

  func setIMessageChatsToReturn(_ chats: [IMessageChat]) {
    iMessageChatsToReturn = chats
  }

  func listPreparedIMessageChats(proof _: BrokerRuntimeState) throws -> [IMessageChat] {
    guard iMessageDiscoveryPrepared else {
      throw CoreClientError.contractViolation("iMessage discovery was not prepared.")
    }
    iMessageChatsListCount += 1
    iMessageDiscoveryPrepared = false
    return iMessageChatsToReturn
  }

  func activateIMessage(proof _: BrokerRuntimeState) async throws -> ChannelStatusResponse {
    if iMessageActivationDelay > .zero {
      try await Task.sleep(for: iMessageActivationDelay)
    }
    if rejectNextIMessageActivation {
      rejectNextIMessageActivation = false
      throw CoreClientError.remote(code: -32_020, message: "Channel listener unavailable")
    }
    iMessageStartCount += 1
    iMessageSessionRunning = true
    return ChannelStatusResponse(status: "connected")
  }

  func channelStatus(_ channel: ChannelKind) -> ChannelStatusResponse {
    switch channel {
    case .iMessage:
      return ChannelStatusResponse(
        status: iMessageSessionRunning ? "connected" : "disconnected")
    case .discord:
      discordStatusReadCount += 1
      if !queuedDiscordStatusResponses.isEmpty {
        channelPollConnectionStatus = queuedDiscordStatusResponses.removeFirst()
      }
      return ChannelStatusResponse(
        status: discordSessionRunning ? channelPollConnectionStatus : "disconnected")
    }
  }

  func stopChannel(_ channel: ChannelKind) -> ChannelStatusResponse {
    stoppedChannels.append(channel)
    if channel == .discord { discordSessionRunning = false }
    if channel == .iMessage {
      iMessageDiscoveryPrepared = false
      iMessageSessionRunning = false
    }
    return ChannelStatusResponse(status: "disconnected")
  }

  func pollChannel(
    _ channel: ChannelKind,
    modelWorkAllowed: Bool,
    proof _: BrokerRuntimeState
  ) async throws -> ChannelPollResponse {
    channelPollInvocationCount += 1
    channelPollFenceStates.append(activeGenerationFence != nil)
    channelPollModelWorkAllowances.append(modelWorkAllowed)
    channelPollCount += 1
    if let error = nextChannelPollErrors.removeValue(forKey: channel) {
      throw error
    }
    if let error = nextChannelPollError {
      nextChannelPollError = nil
      throw error
    }
    if rejectNextChannelPoll {
      rejectNextChannelPoll = false
      throw CoreClientError.contractViolation("Channel poll failed.")
    }
    let queuedResponse: ChannelPollResponse?
    if var channelResponses = queuedChannelPollResponsesByChannel[channel],
      !channelResponses.isEmpty
    {
      queuedResponse = channelResponses.removeFirst()
      queuedChannelPollResponsesByChannel[channel] = channelResponses
    } else {
      queuedResponse =
        queuedChannelPollResponses.isEmpty ? nil : queuedChannelPollResponses.removeFirst()
    }
    if channelPollDelay > .zero {
      let delay = channelPollDelay
      await Task.detached { try? await Task.sleep(for: delay) }.value
    }
    if loseNextChannelPollResponse {
      loseNextChannelPollResponse = false
      throw CoreClientError.requestTimedOut
    }
    if let queuedResponse {
      return queuedResponse
    }
    defer {
      queuedChannelSuggestion = nil
      queuedChannelMissionEvent = nil
    }
    return ChannelPollResponse(
      connectionStatus: channelPollConnectionStatus,
      eventStatus:
        queuedChannelMissionEvent == nil
        ? (queuedChannelSuggestion == nil ? "idle" : "ready")
        : queuedChannelMissionEventStatus,
      suggestion: queuedChannelSuggestion,
      missionEvent: queuedChannelMissionEvent
    )
  }

  func acknowledgeChannelFailure(
    _ incident: ChannelFailureIncident,
    acknowledgedAtMs: Int64,
    proof: BrokerRuntimeState
  ) async throws -> ChannelFailureIncident {
    channelFailureAcknowledgementCount += 1
    if let gate = nextChannelFailureAcknowledgementGate {
      nextChannelFailureAcknowledgementGate = nil
      try await gate.wait()
    }
    if channelFailureAcknowledgementDelay > .zero {
      let delay = channelFailureAcknowledgementDelay
      channelFailureAcknowledgementDelay = .zero
      await Task.detached { try? await Task.sleep(for: delay) }.value
    }
    guard control.enabled,
      proof.authorization.enabled,
      proof.authorization.revision == control.revision
    else {
      throw CoreClientError.contractViolation(
        "Incident acknowledgement lost its protected On revision."
      )
    }
    if rejectNextChannelFailureAcknowledgement {
      rejectNextChannelFailureAcknowledgement = false
      throw CoreClientError.contractViolation("Incident acknowledgement failed closed.")
    }
    if rejectedChannelFailureIncidentIds.remove(incident.incidentId) != nil {
      throw CoreClientError.contractViolation("Incident acknowledgement failed closed.")
    }
    let acknowledged = ChannelFailureIncident(
      incidentId: incident.incidentId,
      channel: incident.channel,
      failureClass: incident.failureClass,
      occurredAtMs: incident.occurredAtMs,
      runtimeRevision: incident.runtimeRevision,
      dispatchStateHash: incident.dispatchStateHash,
      sourceAuditAnchor: incident.sourceAuditAnchor,
      incidentAuditAnchor: incident.incidentAuditAnchor,
      acknowledgement: ChannelFailureAcknowledgement(
        acknowledgedAtMs: max(acknowledgedAtMs, incident.occurredAtMs),
        runtimeRevision: proof.authorization.revision,
        auditAnchor: ChannelFailureAuditAnchor(
          sequence: incident.incidentAuditAnchor.sequence + 1,
          entryHash: String(repeating: "d", count: 64),
          signatureHex: String(repeating: "4", count: 128)
        )
      )
    )
    dashboardChannelFailureIncidents = dashboardChannelFailureIncidents.map {
      $0.incidentId == acknowledged.incidentId ? acknowledged : $0
    }
    if let dashboard = dashboardAfterNextChannelFailureAcknowledgement {
      dashboardAfterNextChannelFailureAcknowledgement = nil
      dashboardOverride = dashboard
    }
    return acknowledged
  }

  func sendChannelMessage(
    missionId: String,
    routeId: String,
    kind: ChannelMessageKind,
    content: String,
    approvedAtMs: Int64,
    proof _: BrokerRuntimeState
  ) throws -> ChannelSendResponse {
    channelSendAttemptCount += 1
    if channelSends.contains(where: {
      $0.missionId == missionId && $0.routeId == routeId && $0.kind == kind
        && $0.content == content
    }) {
      return ChannelSendResponse(status: "sent", providerMessageId: "provider-message-1")
    }
    channelSends.append(
      TestChannelSend(
        missionId: missionId,
        routeId: routeId,
        kind: kind,
        content: content,
        approvedAtMs: approvedAtMs
      ))
    if loseNextChannelSendResponse {
      loseNextChannelSendResponse = false
      throw CoreClientError.requestTimedOut
    }
    if returnNextChannelSendUncertain {
      returnNextChannelSendUncertain = false
      return ChannelSendResponse(status: "needYou", providerMessageId: nil)
    }
    return ChannelSendResponse(status: "sent", providerMessageId: "provider-message-1")
  }

  func bindChannelRoute(
    _ approval: ChannelRouteApproval, proof _: BrokerRuntimeState
  ) throws -> ChannelRouteSet {
    guard var routeSet = dashboardChannelRouteSet,
      routeSet.missionId == approval.missionId,
      routeSet.revision == approval.expectedRouteSetRevision,
      approval.decision == .approve,
      let pairing = channelPairings[approval.channel],
      pairing.conversationId == approval.conversationId,
      pairing.ownerSenderId == approval.ownerSenderId,
      pairing.discord?.applicationId == approval.providerIdentity
    else {
      throw CoreClientError.contractViolation("Mock rejected route approval.")
    }
    channelRouteApprovals.append(approval)
    let revision = routeSet.revision + 1
    let route = ChannelRoute(
      routeId: "route-additional-\(approval.channel.rawValue)",
      role: .additional,
      channel: approval.channel,
      conversationId: approval.conversationId,
      ownerSenderId: approval.ownerSenderId,
      providerIdentity: approval.providerIdentity,
      sourceMessageId: pairing.discord?.setupSourceMessageId,
      allowedInboundClasses: approval.allowedInboundClasses,
      allowedOutboundClasses: approval.allowedOutboundClasses,
      revision: revision,
      approvalId: approval.approvalId,
      auditId: "audit-additional-\(revision)",
      boundAtMs: approval.decidedAtMs,
      updatedAtMs: approval.decidedAtMs
    )
    routeSet = ChannelRouteSet(
      missionId: routeSet.missionId,
      revision: revision,
      primaryRouteId: routeSet.primaryRouteId,
      routes: routeSet.routes + [route]
    )
    dashboardChannelRouteSet = routeSet
    return routeSet
  }

  func account(proof: BrokerRuntimeState) -> AccountState {
    proofNonces.append(proof.receipt.requestNonce ?? "")
    return loginCompleted ? .chatGpt(email: "owner@example.com", planType: "plus") : .notConnected
  }

  func beginLogin(proof _: BrokerRuntimeState) throws -> ChatGptLogin {
    guard codexInitialized, !loginCompleted else {
      throw CoreClientError.contractViolation("Login-only Codex was not initialized.")
    }
    loginBeginCount += 1
    return ChatGptLogin(authUrl: loginAuthURL, loginId: "login-1")
  }

  func awaitLogin(identifier _: String, proof _: BrokerRuntimeState) throws {
    loginAwaitCount += 1
    if rejectNextLoginAwait {
      rejectNextLoginAwait = false
      throw CoreClientError.contractViolation("Managed login was cancelled.")
    }
    loginCompleted = true
    codexInitialized = false
    leaseInstalled = false
  }

  func models(proof: BrokerRuntimeState) -> [GptModel] {
    proofNonces.append(proof.receipt.requestNonce ?? "")
    return loginCompleted ? modelCatalog : []
  }

  func modelSetup(proof: BrokerRuntimeState) -> ModelSetup {
    proofNonces.append(proof.receipt.requestNonce ?? "")
    choiceCallTrace.append("modelSetup")
    guard loginCompleted else {
      return ModelSetup(
        account: .notConnected,
        models: [],
        selection: nil,
        selectionStatus: .unselected,
        catalogSnapshotId: String(repeating: "0", count: 64),
        catalogFingerprint: String(repeating: "0", count: 64),
        catalogRevision: 1
      )
    }
    let binding = Self.catalogBinding(for: modelCatalog)
    let current =
      persistedModelSelection.flatMap { selection in
        guard selection.catalogFingerprint == binding.fingerprint,
          selection.catalogRevision == binding.revision,
          selection.accountDisplayClass == "chatgpt:plus",
          let model = modelCatalog.first(where: { $0.id == selection.modelId })
        else { return false }
        if selection.requestedEffort == "not_applicable" {
          return selection.actualEffort == "not_applicable"
            && model.supportedReasoningEfforts.isEmpty
        }
        return selection.actualEffort == selection.requestedEffort
          && model.supportedReasoningEfforts.contains(selection.requestedEffort)
      } ?? false
    return ModelSetup(
      account: .chatGpt(email: "owner@example.com", planType: "plus"),
      models: modelCatalog,
      selection: persistedModelSelection,
      selectionStatus: persistedModelSelection == nil
        ? .unselected : (current ? .current : .unavailable),
      catalogSnapshotId: Self.catalogSnapshotId(for: binding),
      catalogFingerprint: binding.fingerprint,
      catalogRevision: binding.revision
    )
  }

  func selectModel(
    modelId: String,
    requestedEffort: String,
    catalogSnapshotId: String,
    catalogFingerprint: String,
    catalogRevision: UInt64,
    proof _: BrokerRuntimeState
  ) throws -> ModelSelection {
    guard loginCompleted,
      let model = modelCatalog.first(where: { $0.id == modelId })
    else {
      throw CoreClientError.contractViolation("Mock model is unavailable.")
    }
    let validEffort =
      model.supportedReasoningEfforts.isEmpty
      ? requestedEffort == "not_applicable"
      : model.supportedReasoningEfforts.contains(requestedEffort)
    guard validEffort else {
      throw CoreClientError.contractViolation("Mock effort is unavailable.")
    }
    let binding = Self.catalogBinding(for: modelCatalog)
    guard catalogSnapshotId == Self.catalogSnapshotId(for: binding),
      catalogFingerprint == binding.fingerprint,
      catalogRevision == binding.revision
    else {
      throw CoreClientError.contractViolation("Mock catalog snapshot is stale.")
    }
    let selection = ModelSelection(
      id: "mock-selection-\(modelId)",
      modelId: modelId,
      requestedEffort: requestedEffort,
      actualEffort: requestedEffort,
      catalogFingerprint: binding.fingerprint,
      catalogRevision: binding.revision,
      accountDisplayClass: "chatgpt:plus",
      protocolSchemaRevision: 1
    )
    persistedModelSelection = selection
    modelSelectionWriteCount += 1
    return selection
  }

  func confirmSuggestion(
    identifier: String, reminderTarget: ReminderTarget
  ) throws -> ConfirmedMission {
    guard identifier == testSuggestionOneId || identifier == testSuggestionTwoId else {
      throw CoreClientError.contractViolation("Unexpected suggestion identifier.")
    }
    confirmationCount += 1
    guard
      reminderTarget
        == ReminderTarget(sourceIdentifier: "source-1", calendarIdentifier: "calendar-1")
    else {
      throw CoreClientError.contractViolation("Unexpected Reminder target.")
    }
    let isFirst = identifier == testSuggestionOneId
    let mission = testConfirmedMission(
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
    dashboardConfirmedMission = mission
    dashboardReceipt = nil
    dashboardNeedsYou = nil
    // Consuming a recovered historical suggestion transitions durable
    // dashboard state to the Mission. Leaving it behind would make a later
    // Receipt contradictory rather than exercising the receipt route.
    dashboardSuggestion = nil
    if let routeSet = dashboardChannelRouteSetAfterNextConfirmation {
      dashboardChannelRouteSetAfterNextConfirmation = nil
      dashboardChannelRouteSet = routeSet
    }
    return mission
  }

  func cancelMission(
    identifier: String, proof: BrokerRuntimeState
  ) throws -> MissionCancellation {
    missionCancellationCount += 1
    proofNonces.append(proof.receipt.requestNonce ?? "")
    if rejectNextMissionCancellation {
      rejectNextMissionCancellation = false
      throw CoreClientError.contractViolation("Mission cancellation failed closed.")
    }
    if completeBeforeNextMissionCancellation {
      completeBeforeNextMissionCancellation = false
      dashboardConfirmedMission = nil
      dashboardNeedsYou = nil
      dashboardChannelRouteSet = nil
      dashboardReceipt = MissionReceipt(
        id: "receipt-race",
        missionId: identifier,
        summary: "Completed before cancellation",
        actualModel: "gpt-test-model",
        evidenceIds: ["evidence-race"],
        outputHashes: [],
        completedAtMs: 20
      )
      throw CoreClientError.contractViolation("Mission already completed.")
    }
    guard
      dashboardConfirmedMission?.missionId == identifier
        || dashboardNeedsYou?.missionId == identifier
    else {
      throw CoreClientError.contractViolation("Mock Mission is not cancellable.")
    }
    if dashboardConfirmedMission?.missionId == identifier {
      dashboardConfirmedMission = nil
    }
    if dashboardNeedsYou?.missionId == identifier {
      dashboardNeedsYou = nil
    }
    if dashboardChannelRouteSet?.missionId == identifier {
      dashboardChannelRouteSet = nil
    }
    let result = MissionCancellation(
      missionId: identifier,
      status: "cancelled",
      auditAnchor: MissionAuditAnchor(
        sequence: 50 + Int64(missionCancellationCount),
        entryHash: String(repeating: "a", count: 64),
        signatureHex: String(repeating: "b", count: 128)
      )
    )
    if loseNextMissionCancellationResponse {
      loseNextMissionCancellationResponse = false
      throw CoreClientError.requestTimedOut
    }
    return result
  }

  func completeReminderMission(
    identifier: String,
    completions: [ReminderCompletionInput],
    receiptReturnApprovedAtMs: Int64?,
    receiptReturnRouteId: String?
  ) throws -> MissionReceipt {
    guard identifier == "mission-1" || identifier == "mission-2" else {
      throw CoreClientError.contractViolation("Unexpected Mission identifier.")
    }
    reminderCompletionPayloads.append(completions)
    receiptReturnApprovals.append(receiptReturnApprovedAtMs)
    receiptReturnRouteIds.append(receiptReturnRouteId)
    let receipt = MissionReceipt(
      id: identifier == "mission-1" ? "receipt-1" : "receipt-2",
      missionId: identifier,
      summary: "Completed Plan the day",
      actualModel: "gpt-test-model",
      evidenceIds: invalidCompletionReceipt
        ? [] : completions.map { "evidence-\($0.workItemId)" },
      outputHashes: [],
      completedAtMs: 10
    )
    if let next = dashboardAfterNextCompletion {
      dashboardAfterNextCompletion = nil
      dashboardOverride = next
    } else {
      dashboardConfirmedMission = nil
      dashboardNeedsYou = nil
      dashboardReceipt = receipt
    }
    return receipt
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
  func awaitLogin(identifier _: String, proof _: BrokerRuntimeState) {}
  func models(proof _: BrokerRuntimeState) -> [GptModel] { [] }
  func confirmSuggestion(
    identifier _: String, reminderTarget _: ReminderTarget
  ) throws -> ConfirmedMission {
    throw CoreClientError.contractViolation("Unexpected Mission confirmation.")
  }
  func completeReminderMission(
    identifier _: String,
    completions _: [ReminderCompletionInput],
    receiptReturnApprovedAtMs _: Int64?,
    receiptReturnRouteId _: String?
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
  func awaitLogin(identifier _: String, proof _: BrokerRuntimeState) {}
  func models(proof _: BrokerRuntimeState) -> [GptModel] { [] }
}

private actor MockBroker: BrokerRuntimeServing {
  var control: RuntimeControlAuthorization?
  var receipt: RuntimeControlReceipt?
  var appliedValues: [Bool] = []
  var leaseAcquireCount = 0
  var durableLeaseGeneration = 0
  var durableLeaseRotationCount = 0
  var runtimeHomePrepareCount = 0
  var rejectFurtherLeaseAcquisition = false
  var loseNextLeaseAcquireResponseAfterPersistence = false
  var rejectNextProvision = false
  var rejectNextOffBeforePersistence = false
  var loseNextOffResponseAfterPersistence = false
  var delayAndRejectNextOnBeforePersistence = false

  func rejectSubsequentLeaseAcquisition() { rejectFurtherLeaseAcquisition = true }
  func loseNextLeaseAcquireResponse() {
    loseNextLeaseAcquireResponseAfterPersistence = true
  }
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

  func prepareCodexRuntimeHome() -> String { "/test/runtime/CodexHome" }

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

private actor FailingRuntimeHomeBroker: BrokerRuntimeServing {
  var prepareCount = 0

  func provision(coreIdentity _: CoreEffectIdentity) throws -> EnrolledBrokerTrustAnchor {
    try testBrokerAnchor()
  }

  func prepareCodexRuntimeHome() throws -> String {
    prepareCount += 1
    throw CoreClientError.contractViolation("Runtime home preparation failed.")
  }

  func acquireCoreLease(
    coreIdentity _: CoreEffectIdentity, codexProcessIdentifier _: Int32
  ) throws -> Data {
    throw CoreClientError.contractViolation("Unexpected lease acquisition.")
  }

  func status(challenge _: String) -> BrokerRuntimeState? { nil }

  func apply(_: RuntimeControlAuthorization) throws -> RuntimeControlReceipt {
    throw CoreClientError.contractViolation("Unexpected runtime apply.")
  }
}

extension MockCore {
  func prepareCodexRuntime() throws -> Int32 {
    codexPrepareCount += 1
    if rejectNextCodexPrepare {
      rejectNextCodexPrepare = false
      throw CoreClientError.contractViolation("Model runtime preparation failed.")
    }
    candidateBrokerBound = false
    return 99
  }
  func prepareCodexLoginRuntime() -> Int32 {
    codexLoginPrepareCount += 1
    codexInitialized = false
    leaseInstalled = false
    candidateBrokerBound = false
    return 100
  }
  func bindCodexCandidateForBroker() {
    codexCandidateBindCount += 1
    candidateBrokerBound = true
  }
  func initializeCodexRuntime() throws {
    guard candidateBrokerBound, leaseInstalled else {
      throw CoreClientError.contractViolation("Codex initialized before lease installation.")
    }
    guard !codexInitialized else {
      throw CoreClientError.contractViolation("Codex initialized more than once.")
    }
    codexInitializeCount += 1
    codexInitialized = true
  }
  func abortCodexCandidate() {
    codexAbortCount += 1
    abortedCandidateBoundStates.append(candidateBrokerBound)
    codexInitialized = false
    leaseInstalled = false
    candidateBrokerBound = false
  }
  func installCoreLease(_: Data) throws {
    guard candidateBrokerBound else {
      throw CoreClientError.contractViolation("Lease installed before broker handoff.")
    }
    leaseInstalled = true
    if loseNextLeaseInstallResponse {
      loseNextLeaseInstallResponse = false
      throw CoreClientError.contractViolation("Core lease install response was lost.")
    }
  }
  func setLoginAuthURL(_ value: String) { loginAuthURL = value }
  func setModelCatalog(_ value: [GptModel]) { modelCatalog = value }
  func clearPersistedModelSelection() { persistedModelSelection = nil }
  func rejectNextLoginAwaitOperation() { rejectNextLoginAwait = true }
  func rejectNextModelRuntimePreparation() { rejectNextCodexPrepare = true }
  func loseNextCoreLeaseInstallResponse() { loseNextLeaseInstallResponse = true }
}

extension FailClosedOffCore {
  func prepareCodexRuntime() -> Int32 { 99 }
  func bindCodexCandidateForBroker() {}
  func initializeCodexRuntime() {}
  func abortCodexCandidate() {}
  func installCoreLease(_: Data) {}
}

extension DelayedSwitchCore {
  func prepareCodexRuntime() -> Int32 { 99 }
  func bindCodexCandidateForBroker() {}
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
    receiptReturnApprovedAtMs _: Int64?,
    receiptReturnRouteId _: String?
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
    case delayBeforeTarget
    case waitPrecommitUntilCancelled
    case tamperedMirror
    case failBeforeCommit
    case commitThenFailReadback
    case commitThenDelay
    case cancelBeforeCommit
  }

  var mode: Mode
  private(set) var executeCount = 0
  private(set) var recoverCount = 0
  private(set) var precommitCancelCount = 0
  private var storedLinks: [String: [ReminderLink]] = [:]

  init(mode: Mode = .complete) {
    self.mode = mode
  }

  func prepareTarget() async throws -> ReminderTarget {
    if mode == .delayBeforeTarget {
      try await Task.sleep(for: .seconds(1))
    }
    return ReminderTarget(sourceIdentifier: "source-1", calendarIdentifier: "calendar-1")
  }

  func executeInitialMirror(_ start: ReminderDispatchStart) async throws -> [ReminderLink] {
    let mission = start.mission
    executeCount += 1
    if mode == .waitPrecommitUntilCancelled {
      do {
        try await Task.sleep(for: .seconds(60))
      } catch is CancellationError {
        precommitCancelCount += 1
        throw RemindersClientError.cancelledBeforeCommit
      }
    }
    if mode == .cancelBeforeCommit {
      precommitCancelCount += 1
      throw RemindersClientError.cancelledBeforeCommit
    }
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
    if mode == .tamperedMirror {
      throw RemindersClientError.reminderChanged(mission.title)
    }
    guard let links = storedLinks[mission.missionId] else {
      throw RemindersClientError.mirrorAbsent(mission.title)
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
  func prepareCodexRuntimeHome() -> String {
    runtimeHomePrepareCount += 1
    return "/test/runtime/CodexHome"
  }

  func acquireCoreLease(
    coreIdentity _: CoreEffectIdentity, codexProcessIdentifier _: Int32
  ) throws -> Data {
    leaseAcquireCount += 1
    if rejectFurtherLeaseAcquisition {
      throw CoreClientError.contractViolation("The existing Codex process is unavailable.")
    }
    if durableLeaseGeneration > 0 {
      durableLeaseRotationCount += 1
    }
    durableLeaseGeneration += 1
    let lease = Data("lease-\(durableLeaseGeneration)".utf8)
    if loseNextLeaseAcquireResponseAfterPersistence {
      loseNextLeaseAcquireResponseAfterPersistence = false
      throw CoreClientError.contractViolation(
        "Broker lease response was lost after durable exact-lease rotation."
      )
    }
    return lease
  }
}

extension DelayedProofBroker {
  func prepareCodexRuntimeHome() -> String { "/test/runtime/CodexHome" }

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

private final class CoreTerminationEmitter: @unchecked Sendable {
  let stream: AsyncStream<CoreTerminationEvent>
  private let continuation: AsyncStream<CoreTerminationEvent>.Continuation

  init() {
    let pair = AsyncStream<CoreTerminationEvent>.makeStream(
      bufferingPolicy: .bufferingNewest(8))
    stream = pair.stream
    continuation = pair.continuation
  }

  func send(_ event: CoreTerminationEvent) {
    continuation.yield(event)
  }

  deinit {
    continuation.finish()
  }
}

private final class NonCooperativeRpcGate: @unchecked Sendable {
  private let lock = NSLock()
  private var continuation: CheckedContinuation<Void, Error>?
  private var interrupted = false
  private var entered = false

  var isWaiting: Bool {
    lock.withLock { entered && continuation != nil }
  }

  var wasInterrupted: Bool {
    lock.withLock { interrupted }
  }

  func wait() async throws {
    try await withCheckedThrowingContinuation { continuation in
      let reject = lock.withLock { () -> Bool in
        entered = true
        if interrupted { return true }
        self.continuation = continuation
        return false
      }
      if reject {
        continuation.resume(throwing: CoreClientError.processTerminated)
      }
    }
  }

  func interrupt() {
    let pending = lock.withLock { () -> CheckedContinuation<Void, Error>? in
      interrupted = true
      defer { continuation = nil }
      return continuation
    }
    pending?.resume(throwing: CoreClientError.processTerminated)
  }

  func resume() {
    let pending = lock.withLock { () -> CheckedContinuation<Void, Error>? in
      defer { continuation = nil }
      return continuation
    }
    pending?.resume()
  }
}

private struct FailingDiscordTokenStore: DiscordTokenStoring {
  func save(_: String) throws { throw CoreClientError.keychain(errSecMissingEntitlement) }
  func load() throws -> String? { nil }
  func delete() throws {}
}

@MainActor
private func makePairedRecoveryModel(
  core: MockCore,
  shutdownCore: @escaping @Sendable () -> Bool = { true },
  channelPollInterval: Duration = .seconds(1)
) async throws -> (AppModel, CoreTerminationEmitter) {
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  return (
    AppModel(
      core: core,
      broker: MockBroker(),
      discordTokenStore: tokenStore,
      registerLoginItem: {},
      coreTerminationEvents: events.stream,
      shutdownCore: shutdownCore,
      channelPollInterval: channelPollInterval
    ),
    events
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
func globalOffAfterLiveListenerShutdownReenrollsReplacementBeforePreparingOff() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let shutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      shutdowns.withLock { $0 += 1 }
      return true
    }
  )

  await model.updateEnabled(true)
  await model.refreshDashboard()
  for _ in 0..<75 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(await core.channelPairings.count == 2)

  await core.simulateCoreReplacement()
  await core.requireBrokerEnrollmentBeforeOff()
  await model.updateEnabled(false)

  #expect(shutdowns.value == 1)
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.errorMessage == nil)
  #expect(await core.brokerEnrollmentInstallCount == 2)
  #expect(await core.offPrepareCount == 1)
  #expect(await broker.appliedValues == [true, false])
  #expect(await core.channelPairings.count == 2)
  #expect(await core.codexInitializeCount == 1)
  #expect(await broker.leaseAcquireCount == 1)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func globalOffDoesNotPrepareOrProvisionWhenExactLiveCoreShutdownCannotBeProven() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let events = CoreTerminationEmitter()
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  let model = AppModel(
    core: core,
    broker: broker,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: { false }
  )

  await model.updateEnabled(true)
  await model.refreshDashboard()
  #expect(model.iMessageStatus == "connected")
  let enrollmentCount = await core.brokerEnrollmentInstallCount

  await model.updateEnabled(false)

  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.errorMessage == "OpenOpen could not verify that the previous Core stopped.")
  #expect(await core.offPrepareCount == 0)
  #expect(await core.brokerEnrollmentInstallCount == enrollmentCount)
  #expect(await broker.appliedValues == [true])
  #expect(await core.channelPairings.count == 1)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func failedGlobalOffShutdownRetriesOnTheSameModelBeforeAnyProtectedMutation() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let shutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      shutdowns.withLock { $0 += 1 }
      return shutdowns.value > 1
    }
  )

  await model.updateEnabled(true)
  await model.refreshDashboard()
  let enrollmentCount = await core.brokerEnrollmentInstallCount

  await model.updateEnabled(false)

  #expect(shutdowns.value == 1)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.errorMessage == "OpenOpen could not verify that the previous Core stopped.")
  #expect(await core.offPrepareCount == 0)
  #expect(await core.brokerEnrollmentInstallCount == enrollmentCount)
  #expect(await broker.appliedValues == [true])
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  await core.simulateCoreReplacement()
  await core.requireBrokerEnrollmentBeforeOff()
  await model.updateEnabled(false)

  #expect(shutdowns.value == 2)
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.errorMessage == nil)
  #expect(await core.offPrepareCount == 1)
  #expect(await core.brokerEnrollmentInstallCount == enrollmentCount + 1)
  #expect(await broker.appliedValues == [true, false])
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func newerOnAfterFailedOffShutdownRevalidatesBothDurableListeners() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let shutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      shutdowns.withLock { $0 += 1 }
      return false
    }
  )

  await model.updateEnabled(true)
  await model.refreshDashboard()
  for _ in 0..<75 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }
  let iMessageStarts = await core.iMessageStartCount
  let discordStarts = await core.discordSessionStartCount

  await model.updateEnabled(false)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(!model.modelEntryEnabled)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.discordStatus == "disconnected")

  await model.updateEnabled(true)
  for _ in 0..<75 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(shutdowns.value == 1)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.modelEntryEnabled)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(await broker.appliedValues == [true])
  #expect(await core.offPrepareCount == 0)
  #expect(await core.iMessageStartCount == iMessageStarts)
  #expect(await core.discordSessionStartCount == discordStarts)
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func persistedOffWithTwoPairingsRestoresEverythingBeforePublishingOn() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  await core.queueDiscordStatusResponses(["connecting", "reconnecting", "connected"])
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.refreshDashboard()
  #expect(model.runtimeDisplayState == .off)
  events.send(
    CoreTerminationEvent(generation: 1, reason: .explicitShutdown, exitStatus: 0))
  await Task.yield()

  let on = Task { await model.updateEnabled(true) }
  for _ in 0..<100 where await core.discordStatusReadCount == 0 {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(await core.discordStatusReadCount > 0)
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(!model.modelEntryEnabled)

  await on.value

  #expect(model.enabled)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.modelEntryEnabled)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(await core.iMessageStartCount == 1)
  #expect(await core.discordSessionStartCount == 1)
  // Account, catalog, and durable-selection provenance arrive in one atomic
  // Host snapshot. One fresh proof avoids a TOCTOU composition of separate
  // account and catalog reads during protected On restoration.
  #expect(await core.proofNonces.count == 1)
  #expect(await broker.appliedValues == [true])
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func cursorBearingDiscordStartupDrainsTypedRecoveryBeforePublishingConnected() async throws {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  await core.queueDiscordStatusResponses(["connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil)
  ])
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.modelEntryEnabled)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(await core.channelPollInvocationCount == 1)
  #expect(await core.channelPollFenceStates == [true])
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

}

@MainActor
@Test
func noCursorDiscordStartupReachesConnectedWithoutRecoveryPoll() async throws {
  let core = MockCore()
  await core.queueDiscordStatusResponses(["connecting", "connected"])
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.discordStatus == "connected")
  #expect(await core.channelPollInvocationCount == 0)
  #expect(await core.channelPollFenceStates.isEmpty)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func recoveredDuplicateAndCursorAreConsumedOnceBeforeNormalPolling() async throws {
  let core = MockCore()
  await core.queueDiscordStatusResponses(["connecting", "connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connecting", eventStatus: "ignored", suggestion: nil),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil),
  ])
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.discordStatus == "connected")
  #expect(await core.channelPollInvocationCount == 2)
  #expect(await core.channelPollFenceStates == [true, true])
  #expect(model.suggestion == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func existingFailedDispatchDoesNotBlockCursorRecoveryOrStartModelWork() async throws {
  let core = MockCore()
  await core.queueDiscordStatusResponses(["connecting", "connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connecting", eventStatus: "needYou", suggestion: nil,
      failureIncidents: [testChannelFailureIncident(channel: .discord)]),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil),
  ])
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.discordStatus == "connected")
  #expect(await core.channelPollFenceStates == [true, true])
  #expect(model.suggestion == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func recoveredNewerInboundWaitsForCursorThenRunsOnlyInNormalPolling() async throws {
  let correction = OutcomeSuggestion(
    id: testRecoveredSuggestionId,
    title: "Prepare the corrected demo checklist",
    whyNow: "The approved owner sent one bounded correction.",
    proposedSteps: ["Rehearse the opening", "Run OpenOpen", "Prepare a backup"],
    sourceRefs: ["discord:correction"]
  )
  let core = MockCore()
  await core.queueDiscordStatusResponses(["connecting", "connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connecting", eventStatus: "recovering", suggestion: nil),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "ready", suggestion: correction),
  ])
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.suggestion == nil)
  #expect(await core.channelPollFenceStates == [true, true])

  for _ in 0..<150 where model.suggestion != correction {
    try? await Task.sleep(for: .milliseconds(20))
  }
  let fenceStates = await core.channelPollFenceStates
  #expect(model.suggestion == correction)
  #expect(fenceStates.count >= 3)
  #expect(Array(fenceStates.prefix(2)) == [true, true])
  #expect(fenceStates.dropFirst(2).allSatisfy { !$0 })
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordRecoveryResponseLossRestartsWithoutPublishingOrReplaying() async throws {
  let core = MockCore()
  let (model, events) = try await makePairedRecoveryModel(core: core)
  await model.updateEnabled(true)
  #expect(model.runtimeRecoveryState == .ready)

  await core.simulateCoreReplacement()
  await core.queueDiscordStatusResponses([
    "connecting", "connecting", "connecting", "connecting", "faulted",
  ])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil),
  ])
  await core.loseNextChannelPollResponseAfterCommit()
  let invocationsBefore = await core.channelPollInvocationCount

  events.send(CoreTerminationEvent(generation: 1, reason: .transportFailure, exitStatus: 0))
  for _ in 0..<250 {
    let invocations = await core.channelPollInvocationCount
    if model.runtimeRecoveryState == .ready, invocations >= invocationsBefore + 2 { break }
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.discordStatus == "connected")
  #expect(await core.channelPollInvocationCount >= invocationsBefore + 2)
  #expect(model.suggestion == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordRecoveryAdapterFailurePausesWithoutConnectedOrModelWork() async throws {
  let core = MockCore()
  await core.queueDiscordStatusResponses(["connecting", "connecting", "faulted"])
  await core.failNextChannelPoll()
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.discordStatus == "paused")
  #expect(!model.modelEntryEnabled)
  #expect(model.suggestion == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func coreDeathDuringDiscordRecoveryRejectsLateResultAndUsesReplacementFence() async throws {
  let core = MockCore()
  let (model, events) = try await makePairedRecoveryModel(core: core)
  await model.updateEnabled(true)
  #expect(model.runtimeRecoveryState == .ready)

  let invocationsBefore = await core.channelPollInvocationCount
  await core.simulateCoreReplacement()
  await core.queueDiscordStatusResponses([
    "connecting", "connecting", "connecting", "connecting", "faulted",
  ])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil),
  ])
  await core.delayChannelPoll(by: .milliseconds(250))
  events.send(CoreTerminationEvent(generation: 1, reason: .transportFailure, exitStatus: 0))
  for _ in 0..<150 where await core.channelPollInvocationCount == invocationsBefore {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(await core.channelPollInvocationCount == invocationsBefore + 1)

  await core.invalidateNextFenceAtClose()
  await core.delayChannelPoll(by: .zero)
  events.send(CoreTerminationEvent(generation: 2, reason: .transportFailure, exitStatus: 0))

  var publishedFromLateGeneration = false
  for _ in 0..<250 {
    let invocations = await core.channelPollInvocationCount
    if model.runtimeRecoveryState == .ready, invocations < invocationsBefore + 2 {
      publishedFromLateGeneration = true
    }
    if model.runtimeRecoveryState == .ready, invocations >= invocationsBefore + 2 { break }
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(!publishedFromLateGeneration)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.discordStatus == "connected")
  #expect(await core.channelPollInvocationCount >= invocationsBefore + 2)
  #expect(model.suggestion == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func globalOffDuringDiscordRecoveryRejectsItsLateConnectedResponse() async throws {
  let core = MockCore()
  await core.queueDiscordStatusResponses(["connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil)
  ])
  await core.delayChannelPoll(by: .milliseconds(250))
  let (model, _) = try await makePairedRecoveryModel(core: core)

  let turningOn = Task { await model.updateEnabled(true) }
  for _ in 0..<100 where await core.channelPollInvocationCount == 0 {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(await core.channelPollInvocationCount == 1)
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(!model.modelEntryEnabled)

  await model.updateEnabled(false)
  await turningOn.value

  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.discordStatus == "disconnected")
  #expect(model.suggestion == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func changedPairingDuringDiscordRecoveryFailsClosedBeforeConnected() async throws {
  let core = MockCore()
  await core.queueDiscordStatusResponses(["connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil)
  ])
  await core.delayChannelPoll(by: .milliseconds(150))
  let (model, _) = try await makePairedRecoveryModel(core: core)

  let turningOn = Task { await model.updateEnabled(true) }
  for _ in 0..<100 where await core.channelPollInvocationCount == 0 {
    try? await Task.sleep(for: .milliseconds(10))
  }
  await core.setChannelPairing(
    ChannelPairing(
      channel: .discord,
      ownerSenderId: "discord-owner",
      conversationId: "changed-channel",
      discord: testDiscordPairing().discord,
      pairedAtMs: 3
    ))
  await turningOn.value

  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.discordStatus == "paused")
  #expect(!model.modelEntryEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func unexpectedModelPayloadDuringDiscordRecoveryFailsClosed() async throws {
  let unexpected = OutcomeSuggestion(
    id: "unexpected-recovery-model",
    title: "Must not publish",
    whyNow: "Recovery may not run the model.",
    proposedSteps: ["Stop"],
    sourceRefs: ["discord:unexpected"]
  )
  let core = MockCore()
  await core.queueDiscordStatusResponses(["connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "ready", suggestion: unexpected)
  ])
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.discordStatus == "paused")
  #expect(model.suggestion == nil)
  #expect(!model.modelEntryEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func recoveredMissionParticipationUsesThePersistedRouteSetBeforeConnected() async throws {
  let mission = testConfirmedMission()
  let routeSet = testChannelRouteSet(
    missionId: mission.missionId,
    channel: .discord,
    conversationId: "2002",
    ownerSenderId: "1001"
  )
  let event = ChannelMissionEvent(
    eventId: "mission-event-recovered",
    missionId: mission.missionId,
    missionRevision: 2,
    missionAnchorHash: String(repeating: "a", count: 64),
    routeId: "route-primary",
    routeSetRevision: 1,
    messageClass: .missionParticipation,
    channel: .discord,
    sourceMessageId: "discord-recovered-1",
    contentSha256: String(repeating: "b", count: 64),
    recordedAtMs: 3
  )
  let core = MockCore()
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routeSet)
  await core.queueDiscordStatusResponses(["connecting", "connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connecting", eventStatus: "missionUpdateRecovered",
      suggestion: nil, missionEvent: event),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "recovered", suggestion: nil),
  ])
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.latestChannelMissionEvent == event)
  #expect(model.confirmedMission?.missionId == mission.missionId)
  #expect(model.channelRouteSet == routeSet)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func recoveredMissionParticipationOutsidePersistedRouteFailsClosed() async throws {
  let mission = testConfirmedMission()
  let routeSet = testChannelRouteSet(
    missionId: mission.missionId,
    channel: .discord,
    conversationId: "2002",
    ownerSenderId: "1001"
  )
  let wrongRouteEvent = ChannelMissionEvent(
    eventId: "mission-event-wrong-route",
    missionId: mission.missionId,
    missionRevision: 2,
    missionAnchorHash: String(repeating: "a", count: 64),
    routeId: "route-not-approved",
    routeSetRevision: 1,
    messageClass: .missionParticipation,
    channel: .discord,
    sourceMessageId: "discord-recovered-wrong",
    contentSha256: String(repeating: "b", count: 64),
    recordedAtMs: 3
  )
  let core = MockCore()
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routeSet)
  await core.queueDiscordStatusResponses(["connecting", "connecting", "faulted"])
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connecting", eventStatus: "missionUpdateRecovered",
      suggestion: nil, missionEvent: wrongRouteEvent)
  ])
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.discordStatus == "paused")
  #expect(model.latestChannelMissionEvent == nil)
  #expect(!model.modelEntryEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func protectedOnRequiresAnExplicitModelAndEffortSelectionAfterManagedLogin() async {
  let core = MockCore(loginCompleted: false)
  let broker = MockBroker()
  let events = CoreTerminationEmitter()
  let openedURLs = LockIsolated<[URL]>([])
  let model = AppModel(
    core: core,
    broker: broker,
    registerLoginItem: {},
    openOfficialURL: { url in
      openedURLs.withLock { $0.append(url) }
      return true
    },
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)

  #expect(model.enabled)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(model.accountState == .notConnected)
  #expect(model.availableModels.isEmpty)
  #expect(model.accountSetupEnabled)
  #expect(!model.modelEntryEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  await model.connectChatGpt()

  #expect(model.errorMessage == nil)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(model.accountState == .chatGpt(email: "owner@example.com", planType: "plus"))
  #expect(model.availableModels.map(\.id) == ["gpt-test-model"])
  #expect(model.selectedModelId.isEmpty)
  #expect(!model.modelEntryEnabled)

  model.chooseModel("gpt-test-model")
  model.chooseModelEffort("high")
  await model.persistSelectedModel()

  #expect(model.errorMessage == nil)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.modelSelectionStatus == .current)
  #expect(model.modelEntryEnabled)
  #expect(openedURLs.value.map(\.absoluteString) == ["https://example.invalid"])
  #expect(await broker.appliedValues == [true])
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func protectedOffConvergesWithoutAccountOrModelReadiness() async {
  let core = MockCore(loginCompleted: false)
  let broker = MockBroker()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: broker,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.runtimeDisplayState == .turningOn)

  await model.updateEnabled(false)

  #expect(!model.enabled)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.accountState == .notConnected)
  #expect(!model.accountSetupEnabled)
  #expect(!model.modelEntryEnabled)
  #expect(await broker.appliedValues == [true, false])
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func protectedOnWaitsForAnExplicitSelectionWhenTheCurrentCatalogChanges() async {
  let core = MockCore(
    modelCatalog: [
      GptModel(
        id: "gpt-alternate-model", displayName: "Alternate test model",
        supportedReasoningEfforts: ["high"])
    ])
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.modelSelectionStatus == .current)
  #expect(model.modelEntryEnabled)

  await core.setModelCatalog([
    GptModel(
      id: "gpt-test-model", displayName: "Test model",
      supportedReasoningEfforts: ["high"])
  ])
  await model.refreshAccountAndModels()

  #expect(model.errorMessage == nil)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(model.modelSelectionStatus == .unavailable)
  #expect(model.selectedModelId.isEmpty)
  #expect(model.selectedModelEffort.isEmpty)
  #expect(!model.modelEntryEnabled)

  model.chooseModel("gpt-test-model")
  model.chooseModelEffort("high")
  await model.persistSelectedModel()

  #expect(model.errorMessage == nil)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.modelSelectionStatus == .current)
  #expect(model.modelEntryEnabled)
}

@MainActor
@Test
func sameModelIdWithNewCatalogFingerprintRequiresASecondExplicitSelection() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)
  #expect(model.modelSelectionStatus == .current)
  #expect(model.selectedModelId == "gpt-test-model")
  let firstFingerprint = model.catalogFingerprint

  await core.setModelCatalog([
    GptModel(
      id: "gpt-test-model", displayName: "Renamed test model",
      supportedReasoningEfforts: ["high"])
  ])
  await model.refreshAccountAndModels()

  #expect(model.catalogFingerprint != firstFingerprint)
  #expect(model.modelSelectionStatus == .unavailable)
  #expect(model.selectedModelId.isEmpty)
  #expect(model.selectedModelEffort.isEmpty)
  #expect(!model.modelEntryEnabled)

  model.chooseModel("gpt-test-model")
  model.chooseModelEffort("high")
  await model.persistSelectedModel()

  #expect(model.modelSelectionStatus == .current)
  #expect(model.modelEntryEnabled)
}

@MainActor
@Test
func staleHostCatalogSnapshotCannotPersistASelection() async {
  let core = MockCore()
  await core.clearPersistedModelSelection()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)
  #expect(model.modelSelectionCanBeSaved == false)
  model.chooseModel("gpt-test-model")
  model.chooseModelEffort("high")
  #expect(model.modelSelectionCanBeSaved)

  // The visible draft still names the old catalog, but a changed Host-owned
  // snapshot rejects it rather than trusting UI model metadata.
  await core.setModelCatalog([
    GptModel(
      id: "gpt-test-model", displayName: "Changed after display",
      supportedReasoningEfforts: ["high"])
  ])
  await model.persistSelectedModel()

  #expect(model.errorMessage != nil)
  #expect(await core.modelSelectionWriteCount == 0)
  #expect(!model.modelEntryEnabled)

  await model.refreshAccountAndModels()
  #expect(model.modelSelectionStatus == .unselected)
  #expect(model.selectedModelId.isEmpty)
  #expect(model.selectedModelEffort.isEmpty)
}

@MainActor
@Test
func accountLossClearsDraftSelectionWithoutErasingDurableAuditState() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)
  #expect(model.modelSelectionStatus == .current)
  #expect(model.selectedModelId == "gpt-test-model")

  await core.setLoginCompleted(false)
  await model.refreshAccountAndModels()

  #expect(model.accountState == .notConnected)
  #expect(model.modelSelectionStatus == .unselected)
  #expect(model.selectedModelId.isEmpty)
  #expect(model.selectedModelEffort.isEmpty)
  #expect(!model.modelEntryEnabled)
}

@MainActor
@Test
func effortLabelsRemainDistinctForEverySupportedProtocolValue() async {
  let model = AppModel(core: MockCore(), broker: MockBroker()) {}
  let labels = ["low", "medium", "high", "xhigh", "max", "custom"]
    .map { model.modelEffortLabel($0) }
  #expect(Set(labels).count == labels.count)
  #expect(model.modelEffortLabel("custom").contains("custom"))
}

@Test
func choiceLoopReadContractRejectsReplayableTerminalState() throws {
  let manifest = testChoiceLoopManifest()
  let active = ChoiceSession(
    id: "session-1", state: "active", revision: 1,
    modelSelectionState: ChoiceModelSelectionState(
      state: "unselected", modelProvenanceRef: nil, catalogRevision: nil, reason: nil),
    communicationProfileRevision: 0, activeChoiceSetId: nil, activeInterpretationRevision: nil,
    openedAtMs: 0, lastInputAtMs: 1, softIdleAtMs: 1_800_001,
    staleReviewAtMs: 86_400_001, primaryDeliveryBindingId: nil,
    pendingConfirmationId: nil, backgroundMissionIds: []
  )
  let valid = ChoiceLoopSnapshot(
    session: active, activeBatch: nil, interpretation: nil, activeChoiceSet: nil,
    lastSelection: nil, confirmation: nil,
    documentManifest: manifest)
  _ = try valid.validated()

  let terminal = ChoiceSession(
    id: "session-1", state: "completed", revision: 1,
    modelSelectionState: active.modelSelectionState, communicationProfileRevision: 0,
    activeChoiceSetId: "choices-1", activeInterpretationRevision: nil,
    openedAtMs: 0, lastInputAtMs: 1, softIdleAtMs: 1_800_001,
    staleReviewAtMs: 86_400_001, primaryDeliveryBindingId: nil,
    pendingConfirmationId: nil, backgroundMissionIds: []
  )
  let replayable = ChoiceLoopSnapshot(
    session: terminal, activeBatch: nil, interpretation: nil, activeChoiceSet: nil,
    lastSelection: nil, confirmation: nil,
    documentManifest: manifest)
  #expect(throws: CoreClientError.self) { try replayable.validated() }

  let terminalWithoutChoiceSet = ChoiceSession(
    id: "session-1", state: "completed", revision: 2,
    modelSelectionState: active.modelSelectionState, communicationProfileRevision: 0,
    activeChoiceSetId: nil, activeInterpretationRevision: nil,
    openedAtMs: 0, lastInputAtMs: 2, softIdleAtMs: 1_800_002,
    staleReviewAtMs: 86_400_002, primaryDeliveryBindingId: nil,
    pendingConfirmationId: nil, backgroundMissionIds: []
  )
  let replayableSelection = ChoiceLoopSnapshot(
    session: terminalWithoutChoiceSet, activeBatch: nil, interpretation: nil, activeChoiceSet: nil,
    lastSelection: ChoiceSelection(
      type: "optionSelection", id: "selection-1", choiceSessionId: "session-1",
      choiceSetId: "choices-1", selectedOptionId: "option-1", dInputBatchId: nil,
      expectedSessionRevision: 1, selectedAtMs: 2),
    confirmation: nil, documentManifest: manifest)
  #expect(throws: CoreClientError.self) { try replayableSelection.validated() }
}

@Test
func choiceConfirmationDeliveryFieldsRequireOneCompleteBoundTuple() throws {
  let session = ChoiceSession(
    id: "session-1", state: "awaitingConfirmation", revision: 2,
    modelSelectionState: ChoiceModelSelectionState(
      state: "unselected", modelProvenanceRef: nil, catalogRevision: nil, reason: nil),
    communicationProfileRevision: 0, activeChoiceSetId: nil, activeInterpretationRevision: nil,
    openedAtMs: 0, lastInputAtMs: 1, softIdleAtMs: 1_800_001,
    staleReviewAtMs: 86_400_001, primaryDeliveryBindingId: "binding-1",
    pendingConfirmationId: "confirmation-1", backgroundMissionIds: []
  )
  let incomplete = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: nil, deliveryScope: nil)
  let incompleteSnapshot = ChoiceLoopSnapshot(
    session: session, activeBatch: nil, interpretation: nil, activeChoiceSet: nil,
    lastSelection: nil, confirmation: incomplete, documentManifest: testChoiceLoopManifest())
  #expect(throws: CoreClientError.self) { try incompleteSnapshot.validated() }

  let complete = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: "owner-1", deliveryScope: "same-surface")
  let completeSnapshot = ChoiceLoopSnapshot(
    session: session, activeBatch: nil, interpretation: nil, activeChoiceSet: nil,
    lastSelection: nil, confirmation: complete, documentManifest: testChoiceLoopManifest())
  _ = try completeSnapshot.validated()
}

@Test
func choiceConfirmationSealBindsEachExactReminderField() {
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: nil, recipient: nil, deliveryScope: nil)
  #expect(confirmation.canonicalPayloadDigest() == confirmation.payloadDigest)
  #expect(confirmation.canonicalReminderPayloadDigest() == confirmation.reminderPayloadDigest)
  #expect(confirmation.validated())
  let changedItem = ChoiceReminderItem(
    id: confirmation.reminderItems[0].id, text: confirmation.reminderItems[0].text,
    dueAtMs: confirmation.reminderItems[0].dueAtMs + 1,
    timeZone: confirmation.reminderItems[0].timeZone,
    evidenceIntent: confirmation.reminderItems[0].evidenceIntent)
  let changed = ChoiceConsolidatedConfirmation(
    id: confirmation.id, choiceSessionId: confirmation.choiceSessionId,
    choiceSetId: confirmation.choiceSetId, selectionId: confirmation.selectionId,
    expectedSessionRevision: confirmation.expectedSessionRevision,
    interpretationRevision: confirmation.interpretationRevision,
    payloadRevision: confirmation.payloadRevision, payloadDigest: confirmation.payloadDigest,
    goal: confirmation.goal, steps: confirmation.steps,
    markdownEntry: confirmation.markdownEntry,
    markdownExpectedBase: confirmation.markdownExpectedBase,
    markdownManifestDigests: confirmation.markdownManifestDigests,
    documentDiffDigest: confirmation.documentDiffDigest,
    modelProvenance: confirmation.modelProvenance,
    personaRevision: confirmation.personaRevision,
    reminderListId: confirmation.reminderListId, reminderItems: [changedItem],
    reminderCount: confirmation.reminderCount,
    reminderPayloadDigest: confirmation.reminderPayloadDigest,
    evidenceRequirements: confirmation.evidenceRequirements,
    deliveryBindingId: confirmation.deliveryBindingId, recipient: confirmation.recipient,
    deliveryScope: confirmation.deliveryScope, dataCategories: confirmation.dataCategories,
    retention: confirmation.retention, permissions: confirmation.permissions,
    effectClasses: confirmation.effectClasses, confirmedAtMs: confirmation.confirmedAtMs)
  #expect(!changed.validated())
  let personaDrifted = ChoiceConsolidatedConfirmation(
    id: confirmation.id, choiceSessionId: confirmation.choiceSessionId,
    choiceSetId: confirmation.choiceSetId, selectionId: confirmation.selectionId,
    expectedSessionRevision: confirmation.expectedSessionRevision,
    interpretationRevision: confirmation.interpretationRevision,
    payloadRevision: confirmation.payloadRevision, payloadDigest: confirmation.payloadDigest,
    goal: confirmation.goal, steps: confirmation.steps,
    markdownEntry: confirmation.markdownEntry,
    markdownExpectedBase: confirmation.markdownExpectedBase,
    markdownManifestDigests: confirmation.markdownManifestDigests,
    documentDiffDigest: confirmation.documentDiffDigest,
    modelProvenance: confirmation.modelProvenance,
    personaRevision: PersonaRevisionRef(
      personaId: confirmation.personaRevision.personaId,
      revision: confirmation.personaRevision.revision,
      aggregateDigest: String(repeating: "0", count: 64),
      instructionsDigest: confirmation.personaRevision.instructionsDigest),
    reminderListId: confirmation.reminderListId, reminderItems: confirmation.reminderItems,
    reminderCount: confirmation.reminderCount,
    reminderPayloadDigest: confirmation.reminderPayloadDigest,
    evidenceRequirements: confirmation.evidenceRequirements,
    deliveryBindingId: confirmation.deliveryBindingId, recipient: confirmation.recipient,
    deliveryScope: confirmation.deliveryScope, dataCategories: confirmation.dataCategories,
    retention: confirmation.retention, permissions: confirmation.permissions,
    effectClasses: confirmation.effectClasses, confirmedAtMs: confirmation.confirmedAtMs)
  #expect(personaDrifted.canonicalPayloadDigest() != confirmation.payloadDigest)
  #expect(!personaDrifted.validated())
}

@Test
func choiceConfirmationTypedPreimageMatchesTheRustGoldenVector() {
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: nil, recipient: nil, deliveryScope: nil)
  #expect(confirmation.canonicalPayloadPreimage()?.count == 2_439)
  #expect(
    confirmation.canonicalPayloadDigest()
      == "5a7b8b6468fc9b773e9605d59c7bb4710c945baea9cb2f2ee155fb8f4626f7f9")
}

@Test
func choiceReminderScheduleContractRequiresAnExplicitBoundedIanaProposal() {
  let valid = ChoiceReminderScheduleInput(
    requestId: "schedule-request-1", choiceSessionId: "session-1",
    expectedSessionRevision: 1, reminderListId: "local-reminders",
    reminderCount: 1,
    dueAtMs: 1, timeZone: "America/Los_Angeles")
  #expect(valid.validated())
  #expect(
    !ChoiceReminderScheduleInput(
      requestId: valid.requestId, choiceSessionId: valid.choiceSessionId,
      expectedSessionRevision: valid.expectedSessionRevision, reminderListId: valid.reminderListId,
      reminderCount: valid.reminderCount,
      dueAtMs: valid.dueAtMs, timeZone: "not/a-time-zone"
    ).validated())
  #expect(
    !ChoiceReminderScheduleInput(
      requestId: valid.requestId, choiceSessionId: valid.choiceSessionId,
      expectedSessionRevision: valid.expectedSessionRevision, reminderListId: "list with spaces",
      reminderCount: valid.reminderCount,
      dueAtMs: valid.dueAtMs, timeZone: valid.timeZone
    ).validated())
  #expect(
    !ChoiceReminderScheduleInput(
      requestId: valid.requestId, choiceSessionId: valid.choiceSessionId,
      expectedSessionRevision: valid.expectedSessionRevision, reminderListId: valid.reminderListId,
      reminderCount: 0, dueAtMs: valid.dueAtMs, timeZone: valid.timeZone
    ).validated())
}

@Test
func typedReminderScheduleRecoveryKeepsTheKnownNextActionVisible() {
  let error = CoreClientError.remote(
    code: -32_024, message: "Choose a complete future Reminder schedule before review.")
  #expect(error.errorDescription == "Choose a complete future Reminder schedule before review.")
}

private func testChoiceLoopManifest(aggregateDigest: String? = nil) -> DocumentManifest {
  let entries = [
    DocumentManifestEntry(
      relativePath: "sessions/session-1/SESSION.md",
      sha256: String(repeating: "a", count: 64), byteLength: 64, mode: 0o600)
  ]
  return DocumentManifest(
    rootVersion: 1,
    entries: entries,
    aggregateDigest: aggregateDigest ?? DocumentManifest.canonicalAggregateDigest(
      entries: entries)!,
    generatedAtMs: 1
  )
}

private func testChoiceConfirmation(
  deliveryBindingId: String?, recipient: String?, deliveryScope: String?,
  choiceSessionId: String = "session-1", choiceSetId: String = "choices-1",
  selectionId: String = "selection-1", expectedSessionRevision: UInt64 = 1
) -> ChoiceConsolidatedConfirmation {
  let markdownEntry = DocumentManifestEntry(
    relativePath: "sessions/session-1/CHOICE.md", sha256: String(repeating: "f", count: 64),
    byteLength: 64, mode: 0o600)
  return ChoiceConsolidatedConfirmation(
    id: "confirmation-1", choiceSessionId: choiceSessionId, choiceSetId: choiceSetId,
    selectionId: selectionId,
    expectedSessionRevision: expectedSessionRevision, interpretationRevision: 1,
    payloadRevision: 1,
    payloadDigest: String(repeating: "a", count: 64), goal: "Prepare a bounded next step",
    steps: ["Review the prepared plan"],
    markdownEntry: markdownEntry,
    markdownExpectedBase: nil,
    markdownManifestDigests: [
      String(repeating: "b", count: 64),
      DocumentManifest.canonicalAggregateDigest(entries: [markdownEntry])!,
    ],
    documentDiffDigest: String(repeating: "c", count: 64),
    modelProvenance: ChoiceModelProvenance(
      id: "provenance-1", modelId: "gpt-test-model", requestedEffort: "not_applicable",
      actualEffort: "not_applicable", catalogFingerprint: String(repeating: "d", count: 64),
      catalogRevision: 1, accountDisplayClass: "ChatGPT account", protocolSchemaRevision: 1,
      turnId: "turn-1"),
    personaRevision: PersonaRevisionRef(
      personaId: "openopen.nondev.default", revision: "draft-03-en",
      aggregateDigest: String(repeating: "e", count: 64),
      instructionsDigest: String(repeating: "f", count: 64)),
    reminderListId: "openopen-default-reminders",
    reminderItems: [
      ChoiceReminderItem(
        id: "reminder-1", text: "Review the prepared plan", dueAtMs: 1,
        timeZone: "Etc/UTC", evidenceIntent: "reminder-readback")
    ],
    reminderCount: 1,
    reminderPayloadDigest: String(repeating: "e", count: 64),
    evidenceRequirements: ["Reminder readback before Done"], deliveryBindingId: deliveryBindingId,
    recipient: recipient, deliveryScope: deliveryScope, dataCategories: ["local task state"],
    retention: "Local until user deletion", permissions: [], effectClasses: ["reminder"],
    confirmedAtMs: 1
  ).withCanonicalPayloadDigest()
}

extension ChoiceConsolidatedConfirmation {
  fileprivate func withCanonicalPayloadDigest() -> ChoiceConsolidatedConfirmation {
    let reminderPayloadDigest = canonicalReminderPayloadDigest()!
    let reminderBound = ChoiceConsolidatedConfirmation(
      id: id, choiceSessionId: choiceSessionId, choiceSetId: choiceSetId,
      selectionId: selectionId,
      expectedSessionRevision: expectedSessionRevision,
      interpretationRevision: interpretationRevision, payloadRevision: payloadRevision,
      payloadDigest: "", goal: goal, steps: steps,
      markdownEntry: markdownEntry, markdownExpectedBase: markdownExpectedBase,
      markdownManifestDigests: markdownManifestDigests, documentDiffDigest: documentDiffDigest,
      modelProvenance: modelProvenance, personaRevision: personaRevision,
      reminderListId: reminderListId,
      reminderItems: reminderItems, reminderCount: reminderCount,
      reminderPayloadDigest: reminderPayloadDigest,
      evidenceRequirements: evidenceRequirements, deliveryBindingId: deliveryBindingId,
      recipient: recipient, deliveryScope: deliveryScope, dataCategories: dataCategories,
      retention: retention, permissions: permissions, effectClasses: effectClasses,
      confirmedAtMs: confirmedAtMs)
    return ChoiceConsolidatedConfirmation(
      id: reminderBound.id, choiceSessionId: reminderBound.choiceSessionId,
      choiceSetId: reminderBound.choiceSetId, selectionId: reminderBound.selectionId,
      expectedSessionRevision: reminderBound.expectedSessionRevision,
      interpretationRevision: reminderBound.interpretationRevision,
      payloadRevision: reminderBound.payloadRevision,
      payloadDigest: reminderBound.canonicalPayloadDigest()!, goal: reminderBound.goal,
      steps: reminderBound.steps, markdownEntry: reminderBound.markdownEntry,
      markdownExpectedBase: reminderBound.markdownExpectedBase,
      markdownManifestDigests: reminderBound.markdownManifestDigests,
      documentDiffDigest: reminderBound.documentDiffDigest,
      modelProvenance: reminderBound.modelProvenance,
      personaRevision: reminderBound.personaRevision,
      reminderListId: reminderBound.reminderListId,
      reminderItems: reminderBound.reminderItems, reminderCount: reminderBound.reminderCount,
      reminderPayloadDigest: reminderBound.reminderPayloadDigest,
      evidenceRequirements: reminderBound.evidenceRequirements,
      deliveryBindingId: reminderBound.deliveryBindingId, recipient: reminderBound.recipient,
      deliveryScope: reminderBound.deliveryScope, dataCategories: reminderBound.dataCategories,
      retention: reminderBound.retention, permissions: reminderBound.permissions,
      effectClasses: reminderBound.effectClasses, confirmedAtMs: reminderBound.confirmedAtMs)
  }
}

private func testChoiceLoopSnapshot(
  sessionID: String = "session-1",
  aggregateDigest: String? = nil
) -> ChoiceLoopSnapshot {
  ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: sessionID, state: "active", revision: 1,
      modelSelectionState: ChoiceModelSelectionState(
        state: "unselected", modelProvenanceRef: nil, catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: nil, activeInterpretationRevision: nil,
      openedAtMs: 0, lastInputAtMs: 1, softIdleAtMs: 1_800_001,
      staleReviewAtMs: 86_400_001, primaryDeliveryBindingId: nil,
      pendingConfirmationId: nil, backgroundMissionIds: []
    ),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil,
    lastSelection: nil, confirmation: nil,
    documentManifest: testChoiceLoopManifest(aggregateDigest: aggregateDigest)
  )
}

private func testInterpretingChoiceLoopSnapshot() -> ChoiceLoopSnapshot {
  ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: "session-choice-1", state: "interpreting", revision: 1,
      modelSelectionState: ChoiceModelSelectionState(
        state: "selected", modelProvenanceRef: "mock-selection-gpt-test-model",
        catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: nil, activeInterpretationRevision: nil,
      openedAtMs: 1, lastInputAtMs: 1, softIdleAtMs: 1_800_001,
      staleReviewAtMs: 86_400_001, primaryDeliveryBindingId: "mac-local-owner",
      pendingConfirmationId: nil, backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: nil, documentManifest: testChoiceLoopManifest())
}

private func testPersonaRevision() -> PersonaRevisionRef {
  PersonaRevisionRef(
    personaId: "openopen.nondev.default", revision: "draft-03-en",
    aggregateDigest: String(repeating: "f", count: 64),
    instructionsDigest: String(repeating: "e", count: 64))
}

private func testActiveChoiceLoopSnapshot() -> ChoiceLoopSnapshot {
  let manifest = testChoiceLoopManifest()
  let provenance = ChoiceModelProvenance(
    id: "choice-provenance-1", modelId: "gpt-test-model", requestedEffort: "high",
    actualEffort: "high", catalogFingerprint: String(repeating: "a", count: 64),
    catalogRevision: 1, accountDisplayClass: "managed", protocolSchemaRevision: 1,
    turnId: "turn-choice-1")
  let interpretation = InterpretationFrame(
    choiceSessionId: "session-choice-active", revision: 1,
    understoodGoal: "Prepare one bounded plan.", currentContext: "Local Mac session.",
    assumptions: [], constraints: [], uncertainties: [], whatToAvoid: [],
    sourceManifestDigest: manifest.aggregateDigest)
  let options = (1...3).map { position in
    ChoiceOption(
      id: "choice-option-\(position)", position: UInt8(position),
      direction: "Direction \(position)", rationale: "Bounded next direction.",
      expectedResult: "A clearer next step.", informationNeeded: [],
      externalEffectsPreview: [], sourceCategories: ["local"])
  }
  let choiceSet = ChoiceSet(
    id: "choice-set-active", choiceSessionId: "session-choice-active", sessionRevision: 2,
    interpretationRevision: 1, generatedAtMs: 10, expiresOnRevision: 3, options: options,
    dAvailable: true, sourceManifestDigest: manifest.aggregateDigest, modelProvenance: provenance,
    personaRevision: testPersonaRevision())
  return ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: "session-choice-active", state: "active", revision: 2,
      modelSelectionState: ChoiceModelSelectionState(
        state: "selected", modelProvenanceRef: "mock-selection-gpt-test-model",
        catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: choiceSet.id,
      activeInterpretationRevision: 1, openedAtMs: 1, lastInputAtMs: 10,
      softIdleAtMs: 1_800_010, staleReviewAtMs: 86_400_010,
      primaryDeliveryBindingId: "mac-local-owner", pendingConfirmationId: nil,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: interpretation, activeChoiceSet: choiceSet,
    lastSelection: nil, confirmation: nil, documentManifest: manifest)
}

private func testRefiningChoiceLoopSnapshot(from active: ChoiceLoopSnapshot) -> ChoiceLoopSnapshot {
  let selection = ChoiceSelection(
    type: "optionSelection", id: "choice-selection-1", choiceSessionId: active.session.id,
    choiceSetId: "choice-set-active", selectedOptionId: "choice-option-1", dInputBatchId: nil,
    expectedSessionRevision: active.session.revision, selectedAtMs: 11)
  return ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: active.session.id, state: "refining", revision: 3,
      modelSelectionState: active.session.modelSelectionState, communicationProfileRevision: 0,
      activeChoiceSetId: nil, activeInterpretationRevision: 1, openedAtMs: 1,
      lastInputAtMs: 11, softIdleAtMs: 1_800_011, staleReviewAtMs: 86_400_011,
      primaryDeliveryBindingId: "mac-local-owner", pendingConfirmationId: nil,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: active.interpretation, activeChoiceSet: nil,
    lastSelection: selection,
    pendingRefinementOperation: ChoiceRefinementOperation(
      id: "choice-refinement-operation-1", selectionId: selection.id,
      choiceSessionId: active.session.id, sourceEnvelopeId: "source-envelope-choice-1",
      conversationTurnBatchId: "batch-choice-1", expectedSessionRevision: 3, expectedGeneration: 1,
      modelProvenance: active.activeChoiceSet!.modelProvenance,
      sourceManifestDigest: active.documentManifest.aggregateDigest,
      personaRevision: active.activeChoiceSet!.personaRevision,
      dRequestId: nil, dInputDigest: nil, createdAtMs: 11),
    confirmation: nil, documentManifest: active.documentManifest)
}

private func testChoiceLoopSnapshot(
  from active: ChoiceLoopSnapshot, state: String, revision: UInt64? = nil
) -> ChoiceLoopSnapshot {
  ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: active.session.id, state: state, revision: revision ?? active.session.revision,
      modelSelectionState: active.session.modelSelectionState,
      communicationProfileRevision: active.session.communicationProfileRevision,
      activeChoiceSetId: active.session.activeChoiceSetId,
      activeInterpretationRevision: active.session.activeInterpretationRevision,
      openedAtMs: active.session.openedAtMs, lastInputAtMs: active.session.lastInputAtMs,
      softIdleAtMs: active.session.softIdleAtMs,
      staleReviewAtMs: active.session.staleReviewAtMs,
      primaryDeliveryBindingId: active.session.primaryDeliveryBindingId,
      pendingConfirmationId: active.session.pendingConfirmationId,
      backgroundMissionIds: active.session.backgroundMissionIds),
    activeBatch: active.activeBatch, interpretation: active.interpretation,
    activeChoiceSet: active.activeChoiceSet, lastSelection: active.lastSelection,
    pendingRefinementOperation: active.pendingRefinementOperation,
    confirmation: active.confirmation, documentManifest: active.documentManifest)
}

/// Matches the Store's post-resume-failure state: the old ChoiceSet has been
/// retired, but the persisted interpretation remains a bounded local recap
/// source for one later authenticated owner return.
private func testResumeIdleChoiceLoopSnapshot(
  from active: ChoiceLoopSnapshot, revision: UInt64
) -> ChoiceLoopSnapshot {
  ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: active.session.id, state: "softIdle", revision: revision,
      modelSelectionState: active.session.modelSelectionState,
      communicationProfileRevision: active.session.communicationProfileRevision,
      activeChoiceSetId: nil,
      activeInterpretationRevision: active.interpretation?.revision,
      openedAtMs: active.session.openedAtMs, lastInputAtMs: active.session.lastInputAtMs,
      softIdleAtMs: active.session.softIdleAtMs,
      staleReviewAtMs: active.session.staleReviewAtMs,
      primaryDeliveryBindingId: active.session.primaryDeliveryBindingId,
      pendingConfirmationId: nil, backgroundMissionIds: active.session.backgroundMissionIds),
    activeBatch: nil, interpretation: active.interpretation, activeChoiceSet: nil,
    lastSelection: nil, pendingRefinementOperation: nil, confirmation: nil,
    documentManifest: active.documentManifest)
}

private func testCancelledChoiceLoopSnapshot(
  from active: ChoiceLoopSnapshot
) -> ChoiceLoopSnapshot {
  ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: active.session.id, state: "cancelled", revision: active.session.revision + 1,
      modelSelectionState: active.session.modelSelectionState, communicationProfileRevision: 0,
      activeChoiceSetId: nil, activeInterpretationRevision: nil, openedAtMs: 1,
      lastInputAtMs: 11, softIdleAtMs: 1_800_011, staleReviewAtMs: 86_400_011,
      primaryDeliveryBindingId: "mac-local-owner", pendingConfirmationId: nil,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: nil, documentManifest: active.documentManifest)
}

private func testExecutingChoiceLoopSnapshot(
  from active: ChoiceLoopSnapshot
) -> ChoiceLoopSnapshot {
  ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: active.session.id, state: "executing", revision: active.session.revision + 1,
      modelSelectionState: active.session.modelSelectionState, communicationProfileRevision: 0,
      activeChoiceSetId: nil, activeInterpretationRevision: nil, openedAtMs: 1,
      lastInputAtMs: 11, softIdleAtMs: 1_800_011, staleReviewAtMs: 86_400_011,
      primaryDeliveryBindingId: "mac-local-owner", pendingConfirmationId: nil,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: nil, documentManifest: active.documentManifest)
}

@MainActor
@Test
func dashboardProjectsReadOnlyPersonaProvenanceWithoutLifecycleAuthority() async {
  let core = MockCore()
  let persona = PersonaRevisionRef(
    personaId: "openopen.nondev.default", revision: "draft-03-en",
    aggregateDigest: String(repeating: "a", count: 64),
    instructionsDigest: String(repeating: "b", count: 64))
  await core.setPersonaStatus(
    PersonaStatusView(
      status: PersonaStatus(active: persona, staged: nil, warning: nil, changeNotePending: false),
      changeNote: nil))
  let model = AppModel(core: core, broker: MockBroker()) {}

  await model.refreshDashboard()

  #expect(model.personaStatus?.status.active == persona)
  #expect(model.personaStatus?.status.staged == nil)
}

@Test
func editorialPersonaProvenanceIsReadOnlyAndHasNoLifecycleControls() throws {
  let sourceURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/OpenOpenAppSupport/OpenOpenViews.swift")
  let source = try String(contentsOf: sourceURL, encoding: .utf8)

  #expect(source.contains("openopen-persona-provenance-revision"))
  #expect(source.contains("openopen-persona-provenance-digest"))
  #expect(!source.contains("persona.stage"))
  #expect(!source.contains("persona.activate"))
  #expect(!source.contains("persona.rollback"))
}

@Test
func frozenReminderScheduleUsesNativeBoundedControlsWithoutRawContractFields() throws {
  let sourceURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/OpenOpenAppSupport/OpenOpenViews.swift")
  let source = try String(contentsOf: sourceURL, encoding: .utf8)

  #expect(source.contains("GroupBox(\"A time is still needed\")"))
  #expect(source.contains("DatePicker("))
  #expect(source.contains("Button(\"Back\")"))
  #expect(source.contains("Button(\"Review reminder\")"))
  #expect(source.contains("!model.choiceReminderScheduleReadyForReview"))
  #expect(source.contains("Nothing will be guessed."))
  #expect(source.contains("model.requestChoiceReminderWrite()"))
  #expect(source.contains("openopen-choice-reminder-recover"))
  #expect(source.contains("model.confirmedMission?.choiceConfirmationId != nil"))
  #expect(source.contains("model.confirmedMission?.choiceConfirmationId == nil"))
  #expect(source.contains("Text(\"Reminders\")"))
  #expect(!source.contains("Text(confirmation.reminderListId)"))
  #expect(!source.contains("Text(item.evidenceIntent)"))
  #expect(source.contains("GroupBox(\"Reminder added and verified\")"))
  #expect(
    source.contains(
      "The saved Reminder matches the confirmed date, time, time zone, list, and count."))
  #expect(source.contains("Readable proof is available without exposing private content."))
  #expect(source.contains("model.receiptIsForCurrentChoice"))
  #expect(source.contains("Button(\"View evidence\")"))
  #expect(source.contains("Button(\"Continue\")"))
  #expect(!source.contains("Date and time (YYYY-MM-DDTHH:MM)"))
  #expect(!source.contains("TextField(\"Reminder list\""))
  #expect(!source.contains("TextField(\"Reminder count\""))
}

@Test
func postReceiptOwnerReturnReleasesBusyFenceBeforeAuthenticatedResume() throws {
  let sourceURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/OpenOpenAppSupport/AppModel.swift")
  let source = try String(contentsOf: sourceURL, encoding: .utf8)
  let function = try #require(source.range(of: "public func checkChoiceReminderProgress() async"))
  let tail = source[function.lowerBound...]
  let release = try #require(tail.range(of: "isBusy = false"))
  let resume = try #require(
    tail.range(of: "await refreshDashboard(authenticatedHomeForeground: true)"))
  #expect(release.lowerBound < resume.lowerBound)
}

@MainActor
@Test
func reminderScheduleStartsVisiblyUnselectedAndBackIsLocalOnly() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let snapshot = testChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(snapshot)
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()

  #expect(!model.choiceReminderPickerIsPresented)
  #expect(!model.choiceReminderScheduleReadyForReview)
  model.presentChoiceReminderDatePicker()
  #expect(model.choiceReminderPickerIsPresented)
  #expect(!model.choiceReminderScheduleReadyForReview)
  model.selectChoiceReminderTimeZone("Etc/UTC")
  model.selectChoiceReminderList("openopen.default-reminders")
  model.selectChoiceReminderDate(Date().addingTimeInterval(3_600))
  #expect(model.choiceReminderScheduleReadyForReview)

  model.backFromChoiceReminderSchedule()
  #expect(!model.choiceReminderScheduleIsVisible)
  #expect(model.choiceReminderDateTime.isEmpty)
  #expect(model.choiceLoopSnapshot == snapshot)
  #expect(await core.recordedChoiceReminderScheduleInputs().isEmpty)
}

@MainActor
@Test
func globalOffCancelsTheOwnedChoiceReminderTaskBeforeAnySave() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .delayBeforeTarget)
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: "owner-1", deliveryScope: "same-surface")
  let snapshot = ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: "session-1", state: "awaitingConfirmation", revision: 2,
      modelSelectionState: ChoiceModelSelectionState(
        state: "selected", modelProvenanceRef: "mock-selection-gpt-test-model",
        catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: nil,
      activeInterpretationRevision: nil, openedAtMs: 0, lastInputAtMs: 1,
      softIdleAtMs: 1_800_001, staleReviewAtMs: 86_400_001,
      primaryDeliveryBindingId: "binding-1", pendingConfirmationId: confirmation.id,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: confirmation, documentManifest: testChoiceLoopManifest())
  _ = try? snapshot.validated()
  await core.setChoiceLoopSnapshot(snapshot)
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot != nil)
  #expect(model.choiceLoopSnapshot?.session.state == "awaitingConfirmation")
  #expect(model.choiceLoopSnapshot?.confirmation?.id == confirmation.id)
  #expect(model.errorMessage == nil)
  #expect(model.storeControlEnabled)
  #expect(!model.isBusy)

  model.requestChoiceReminderWrite()
  try? await Task.sleep(for: .milliseconds(20))
  model.requestEnabled(false)
  try? await Task.sleep(for: .milliseconds(80))

  #expect(reminders.executeCount == 0)
  #expect(!model.isBusy)
}

@MainActor
@Test
func choiceReminderPrecommitCancellationRecordsAbortBeforeRetryAuthority() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .cancelBeforeCommit)
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: "owner-1", deliveryScope: "same-surface")
  let mission = testChoiceConfirmedMission(
    confirmation: confirmation,
    dispatch: [
      ConfirmedReminderDispatch(
        workItemId: confirmation.reminderItems[0].id,
        token: "dispatch-choice-work-1")
    ])
  #expect((try? mission.validated()) != nil)
  await core.setChoiceReminderMission(mission, executeNow: true)
  await core.loseNextChoiceReminderAbortResponse()
  let snapshot = ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: "session-1", state: "awaitingConfirmation", revision: 2,
      modelSelectionState: ChoiceModelSelectionState(
        state: "selected", modelProvenanceRef: "mock-selection-gpt-test-model",
        catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: nil,
      activeInterpretationRevision: nil, openedAtMs: 0, lastInputAtMs: 1,
      softIdleAtMs: 1_800_001, staleReviewAtMs: 86_400_001,
      primaryDeliveryBindingId: "binding-1", pendingConfirmationId: confirmation.id,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: confirmation, documentManifest: testChoiceLoopManifest())
  #expect((try? snapshot.validated()) != nil)
  await core.setChoiceLoopSnapshot(snapshot)
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot?.session.state == "awaitingConfirmation")
  #expect(model.choiceLoopSnapshot?.confirmation?.id == confirmation.id)
  #expect(model.storeControlEnabled)
  #expect(!model.isBusy)

  model.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if await core.choiceReminderAbortCount >= 2 { break }
    try? await Task.sleep(for: .milliseconds(10))
  }

  #expect(reminders.executeCount == 1)
  let abortCount = await core.choiceReminderAbortCount
  #expect(abortCount == 2)
  #expect(model.reminderLinks.isEmpty)
  #expect(model.errorMessage?.contains("stopped before EventKit committed") == true)

  // The signed pre-commit abort makes the same explicit action reachable
  // after restart; it is not an automatic retry.
  await model.updateEnabled(false)
  reminders.mode = .complete
  let restarted = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await restarted.updateEnabled(true)
  await restarted.refreshAccountAndModels()
  await restarted.refreshDashboard()
  #expect(restarted.confirmedMission?.choiceConfirmationId == confirmation.id)
  #expect(restarted.reminderLinks.isEmpty)
  #expect(restarted.storeControlEnabled)
  #expect(!restarted.isBusy)
  #expect(restarted.errorMessage == nil)
  restarted.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if !restarted.reminderLinks.isEmpty { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(reminders.executeCount == 2)
  #expect(reminders.recoverCount == 0)
  #expect(restarted.reminderLinks.count == 1)
  #expect(restarted.errorMessage == nil)
  await restarted.updateEnabled(false)
}

@MainActor
@Test
func globalOffQuiescesAStartedPrecommitReminderBeforeCoreShutdownAndRetry() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .waitPrecommitUntilCancelled)
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: "owner-1", deliveryScope: "same-surface")
  let mission = testChoiceConfirmedMission(
    confirmation: confirmation,
    dispatch: [
      ConfirmedReminderDispatch(
        workItemId: confirmation.reminderItems[0].id,
        token: "dispatch-choice-work-1")
    ])
  await core.setChoiceReminderMission(mission, executeNow: true)
  await core.loseNextChoiceReminderAbortResponse()
  let snapshot = ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: "session-1", state: "awaitingConfirmation", revision: 2,
      modelSelectionState: ChoiceModelSelectionState(
        state: "selected", modelProvenanceRef: "mock-selection-gpt-test-model",
        catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: nil,
      activeInterpretationRevision: nil, openedAtMs: 0, lastInputAtMs: 1,
      softIdleAtMs: 1_800_001, staleReviewAtMs: 86_400_001,
      primaryDeliveryBindingId: "binding-1", pendingConfirmationId: confirmation.id,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: confirmation, documentManifest: testChoiceLoopManifest())
  await core.setChoiceLoopSnapshot(snapshot)
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()

  model.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if reminders.executeCount == 1 { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(reminders.executeCount == 1)
  let challengesBeforeOff = await core.challengesIssued
  await model.updateEnabled(false)

  let offAbortCount = await core.choiceReminderAbortCount
  let challengesAfterOff = await core.challengesIssued
  #expect(reminders.precommitCancelCount == 1)
  #expect(challengesAfterOff > challengesBeforeOff)
  #expect(offAbortCount == 2)
  #expect(model.errorMessage == nil)
  #expect(reminders.executeCount == 1)
  #expect(model.reminderLinks.isEmpty)
  #expect(!model.storeControlEnabled)

  reminders.mode = .complete
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()
  model.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if !model.reminderLinks.isEmpty { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(reminders.executeCount == 2)
  #expect(reminders.recoverCount == 0)
  #expect(model.reminderLinks.count == 1)

  // A repeated explicit recovery reattaches read-only and never writes again.
  model.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if reminders.recoverCount == 1 { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(reminders.executeCount == 2)
  #expect(reminders.recoverCount == 1)
  #expect(model.reminderLinks.count == 1)
  await model.updateEnabled(false)
}

@MainActor
@Test
func exhaustedOffAbortReconcilesProvenAbsenceAfterRestartBeforeOneExplicitRetry() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .waitPrecommitUntilCancelled)
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: "owner-1", deliveryScope: "same-surface")
  let mission = testChoiceConfirmedMission(
    confirmation: confirmation,
    dispatch: [
      ConfirmedReminderDispatch(
        workItemId: confirmation.reminderItems[0].id,
        token: "dispatch-choice-work-1")
    ])
  await core.setChoiceReminderMission(mission, executeNow: true)
  await core.failNextChoiceReminderAbortResponses(3)
  let snapshot = ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: "session-1", state: "awaitingConfirmation", revision: 2,
      modelSelectionState: ChoiceModelSelectionState(
        state: "selected", modelProvenanceRef: "mock-selection-gpt-test-model",
        catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: nil,
      activeInterpretationRevision: nil, openedAtMs: 0, lastInputAtMs: 1,
      softIdleAtMs: 1_800_001, staleReviewAtMs: 86_400_001,
      primaryDeliveryBindingId: "binding-1", pendingConfirmationId: confirmation.id,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: confirmation, documentManifest: testChoiceLoopManifest())
  await core.setChoiceLoopSnapshot(snapshot)
  let original = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await original.updateEnabled(true)
  await original.refreshAccountAndModels()
  await original.refreshDashboard()
  original.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if reminders.executeCount == 1 { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  await original.updateEnabled(false)
  #expect(reminders.precommitCancelCount == 1)
  #expect(await core.choiceReminderAbortCount == 3)
  #expect(original.reminderLinks.isEmpty)
  #expect(!original.storeControlEnabled)

  reminders.mode = .tamperedMirror
  let restarted = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await restarted.updateEnabled(true)
  await restarted.refreshAccountAndModels()
  await restarted.refreshDashboard()

  // A plausible tampered row is never misclassified as absence and cannot
  // retire the attempt or unlock another write.
  restarted.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if reminders.recoverCount == 1 { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(await core.choiceReminderAbortCount == 3)
  #expect(reminders.executeCount == 1)
  #expect(restarted.reminderLinks.isEmpty)
  #expect(restarted.errorMessage?.contains("changed") == true)

  reminders.mode = .complete
  // The next explicit action is still read-only recovery. Proven total
  // absence retires the stale started attempt but never enters EventKit.
  restarted.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if await core.choiceReminderAbortCount == 4 { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(await core.choiceReminderAbortCount == 4)
  #expect(reminders.executeCount == 1)
  #expect(reminders.recoverCount == 2)
  #expect(restarted.reminderLinks.isEmpty)
  #expect(restarted.errorMessage?.contains("choose Check Reminder again") == true)

  // Only a second, separately explicit action may consume the new attempt.
  restarted.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if !restarted.reminderLinks.isEmpty { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(reminders.executeCount == 2)
  #expect(reminders.recoverCount == 2)
  #expect(restarted.reminderLinks.count == 1)
  restarted.requestChoiceReminderWrite()
  for _ in 0..<20 { try? await Task.sleep(for: .milliseconds(5)) }
  #expect(reminders.executeCount == 2)
  await restarted.updateEnabled(false)
}

@MainActor
@Test
func choiceReminderAmbiguousCommitRestartsIntoReadOnlyRecoveryWithoutSecondWrite() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .commitThenFailReadback)
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: "owner-1", deliveryScope: "same-surface")
  let mission = testChoiceConfirmedMission(
    confirmation: confirmation,
    dispatch: [
      ConfirmedReminderDispatch(
        workItemId: confirmation.reminderItems[0].id,
        token: "dispatch-choice-work-1")
    ])
  await core.setChoiceReminderMission(mission, executeNow: true)
  let snapshot = ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: "session-1", state: "awaitingConfirmation", revision: 2,
      modelSelectionState: ChoiceModelSelectionState(
        state: "selected", modelProvenanceRef: "mock-selection-gpt-test-model",
        catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: nil,
      activeInterpretationRevision: nil, openedAtMs: 0, lastInputAtMs: 1,
      softIdleAtMs: 1_800_001, staleReviewAtMs: 86_400_001,
      primaryDeliveryBindingId: "binding-1", pendingConfirmationId: confirmation.id,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: confirmation, documentManifest: testChoiceLoopManifest())
  await core.setChoiceLoopSnapshot(snapshot)

  let first = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await first.updateEnabled(true)
  await first.refreshAccountAndModels()
  await first.refreshDashboard()
  first.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if first.errorMessage != nil { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(reminders.executeCount == 1)
  #expect(reminders.recoverCount == 0)
  #expect(first.reminderLinks.isEmpty)

  // A persisted started dispatch is ambiguous. The Host returns recover-only;
  // the visible recovery action must never issue a second EventKit write.
  await first.updateEnabled(false)
  await core.setChoiceReminderMission(mission, executeNow: false)
  let restarted = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await restarted.updateEnabled(true)
  await restarted.refreshAccountAndModels()
  await restarted.refreshDashboard()
  #expect(restarted.confirmedMission?.choiceConfirmationId == confirmation.id)
  #expect(restarted.reminderLinks.isEmpty)
  #expect(restarted.storeControlEnabled)
  #expect(!restarted.isBusy)
  #expect(restarted.errorMessage == nil)
  restarted.requestChoiceReminderWrite()
  for _ in 0..<50 {
    if !restarted.reminderLinks.isEmpty { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(reminders.executeCount == 1)
  #expect(reminders.recoverCount == 1)
  #expect(restarted.reminderLinks.count == 1)
  #expect(restarted.errorMessage == nil)
  await restarted.updateEnabled(false)
}

@MainActor
@Test
func choiceReceiptKeepsItsTypedPresentationAfterRestartWithoutMissionFocus() async {
  let core = MockCore()
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: "owner-1", deliveryScope: "same-surface")
  let snapshot = ChoiceLoopSnapshot(
    session: ChoiceSession(
      id: "session-1", state: "softIdle", revision: 3,
      modelSelectionState: ChoiceModelSelectionState(
        state: "selected", modelProvenanceRef: "mock-selection-gpt-test-model",
        catalogRevision: nil, reason: nil),
      communicationProfileRevision: 0, activeChoiceSetId: nil,
      activeInterpretationRevision: nil, openedAtMs: 0, lastInputAtMs: 1,
      softIdleAtMs: 1_800_001, staleReviewAtMs: 86_400_001,
      primaryDeliveryBindingId: "binding-1", pendingConfirmationId: confirmation.id,
      backgroundMissionIds: []),
    activeBatch: nil, interpretation: nil, activeChoiceSet: nil, lastSelection: nil,
    confirmation: confirmation, documentManifest: testChoiceLoopManifest())
  #expect((try? snapshot.validated()) != nil)
  await core.setChoiceLoopSnapshot(snapshot)
  await core.restoreFromDashboard(
    mission: nil,
    receipt: MissionReceipt(
      id: "choice-receipt-1", missionId: "choice-mission-1", summary: "Verified",
      actualModel: confirmation.modelProvenance.modelId, evidenceIds: ["evidence-1"],
      outputHashes: [confirmation.payloadDigest], completedAtMs: 10))
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()

  #expect(model.confirmedMission == nil)
  #expect(model.receipt?.id == "choice-receipt-1")
  #expect(model.receiptIsForCurrentChoice)
  #expect(model.receiptIsPresentableOnHome)

  // The authenticated next-choice handoff may clear the old confirmation.
  // The durable Receipt stays in Activity, but Home must not reinterpret it
  // as a retired historical Done card beside the new Choice.
  await core.setChoiceLoopSnapshot(testActiveChoiceLoopSnapshot())
  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(model.receipt?.id == "choice-receipt-1")
  #expect(!model.receiptIsForCurrentChoice)
  #expect(!model.receiptIsPresentableOnHome)
  await model.updateEnabled(false)
}

@MainActor
@Test
func choiceBeginUsesOnlyCurrentSelectionAndHostDerivedAcceptance() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  #expect(model.modelEntryEnabled)
  #expect(model.choiceLoopContinuityState == .empty)
  #expect(model.dashboardControls.outcomeInputEnabled)
  #expect(model.persistedModelSelection != nil)
  #expect(model.modelSelectionStatus == .current)
  let accepted = ChoiceBeginAccepted(
    requestId: "choice-request-1", operationId: "choice-operation-1",
    choiceSessionId: "session-choice-1", acceptedSessionRevision: 1,
    sourceEnvelopeId: "source-envelope-1", conversationTurnBatchId: "batch-choice-1",
    state: "interpreting")
  await core.setChoiceLoopSnapshot(testInterpretingChoiceLoopSnapshot())
  await core.setChoiceBeginAccepted(accepted)
  #expect(model.choiceLoopSnapshot == nil)
  #expect(model.modelEntryEnabled)
  #expect(model.dashboardControls.outcomeInputEnabled)

  model.choiceQuestion = "Plan one bounded task"
  await model.submitChoiceQuestion()

  let parameters = await core.choiceBeginParameters
  #expect(parameters.count == 1)
  #expect(parameters[0].boundedLocalQuestion == "Plan one bounded task")
  #expect(parameters[0].expectedModelProvenanceRef == "mock-selection-gpt-test-model")
  #expect(model.choiceLoopSnapshot?.session.id == accepted.choiceSessionId)
  #expect(model.choiceLoopSnapshot?.session.state == "interpreting")
  #expect(model.choiceQuestion.isEmpty)
  #expect(await core.proposalCount == 0)
  await model.updateEnabled(false)
}

@MainActor
@Test
func homeComposerUsesTheExactChoiceIntakeByteBoundary() async {
  let acceptedCore = MockCore()
  let acceptedModel = AppModel(core: acceptedCore, broker: MockBroker()) {}
  await acceptedModel.updateEnabled(true)
  await acceptedModel.refreshAccountAndModels()
  let accepted = ChoiceBeginAccepted(
    requestId: "choice-request-1", operationId: "choice-operation-1",
    choiceSessionId: "session-choice-1", acceptedSessionRevision: 1,
    sourceEnvelopeId: "source-envelope-1",
    conversationTurnBatchId: "batch-choice-1", state: "interpreting")
  await acceptedCore.setChoiceLoopSnapshot(testInterpretingChoiceLoopSnapshot())
  await acceptedCore.setChoiceBeginAccepted(accepted)

  acceptedModel.choiceQuestion = String(repeating: "a", count: 4_096)
  #expect(acceptedModel.dashboardControls.outcomeSubmitEnabled)
  await acceptedModel.submitHomeComposer()
  #expect(await acceptedCore.choiceBeginParameters.count == 1)
  #expect(
    (await acceptedCore.choiceBeginParameters.first)?.boundedLocalQuestion.utf8.count == 4_096)
  #expect(acceptedModel.choiceQuestion.isEmpty)

  let rejectedCore = MockCore()
  let rejectedModel = AppModel(core: rejectedCore, broker: MockBroker()) {}
  await rejectedModel.updateEnabled(true)
  await rejectedModel.refreshAccountAndModels()
  rejectedModel.choiceQuestion = String(repeating: "a", count: 4_097)
  #expect(!rejectedModel.dashboardControls.outcomeSubmitEnabled)
  await rejectedModel.submitHomeComposer()
  #expect(await rejectedCore.choiceBeginParameters.isEmpty)
  #expect(rejectedModel.choiceQuestion.utf8.count == 4_097)
}

@MainActor
@Test
func choiceOptionAndCancellationRemainLocalIntentOperations() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot == active)

  let refining = testRefiningChoiceLoopSnapshot(from: active)
  await core.setChoiceLoopSnapshot(refining)
  await model.selectChoiceOption(active.activeChoiceSet!.options[0])
  let selections = await core.choiceSelections
  #expect(selections.count == 1)
  #expect(selections[0].type == "optionSelection")
  #expect(selections[0].dInputBatchId == nil)
  #expect(model.choiceLoopSnapshot?.session.state == "refining")
  #expect(model.choiceLoopSnapshot?.lastSelection == selections[0])
  #expect(await core.proposalCount == 0)

  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()
  await core.setChoiceCancellationResponse(testCancelledChoiceLoopSnapshot(from: active))
  await model.cancelChoiceSession()
  #expect(model.choiceLoopSnapshot?.session.state == "cancelled")
  #expect(await core.proposalCount == 0)
  await model.updateEnabled(false)
}

@MainActor
@Test
func hydratedReminderScheduleReplaysExactlyAndEditsMintANewRevision() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let snapshot = testChoiceLoopSnapshot()
  let dueAtMs = Int64(Date().timeIntervalSince1970 * 1_000) + 3_600_000
  let input = ChoiceReminderScheduleInput(
    requestId: "schedule-request-1", choiceSessionId: snapshot.session.id,
    expectedSessionRevision: snapshot.session.revision, reminderListId: "local-reminders",
    reminderCount: 1, dueAtMs: dueAtMs, timeZone: "America/Los_Angeles")
  let schedule = ChoiceReminderSchedule(
    id: "schedule-1", input: input, revision: 1,
    acceptedAtMs: Int64(Date().timeIntervalSince1970 * 1_000))
  await core.setChoiceLoopSnapshot(snapshot)
  await core.setChoiceReminderSchedule(schedule)
  await core.setChoiceConfirmationResponse(
    testChoiceConfirmation(
      deliveryBindingId: nil, recipient: nil, deliveryScope: nil))
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()

  await model.prepareChoiceConfirmation()
  let replay = await core.recordedChoiceReminderScheduleInputs()
  #expect(replay.isEmpty, "restart hydration must not mint or rewrite a schedule revision")

  model.choiceReminderCount = "2"
  model.invalidateChoiceReminderScheduleDraft()
  await model.prepareChoiceConfirmation()
  let edited = await core.recordedChoiceReminderScheduleInputs()
  #expect(edited.count == 1)
  #expect(edited[0].requestId != input.requestId)
  #expect(edited[0].reminderCount == 2)
  await model.updateEnabled(false)
}

@MainActor
@Test
func nativeReminderPickerCreatesNoScheduleUntilEveryExplicitFieldIsBound() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let snapshot = testChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(snapshot)
  await core.setChoiceConfirmationResponse(
    testChoiceConfirmation(deliveryBindingId: nil, recipient: nil, deliveryScope: nil))
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()

  #expect(model.choiceReminderDateTime.isEmpty)
  #expect(model.choiceReminderTimeZone.isEmpty)
  #expect(model.choiceReminderListId.isEmpty)
  #expect(model.choiceReminderCount.isEmpty)

  model.selectChoiceReminderDate(Date().addingTimeInterval(3_600))
  #expect(model.choiceReminderDateTime.isEmpty)
  model.selectChoiceReminderTimeZone("Etc/UTC")
  model.selectChoiceReminderList("openopen.default-reminders")
  await model.prepareChoiceConfirmation()

  let inputs = await core.recordedChoiceReminderScheduleInputs()
  #expect(inputs.count == 1)
  #expect(inputs[0].timeZone == "Etc/UTC")
  #expect(inputs[0].reminderListId == "openopen.default-reminders")
  #expect(inputs[0].reminderCount == 1)
  #expect(inputs[0].dueAtMs > Int64(Date().timeIntervalSince1970 * 1_000))
  await model.updateEnabled(false)
}

@MainActor
@Test
func mismatchedDurableReminderScheduleCannotPublishAFalseHealthyChoiceState() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let snapshot = testChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(snapshot)
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()
  let knownGood = model.choiceLoopSnapshot

  let now = Int64(Date().timeIntervalSince1970 * 1_000)
  let mismatched = ChoiceReminderSchedule(
    id: "schedule-mismatched",
    input: ChoiceReminderScheduleInput(
      requestId: "schedule-mismatched-request", choiceSessionId: "other-session",
      expectedSessionRevision: snapshot.session.revision, reminderListId: "local-reminders",
      reminderCount: 1, dueAtMs: now + 3_600_000, timeZone: "Etc/UTC"),
    revision: 1, acceptedAtMs: now)
  await core.setChoiceReminderSchedule(mismatched)
  await model.refreshDashboard()

  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .needsYou(.invalidContract))
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  await model.updateEnabled(false)
}

@MainActor
@Test
func choiceCancellationStaysReachableWhenCurrentModelSelectionDrifts() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream)
  await model.updateEnabled(true)
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()

  await core.setModelCatalog([
    GptModel(
      id: "gpt-test-model", displayName: "Changed catalog label",
      supportedReasoningEfforts: ["high"])
  ])
  await model.refreshAccountAndModels()
  #expect(!model.modelEntryEnabled)
  #expect(model.storeControlEnabled)

  await core.setChoiceCancellationResponse(testCancelledChoiceLoopSnapshot(from: active))
  await model.cancelChoiceSession()
  #expect(model.choiceLoopSnapshot?.session.state == "cancelled")
  #expect(await core.proposalCount == 0)
  await model.updateEnabled(false)
}

@MainActor
@Test
func durableExecutingChoiceJournalAllowsAnExplicitNextLocalQuestion() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let executing = testExecutingChoiceLoopSnapshot(from: testActiveChoiceLoopSnapshot())
  await core.setChoiceLoopSnapshot(executing)
  await model.refreshDashboard()

  #expect(model.choiceLoopSnapshot?.session.state == "executing")
  #expect(model.dashboardControls.outcomeInputEnabled)
  #expect(!model.dashboardControls.outcomeSubmitEnabled)
  #expect(await core.proposalCount == 0)
  await model.updateEnabled(false)
}

@Test
func documentManifestDigestMatchesRustGoldenVectorAndRejectsMutations() throws {
  let first = DocumentManifestEntry(
    relativePath: "sessions/session-1/SESSION.md",
    sha256: String(repeating: "a", count: 64), byteLength: 64, mode: 0o600)
  let second = DocumentManifestEntry(
    relativePath: "sessions/session-1/choice-sets/choices-1.md",
    sha256: String(repeating: "b", count: 64), byteLength: 128, mode: 0o600)
  let entries = [first, second]
  let digest = try #require(DocumentManifest.canonicalAggregateDigest(entries: entries))

  // This is emitted by the Rust canonical_document_manifest_digest test, not
  // recomputed by a second Swift implementation during this assertion.
  #expect(digest == "2556788916fce4a341d7a2a2fbbd81c51d97d2af8da83e2fe890530822f4f8e7")
  #expect(DocumentManifest.canonicalAggregateDigest(entries: [second, first]) == digest)

  let changedLength = DocumentManifestEntry(
    relativePath: first.relativePath, sha256: first.sha256,
    byteLength: first.byteLength + 1, mode: first.mode)
  #expect(DocumentManifest.canonicalAggregateDigest(entries: [changedLength, second]) != digest)

  let changedPath = DocumentManifestEntry(
    relativePath: "sessions/session-1/choice-sets/choices-2.md", sha256: first.sha256,
    byteLength: first.byteLength, mode: first.mode)
  #expect(DocumentManifest.canonicalAggregateDigest(entries: [changedPath, second]) != digest)

  let changedMode = DocumentManifestEntry(
    relativePath: first.relativePath, sha256: first.sha256,
    byteLength: first.byteLength, mode: 0o601)
  #expect(DocumentManifest.canonicalAggregateDigest(entries: [changedMode, second]) == nil)

  let unknownPath = DocumentManifestEntry(
    relativePath: "scratch/plan.md", sha256: first.sha256,
    byteLength: first.byteLength, mode: first.mode)
  #expect(DocumentManifest.canonicalAggregateDigest(entries: [unknownPath, second]) == nil)
}

@Test
func choicePersonaProvenanceIsRequiredAndRejectsDigestDrift() throws {
  let snapshot = testActiveChoiceLoopSnapshot()
  let encoded = try JSONEncoder().encode(snapshot)
  var missing = try #require(
    JSONSerialization.jsonObject(with: encoded) as? [String: Any])
  var activeChoiceSet = try #require(missing["activeChoiceSet"] as? [String: Any])
  activeChoiceSet.removeValue(forKey: "personaRevision")
  missing["activeChoiceSet"] = activeChoiceSet
  let missingData = try JSONSerialization.data(withJSONObject: missing)
  #expect(throws: DecodingError.self) {
    _ = try JSONDecoder().decode(ChoiceLoopSnapshot.self, from: missingData)
  }

  var drifted = try #require(
    JSONSerialization.jsonObject(with: encoded) as? [String: Any])
  var driftedChoiceSet = try #require(drifted["activeChoiceSet"] as? [String: Any])
  var persona = try #require(driftedChoiceSet["personaRevision"] as? [String: Any])
  persona["aggregateDigest"] = String(repeating: "g", count: 64)
  driftedChoiceSet["personaRevision"] = persona
  drifted["activeChoiceSet"] = driftedChoiceSet
  let driftedData = try JSONSerialization.data(withJSONObject: drifted)
  let decoded = try JSONDecoder().decode(ChoiceLoopSnapshot.self, from: driftedData)
  #expect(throws: CoreClientError.self) {
    _ = try decoded.validated()
  }
}

@MainActor
@Test
func choiceContinuityDistinguishesEmptyFromFailureAndPreservesLastKnownGood() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let knownGood = testChoiceLoopSnapshot()

  await core.setChoiceLoopSnapshot(knownGood)
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .current)

  await core.failNextChoiceLoopRead()
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .needsYou(.readFailed))
  #expect(model.errorMessage == nil)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)

  await core.failNextChoiceLoopRead()
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .needsYou(.readFailed))
  #expect(model.errorMessage == nil)

  await core.setChoiceLoopSnapshot(nil)
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot == nil)
  #expect(model.choiceLoopContinuityState == .empty)
}

@MainActor
@Test
func choiceClockUncertaintyPreservesLastKnownGoodWithoutBlockingOffOrSettings() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let knownGood = testChoiceLoopSnapshot()

  await core.setChoiceLoopSnapshot(knownGood)
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot == knownGood)

  await core.failNextChoiceLoopRead(
    with: .remote(
      code: -32_025,
      message: "Local clock continuity is uncertain. Refresh before choosing or confirming."))
  await model.refreshDashboard()

  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .needsYou(.clockUncertain))
  #expect(model.errorMessage == nil)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
}

@MainActor
@Test
func reminderScheduleClockFenceIsTypedAndNeverPublishesAnUnverifiedSnapshot() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let knownGood = testChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(knownGood)
  await model.refreshDashboard()

  let later = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(later)
  await core.failNextChoiceReminderScheduleRead(
    with: .remote(
      code: -32_025,
      message: "Local clock continuity is uncertain. Refresh before choosing or confirming."))
  await model.refreshDashboard()

  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .needsYou(.clockUncertain))
  #expect(model.errorMessage == nil)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)

  await core.failNextChoiceReminderScheduleRead(
    with: .remote(
      code: -32_026,
      message: "The Choice session advanced. Refresh before choosing or confirming."))
  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .needsYou(.refreshRequired))
  #expect(model.dashboardControls.globalToggleEnabled)
}

@MainActor
@Test
func choiceContinuityRejectsInvalidAggregateWithoutErasingKnownGood() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let knownGood = testChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(knownGood)
  await model.refreshDashboard()

  await core.setChoiceLoopSnapshot(
    testChoiceLoopSnapshot(aggregateDigest: String(repeating: "b", count: 64)))
  await model.refreshDashboard()

  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .needsYou(.invalidContract))
  #expect(model.errorMessage == nil)
  #expect(model.dashboardControls.globalToggleEnabled)
}

private enum ChoiceContinuityActionFenceFixture: CaseIterable {
  case clockUncertain
  case refreshRequired
  case invalidContract
  case readFailed

  var expectedState: ChoiceLoopContinuityState {
    switch self {
    case .clockUncertain: .needsYou(.clockUncertain)
    case .refreshRequired: .needsYou(.refreshRequired)
    case .invalidContract: .needsYou(.invalidContract)
    case .readFailed: .needsYou(.readFailed)
    }
  }
}

private func injectChoiceContinuityFailure(
  _ fixture: ChoiceContinuityActionFenceFixture, into core: MockCore
) async {
  switch fixture {
  case .clockUncertain:
    await core.failNextChoiceLoopRead(
      with: .remote(
        code: -32_025,
        message: "Local clock continuity is uncertain. Refresh before continuing."))
  case .refreshRequired:
    await core.failNextChoiceReminderScheduleRead(
      with: .remote(
        code: -32_026,
        message: "The Choice session advanced. Refresh before continuing."))
  case .invalidContract:
    await core.setChoiceLoopSnapshot(
      testChoiceLoopSnapshot(aggregateDigest: String(repeating: "b", count: 64)))
  case .readFailed:
    await core.failNextChoiceLoopRead()
  }
}

@MainActor
@Test
func unhealthyChoiceContinuityFencesEveryConsumingActionButKeepsSafetyControlsUsable() async {
  for fixture in ChoiceContinuityActionFenceFixture.allCases {
    let core = MockCore()
    let model = AppModel(core: core, broker: MockBroker()) {}
    await model.updateEnabled(true)
    await model.refreshAccountAndModels()

    let active = testActiveChoiceLoopSnapshot()
    await core.setChoiceLoopSnapshot(active)
    await core.setChoiceConfirmationResponse(
      testChoiceConfirmation(
        deliveryBindingId: nil, recipient: nil, deliveryScope: nil,
        choiceSessionId: active.session.id, choiceSetId: active.activeChoiceSet!.id,
        selectionId: "choice-selection-preview",
        expectedSessionRevision: active.session.revision))
    await model.refreshDashboard()
    await model.prepareChoiceConfirmation()
    #expect(model.choiceConfirmationPreview != nil)

    let optionCalls = await core.choiceSelections.count
    let dCalls = await core.choiceDInputs.count
    let prepareCalls = await core.choiceConfirmationPrepareCount
    let confirmCalls = await core.choiceConfirmations.count
    let scheduleCalls = await core.choiceReminderScheduleInputs.count

    await injectChoiceContinuityFailure(fixture, into: core)
    await model.refreshDashboard()
    #expect(model.choiceLoopContinuityState == fixture.expectedState)
    #expect(!model.choiceSessionActionEnabled)
    #expect(model.choiceConfirmationPreview != nil)
    #expect(model.dashboardControls.globalToggleEnabled)
    #expect(model.dashboardControls.settingsEnabled)

    await model.selectChoiceOption(active.activeChoiceSet!.options[0])
    await model.selectChoiceD("Keep this local")
    await model.prepareChoiceConfirmation()
    await model.confirmPreparedChoice()

    #expect(await core.choiceSelections.count == optionCalls)
    #expect(await core.choiceDInputs.count == dCalls)
    #expect(await core.choiceConfirmationPrepareCount == prepareCalls)
    #expect(await core.choiceConfirmations.count == confirmCalls)
    #expect(await core.choiceReminderScheduleInputs.count == scheduleCalls)

    await core.setChoiceCancellationResponse(testCancelledChoiceLoopSnapshot(from: active))
    await model.cancelChoiceSession()
    #expect(model.choiceLoopSnapshot?.session.state == "cancelled")
    model.showsSettings = true
    #expect(model.showsSettings)

    // A terminal last-known-good session would normally permit a new local
    // question. Re-introduce the continuity issue and prove begin is fenced by
    // AppModel itself rather than merely hidden by the view.
    await injectChoiceContinuityFailure(fixture, into: core)
    await model.refreshDashboard()
    #expect(model.choiceLoopContinuityState == fixture.expectedState)
    #expect(!model.dashboardControls.outcomeInputEnabled)
    model.choiceQuestion = "Start another bounded choice"
    await model.submitChoiceQuestion()
    #expect(await core.choiceBeginParameters.isEmpty)

    await model.updateEnabled(false)
    #expect(model.runtimeDisplayState == .off)
    #expect(!model.enabled)
  }
}

@MainActor
@Test
func dCardUsesTheExistingComposerWithoutBeginFallback() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()

  model.focusChoiceDComposer()
  #expect(model.choiceDComposerFocusRequested)
  #expect(model.dashboardControls.outcomeInputEnabled)
  model.consumeChoiceDComposerFocusRequest()
  #expect(!model.choiceDComposerFocusRequested)

  model.choiceQuestion = "Use a different local direction"
  #expect(model.dashboardControls.outcomeSubmitEnabled)

  await core.setChoiceLoopSnapshot(testRefiningChoiceLoopSnapshot(from: active))
  await model.submitHomeComposer()

  #expect(await core.choiceDInputs.count == 1)
  #expect((await core.choiceDInputs.first)?.boundedText == "Use a different local direction")
  #expect(await core.choiceBeginParameters.isEmpty)
  #expect(model.choiceQuestion.isEmpty)
}

@MainActor
@Test
func dComposerUsesTheExactChoiceIntakeByteBoundaryWithoutBeginFallback() async {
  let acceptedCore = MockCore()
  let acceptedModel = AppModel(core: acceptedCore, broker: MockBroker()) {}
  await acceptedModel.updateEnabled(true)
  await acceptedModel.refreshAccountAndModels()
  let acceptedActive = testActiveChoiceLoopSnapshot()
  await acceptedCore.setChoiceLoopSnapshot(acceptedActive)
  await acceptedModel.refreshDashboard()
  acceptedModel.focusChoiceDComposer()
  acceptedModel.choiceQuestion = String(repeating: "d", count: 4_096)
  #expect(acceptedModel.dashboardControls.outcomeSubmitEnabled)
  await acceptedCore.setChoiceLoopSnapshot(testRefiningChoiceLoopSnapshot(from: acceptedActive))
  await acceptedModel.submitHomeComposer()
  #expect(await acceptedCore.choiceDInputs.count == 1)
  #expect((await acceptedCore.choiceDInputs.first)?.boundedText.utf8.count == 4_096)
  #expect(await acceptedCore.choiceBeginParameters.isEmpty)
  #expect(acceptedModel.choiceQuestion.isEmpty)

  let rejectedCore = MockCore()
  let rejectedModel = AppModel(core: rejectedCore, broker: MockBroker()) {}
  await rejectedModel.updateEnabled(true)
  await rejectedModel.refreshAccountAndModels()
  let rejectedActive = testActiveChoiceLoopSnapshot()
  await rejectedCore.setChoiceLoopSnapshot(rejectedActive)
  await rejectedModel.refreshDashboard()
  rejectedModel.focusChoiceDComposer()
  rejectedModel.choiceQuestion = String(repeating: "d", count: 4_097)
  #expect(!rejectedModel.dashboardControls.outcomeSubmitEnabled)
  await rejectedModel.submitHomeComposer()
  #expect(await rejectedCore.choiceDInputs.isEmpty)
  #expect(await rejectedCore.choiceBeginParameters.isEmpty)
  #expect(rejectedModel.choiceQuestion.utf8.count == 4_097)
}

@MainActor
@Test
func globalOffRevokesTheTransientDTargetWithoutReinterpretingItsDraft() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()
  model.focusChoiceDComposer()
  model.choiceQuestion = "Keep this draft across Off"

  await model.updateEnabled(false)
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()
  await model.submitHomeComposer()

  #expect(model.choiceQuestion == "Keep this draft across Off")
  #expect(await core.choiceDInputs.isEmpty)
  #expect(await core.choiceBeginParameters.isEmpty)
}

@MainActor
@Test
func dResponseLossAndExplicitCancellationRetireLocalComposerBodies() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()
  model.focusChoiceDComposer()
  model.choiceQuestion = "Keep only the durable D request"
  await core.setChoiceLoopSnapshot(testRefiningChoiceLoopSnapshot(from: active))
  await core.failNextChoiceDInputAfterAcceptance(
    with: .requestTimedOut)

  await model.submitHomeComposer()
  for _ in 0..<100 where !model.choiceQuestion.isEmpty {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.choiceQuestion.isEmpty)
  #expect(await core.choiceBeginParameters.isEmpty)

  // An unaccepted request remains visible for the owner, but an explicit
  // Choice cancellation clears the local body and replay cache.
  let secondActive = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(secondActive)
  await model.refreshDashboard()
  model.focusChoiceDComposer()
  model.choiceQuestion = "Cancel this local D draft"
  await core.failNextChoiceDInput(with: .requestTimedOut)
  await model.submitHomeComposer()
  #expect(model.choiceQuestion == "Cancel this local D draft")
  await core.setChoiceCancellationResponse(testCancelledChoiceLoopSnapshot(from: secondActive))
  await core.failNextChoiceCancellationAfterAcceptance(with: .requestTimedOut)
  await model.cancelChoiceSession()
  for _ in 0..<100 where !model.choiceQuestion.isEmpty {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.choiceQuestion.isEmpty)
}

@MainActor
@Test
func unexpectedAcceptedDResponseStillRetiresOnlyTheDurablyAcceptedComposerBody() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()
  model.focusChoiceDComposer()
  model.choiceQuestion = "Retire this accepted D body after a malformed success"
  await core.setChoiceLoopSnapshot(testRefiningChoiceLoopSnapshot(from: active))
  // The mocked Core commits the exact D request, but returns the old Active
  // snapshot. App-side state validation rejects that response; only the
  // follow-up durable read may prove acceptance and clear local plaintext.
  await core.returnUnexpectedChoiceDResponseAfterAcceptance(active)

  await model.submitHomeComposer()
  for _ in 0..<100 where !model.choiceQuestion.isEmpty {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.choiceQuestion.isEmpty)
  #expect(await core.choiceBeginParameters.isEmpty)
}

@MainActor
@Test
func acceptedDCoreTerminationRaceRetiresTheComposerBodyAfterGenerationRecovery() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core, broker: MockBroker(), registerLoginItem: {},
    coreTerminationEvents: events.stream)
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()
  model.focusChoiceDComposer()
  model.choiceQuestion = "Retire D text after the accepted Core-termination race"
  await core.setChoiceLoopSnapshot(testRefiningChoiceLoopSnapshot(from: active))
  await core.failNextChoiceDInputAfterAcceptance(
    with: .processTerminated,
    beforeThrow: {
      events.send(CoreTerminationEvent(generation: 1, reason: .transportFailure, exitStatus: nil))
    })

  await model.submitHomeComposer()
  for _ in 0..<200 where !model.choiceQuestion.isEmpty {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.choiceQuestion.isEmpty)
  #expect(await core.choiceBeginParameters.isEmpty)
}

@MainActor
@Test
func staleDRejectionPreservesComposerTextAndCannotFallBackToChoiceBegin() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(active)
  await model.refreshDashboard()
  model.focusChoiceDComposer()
  model.choiceQuestion = "Keep this unaccepted D draft"

  // The Host rejects after a durable state advance. AppModel must retain the
  // text and never reinterpret it as a new first local question.
  await core.setChoiceLoopSnapshot(testRefiningChoiceLoopSnapshot(from: active))
  await core.failNextChoiceDInput(
    with: .remote(code: -32_001, message: "ChoiceSet is no longer current."))
  await model.submitHomeComposer()
  #expect(model.choiceQuestion == "Keep this unaccepted D draft")
  #expect(await core.choiceBeginParameters.isEmpty)

  // The rejection is definitive, but its continuity refresh is still
  // read-only. The old active card must not remain the only visible path.
  for _ in 0..<100 where model.choiceLoopSnapshot?.session.state != "refining" {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.choiceLoopSnapshot?.session.state == "refining")
  #expect(model.choiceQuestion == "Keep this unaccepted D draft")

  await model.submitHomeComposer()
  #expect(model.choiceQuestion == "Keep this unaccepted D draft")
  #expect(await core.choiceBeginParameters.isEmpty)
}

@MainActor
@Test
func idleRefreshResumesOnceAndNeverInvokesResumeWhileOff() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  let idle = testChoiceLoopSnapshot(from: active, state: "softIdle")
  await core.setChoiceLoopSnapshot(idle)
  await core.setChoiceResumeResponse(testRefiningChoiceLoopSnapshot(from: active))
  await core.clearChoiceCallTrace()

  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 1)
  let trace = await core.choiceCallTrace
  #expect(trace.lastIndex(of: "modelSetup")! < trace.lastIndex(of: "resume")!)
  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 1)

  await model.updateEnabled(false)
  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 1)
}

@MainActor
@Test
func idleResumeFailureAllowsOneLaterNewRevisionButNeverLoopsTheSameRevision() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  let firstIdle = testChoiceLoopSnapshot(from: active, state: "softIdle")
  await core.setChoiceLoopSnapshot(firstIdle)

  // A successful resume begins the private worker/poll path. When that worker
  // later returns a fresh idle revision, the poll is a read-only observation:
  // it must not turn the failure into another automatic resume attempt.
  await core.setChoiceResumeResponse(testRefiningChoiceLoopSnapshot(from: active))
  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 1)
  let laterIdle = testResumeIdleChoiceLoopSnapshot(from: active, revision: 3)
  await core.setChoiceLoopSnapshot(laterIdle)
  try? await Task.sleep(for: .milliseconds(1_100))
  #expect(await core.choiceResumeCount == 1)

  // A later genuine dashboard/foreground return may receive one new attempt.
  await core.setChoiceResumeResponse(testRefiningChoiceLoopSnapshot(from: active))
  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 2)
  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 2)
}

@MainActor
@Test
func acceptedResumeResponseLossReattachesOnlyThePersistedResultWorker() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  let idle = testChoiceLoopSnapshot(from: active, state: "softIdle")
  let ordinaryRefining = testRefiningChoiceLoopSnapshot(from: active)
  let ownerResumeOperation = ChoiceRefinementOperation(
    id: "resume-operation-choice-1", selectionId: "resume-soft-idle-choice-1",
    choiceSessionId: active.session.id, sourceEnvelopeId: "source-envelope-choice-1",
    conversationTurnBatchId: "batch-choice-1", expectedSessionRevision: 3,
    expectedGeneration: 1, modelProvenance: active.activeChoiceSet!.modelProvenance,
    sourceManifestDigest: active.documentManifest.aggregateDigest,
    personaRevision: active.activeChoiceSet!.personaRevision,
    dRequestId: nil, dInputDigest: nil, createdAtMs: 11)
  let refining = ChoiceLoopSnapshot(
    session: ordinaryRefining.session, activeBatch: ordinaryRefining.activeBatch,
    interpretation: ordinaryRefining.interpretation,
    activeChoiceSet: ordinaryRefining.activeChoiceSet,
    lastSelection: ordinaryRefining.lastSelection, pendingRefinementOperation: ownerResumeOperation,
    confirmation: ordinaryRefining.confirmation, documentManifest: ordinaryRefining.documentManifest
  )
  await core.setChoiceLoopSnapshot(idle)
  await core.setChoiceResumeResponse(refining)
  await core.failNextChoiceResumeAfterAcceptance(with: .requestTimedOut)

  await model.refreshDashboard(authenticatedHomeForeground: true)
  for _ in 0..<100 where model.choiceLoopSnapshot?.session.state != "refining" {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(await core.choiceResumeCount == 1)
  #expect(model.choiceLoopSnapshot?.session.state == "refining")

  await core.setChoiceLoopSnapshot(active)
  for _ in 0..<150 where model.choiceLoopSnapshot?.session.state != "active" {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.choiceLoopSnapshot?.session.state == "active")
  #expect(await core.choiceResumeCount == 1)
}

@MainActor
@Test
func idleChoiceRefreshRequiresAnExplicitHomeForegroundSignal() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(testChoiceLoopSnapshot(from: active, state: "softIdle"))
  await core.setChoiceResumeResponse(testRefiningChoiceLoopSnapshot(from: active))

  // Settings/root/recovery reads preserve continuity but cannot become an
  // owner-return event merely by presenting the root window.
  await model.refreshDashboard()
  #expect(await core.choiceResumeCount == 0)

  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 1)
}

@MainActor
@Test
func dashboardRetryCannotTurnOneHomeReturnIntoTwoResumeAttempts() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(testChoiceLoopSnapshot(from: active, state: "softIdle"))
  await core.setChoiceResumeResponse(testRefiningChoiceLoopSnapshot(from: active))

  // The first dashboard read fails and the AppModel retries internally. That
  // retry must remain a read; the next resume is reserved for a later Home
  // return, not an implementation retry.
  await core.failNextDashboardRead()
  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 0)

  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 1)
}

@MainActor
@Test
func settingsFirstRootPresentationNeverSignalsAnOwnerHomeReturn() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(testChoiceLoopSnapshot(from: active, state: "softIdle"))
  await core.setChoiceResumeResponse(testRefiningChoiceLoopSnapshot(from: active))
  model.showsSettings = true

  _ = NSApplication.shared
  let hosting = NSHostingView(rootView: OpenOpenRootView(model: model))
  let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: 900, height: 760),
    styleMask: [.titled, .closable], backing: .buffered, defer: false)
  window.animationBehavior = .none
  window.isReleasedWhenClosed = false
  window.contentView = hosting
  window.makeKeyAndOrderFront(nil)
  defer {
    window.orderOut(nil)
    window.contentView = nil
  }

  // Give the root's read-only refresh a chance to settle.  The initial
  // section is Settings, so DashboardView is never transiently constructed.
  try? await Task.sleep(for: .milliseconds(80))
  #expect(await core.choiceResumeCount == 0)
}

@MainActor
@Test
func recoveredCoreGenerationMayResumeTheExactPersistedOwnerResumeOnce() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core, broker: MockBroker(), registerLoginItem: {},
    coreTerminationEvents: events.stream)
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  let active = testActiveChoiceLoopSnapshot()
  let idle = testChoiceLoopSnapshot(from: active, state: "softIdle")
  let ordinaryRefining = testRefiningChoiceLoopSnapshot(from: active)
  let ownerResumeOperation = ChoiceRefinementOperation(
    id: "resume-operation-choice-1", selectionId: "resume-soft-idle-choice-1",
    choiceSessionId: active.session.id, sourceEnvelopeId: "source-envelope-choice-1",
    conversationTurnBatchId: "batch-choice-1", expectedSessionRevision: 3,
    expectedGeneration: 1, modelProvenance: active.activeChoiceSet!.modelProvenance,
    sourceManifestDigest: active.documentManifest.aggregateDigest,
    personaRevision: active.activeChoiceSet!.personaRevision,
    dRequestId: nil, dInputDigest: nil, createdAtMs: 11)
  let refining = ChoiceLoopSnapshot(
    session: ordinaryRefining.session, activeBatch: ordinaryRefining.activeBatch,
    interpretation: ordinaryRefining.interpretation,
    activeChoiceSet: ordinaryRefining.activeChoiceSet,
    lastSelection: ordinaryRefining.lastSelection, pendingRefinementOperation: ownerResumeOperation,
    confirmation: ordinaryRefining.confirmation, documentManifest: ordinaryRefining.documentManifest
  )
  await core.setChoiceLoopSnapshot(idle)
  await core.setChoiceResumeResponse(refining)

  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 1)

  // The durable Store row is exactly the owner-resume operation returned by
  // the first Core. A replacement generation may recover that row once; the
  // prior generation's local dedupe must never strand the foreground session.
  await core.setChoiceLoopSnapshot(refining)
  await core.simulateCoreReplacement()
  let dashboardReadsBeforeRecovery = await core.dashboardInvocationCount
  events.send(CoreTerminationEvent(generation: 1, reason: .exited, exitStatus: 0))
  for _ in 0..<250 where await core.dashboardInvocationCount <= dashboardReadsBeforeRecovery {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(await core.dashboardInvocationCount > dashboardReadsBeforeRecovery)
  for _ in 0..<250
  where model.choiceLoopSnapshot?.session.state != "refining"
    || model.runtimeRecoveryState != .ready
  {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.modelEntryEnabled)
  #expect(model.choiceLoopSnapshot?.session.state == "refining")
  #expect(model.choiceLoopSnapshot?.pendingRefinementOperation?.isOwnerResume == true)

  await model.refreshDashboard(authenticatedHomeForeground: true)
  #expect(await core.choiceResumeCount == 2)
}

@Test
func ownerResumeRequiresStoreMintedOperationAndSelectionMarkers() {
  let active = testActiveChoiceLoopSnapshot()
  let forged = ChoiceRefinementOperation(
    id: "choice-refinement-forged", selectionId: "resume-soft-idle-forged",
    choiceSessionId: active.session.id, sourceEnvelopeId: "source-envelope-choice-1",
    conversationTurnBatchId: "batch-choice-1", expectedSessionRevision: 3,
    expectedGeneration: 1, modelProvenance: active.activeChoiceSet!.modelProvenance,
    sourceManifestDigest: active.documentManifest.aggregateDigest,
    personaRevision: active.activeChoiceSet!.personaRevision,
    dRequestId: nil, dInputDigest: nil, createdAtMs: 11)
  #expect(!forged.isOwnerResume)
}

@MainActor
@Test
func idleAndStaleReviewSnapshotsCannotConsumeAnOldChoiceSet() async {
  for state in ["softIdle", "staleReview"] {
    let core = MockCore()
    let model = AppModel(core: core, broker: MockBroker()) {}
    await model.updateEnabled(true)
    await model.refreshAccountAndModels()

    let active = testActiveChoiceLoopSnapshot()
    await core.setChoiceLoopSnapshot(active)
    await core.setChoiceConfirmationResponse(
      testChoiceConfirmation(
        deliveryBindingId: nil, recipient: nil, deliveryScope: nil,
        choiceSessionId: active.session.id, choiceSetId: active.activeChoiceSet!.id,
        selectionId: "choice-selection-preview",
        expectedSessionRevision: active.session.revision))
    await model.refreshDashboard()
    await model.prepareChoiceConfirmation()
    #expect(model.choiceConfirmationPreview != nil)

    let gated = testChoiceLoopSnapshot(from: active, state: state)
    await core.setChoiceLoopSnapshot(gated)
    await model.refreshDashboard()
    #expect(model.choiceLoopContinuityState == .current)
    #expect(model.choiceLoopSnapshot?.session.state == state)
    #expect(!model.choiceSessionActionEnabled)
    #expect(model.choiceConfirmationPreview == nil)

    let optionCalls = await core.choiceSelections.count
    let dCalls = await core.choiceDInputs.count
    let prepareCalls = await core.choiceConfirmationPrepareCount
    let confirmCalls = await core.choiceConfirmations.count
    await model.selectChoiceOption(active.activeChoiceSet!.options[0])
    await model.selectChoiceD("Do not reuse the old card")
    await model.prepareChoiceConfirmation()
    await model.confirmPreparedChoice()
    #expect(await core.choiceSelections.count == optionCalls)
    #expect(await core.choiceDInputs.count == dCalls)
    #expect(await core.choiceConfirmationPrepareCount == prepareCalls)
    #expect(await core.choiceConfirmations.count == confirmCalls)

    await core.setChoiceCancellationResponse(testCancelledChoiceLoopSnapshot(from: gated))
    await model.cancelChoiceSession()
    #expect(model.choiceLoopSnapshot?.session.state == "cancelled")
    #expect(model.dashboardControls.globalToggleEnabled)
    #expect(model.dashboardControls.settingsEnabled)
    await model.updateEnabled(false)
  }
}

@MainActor
@Test
func cancelledChoiceExposesReceiptCleanupOnlyWhenHostProvesItAvailable() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let cancelled = testCancelledChoiceLoopSnapshot(from: testActiveChoiceLoopSnapshot())
  await core.setChoiceLoopSnapshot(cancelled)

  await model.refreshDashboard()
  #expect(model.choiceLoopSnapshot == cancelled)
  #expect(!model.choiceMarkdownReceiptCleanupAvailable)

  await core.setChoiceMarkdownReceiptCleanupAvailable(true)
  await model.refreshDashboard()
  #expect(model.choiceMarkdownReceiptCleanupAvailable)

  await model.reconcileChoiceMarkdown()
  #expect(await core.choiceMarkdownReceiptCleanupInvocations() == 1)
  // A local transition result cannot inherit cleanup availability from the
  // preceding terminal session; only a new authenticated Host read may set it.
  #expect(!model.choiceMarkdownReceiptCleanupAvailable)
}

@MainActor
@Test
func markdownReconciliationFailureIsANonblockingContinuityIncident() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let knownGood = testExecutingChoiceLoopSnapshot(from: testActiveChoiceLoopSnapshot())
  await core.setChoiceLoopSnapshot(knownGood)
  await model.updateEnabled(true)
  await model.refreshDashboard()

  await core.failNextChoiceMarkdownReconcile()
  await model.reconcileChoiceMarkdown()

  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.choiceLoopContinuityState == .needsYou(.readFailed))
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
}

@MainActor
@Test
func choiceContinuityRejectsLateRefreshAfterOffAndRecoversOnRestart() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  let knownGood = testChoiceLoopSnapshot()
  await core.setChoiceLoopSnapshot(knownGood)
  await model.refreshDashboard()

  let staleRefresh = testChoiceLoopSnapshot(sessionID: "session-2")
  let gate = NonCooperativeRpcGate()
  await core.setChoiceLoopSnapshot(staleRefresh)
  await core.blockNextChoiceLoopRead(on: gate)
  let refresh = Task { await model.refreshDashboard() }
  for _ in 0..<100 where !gate.isWaiting {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(gate.isWaiting)
  model.requestEnabled(false)
  gate.resume()
  await refresh.value

  #expect(model.choiceLoopSnapshot == knownGood)
  #expect(model.dashboardControls.globalToggleEnabled)

  await core.setChoiceLoopSnapshot(knownGood)
  let restarted = AppModel(core: core, broker: MockBroker()) {}
  await restarted.refreshDashboard()
  #expect(restarted.choiceLoopSnapshot == knownGood)
  #expect(restarted.choiceLoopContinuityState == .current)
}

@MainActor
@Test
func protectedOnRemainsAwaitingWhenNoCurrentModelSelectionExists() async {
  let core = MockCore(
    modelCatalog: [
      GptModel(
        id: "gpt-test-model", displayName: "Test model",
        supportedReasoningEfforts: ["low", "medium"])
    ])
  await core.clearPersistedModelSelection()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(model.accountSetupEnabled)
  #expect(!model.modelEntryEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func discordConnectingThenReconnectingThenFaultedIsolatesTheListener() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  await core.queueDiscordStatusResponses(["connecting", "reconnecting", "faulted"])
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)

  #expect(model.enabled)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.modelEntryEnabled)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "faulted")
  #expect(model.channelListenerFeedback[.discord] != nil)
  #expect(await core.discordStatusReadCount == 3)
  #expect(await broker.appliedValues == [true])
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func discordConnectingWithoutConnectedProofTimesOutOnlyThatListener() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  await core.setChannelPollConnectionStatus("connecting")
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)

  #expect(model.enabled)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.modelEntryEnabled)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "faulted")
  #expect(model.channelListenerFeedback[.discord] != nil)
  #expect(await core.discordStatusReadCount == 6)
  #expect(await broker.appliedValues == [true])
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func newerOffWhileDiscordIsConnectingCompletesOffWithoutReplay() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let shutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  await core.queueDiscordStatusResponses(["connecting"])
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      shutdowns.withLock { $0 += 1 }
      return true
    }
  )

  let on = Task { await model.updateEnabled(true) }
  for _ in 0..<100 where await core.discordStatusReadCount == 0 {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(await core.discordStatusReadCount > 0)
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(!model.modelEntryEnabled)

  await model.updateEnabled(false)
  await on.value

  #expect(shutdowns.value == 1)
  #expect(!model.enabled)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .off)
  #expect(!model.modelEntryEnabled)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.discordStatus == "disconnected")
  #expect(await broker.appliedValues == [true, false])
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func newerOnDuringQuiescedOffCompletionRestoresBothListenersBeforePublishingOn() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let blockedIdentity = NonCooperativeRpcGate()
  let shutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      shutdowns.withLock { $0 += 1 }
      return true
    }
  )

  await model.updateEnabled(true)
  await model.refreshDashboard()
  for _ in 0..<75 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")

  await core.simulateCoreReplacement()
  await core.requireBrokerEnrollmentBeforeOff()
  await core.blockNextEffectIdentity(on: blockedIdentity)
  let off = Task { await model.updateEnabled(false) }
  for _ in 0..<75 where !blockedIdentity.isWaiting {
    try? await Task.sleep(for: .milliseconds(5))
  }
  #expect(blockedIdentity.isWaiting)
  model.requestEnabled(true)

  #expect(model.runtimeDisplayState == .turningOn)
  #expect(!model.modelEntryEnabled)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.discordStatus == "disconnected")

  blockedIdentity.resume()
  await off.value
  for _ in 0..<75 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(shutdowns.value == 1)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.modelEntryEnabled)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(await broker.appliedValues == [true, false, true])
  #expect(await core.offPrepareCount == 1)
  #expect(await core.brokerEnrollmentInstallCount == 3)
  #expect(await core.codexInitializeCount == 2)
  #expect(await broker.leaseAcquireCount == 2)
  #expect(await core.iMessageStartCount == 2)
  #expect(await core.discordSessionStartCount == 2)
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func newerOffDuringQuiescedOnRestorationConvergesOffWithoutProviderReplay() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let blockedDiscordStart = NonCooperativeRpcGate()
  let shutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      shutdowns.withLock { $0 += 1 }
      if shutdowns.value > 1 { blockedDiscordStart.interrupt() }
      return true
    }
  )

  await model.updateEnabled(true)
  await model.refreshDashboard()
  for _ in 0..<75 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }

  await core.simulateCoreReplacement()
  await core.requireBrokerEnrollmentBeforeOff()
  await core.blockNextDiscordStart(on: blockedDiscordStart)
  let identityAttempts = await core.effectIdentityAttemptCount
  await core.delayNextEffectIdentity(by: .milliseconds(150))
  let firstOff = Task { await model.updateEnabled(false) }
  for _ in 0..<75 where await core.effectIdentityAttemptCount == identityAttempts {
    try? await Task.sleep(for: .milliseconds(5))
  }
  let newerOn = Task { await model.updateEnabled(true) }
  for _ in 0..<150 where !blockedDiscordStart.isWaiting {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(blockedDiscordStart.isWaiting)
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(!model.modelEntryEnabled)

  await model.updateEnabled(false)
  await firstOff.value
  await newerOn.value

  #expect(shutdowns.value == 2)
  #expect(blockedDiscordStart.wasInterrupted)
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(!model.modelEntryEnabled)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.discordStatus == "disconnected")
  #expect(await broker.appliedValues == [true, false, true, false])
  #expect(await core.offPrepareCount == 2)
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func coreTerminationImmediatelyClearsCachedStatusAndRestoresBothDurableListenersOnce() async throws
{
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)
  await model.refreshDashboard()
  for _ in 0..<75 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(await core.iMessageStartCount == 1)
  #expect(await core.discordSessionStartCount == 1)

  await core.simulateCoreReplacement()
  await core.delayNextEffectIdentity(by: .milliseconds(300))
  events.send(
    CoreTerminationEvent(generation: 1, reason: .exited, exitStatus: 0))
  events.send(
    CoreTerminationEvent(generation: 1, reason: .exited, exitStatus: 0))

  for _ in 0..<50 where model.runtimeRecoveryState != .recovering {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(model.iMessageStatus == "paused")
  #expect(model.discordStatus == "paused")
  #expect(!model.modelEntryEnabled)

  for _ in 0..<250
  where model.runtimeRecoveryState != .ready || model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(await core.iMessageStartCount == 2)
  #expect(await core.discordSessionStartCount == 2)
  #expect(await core.pairChannelCount == 0)
  #expect(await core.channelSends.isEmpty)
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func coreTerminationRecoveryVerifiesAnEmptyChoiceStoreBeforeRepublishingOn() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.choiceLoopContinuityState == .empty)
  #expect(model.dashboardControls.outcomeInputEnabled)

  await core.simulateCoreReplacement()
  await core.delayNextEffectIdentity(by: .milliseconds(150))
  events.send(CoreTerminationEvent(generation: 1, reason: .exited, exitStatus: 0))

  for _ in 0..<100 where model.runtimeRecoveryState != .recovering {
    try? await Task.sleep(for: .milliseconds(5))
  }
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(!model.dashboardControls.outcomeInputEnabled)

  for _ in 0..<250 where model.runtimeRecoveryState != .ready {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.choiceLoopContinuityState == .empty)
  #expect(model.choiceLoopSnapshot == nil)
  #expect(model.dashboardControls.outcomeInputEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)
}

@MainActor
@Test
func coreTerminationRecoveryReadFailureNeverPublishesFalseOnOrLocalInput() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)
  #expect(model.runtimeDisplayState == .on)
  #expect(model.dashboardControls.outcomeInputEnabled)

  await core.simulateCoreReplacement()
  await core.failNextChoiceLoopReadAttempts(3)
  events.send(CoreTerminationEvent(generation: 1, reason: .exited, exitStatus: 0))

  for _ in 0..<350 where model.runtimeRecoveryState != .paused {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.choiceLoopContinuityState == .needsYou(.readFailed))
  #expect(!model.modelEntryEnabled)
  #expect(!model.dashboardControls.outcomeInputEnabled)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)
}

@MainActor
@Test
func coreTerminationRecoveryRejectsLateChoiceReadFromAnOlderGeneration() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)
  #expect(model.choiceLoopSnapshot == nil)

  let lateSnapshot = testChoiceLoopSnapshot(sessionID: "late-recovery-session")
  let gate = NonCooperativeRpcGate()
  await core.setChoiceLoopSnapshot(lateSnapshot)
  await core.blockNextChoiceLoopRead(on: gate)
  await core.simulateCoreReplacement()
  events.send(CoreTerminationEvent(generation: 1, reason: .exited, exitStatus: 0))

  for _ in 0..<250 where !gate.isWaiting {
    try? await Task.sleep(for: .milliseconds(5))
  }
  #expect(gate.isWaiting)
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(!model.dashboardControls.outcomeInputEnabled)

  await core.setChoiceLoopSnapshot(nil)
  model.requestEnabled(false)
  gate.resume()
  for _ in 0..<250 where model.runtimeDisplayState != .off {
    try? await Task.sleep(for: .milliseconds(10))
  }

  #expect(model.runtimeDisplayState == .off)
  #expect(!model.enabled)
  #expect(model.choiceLoopSnapshot == nil)
  #expect(!model.dashboardControls.outcomeInputEnabled)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func threeFailedCoreRecoveryAttemptsPauseWithoutCachedListenersOrOutbound() async throws {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )
  await model.updateEnabled(true)
  await model.refreshDashboard()
  await core.simulateCoreReplacement()
  await core.delayNextEffectIdentity(by: .milliseconds(200))
  await core.failNextEffectIdentityAttempts(3)
  let attemptsBeforeRecovery = await core.effectIdentityAttemptCount

  events.send(
    CoreTerminationEvent(generation: 1, reason: .uncaughtSignal, exitStatus: 9))
  for _ in 0..<50 where model.runtimeRecoveryState != .recovering {
    try? await Task.sleep(for: .milliseconds(10))
  }
  events.send(
    CoreTerminationEvent(generation: 2, reason: .uncaughtSignal, exitStatus: 9))
  events.send(
    CoreTerminationEvent(generation: 3, reason: .uncaughtSignal, exitStatus: 9))

  for _ in 0..<250 where model.runtimeRecoveryState != .paused {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.iMessageStatus == "paused")
  #expect(model.discordStatus == "paused")
  #expect(!model.modelEntryEnabled)
  #expect(
    model.errorMessage
      == "Need you: OpenOpen paused after Core stopped. No listener, model, or outbound work is running."
  )
  #expect(await core.channelSends.isEmpty)
  #expect(await core.pairChannelCount == 0)
  #expect(await core.effectIdentityAttemptCount - attemptsBeforeRecovery == 3)
  await model.updateEnabled(false)
}

@MainActor
@Test
func pausedRuntimeOffProvisionsReplacementBeforePrepareAndCommitsOnce() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let shutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      shutdowns.withLock { $0 += 1 }
      return true
    }
  )

  await model.updateEnabled(true)
  await core.simulateCoreReplacement()
  await core.requireBrokerEnrollmentBeforeOff()
  await core.failNextEffectIdentityAttempts(3)
  events.send(
    CoreTerminationEvent(generation: 1, reason: .uncaughtSignal, exitStatus: 9))
  for _ in 0..<250 where model.runtimeRecoveryState != .paused {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  let shutdownsBeforeOff = shutdowns.value
  let enrollmentsBeforeOff = await core.brokerEnrollmentInstallCount

  await model.updateEnabled(false)

  #expect(shutdowns.value == shutdownsBeforeOff + 1)
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.errorMessage == nil)
  #expect(await core.offPrepareCount == 1)
  // Requiring enrollment inside prepareRuntime(false) makes successful Off a
  // direct ordering proof: the existing quiesced path installed trust first.
  #expect(await core.brokerEnrollmentInstallCount == enrollmentsBeforeOff + 1)
  #expect(await broker.appliedValues == [true, false])
  #expect(
    await core.runtime()
      == RuntimeControl(enabled: false, revision: 2, updatedAtMs: 2))
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.confirmationCount == 0)
  #expect(await core.dispatchBeginCount == 0)
  #expect(await core.dashboardConfirmedMission == nil)
  #expect(await core.dashboardReceipt == nil)
  #expect(await core.channelSends.isEmpty)
  #expect(await core.codexInitializeCount == 1)
  #expect(await broker.leaseAcquireCount == 1)
}

@MainActor
@Test
func pausedRuntimeOffFailuresNeverPublishFalseOffOrExternalWork() async throws {
  let core = MockCore()
  let broker = MockBroker()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let shutdowns = LockIsolated(0)
  let shutdownAllowed = LockIsolated(true)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: broker,
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      shutdowns.withLock { $0 += 1 }
      return shutdownAllowed.value
    }
  )

  await model.updateEnabled(true)
  await core.simulateCoreReplacement()
  await core.requireBrokerEnrollmentBeforeOff()
  await core.failNextEffectIdentityAttempts(3)
  events.send(
    CoreTerminationEvent(generation: 1, reason: .uncaughtSignal, exitStatus: 9))
  for _ in 0..<250 where model.runtimeRecoveryState != .paused {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.runtimeRecoveryState == .paused)
  let shutdownsBeforeOff = shutdowns.value
  let enrollmentsBeforeOff = await core.brokerEnrollmentInstallCount

  shutdownAllowed.withLock { $0 = false }
  await model.updateEnabled(false)
  #expect(shutdowns.value == shutdownsBeforeOff + 1)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(await core.offPrepareCount == 0)
  #expect(await core.brokerEnrollmentInstallCount == enrollmentsBeforeOff)
  #expect(await broker.appliedValues == [true])

  shutdownAllowed.withLock { $0 = true }
  await broker.failNextProvision()
  await model.updateEnabled(false)
  #expect(shutdowns.value == shutdownsBeforeOff + 2)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(await core.offPrepareCount == 0)
  #expect(await core.brokerEnrollmentInstallCount == enrollmentsBeforeOff)
  #expect(await broker.appliedValues == [true])

  await core.failNextOffPreparation()
  await model.updateEnabled(false)
  #expect(shutdowns.value == shutdownsBeforeOff + 2)
  #expect(model.enabled)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(await core.offPrepareCount == 0)
  #expect(await core.brokerEnrollmentInstallCount == enrollmentsBeforeOff + 1)
  #expect(await broker.appliedValues == [true])

  await model.updateEnabled(false)
  #expect(shutdowns.value == shutdownsBeforeOff + 2)
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(await core.offPrepareCount == 1)
  #expect(await broker.appliedValues == [true, false])
  #expect(
    await core.runtime()
      == RuntimeControl(enabled: false, revision: 2, updatedAtMs: 2))
  #expect(await core.channelPairings.count == 2)
  #expect(await core.proposalCount == 0)
  #expect(await core.confirmationCount == 0)
  #expect(await core.dispatchBeginCount == 0)
  #expect(await core.dashboardConfirmedMission == nil)
  #expect(await core.dashboardReceipt == nil)
  #expect(await core.channelSends.isEmpty)
  #expect(await core.codexInitializeCount == 1)
  #expect(await broker.leaseAcquireCount == 1)
}

@MainActor
@Test
func generationDriftConsumesOneRecoveryAttemptAndCannotPublishStitchedReadyState() async throws {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )
  await model.updateEnabled(true)
  await model.refreshDashboard()
  #expect(model.runtimeRecoveryState == .ready)
  let fencesBeforeRecovery = await core.generationFenceBeginCount

  await core.simulateCoreReplacement()
  await core.invalidateNextFenceAtClose(followedByEffectIdentityFailures: 2)
  events.send(
    CoreTerminationEvent(generation: 1, reason: .exited, exitStatus: 0))
  for _ in 0..<100 where model.runtimeRecoveryState != .recovering {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(model.runtimeRecoveryState == .recovering)

  var publishedReady = false
  for _ in 0..<300 where model.runtimeRecoveryState != .paused {
    if model.runtimeRecoveryState == .ready { publishedReady = true }
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(!publishedReady)
  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.iMessageStatus == "paused")
  #expect(model.discordStatus == "paused")
  #expect(await core.generationFenceBeginCount - fencesBeforeRecovery == 3)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func completeStartupRestoreUsesOneGenerationFenceFromItsFirstCoreRpc() async throws {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )
  await model.updateEnabled(true)
  await core.resetEffectIdentityFenceStates()
  let fencesBefore = await core.generationFenceBeginCount

  await model.refreshDashboard()

  let observedFenceStates = await core.effectIdentityFenceStates
  #expect(!observedFenceStates.isEmpty)
  #expect(observedFenceStates.allSatisfy { $0 })
  #expect(await core.generationFenceBeginCount - fencesBefore == 1)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  await model.updateEnabled(false)
}

@MainActor
@Test
func startupGenerationDriftSharesTheSameThreeAttemptBudgetAndNeverPublishesReady() async throws {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )
  await model.updateEnabled(true)
  let fencesBefore = await core.generationFenceBeginCount
  await core.invalidateNextFenceAtClose(followedByEffectIdentityFailures: 2)

  await model.refreshDashboard()

  #expect(await core.generationFenceBeginCount - fencesBefore == 3)
  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.iMessageStatus == "paused")
  #expect(model.discordStatus == "paused")
  #expect(!model.modelEntryEnabled)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func globalOffInterruptsTheTrackedStartupRestoreBeforeItsFirstCoreRpcCompletes() async throws {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  let blockedIdentity = NonCooperativeRpcGate()
  let coreShutdowns = LockIsolated(0)
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      coreShutdowns.withLock { $0 += 1 }
      blockedIdentity.interrupt()
      return true
    }
  )
  await model.updateEnabled(true)
  await core.blockNextEffectIdentity(on: blockedIdentity)

  let refresh = Task { await model.refreshDashboard() }
  for _ in 0..<150 where !blockedIdentity.isWaiting {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(blockedIdentity.isWaiting)
  #expect(model.runtimeRecoveryState == .recovering)

  await model.updateEnabled(false)
  await refresh.value

  #expect(coreShutdowns.value == 1)
  #expect(blockedIdentity.wasInterrupted)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.discordStatus == "disconnected")
  #expect(!model.modelEntryEnabled)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func globalOffDuringCoreRecoveryCancelsRestorationAndLeavesBothListenersStopped() async throws {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let blockedDiscordStart = NonCooperativeRpcGate()
  let coreShutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      coreShutdowns.withLock { $0 += 1 }
      blockedDiscordStart.interrupt()
      return true
    }
  )
  await model.updateEnabled(true)
  await model.refreshDashboard()
  await core.simulateCoreReplacement()
  await core.blockNextDiscordStart(on: blockedDiscordStart)
  events.send(
    CoreTerminationEvent(generation: 1, reason: .transportFailure, exitStatus: 0))
  for _ in 0..<150 where !blockedDiscordStart.isWaiting {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(blockedDiscordStart.isWaiting)
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(await core.iMessageStartCount == 2)
  #expect(await core.discordSessionStartCount == 1)

  await model.updateEnabled(false)
  try? await Task.sleep(for: .milliseconds(100))

  #expect(coreShutdowns.value == 1)
  #expect(blockedDiscordStart.wasInterrupted)
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.discordStatus == "disconnected")
  #expect(!model.modelEntryEnabled)
  #expect(await core.channelSends.isEmpty)
  #expect(await core.discordSessionStartCount == 1)
}

@MainActor
@Test
func globalOffInterruptsUntrackedStartupListenerRestorationBeforeProviderCommit() async throws {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let blockedDiscordStart = NonCooperativeRpcGate()
  let coreShutdowns = LockIsolated(0)
  try tokenStore.save("test-only-discord-token")
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: {
      coreShutdowns.withLock { $0 += 1 }
      blockedDiscordStart.interrupt()
      return true
    }
  )

  await model.updateEnabled(true)
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  await core.blockNextDiscordStart(on: blockedDiscordStart)
  let refresh = Task { await model.refreshDashboard() }
  for _ in 0..<150 where !blockedDiscordStart.isWaiting {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(blockedDiscordStart.isWaiting)
  #expect(model.runtimeRecoveryState == .recovering)
  #expect(await core.iMessageStartCount == 1)
  #expect(await core.discordSessionStartCount == 0)

  await model.updateEnabled(false)
  await refresh.value
  try? await Task.sleep(for: .milliseconds(100))

  #expect(coreShutdowns.value == 1)
  #expect(blockedDiscordStart.wasInterrupted)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.discordStatus == "disconnected")
  #expect(await core.discordSessionStartCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func failedExactCoreTerminationPausesWithoutClaimingProviderShutdown() async {
  let core = MockCore()
  let events = CoreTerminationEmitter()
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: MockDiscordTokenStore(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    shutdownCore: { false },
    channelPollInterval: .milliseconds(1)
  )

  await model.updateEnabled(true)
  await core.failNextChannelPoll(
    with: .remote(
      code: -32_017,
      message: "Channel model failed; bounded runtime recovery is required"
    ))
  for _ in 0..<200 where model.runtimeRecoveryState != .paused {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.iMessageStatus == "paused")
  #expect(model.discordStatus == "paused")
  #expect(model.errorMessage == nil)
  #expect(model.channelFailureFeedback?.contains("paused its local runtime") == true)
  #expect(!model.modelEntryEnabled)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.channelFailureFeedback?.contains("paused its local runtime") == true)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func durableOffAndCompletedOnClearOnlyTransientTerminalRecoveryFeedback() async throws {
  let incident = testChannelFailureIncident()
  let core = MockCore()
  let shutdownAttempts = LockIsolated(0)
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  let (model, _) = try await makePairedRecoveryModel(
    core: core,
    shutdownCore: {
      shutdownAttempts.withLock { $0 += 1 }
      return shutdownAttempts.value > 1
    },
    channelPollInterval: .milliseconds(1)
  )
  await model.refreshDashboard()
  await model.updateEnabled(true)
  await core.failNextChannelPoll(
    with: .remote(
      code: -32_017,
      message: "Channel model failed; bounded runtime recovery is required"
    ))

  for _ in 0..<200 where model.runtimeRecoveryState != .paused {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.channelFailureFeedback?.contains("paused its local runtime") == true)
  #expect(model.channelFailureIncidents.map(\.incidentId) == [incident.incidentId])
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)

  #expect(model.runtimeDisplayState == .off)
  #expect(model.channelFailureFeedback == nil)
  #expect(model.channelFailureIncidents.map(\.incidentId) == [incident.incidentId])
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(true)
  for _ in 0..<100
  where model.runtimeDisplayState != .on || model.iMessageStatus != "connected"
    || model.discordStatus != "connected"
  {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(model.runtimeDisplayState == .on)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(model.channelFailureFeedback == nil)
  #expect(model.channelFailureIncidents.map(\.incidentId) == [incident.incidentId])
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  model.acknowledgeChannelFailure(incident.incidentId)
  for _ in 0..<100 where model.channelFailureIncidents.first?.acknowledgement == nil {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.channelFailureIncidents.first?.acknowledgement != nil)
  #expect(model.channelFailureFeedback == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)
  let restarted = AppModel(core: core, broker: MockBroker(), registerLoginItem: {})
  await restarted.refreshDashboard()
  #expect(restarted.channelFailureIncidents.first?.acknowledgement != nil)
  #expect(restarted.channelFailureFeedback == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func chatGptLoginRotatesThroughLoginOnlyThenFreshReadOnlyCodex() async {
  let core = MockCore(loginCompleted: false)
  let broker = MockBroker()
  let openedURLs = LockIsolated<[URL]>([])
  let model = AppModel(
    core: core,
    broker: broker,
    registerLoginItem: {},
    openOfficialURL: { url in
      openedURLs.withLock { $0.append(url) }
      return true
    }
  )
  await model.updateEnabled(true)
  await model.connectChatGpt()

  #expect(model.errorMessage == nil)
  #expect(model.accountState == .chatGpt(email: "owner@example.com", planType: "plus"))
  #expect(model.availableModels.map(\.id) == ["gpt-test-model"])
  #expect(openedURLs.value.map(\.absoluteString) == ["https://example.invalid"])
  #expect(await core.codexLoginPrepareCount == 1)
  #expect(await core.loginBeginCount == 1)
  #expect(await core.loginAwaitCount == 1)
  #expect(await core.codexPrepareCount == 2)
  #expect(await core.codexInitializeCount == 3)
  #expect(await broker.leaseAcquireCount == 3)
}

@MainActor
@Test
func failedOfficialLoginLaunchDestroysLoginOnlyCodexAndDoesNotExposeModels() async {
  let core = MockCore(loginCompleted: false)
  let broker = MockBroker()
  let launchAttempts = LockIsolated(0)
  let model = AppModel(
    core: core,
    broker: broker,
    registerLoginItem: {},
    openOfficialURL: { _ in
      launchAttempts.withLock { $0 += 1 }
      return launchAttempts.value > 1
    }
  )
  await model.updateEnabled(true)
  await model.connectChatGpt()

  #expect(model.errorMessage == "OpenOpen could not open the official sign-in page.")
  #expect(model.accountState == .notConnected)
  #expect(model.availableModels.isEmpty)
  #expect(await core.codexLoginPrepareCount == 1)
  #expect(await core.loginBeginCount == 1)
  #expect(await core.loginAwaitCount == 0)
  #expect(await core.codexAbortCount == 1)
  #expect(!(await core.codexInitialized))

  await model.connectChatGpt()
  #expect(model.errorMessage == nil)
  #expect(model.accountState == .chatGpt(email: "owner@example.com", planType: "plus"))
  #expect(model.availableModels.map(\.id) == ["gpt-test-model"])
  #expect(await core.codexLoginPrepareCount == 2)
  #expect(await core.loginBeginCount == 2)
  #expect(await core.loginAwaitCount == 1)
}

@MainActor
@Test
func invalidLoginURLAndCancelledAwaitBothRetryWithoutToggleOrRestart() async {
  let core = MockCore(loginCompleted: false)
  let model = AppModel(
    core: core, broker: MockBroker(), registerLoginItem: {}, openOfficialURL: { _ in true }
  )
  await model.updateEnabled(true)

  await core.setLoginAuthURL("http://example.invalid")
  await model.connectChatGpt()
  #expect(model.errorMessage == "OpenOpen received an invalid sign-in URL.")
  #expect(await core.codexAbortCount == 1)

  await core.setLoginAuthURL("https://example.invalid")
  await core.rejectNextLoginAwaitOperation()
  await model.connectChatGpt()
  #expect(model.errorMessage == "Managed login was cancelled.")
  #expect(await core.codexAbortCount == 2)

  await model.connectChatGpt()
  #expect(model.errorMessage == nil)
  #expect(model.accountState == .chatGpt(email: "owner@example.com", planType: "plus"))
  #expect(model.availableModels.map(\.id) == ["gpt-test-model"])
  #expect(await core.codexLoginPrepareCount == 3)
  #expect(await core.loginBeginCount == 3)
  #expect(await core.loginAwaitCount == 2)
}

@MainActor
@Test
func completedLoginModelPreparationFailureRetriesAccountWithoutSecondLogin() async {
  let core = MockCore(loginCompleted: false)
  let model = AppModel(
    core: core, broker: MockBroker(), registerLoginItem: {}, openOfficialURL: { _ in true }
  )
  await model.updateEnabled(true)
  await core.rejectNextModelRuntimePreparation()

  await model.connectChatGpt()
  #expect(model.errorMessage == "Model runtime preparation failed.")
  #expect(await core.loginBeginCount == 1)
  #expect(await core.loginAwaitCount == 1)

  await model.connectChatGpt()
  #expect(model.errorMessage == nil)
  #expect(model.accountState == .chatGpt(email: "owner@example.com", planType: "plus"))
  #expect(model.availableModels.map(\.id) == ["gpt-test-model"])
  #expect(await core.loginBeginCount == 1)
  #expect(await core.loginAwaitCount == 1)
}

@MainActor
@Test
func runtimeHomeBoundaryPrecedesCodexAndLeaseCreation() async {
  let core = MockCore()
  let broker = FailingRuntimeHomeBroker()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  #expect(await broker.prepareCount == 1)
  #expect(await core.codexPrepareCount == 0)
  #expect(!(await core.leaseInstalled))
  #expect(!(await core.codexInitialized))
  #expect(model.runtimeDisplayState != .on)
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
  #expect(await core.codexCandidateBindCount == 1)
  #expect(await core.abortedCandidateBoundStates == [true])
  #expect(!(await core.codexInitialized))
}

@MainActor
@Test
func leaseInstallResponseLossAbortsOnlyAnAlreadyBrokerBoundCandidate() async {
  let core = MockCore()
  let broker = MockBroker()
  await core.loseNextCoreLeaseInstallResponse()
  let model = AppModel(core: core, broker: broker) {}
  await model.updateEnabled(true)
  #expect(!model.enabled)
  #expect(await broker.leaseAcquireCount == 1)
  #expect(await core.codexCandidateBindCount == 1)
  #expect(await core.codexAbortCount == 1)
  #expect(await core.abortedCandidateBoundStates == [true])
  #expect(!(await core.codexInitialized))
}

@MainActor
@Test
func brokerAcquireResponseLossRetriesThroughDurableLeaseRotation() async {
  let core = MockCore()
  let broker = MockBroker()
  await broker.loseNextLeaseAcquireResponse()
  let model = AppModel(core: core, broker: broker) {}

  await model.updateEnabled(true)
  #expect(!model.enabled)
  #expect(await broker.leaseAcquireCount == 1)
  #expect(await broker.durableLeaseGeneration == 1)
  #expect(await broker.durableLeaseRotationCount == 0)
  #expect(await core.codexCandidateBindCount == 1)
  #expect(await core.codexAbortCount == 1)
  #expect(await core.abortedCandidateBoundStates == [true])
  #expect(!(await core.codexInitialized))

  await model.updateEnabled(true)
  #expect(model.enabled)
  #expect(model.errorMessage == nil)
  #expect(await broker.leaseAcquireCount == 2)
  #expect(await broker.durableLeaseGeneration == 2)
  #expect(await broker.durableLeaseRotationCount == 1)
  #expect(await core.codexCandidateBindCount == 2)
  #expect(await core.codexAbortCount == 1)
  #expect(await core.codexInitialized)
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
  #expect(text.contains("proc_terminate_with_audittoken"))
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
  let core = MockCore(dashboardDelay: .milliseconds(100))
  let model = AppModel(
    core: core,
    broker: MockBroker()
  ) {}
  let refresh = Task { await model.refreshDashboard() }
  for _ in 0..<100 where await core.dashboardInvocationCount == 0 {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(await core.dashboardInvocationCount == 1)
  await model.updateEnabled(true)
  await refresh.value
  #expect(model.enabled)
}

@MainActor
@Test
func staleDashboardFailureCannotOverwriteANewerSuccessfulToggle() async {
  let core = MockCore(dashboardDelay: .milliseconds(100), dashboardFails: true)
  let model = AppModel(
    core: core,
    broker: MockBroker()
  ) {}
  let refresh = Task { await model.refreshDashboard() }
  for _ in 0..<100 where await core.dashboardInvocationCount == 0 {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(await core.dashboardInvocationCount == 1)
  await model.updateEnabled(true)
  await refresh.value
  #expect(model.enabled)
  #expect(model.errorMessage == nil)
}

@MainActor
@Test
func modelSetupConsumesOneFreshRuntimeChallengeForAnAtomicCatalogSnapshot() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  let before = await core.challengesIssued
  await model.refreshAccountAndModels()
  let nonces = await core.proofNonces
  #expect(await core.challengesIssued == before + 1)
  #expect(nonces.count == 1)
  #expect(Set(nonces).count == 1)
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
  await core.setChannelPollConnectionStatus("connecting")
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()

  #expect(model.discordTokenDraft.isEmpty)
  #expect(model.discordStatus == "connecting")
  #expect(model.discordSetupFeedback == "Discord is connecting to the official Gateway.")
  #expect(model.errorMessage == nil)
  #expect(await core.pairChannelCount == 0)
  #expect(await core.discordStartTokens == ["test-only-discord-token"])

  await core.setChannelPollConnectionStatus("reconnecting")
  for _ in 0..<75 where model.discordStatus != "reconnecting" {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.discordStatus == "reconnecting")
  #expect(model.discordSetupFeedback == "Discord is reconnecting to the official Gateway.")
  #expect(model.discordSetupFeedback != "Discord connected to the approved channel.")

  await core.setChannelPollConnectionStatus("connected")
  for _ in 0..<75 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.discordStatus == "connected")
  #expect(model.discordSetupFeedback == "Discord connected to the approved channel.")
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordStartResponseLossRetryReattachesWithoutDuplicateSession() async {
  let core = MockCore()
  await core.setChannelPairing(
    ChannelPairing(
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
    ))
  await core.setChannelPollConnectionStatus("connecting")
  await core.loseDiscordStartResponseAfterCommit()
  let tokenStore = MockDiscordTokenStore()
  let model = AppModel(core: core, broker: MockBroker(), discordTokenStore: tokenStore) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()
  #expect(model.discordStatus == "faulted")
  #expect(await core.discordSessionStartCount == 1)

  await model.connectDiscord()
  #expect(model.discordStatus == "connecting")
  #expect(model.errorMessage == nil)
  #expect(await core.discordSessionStartCount == 1)
  #expect(
    await core.discordStartTokens
      == ["test-only-discord-token", "test-only-discord-token"]
  )
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordKeychainFailureIsVisibleWithoutEchoingTheDraftToken() async {
  let model = AppModel(
    core: MockCore(),
    broker: MockBroker(),
    discordTokenStore: FailingDiscordTokenStore()
  ) {}
  await model.updateEnabled(true)
  let draft = "test-only-sensitive-draft"
  model.discordTokenDraft = draft

  await model.connectDiscord()

  #expect(model.discordTokenDraft.isEmpty)
  #expect(model.discordStatus == "faulted")
  #expect(model.discordSetupFeedback == "Discord setup failed safely. Review the status and retry.")
  #expect(model.discordSetupFeedback?.contains(draft) == false)
  #expect(model.errorMessage == "OpenOpen could not access its local security key.")
  #expect(model.errorMessage?.contains(draft) == false)
  await model.updateEnabled(false)
}

@MainActor
@Test
func globalOffErasesAnUnsubmittedDiscordTokenDraft() async {
  let model = AppModel(
    core: MockCore(),
    broker: MockBroker(),
    discordTokenStore: MockDiscordTokenStore()
  ) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-sensitive-draft"

  await model.updateEnabled(false)

  #expect(model.discordTokenDraft.isEmpty)
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
  #expect(
    model.discordSetupFeedback
      == "Discord security key saved in Keychain. Continue the official setup below."
  )
  #expect(model.discordSetup?.identity.botUserId == 3003)
  #expect(model.discordSetup?.installUrl.contains("permissions=101376") == true)
  #expect(model.discordPairingCandidate == nil)
  #expect(await core.pairedDiscordApplicationId() == nil)
  #expect(await core.stoppedChannels == [.discord])

  await model.checkDiscordPairingMessage()
  #expect(
    model.discordSetupFeedback
      == "Discord pairing message and permissions verified. Confirm the exact owner and channel."
  )
  #expect(model.discordPairingCandidate?.ownerUserId == "1001")
  #expect(model.discordPairingCandidate?.channelId == "2002")
  #expect(await core.pairedDiscordApplicationId() == nil)

  await model.confirmDiscordPairing()

  #expect(model.discordStatus == "connecting")
  #expect(model.discordSetupFeedback == "Discord is connecting to the official Gateway.")
  #expect(model.errorMessage == nil)
  #expect(await core.discordSetupStartCount == 1)
  #expect(await core.discordSetupConfirmCount == 1)
  #expect(await core.pairedDiscordApplicationId() == "4004")
  #expect(await core.discordStartTokens == ["test-only-discord-token"])
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordSetupActionsDisableAndReenableWithoutLosingVerifiedCandidate() async {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"
  await model.connectDiscord()
  await model.checkDiscordPairingMessage()

  let setup = model.discordSetup
  let candidate = model.discordPairingCandidate
  #expect(setup != nil)
  #expect(candidate != nil)
  #expect(model.discordSetupCheckEnabled)
  #expect(model.discordSetupConfirmationEnabled)

  await core.setLoginCompleted(false)
  await model.refreshAccountAndModels()
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(!model.modelEntryEnabled)
  #expect(!model.discordSetupCheckEnabled)
  #expect(!model.discordSetupConfirmationEnabled)
  #expect(model.discordSetup == setup)
  #expect(model.discordPairingCandidate == candidate)
  let pollsBeforeDisabledActions = await core.discordSetupPollCount
  let confirmationsBeforeDisabledActions = await core.discordSetupConfirmCount

  await model.checkDiscordPairingMessage()
  await model.confirmDiscordPairing()
  #expect(await core.discordSetupPollCount == pollsBeforeDisabledActions)
  #expect(await core.discordSetupConfirmCount == confirmationsBeforeDisabledActions)
  #expect(model.discordSetup == setup)
  #expect(model.discordPairingCandidate == candidate)

  await core.setLoginCompleted(true)
  await model.refreshAccountAndModels()
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.modelEntryEnabled)
  #expect(model.discordSetupCheckEnabled)
  #expect(model.discordSetupConfirmationEnabled)
  #expect(model.discordSetup == setup)
  #expect(model.discordPairingCandidate == candidate)
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordConfirmationResponseLossRecoversTheExactDurablePairing() async {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let model = AppModel(core: core, broker: MockBroker(), discordTokenStore: tokenStore) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"
  await model.connectDiscord()
  await model.checkDiscordPairingMessage()
  let candidate = model.discordPairingCandidate
  #expect(candidate != nil)
  await core.loseDiscordConfirmResponseAfterCommit()

  await model.confirmDiscordPairing()

  #expect(await core.discordSetupConfirmCount == 1)
  #expect(await core.discordSessionStartCount == 1)
  #expect(await core.pairedDiscordApplicationId() == candidate?.applicationId)
  #expect(model.discordSetup == nil)
  #expect(model.discordPairingCandidate == nil)
  #expect(model.discordStatus == "connecting")
  #expect(model.discordSetupFeedback == "Discord is connecting to the official Gateway.")
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordPollingReplacesConnectingFeedbackWithFaultedState() async {
  let core = MockCore()
  await core.setChannelPairing(
    ChannelPairing(
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
    ))
  await core.setChannelPollConnectionStatus("faulted")
  let tokenStore = MockDiscordTokenStore()
  let model = AppModel(core: core, broker: MockBroker(), discordTokenStore: tokenStore) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()
  for _ in 0..<50 where model.discordStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.discordStatus == "faulted")
  #expect(
    model.discordSetupFeedback
      == "Discord connection failed safely. Review the status and retry."
  )
  #expect(model.discordSetupFeedback != "Discord connected to the approved channel.")
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordFaultedSessionRetryStartsOneReplacementSession() async {
  let core = MockCore()
  await core.setChannelPairing(
    ChannelPairing(
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
    ))
  let tokenStore = MockDiscordTokenStore()
  let model = AppModel(core: core, broker: MockBroker(), discordTokenStore: tokenStore) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()
  #expect(await core.discordSessionStartCount == 1)
  await core.setChannelPollConnectionStatus("faulted")
  for _ in 0..<50 where model.discordStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.discordStatus == "faulted")

  await model.connectDiscord()

  #expect(await core.discordSessionStartCount == 2)
  #expect(model.discordStatus == "connecting")
  #expect(model.discordSetupFeedback == "Discord is connecting to the official Gateway.")
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func discordPollErrorCannotLeaveStaleConnectedFeedback() async {
  let core = MockCore()
  await core.setChannelPairing(
    ChannelPairing(
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
    ))
  await core.failNextChannelPoll()
  let tokenStore = MockDiscordTokenStore()
  let model = AppModel(core: core, broker: MockBroker(), discordTokenStore: tokenStore) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()
  for _ in 0..<50 where model.discordStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.discordStatus == "faulted")
  #expect(
    model.discordSetupFeedback
      == "Discord connection failed safely. Review the status and retry."
  )
  #expect(model.errorMessage == nil)
  #expect(model.channelListenerFeedback[.discord] != nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func globalOffDuringDiscordStartRejectsTheLateGenerationWithoutResurrectingState() async {
  let core = MockCore()
  await core.setChannelPairing(
    ChannelPairing(
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
    ))
  await core.delayDiscordStart(by: .milliseconds(100))
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: MockDiscordTokenStore()
  ) {}
  await model.updateEnabled(true)
  model.discordTokenDraft = "test-only-discord-token"

  let connecting = Task { await model.connectDiscord() }
  try? await Task.sleep(for: .milliseconds(20))
  await model.updateEnabled(false)
  await connecting.value

  #expect(!model.enabled)
  #expect(model.discordStatus == "disconnected")
  #expect(model.discordSetupFeedback == nil)
  #expect(model.errorMessage == nil)
  #expect(await core.channelPollCount == 0)
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
  #expect(!model.iMessagePairingSelectionComplete)
  #expect(model.iMessageOwnerOptions == ["owner@example.invalid", "family@example.invalid"])
  model.selectIMessageOwner("family@example.invalid")
  #expect(model.iMessageOwnerSender == "family@example.invalid")
  #expect(model.iMessagePairingSelectionComplete)
  #expect(!model.iMessageIsConnected)
  await model.updateEnabled(false)
}

@MainActor
@Test
func unnamedIMessageChatUsesParticipantFallbackAndEmptyDiscoveryIsExplicit() async {
  let unnamed = IMessageChat(
    chatId: "42", name: "", service: "iMessage",
    participants: ["owner@example.invalid"])
  #expect(unnamed.displayName == "owner@example.invalid")

  let core = MockCore()
  await core.setIMessageChatsToReturn([])
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)

  await model.refreshIMessageChats()

  #expect(model.iMessageChats.isEmpty)
  #expect(model.errorMessage == "No Messages conversations found.")
  #expect(await core.iMessageChatsListCount == 1)
  await model.updateEnabled(false)
}

@MainActor
@Test
func iMessageConnectionPairsPollsOneSuggestionAndOffStopsPolling() async {
  let core = MockCore()
  await core.queueChannelSuggestion(
    OutcomeSuggestion(
      id: testChannelSuggestionId,
      title: "Prepare the update",
      whyNow: "The owner explicitly asked in Messages",
      proposedSteps: ["Draft the concise update"],
      sourceRefs: ["imessage:message-1"]
    ))
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")

  await model.connectIMessage()
  for _ in 0..<50 where model.suggestion == nil {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.iMessageStatus == "connected")
  #expect(model.iMessageIsConnected)
  #expect(model.suggestion?.id == testChannelSuggestionId)
  #expect(await core.pairChannelCount == 1)
  #expect(await core.iMessageStartCount == 1)
  #expect(await core.channelPollCount >= 1)
  let pollAllowances = await core.channelPollModelWorkAllowances
  #expect(!pollAllowances.isEmpty)
  #expect(pollAllowances.allSatisfy { $0 })

  let stoppedBeforeLockedActions = await core.stoppedChannels
  model.selectIMessageChat("84")
  model.selectIMessageOwner("family@example.invalid")
  await model.refreshIMessageChats()
  #expect(model.iMessageChatId == "42")
  #expect(model.iMessageOwnerSender == "owner@example.invalid")
  #expect(model.iMessageIsConnected)
  #expect(await core.stoppedChannels == stoppedBeforeLockedActions)

  await model.updateEnabled(false)
  let pollsAfterOff = await core.channelPollCount
  try? await Task.sleep(for: .milliseconds(1_100))
  #expect(await core.channelPollCount == pollsAfterOff)
  #expect(model.iMessageStatus == "disconnected")
}

@MainActor
@Test
func channelRecoveryHidesDurableOriginalAndPublishesOnlyFinalCorrection() async {
  let original = OutcomeSuggestion(
    id: testOriginalSuggestionId,
    title: "Prepare the original draft",
    whyNow: "The original channel message requested it",
    proposedSteps: ["Draft the original"],
    sourceRefs: ["channel:original"]
  )
  let correction = OutcomeSuggestion(
    id: testCorrectionSuggestionId,
    title: "Prepare only the revised draft",
    whyNow: "The later channel correction supersedes the original",
    proposedSteps: ["Draft only the revision"],
    sourceRefs: ["channel:correction"]
  )
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil, receipt: nil, suggestion: original
  )
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connecting", eventStatus: "recovering", suggestion: nil),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "superseded", suggestion: nil),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "ready", suggestion: correction),
  ])
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  #expect(model.suggestion == original)

  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()
  for _ in 0..<50 where await core.channelPollCount < 1 {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.suggestion == nil)

  for _ in 0..<150 where model.suggestion != correction {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.suggestion == correction)
  #expect(await core.channelPollCount >= 3)

  await model.updateEnabled(false)
}

@MainActor
@Test
func unrecoverableStartedChannelModelSurfacesNeedYouAndKeepsCorrectionPollingLive() async {
  let core = MockCore()
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "needYou", suggestion: nil,
      failureIncidents: [testChannelFailureIncident()])
  ])
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()

  for _ in 0..<50 where await core.channelPollCount < 1 {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.errorMessage == nil)
  #expect(model.channelFailureIncidents.count == 1)
  let pollsAfterNeedYou = await core.channelPollCount
  try? await Task.sleep(for: .milliseconds(1_100))
  #expect(await core.channelPollCount > pollsAfterNeedYou)

  await model.updateEnabled(false)
}

@MainActor
@Test
func channelNeedYouInvalidatesOnlyItsExactSuggestionIdentity() async {
  let retained = OutcomeSuggestion(
    id: testRetainedSuggestionId,
    title: "Keep the valid outcome",
    whyNow: "The other paired route produced it.",
    proposedSteps: ["Keep it visible"],
    sourceRefs: ["channel:other"]
  )
  let core = MockCore()
  await core.restoreFromDashboard(mission: nil, receipt: nil, suggestion: retained)
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "needYou", suggestion: nil,
      invalidateSuggestionId: testInvalidatedSuggestionId,
      failureIncidents: [testChannelFailureIncident()]),
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "needYou", suggestion: nil,
      invalidateSuggestionId: retained.id,
      failureIncidents: [testChannelFailureIncident()]),
  ])
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()

  for _ in 0..<50 where await core.channelPollCount < 1 {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.suggestion == retained)
  for _ in 0..<100 where await core.channelPollCount < 2 {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.suggestion == nil)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)
}

@MainActor
@Test
func channelModelFailureRestartsExactCoreWithoutReplayingOrSending() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()
  let pollsBeforeFailure = await core.channelPollCount
  let codexInitializationsBeforeFailure = await core.codexInitializeCount
  await core.simulateCoreReplacement()
  await core.failNextChannelPoll(
    with: .remote(
      code: -32_017,
      message: "Channel model failed; bounded runtime recovery is required"
    ))
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "needYou", suggestion: nil,
      failureIncidents: [testChannelFailureIncident()])
  ])

  for _ in 0..<250 {
    let currentInitializations = await core.codexInitializeCount
    let currentPolls = await core.channelPollCount
    if currentPolls > pollsBeforeFailure,
      model.runtimeRecoveryState == .ready,
      currentInitializations > codexInitializationsBeforeFailure
    {
      break
    }
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(await core.channelPollCount > pollsBeforeFailure)
  let finalInitializationCount = await core.codexInitializeCount
  #expect(
    finalInitializationCount > codexInitializationsBeforeFailure,
    "bounded recovery must initialize a fresh Codex runtime; observed \(finalInitializationCount) after \(codexInitializationsBeforeFailure)"
  )
  #expect(
    finalInitializationCount <= codexInitializationsBeforeFailure + 3,
    "one recovery episode may make at most three attempts"
  )
  for _ in 0..<150 where model.channelFailureIncidents.isEmpty {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.iMessageStatus == "connected")
  #expect(model.errorMessage == nil)
  #expect(model.channelFailureIncidents.count == 1)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)
}

@MainActor
@Test
func oneHundredIdenticalTerminalPollsStayInlineAndKeepDashboardUsable() async {
  let incident = testChannelFailureIncident()
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  await core.queueChannelPollResponses(
    (0..<100).map { _ in
      ChannelPollResponse(
        connectionStatus: "connected",
        eventStatus: "needYou",
        suggestion: nil,
        failureIncidents: [incident]
      )
    })
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    channelPollInterval: .milliseconds(1)
  )
  await model.refreshDashboard()
  await model.updateEnabled(true)
  model.prompt = "Keep this local draft unchanged."
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()

  for _ in 0..<300 where await core.channelPollCount < 100 {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(await core.channelPollCount >= 100)
  #expect(model.channelFailureIncidents == [incident])
  #expect(model.errorMessage == nil)
  #expect(model.prompt == "Keep this local draft unchanged.")
  #expect(model.modelEntryEnabled)
  model.showsSettings = true
  #expect(model.showsSettings)
  model.showsSettings = false

  await model.updateEnabled(false)
  #expect(model.runtimeDisplayState == .off)
}

@MainActor
@Test
func conflictingIncidentSnapshotPublishesNoSuggestionMissionRouteOrEffectState() async {
  let incident = testChannelFailureIncident()
  let conflictingIncident = ChannelFailureIncident(
    incidentId: incident.incidentId,
    channel: incident.channel,
    failureClass: incident.failureClass,
    occurredAtMs: incident.occurredAtMs,
    runtimeRevision: incident.runtimeRevision,
    dispatchStateHash: String(repeating: "f", count: 64),
    sourceAuditAnchor: incident.sourceAuditAnchor,
    incidentAuditAnchor: incident.incidentAuditAnchor,
    acknowledgement: nil
  )

  let originalSuggestion = OutcomeSuggestion(
    id: testSuggestionOneId,
    title: "Keep the original proposal",
    whyNow: "It belongs to the accepted Dashboard snapshot.",
    proposedSteps: ["Keep the original"],
    sourceRefs: []
  )
  let replacementSuggestion = OutcomeSuggestion(
    id: testSuggestionTwoId,
    title: "Reject this partial proposal",
    whyNow: "Its incident projection conflicts.",
    proposedSteps: ["Never publish this"],
    sourceRefs: []
  )
  let suggestionCore = MockCore()
  await suggestionCore.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    suggestion: originalSuggestion,
    channelFailureIncidents: [incident]
  )
  let suggestionModel = AppModel(core: suggestionCore, broker: MockBroker()) {}
  await suggestionModel.refreshDashboard()
  await suggestionCore.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    suggestion: replacementSuggestion,
    channelFailureIncidents: [conflictingIncident]
  )

  await suggestionModel.refreshDashboard()

  #expect(suggestionModel.suggestion == originalSuggestion)
  #expect(suggestionModel.channelFailureIncidents == [incident])
  #expect(suggestionModel.confirmedMission == nil)
  #expect(suggestionModel.channelRouteSet == nil)
  #expect(await suggestionCore.channelSends.isEmpty)

  let originalMission = testConfirmedMission()
  let originalRoutes = testChannelRouteSet(missionId: originalMission.missionId)
  let replacementMission = testConfirmedMission(
    missionId: "mission-conflicting-snapshot",
    title: "Reject this Mission",
    workItems: [MissionWorkItem(id: "work-conflict", title: "Never publish this Mission")]
  )
  let replacementRoutes = testChannelRouteSet(missionId: replacementMission.missionId)
  let missionCore = MockCore()
  await missionCore.restoreFromDashboard(
    mission: originalMission,
    receipt: nil,
    channelRouteSet: originalRoutes,
    channelFailureIncidents: [incident]
  )
  let missionModel = AppModel(core: missionCore, broker: MockBroker()) {}
  await missionModel.refreshDashboard()
  await missionCore.restoreFromDashboard(
    mission: replacementMission,
    receipt: nil,
    channelRouteSet: replacementRoutes,
    channelFailureIncidents: [conflictingIncident]
  )

  await missionModel.refreshDashboard()

  #expect(missionModel.confirmedMission == originalMission)
  #expect(missionModel.channelRouteSet == originalRoutes)
  #expect(missionModel.channelFailureIncidents == [incident])
  #expect(missionModel.selectedChannelRouteId == originalRoutes.primaryRouteId)
  #expect(await missionCore.channelSends.isEmpty)
}

@MainActor
@Test
func twoListenersSurviveOneHundredTerminalPollsEachWithoutFocusOrModalStorm() async throws {
  let iMessageIncident = testChannelFailureIncident(channel: .iMessage, seed: "a")
  let discordIncident = testChannelFailureIncident(
    channel: .discord, seed: "d", occurredAtMs: 11)
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [iMessageIncident, discordIncident]
  )
  await core.queueChannelPollResponses(
    (0..<100).map { _ in
      ChannelPollResponse(
        connectionStatus: "connected",
        eventStatus: "needYou",
        suggestion: nil,
        failureIncidents: [iMessageIncident]
      )
    },
    for: .iMessage
  )
  await core.queueChannelPollResponses(
    (0..<100).map { _ in
      ChannelPollResponse(
        connectionStatus: "connected",
        eventStatus: "needYou",
        suggestion: nil,
        failureIncidents: [discordIncident]
      )
    },
    for: .discord
  )
  let (model, _) = try await makePairedRecoveryModel(
    core: core,
    channelPollInterval: .milliseconds(1)
  )
  model.prompt = "This exact local draft must keep focus and bytes."
  await model.updateEnabled(true)

  for _ in 0..<500 where await core.channelPollCount < 200 {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(await core.channelPollCount >= 200)
  #expect(model.channelFailureIncidents == [iMessageIncident, discordIncident])
  #expect(model.prompt == "This exact local draft must keep focus and bytes.")
  #expect(model.errorMessage == nil)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")
  #expect(model.dashboardControls.outcomeInputEnabled)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)
  #expect(model.runtimeDisplayState == .off)
}

@MainActor
@Test
func hostedDashboardKeepsFirstResponderAndPresentsNoSheetAcrossTwoHundredIncidentPolls()
  async throws
{
  let iMessageIncident = testChannelFailureIncident(channel: .iMessage, seed: "a")
  let discordIncident = testChannelFailureIncident(
    channel: .discord, seed: "d", occurredAtMs: 11)
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [iMessageIncident, discordIncident]
  )
  await core.queueChannelPollResponses(
    (0..<100).map { _ in
      ChannelPollResponse(
        connectionStatus: "connected",
        eventStatus: "needYou",
        suggestion: nil,
        failureIncidents: [iMessageIncident]
      )
    },
    for: .iMessage
  )
  await core.queueChannelPollResponses(
    (0..<100).map { _ in
      ChannelPollResponse(
        connectionStatus: "connected",
        eventStatus: "needYou",
        suggestion: nil,
        failureIncidents: [discordIncident]
      )
    },
    for: .discord
  )
  await core.delayChannelPoll(by: .milliseconds(5))
  let (model, _) = try await makePairedRecoveryModel(
    core: core,
    channelPollInterval: .milliseconds(1)
  )
  model.prompt = "Keep these exact local draft bytes focused."

  let application = NSApplication.shared
  let originalActivationPolicy = application.activationPolicy()
  #expect(application.setActivationPolicy(.accessory))
  application.activate(ignoringOtherApps: true)
  defer { _ = application.setActivationPolicy(originalActivationPolicy) }
  let hosting = NSHostingView(rootView: OpenOpenRootView(model: model))
  let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: 900, height: 760),
    styleMask: [.titled, .closable],
    backing: .buffered,
    defer: false
  )
  window.animationBehavior = .none
  window.isReleasedWhenClosed = false
  window.contentView = hosting
  window.makeKeyAndOrderFront(nil)
  hosting.layoutSubtreeIfNeeded()
  // Let the root view's one-time Dashboard restore finish before focus is
  // established. The assertion below then isolates steady-state poll churn.
  try? await Task.sleep(for: .milliseconds(50))
  await model.updateEnabled(true)
  try? await Task.sleep(for: .milliseconds(20))
  hosting.layoutSubtreeIfNeeded()
  let outcomeField = try #require(dashboardOutcomeField(in: hosting))
  #expect(outcomeField.isEnabled)
  #expect(window.makeFirstResponder(outcomeField))
  let initialResponder = window.firstResponder
  #expect(initialResponder != nil)

  for _ in 0..<1_000 where await core.channelPollCount < 200 {
    try? await Task.sleep(for: .milliseconds(2))
  }
  hosting.layoutSubtreeIfNeeded()

  #expect(await core.channelPollCount >= 200)
  #expect(window.attachedSheet == nil)
  #expect(window.sheets.isEmpty)
  #expect(window.firstResponder === initialResponder)
  #expect(outcomeField.isEnabled)
  #expect(model.prompt == "Keep these exact local draft bytes focused.")
  #expect(model.channelFailureIncidents == [iMessageIncident, discordIncident])
  #expect(model.errorMessage == nil)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  #expect(await core.channelSends.isEmpty)

  await model.updateEnabled(false)
  #expect(model.runtimeDisplayState == .off)
  window.makeFirstResponder(nil)
  window.contentView = nil
  window.orderOut(nil)
  // Let SwiftUI/AppKit retire the hosted view graph before either object is
  // released. Closing a still-rendering test window can race AppKit's window
  // transform teardown and crash the test process instead of testing product
  // liveness.
  try? await Task.sleep(for: .milliseconds(50))
}

@MainActor
@Test
func oneListenerPermissionFailureDoesNotBlockOtherListenerOrLocalInput() async throws {
  let core = MockCore()
  await core.failNextChannelPoll(
    .iMessage,
    with: .contractViolation("Messages permission was revoked."))
  let (model, _) = try await makePairedRecoveryModel(
    core: core,
    channelPollInterval: .milliseconds(1)
  )
  await model.updateEnabled(true)

  for _ in 0..<200 where model.iMessageStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(model.iMessageStatus == "faulted")
  #expect(model.discordStatus == "connected")
  #expect(model.errorMessage == nil)
  #expect(model.channelListenerFeedback[.iMessage] != nil)
  #expect(model.dashboardControls.outcomeInputEnabled)
  #expect(model.dashboardControls.globalToggleEnabled)

  let pollsAfterIsolation = await core.channelPollCount
  for _ in 0..<200 where await core.channelPollCount <= pollsAfterIsolation {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(await core.channelPollCount > pollsAfterIsolation)
  await model.updateEnabled(false)
  #expect(model.runtimeDisplayState == .off)
}

@MainActor
@Test
func restartIsolatesMessagesPermissionFailureAndReconnectClearsOnlyMessagesFeedback() async throws {
  let core = MockCore()
  await core.failNextIMessageActivation()
  let (model, _) = try await makePairedRecoveryModel(
    core: core,
    channelPollInterval: .milliseconds(1)
  )
  await model.updateEnabled(true)

  #expect(model.enabled)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.modelEntryEnabled)
  #expect(model.iMessageStatus == "faulted")
  #expect(model.discordStatus == "connected")
  #expect(model.channelListenerFeedback[.iMessage] != nil)
  #expect(model.channelListenerFeedback[.discord] == nil)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)

  await core.failNextChannelPoll(
    .discord,
    with: .remote(code: -32_020, message: "Channel listener unavailable")
  )
  for _ in 0..<200 where model.channelListenerFeedback[.discord] == nil {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.channelListenerFeedback[.discord] != nil)

  await model.refreshIMessageChats()
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()

  #expect(model.iMessageStatus == "connected")
  #expect(model.channelListenerFeedback[.iMessage] == nil)
  #expect(model.channelListenerFeedback[.discord] != nil)
  #expect(model.dashboardControls.outcomeInputEnabled)
  await model.updateEnabled(false)
}

@MainActor
@Test
func restartWithMissingDiscordTokenKeepsMessagesAndLocalInputRecoverable() async throws {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let events = CoreTerminationEmitter()
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    ))
  await core.setChannelPairing(testDiscordPairing())
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore,
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    channelPollInterval: .milliseconds(1)
  )
  await model.updateEnabled(true)

  #expect(model.enabled)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "faulted")
  #expect(model.channelListenerFeedback[.discord] != nil)
  #expect(model.modelEntryEnabled)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)

  model.discordTokenDraft = "test-only-discord-token"
  await model.connectDiscord()
  for _ in 0..<200 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.discordStatus == "connected")
  #expect(model.channelListenerFeedback[.discord] == nil)
  #expect(model.iMessageStatus == "connected")
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func incidentAcknowledgementNeedsNoModelLeaseAndSurvivesAwaitingAccount() async {
  let incident = testChannelFailureIncident()
  let core = MockCore(loginCompleted: false)
  let events = CoreTerminationEmitter()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )
  await model.refreshDashboard()
  await model.updateEnabled(true)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.canAcknowledgeChannelFailure(incident))
  let initializationsBeforeAck = await core.codexInitializeCount

  model.acknowledgeChannelFailure(incident.incidentId)
  model.acknowledgeChannelFailure(incident.incidentId)
  for _ in 0..<100 where await core.channelFailureAcknowledgementCount == 0 {
    try? await Task.sleep(for: .milliseconds(10))
  }

  #expect(await core.channelFailureAcknowledgementCount == 1)
  #expect(await core.codexInitializeCount == initializationsBeforeAck)
  #expect(model.channelFailureIncidents.first?.acknowledgement != nil)
  #expect(model.errorMessage == nil)

  await model.updateEnabled(false)
  let restarted = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )
  await restarted.refreshDashboard()
  #expect(restarted.channelFailureIncidents.first?.acknowledgement != nil)
  #expect(restarted.errorMessage == nil)
}

@MainActor
@Test
func boundedIncidentProjectionRevealsTheNextUnacknowledgedRowAndSurvivesRestart() async {
  let incidents = (0..<129).map { index in
    testChannelFailureIncident(
      index: index,
      channel: index.isMultiple(of: 2) ? .discord : .iMessage
    )
  }
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: Array(incidents.prefix(128))
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  await model.updateEnabled(true)
  #expect(model.channelFailureIncidents.count == 128)
  #expect(model.channelFailureIncidents.first?.incidentId == incidents[0].incidentId)
  #expect(!model.channelFailureIncidents.contains { $0.incidentId == incidents[128].incidentId })

  let runtime = await core.runtime()
  await core.returnDashboardAfterNextChannelFailureAcknowledgement(
    DashboardState(
      activeCards: [],
      channelFailureIncidents: Array(incidents.dropFirst()),
      microphone: MicrophoneState(available: false, reason: "Unavailable"),
      runtime: runtime,
      suggestion: nil
    )
  )

  model.acknowledgeChannelFailure(incidents[0].incidentId)
  for _ in 0..<100
  where !model.channelFailureIncidents.contains(where: {
    $0.incidentId == incidents[128].incidentId
  }) {
    try? await Task.sleep(for: .milliseconds(5))
  }

  // A successful acknowledgement immediately refreshes the next bounded
  // Store page. The 129th durable row becomes visible without a manual reload.
  #expect(model.channelFailureIncidents.count == 128)
  #expect(!model.channelFailureIncidents.contains { $0.incidentId == incidents[0].incidentId })
  #expect(model.channelFailureIncidents.contains { $0.incidentId == incidents[128].incidentId })
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  await model.updateEnabled(false)

  let restarted = AppModel(core: core, broker: MockBroker()) {}
  await restarted.refreshDashboard()
  #expect(restarted.channelFailureIncidents.count == 128)
  #expect(restarted.channelFailureIncidents.first?.incidentId == incidents[1].incidentId)
  #expect(restarted.channelFailureIncidents.last?.incidentId == incidents[128].incidentId)
  #expect(restarted.dashboardControls.globalToggleEnabled)
  #expect(restarted.dashboardControls.settingsEnabled)
}

@MainActor
@Test
func lateCancelledAcknowledgementCannotRemoveTheCurrentSingleFlightTask() async {
  let incident = testChannelFailureIncident()
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  await model.updateEnabled(true)

  await core.delayChannelFailureAcknowledgement(by: .milliseconds(500))
  model.acknowledgeChannelFailure(incident.incidentId)
  for _ in 0..<100 where await core.channelFailureAcknowledgementCount == 0 {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(await core.channelFailureAcknowledgementCount == 1)

  await model.updateEnabled(false)
  await model.updateEnabled(true)
  await core.delayChannelFailureAcknowledgement(by: .milliseconds(800))
  model.acknowledgeChannelFailure(incident.incidentId)
  for _ in 0..<100 where await core.channelFailureAcknowledgementCount < 2 {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(await core.channelFailureAcknowledgementCount == 2)

  // The cancelled first-generation RPC returns while the second one is still
  // live. Its cleanup must not remove the second task and admit a third tap.
  try? await Task.sleep(for: .milliseconds(550))
  model.acknowledgeChannelFailure(incident.incidentId)
  model.acknowledgeChannelFailure(incident.incidentId)
  #expect(await core.channelFailureAcknowledgementCount == 2)

  for _ in 0..<200 where model.channelFailureIncidents.first?.acknowledgement == nil {
    try? await Task.sleep(for: .milliseconds(5))
  }
  #expect(model.channelFailureIncidents.first?.acknowledgement != nil)
  #expect(await core.channelFailureAcknowledgementCount == 2)
  await model.updateEnabled(false)
}

@MainActor
@Test
func delayedUnacknowledgedPollCannotDowngradeDurableAcknowledgement() async {
  let incident = testChannelFailureIncident()
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected",
      eventStatus: "needYou",
      suggestion: nil,
      failureIncidents: [incident]
    )
  ])
  await core.delayChannelPoll(by: .milliseconds(150))
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    channelPollInterval: .milliseconds(1)
  )
  await model.refreshDashboard()
  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()
  for _ in 0..<100 where await core.channelPollInvocationCount == 0 {
    try? await Task.sleep(for: .milliseconds(5))
  }

  model.acknowledgeChannelFailure(incident.incidentId)
  for _ in 0..<100 where model.channelFailureIncidents.first?.acknowledgement == nil {
    try? await Task.sleep(for: .milliseconds(5))
  }
  try? await Task.sleep(for: .milliseconds(200))

  #expect(model.channelFailureIncidents.first?.acknowledgement != nil)
  #expect(await core.channelFailureAcknowledgementCount == 1)
  #expect(model.errorMessage == nil)

  await model.updateEnabled(false)
}

@MainActor
@Test
func acknowledgementPersistenceFailureKeepsIncidentVisibleAndUnacknowledged() async {
  let incident = testChannelFailureIncident()
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  await core.failNextChannelFailureAcknowledgement()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  await model.updateEnabled(true)
  model.acknowledgeChannelFailure(incident.incidentId)
  for _ in 0..<100 where await core.channelFailureAcknowledgementCount == 0 {
    try? await Task.sleep(for: .milliseconds(10))
  }

  #expect(model.channelFailureIncidents == [incident])
  #expect(model.channelFailureAcknowledgementFeedback[incident.incidentId] != nil)
  #expect(model.errorMessage == nil)
  #expect(model.modelEntryEnabled)

  await model.updateEnabled(false)
}

@MainActor
@Test
func incidentPaginationRefreshFailureDoesNotMisreportOrRetryAcknowledgement() async {
  let incident = testChannelFailureIncident()
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  await model.updateEnabled(true)
  await core.failNextDashboardRead()

  model.acknowledgeChannelFailure(incident.incidentId)
  for _ in 0..<100 where model.channelFailureIncidents.first?.acknowledgement == nil {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(model.channelFailureIncidents.first?.acknowledgement != nil)
  #expect(await core.channelFailureAcknowledgementCount == 1)
  #expect(
    model.channelFailureAcknowledgementFeedback[incident.incidentId]?.hasPrefix(
      "Acknowledgement saved."
    ) == true
  )

  model.acknowledgeChannelFailure(incident.incidentId)
  try? await Task.sleep(for: .milliseconds(10))
  #expect(await core.channelFailureAcknowledgementCount == 1)
  #expect(model.channelFailureIncidents.first?.acknowledgement != nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func concurrentIncidentAcknowledgementFeedbackIsScopedPerIncidentAndOrderIndependent() async {
  let failed = testChannelFailureIncident(index: 0, channel: .discord)
  let successful = testChannelFailureIncident(index: 1, channel: .iMessage)
  let core = MockCore()
  let blockedSuccess = NonCooperativeRpcGate()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [failed, successful]
  )
  await core.blockNextChannelFailureAcknowledgement(on: blockedSuccess)
  await core.failChannelFailureAcknowledgement(failed.incidentId)
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  await model.updateEnabled(true)

  model.acknowledgeChannelFailure(successful.incidentId)
  for _ in 0..<100 where !blockedSuccess.isWaiting {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(blockedSuccess.isWaiting)

  model.acknowledgeChannelFailure(failed.incidentId)
  for _ in 0..<100
  where model.channelFailureAcknowledgementFeedback[failed.incidentId] == nil {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.channelFailureAcknowledgementFeedback[failed.incidentId] != nil)

  blockedSuccess.resume()
  for _ in 0..<100
  where model.channelFailureIncidents.first(where: {
    $0.incidentId == successful.incidentId
  })?.acknowledgement == nil {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(model.channelFailureAcknowledgementFeedback[failed.incidentId] != nil)
  #expect(model.channelFailureAcknowledgementFeedback[successful.incidentId] == nil)
  #expect(
    model.channelFailureIncidents.first(where: {
      $0.incidentId == failed.incidentId
    })?.acknowledgement == nil
  )
  #expect(
    model.channelFailureIncidents.first(where: {
      $0.incidentId == successful.incidentId
    })?.acknowledgement != nil
  )
  await model.updateEnabled(false)

  let reverseCore = MockCore()
  let blockedFailure = NonCooperativeRpcGate()
  await reverseCore.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [failed, successful]
  )
  await reverseCore.blockNextChannelFailureAcknowledgement(on: blockedFailure)
  await reverseCore.failChannelFailureAcknowledgement(failed.incidentId)
  let reverse = AppModel(core: reverseCore, broker: MockBroker()) {}
  await reverse.refreshDashboard()
  await reverse.updateEnabled(true)

  reverse.acknowledgeChannelFailure(failed.incidentId)
  for _ in 0..<100 where !blockedFailure.isWaiting {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(blockedFailure.isWaiting)
  reverse.acknowledgeChannelFailure(successful.incidentId)
  for _ in 0..<100
  where reverse.channelFailureIncidents.first(where: {
    $0.incidentId == successful.incidentId
  })?.acknowledgement == nil {
    try? await Task.sleep(for: .milliseconds(2))
  }

  blockedFailure.resume()
  for _ in 0..<100
  where reverse.channelFailureAcknowledgementFeedback[failed.incidentId] == nil {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(reverse.channelFailureAcknowledgementFeedback[failed.incidentId] != nil)
  #expect(reverse.channelFailureAcknowledgementFeedback[successful.incidentId] == nil)
  #expect(
    reverse.channelFailureIncidents.first(where: {
      $0.incidentId == successful.incidentId
    })?.acknowledgement != nil
  )
  await reverse.updateEnabled(false)
}

@MainActor
@Test
func globalOffCancelsAConcurrentIncidentAcknowledgementAndRejectsItsLateResult() async {
  let incident = testChannelFailureIncident()
  let core = MockCore()
  let blockedAcknowledgement = NonCooperativeRpcGate()
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  await core.blockNextChannelFailureAcknowledgement(on: blockedAcknowledgement)
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.refreshDashboard()
  await model.updateEnabled(true)

  model.acknowledgeChannelFailure(incident.incidentId)
  for _ in 0..<100 where !blockedAcknowledgement.isWaiting {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(blockedAcknowledgement.isWaiting)
  await model.updateEnabled(false)
  blockedAcknowledgement.resume()
  for _ in 0..<100 where blockedAcknowledgement.isWaiting {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(model.runtimeDisplayState == .off)
  #expect(model.channelFailureIncidents == [incident])
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.errorMessage == nil)
  #expect(await core.channelSends.isEmpty)
}

@Test
func terminalIncidentContractsRejectMalformedCrossChannelAndRegressiveState() throws {
  let incident = testChannelFailureIncident()
  let acknowledged = testChannelFailureIncident(acknowledged: true)
  #expect(try acknowledged.mergedMonotonically(with: incident) == acknowledged)

  let malformed = ChannelFailureIncident(
    incidentId: "channel-failure-not-a-digest",
    channel: .iMessage,
    failureClass: .modelResultUnavailable,
    occurredAtMs: 10,
    runtimeRevision: 1,
    dispatchStateHash: String(repeating: "a", count: 64),
    sourceAuditAnchor: incident.sourceAuditAnchor,
    incidentAuditAnchor: incident.incidentAuditAnchor,
    acknowledgement: nil
  )
  #expect(throws: CoreClientError.self) {
    try malformed.validated()
  }
  #expect(throws: CoreClientError.self) {
    try ChannelFailureIncident.validateCollection([incident, incident])
  }
  #expect(throws: CoreClientError.self) {
    try ChannelPollResponse(
      connectionStatus: "connected",
      eventStatus: "needYou",
      suggestion: nil,
      failureIncidents: [incident]
    ).validated(for: .discord)
  }
}

@Test
func channelRouteSetValidationRejectsMalformedPrimaryDuplicateAndMissionMismatch() throws {
  let valid = testChannelRouteSet()
  _ = try valid.validated(expectedMissionId: valid.missionId)
  let primary = try #require(valid.primaryRoute)

  #expect(throws: CoreClientError.self) {
    try ChannelRouteSet(
      missionId: valid.missionId,
      revision: valid.revision,
      primaryRouteId: "route-missing",
      routes: valid.routes
    ).validated(expectedMissionId: valid.missionId)
  }
  #expect(throws: CoreClientError.self) {
    try ChannelRouteSet(
      missionId: valid.missionId,
      revision: valid.revision,
      primaryRouteId: primary.routeId,
      routes: [primary, primary]
    ).validated(expectedMissionId: valid.missionId)
  }
  let futureRoute = ChannelRoute(
    routeId: primary.routeId,
    role: primary.role,
    channel: primary.channel,
    conversationId: primary.conversationId,
    ownerSenderId: primary.ownerSenderId,
    providerIdentity: primary.providerIdentity,
    sourceMessageId: primary.sourceMessageId,
    allowedInboundClasses: primary.allowedInboundClasses,
    allowedOutboundClasses: primary.allowedOutboundClasses,
    revision: valid.revision + 1,
    approvalId: primary.approvalId,
    auditId: primary.auditId,
    boundAtMs: primary.boundAtMs,
    updatedAtMs: primary.updatedAtMs
  )
  #expect(throws: CoreClientError.self) {
    try ChannelRouteSet(
      missionId: valid.missionId,
      revision: valid.revision,
      primaryRouteId: futureRoute.routeId,
      routes: [futureRoute]
    ).validated(expectedMissionId: valid.missionId)
  }
  #expect(throws: CoreClientError.self) {
    try valid.validated(expectedMissionId: "mission-other")
  }
}

@Test
func channelPollValidationRejectsUnknownAndContradictoryStates() {
  let suggestion = OutcomeSuggestion(
    id: testSuggestionOneId,
    title: "Prepare the exact result",
    whyNow: "The owner requested it",
    proposedSteps: ["Prepare it"],
    sourceRefs: ["channel:message-1"]
  )
  #expect(throws: CoreClientError.self) {
    try ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "unknown", suggestion: nil
    ).validated(for: .iMessage)
  }
  #expect(throws: CoreClientError.self) {
    try ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "ready", suggestion: nil
    ).validated(for: .iMessage)
  }
  #expect(throws: CoreClientError.self) {
    try ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "idle", suggestion: suggestion
    ).validated(for: .iMessage)
  }
  #expect(throws: CoreClientError.self) {
    try ChannelPollResponse(
      connectionStatus: "connected", eventStatus: "needYou", suggestion: nil
    ).validated(for: .iMessage)
  }
}

@Test
func channelSendValidationRejectsBlankOrContradictoryProviderIdentity() throws {
  _ = try ChannelSendResponse(status: "sent", providerMessageId: "provider-message-1")
    .validated()
  _ = try ChannelSendResponse(status: "needYou", providerMessageId: nil).validated()
  #expect(throws: CoreClientError.self) {
    try ChannelSendResponse(status: "sent", providerMessageId: " ").validated()
  }
  #expect(throws: CoreClientError.self) {
    try ChannelSendResponse(status: "needYou", providerMessageId: "provider-message-1")
      .validated()
  }
  #expect(throws: CoreClientError.self) {
    try ChannelSendResponse(status: "unknown", providerMessageId: nil).validated()
  }
}

@Test
func discordSetupValidationRejectsUnofficialLinkAndChangedIdentity() throws {
  let identity = DiscordBotIdentity(botUserId: 3_003, applicationId: 4_004, botName: "OpenOpen")
  let validSetup = DiscordSetupStart(
    identity: identity,
    installUrl:
      "https://discord.com/api/oauth2/authorize?client_id=4004&scope=bot&permissions=101376",
    pairingCode: String(repeating: "a", count: 32),
    status: "connecting"
  )
  _ = try validSetup.validated()
  #expect(throws: CoreClientError.self) {
    try DiscordSetupStart(
      identity: identity,
      installUrl: "https://example.invalid/oauth2/authorize",
      pairingCode: String(repeating: "a", count: 32),
      status: "connecting"
    ).validated()
  }

  let incompletePermissions = DiscordPermissionProbe(
    viewChannel: "passed",
    sendMessages: "missing",
    readMessageHistory: "passed",
    attachFiles: "passed",
    historyReadback: "passed",
    effectivePermissionBits: 101_376
  )
  let changedIdentity = DiscordPairingCandidate(
    candidateId: "discord-pair-" + String(repeating: "b", count: 64),
    sourceMessageId: "5005",
    guildId: "6006",
    guildName: "OpenOpen Test",
    channelId: "2002",
    channelName: "outcomes",
    ownerUserId: "1001",
    ownerName: "Owner",
    botUserId: "9999",
    applicationId: "4004",
    receivedAtMs: 1,
    messageContentIntentReady: true,
    permissions: incompletePermissions
  )
  #expect(throws: CoreClientError.self) {
    try changedIdentity.validated(expectedIdentity: identity)
  }
}

@MainActor
@Test
func globalOffDuringIMessageActivationRejectsTheLateGeneration() async {
  let core = MockCore()
  await core.delayIMessageActivation(by: .milliseconds(100))
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")

  let connecting = Task { await model.connectIMessage() }
  try? await Task.sleep(for: .milliseconds(20))
  await model.updateEnabled(false)
  await connecting.value

  #expect(!model.enabled)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.errorMessage == nil)
  #expect(await core.channelPollCount == 0)
  #expect(await core.stoppedChannels == [.iMessage])
}

@MainActor
@Test
func globalOffDuringChannelPollRejectsLateStatusAndSuggestion() async {
  let core = MockCore()
  await core.delayChannelPoll(by: .milliseconds(100))
  await core.queueChannelSuggestion(
    OutcomeSuggestion(
      id: "late-channel-suggestion",
      title: "Must not return after Off",
      whyNow: "A delayed poll completed after the runtime fence changed",
      proposedSteps: ["Remain stopped"],
      sourceRefs: ["imessage:late-message"]
    ))
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()

  for _ in 0..<100 where await core.channelPollCount == 0 {
    try? await Task.sleep(for: .milliseconds(5))
  }
  #expect(await core.channelPollCount == 1)
  await model.updateEnabled(false)
  try? await Task.sleep(for: .milliseconds(150))

  #expect(!model.enabled)
  #expect(model.iMessageStatus == "disconnected")
  #expect(model.suggestion == nil)
  #expect(model.latestChannelMissionEvent == nil)
  #expect(model.errorMessage == nil)
  let pollsAfterLateReturn = await core.channelPollCount
  #expect(pollsAfterLateReturn == 1)
  try? await Task.sleep(for: .milliseconds(1_100))
  #expect(await core.channelPollCount == pollsAfterLateReturn)
}

@MainActor
@Test
func missionBoundChannelEventIsVisibleOnlyOnItsExactMissionRoute() async {
  let core = MockCore()
  let mission = testConfirmedMission()
  let routes = testChannelRouteSet(
    channel: .iMessage,
    conversationId: "42",
    ownerSenderId: "owner@example.invalid"
  )
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routes)
  await core.queueChannelMissionEvent(
    ChannelMissionEvent(
      eventId: "channel-event-1",
      missionId: mission.missionId,
      missionRevision: 12,
      missionAnchorHash: String(repeating: "a", count: 64),
      routeId: routes.primaryRouteId,
      routeSetRevision: routes.revision,
      messageClass: .needYouResponse,
      channel: .iMessage,
      sourceMessageId: "message-2",
      contentSha256: String(repeating: "b", count: 64),
      recordedAtMs: 20
    ))
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")

  await model.connectIMessage()
  for _ in 0..<50 where model.latestChannelMissionEvent == nil {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.latestChannelMissionEvent?.eventId == "channel-event-1")
  #expect(model.latestChannelMissionEvent?.missionId == mission.missionId)
  #expect(model.latestChannelMissionEvent?.messageClass == .needYouResponse)
  #expect(model.confirmedMission?.missionId == mission.missionId)
  #expect(model.receipt == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func missionParticipationCannotLeakFromAReceiptIntoTheNextMission() async {
  let core = MockCore()
  let missionA = testConfirmedMission(missionId: "mission-a", title: "First Mission")
  let routesA = testChannelRouteSet(
    missionId: missionA.missionId,
    channel: .iMessage,
    conversationId: "42",
    ownerSenderId: "owner@example.invalid"
  )
  let eventA = ChannelMissionEvent(
    eventId: "channel-event-mission-a",
    missionId: missionA.missionId,
    missionRevision: 12,
    missionAnchorHash: String(repeating: "a", count: 64),
    routeId: routesA.primaryRouteId,
    routeSetRevision: routesA.revision,
    messageClass: .missionParticipation,
    channel: .iMessage,
    sourceMessageId: "message-mission-a",
    contentSha256: String(repeating: "b", count: 64),
    recordedAtMs: 20
  )
  await core.restoreFromDashboard(mission: missionA, receipt: nil, channelRouteSet: routesA)
  await core.queueChannelMissionEvent(eventA)
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")
  await model.connectIMessage()
  for _ in 0..<50 where model.latestChannelMissionEvent == nil {
    try? await Task.sleep(for: .milliseconds(20))
  }
  #expect(model.latestChannelMissionEvent == eventA)

  let receiptA = MissionReceipt(
    id: "receipt-a",
    missionId: missionA.missionId,
    summary: "Completed First Mission",
    actualModel: "gpt-test-model",
    evidenceIds: ["evidence-a"],
    outputHashes: [],
    completedAtMs: 30
  )
  await core.restoreFromDashboard(mission: nil, receipt: receiptA, channelRouteSet: routesA)
  await model.refreshDashboard()
  #expect(model.receipt == receiptA)
  #expect(model.latestChannelMissionEvent == nil)

  let missionB = testConfirmedMission(missionId: "mission-b", title: "Second Mission")
  let routesB = testChannelRouteSet(missionId: missionB.missionId)
  await core.restoreFromDashboard(mission: missionB, receipt: nil, channelRouteSet: routesB)
  await model.refreshDashboard()
  #expect(model.confirmedMission?.missionId == missionB.missionId)
  #expect(model.latestChannelMissionEvent == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func missionBoundChannelEventRejectsAChangedRouteRevision() async {
  let core = MockCore()
  let mission = testConfirmedMission()
  let routes = testChannelRouteSet(
    channel: .iMessage,
    conversationId: "42",
    ownerSenderId: "owner@example.invalid"
  )
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routes)
  await core.queueChannelMissionEvent(
    ChannelMissionEvent(
      eventId: "channel-event-stale-route",
      missionId: mission.missionId,
      missionRevision: 12,
      missionAnchorHash: String(repeating: "a", count: 64),
      routeId: routes.primaryRouteId,
      routeSetRevision: routes.revision + 1,
      messageClass: .missionParticipation,
      channel: .iMessage,
      sourceMessageId: "message-3",
      contentSha256: String(repeating: "b", count: 64),
      recordedAtMs: 21
    ))
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")

  await model.connectIMessage()
  for _ in 0..<50 where model.iMessageStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.iMessageStatus == "faulted")
  #expect(model.latestChannelMissionEvent == nil)
  #expect(model.errorMessage == nil)
  #expect(model.channelListenerFeedback[.iMessage] != nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func historicalMissionEventRecoversAfterAnAdditionalRouteAdvancesTheSet() async {
  let core = MockCore()
  let mission = testConfirmedMission()
  let routes = testChannelRouteSetWithAdditionalRoute()
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routes)
  await core.queueChannelMissionEvent(
    ChannelMissionEvent(
      eventId: "channel-event-historical-recovery",
      missionId: mission.missionId,
      missionRevision: 12,
      missionAnchorHash: String(repeating: "a", count: 64),
      routeId: routes.primaryRouteId,
      routeSetRevision: 1,
      messageClass: .needYouResponse,
      channel: .iMessage,
      sourceMessageId: "message-historical",
      contentSha256: String(repeating: "b", count: 64),
      recordedAtMs: 20
    ),
    eventStatus: "missionUpdateRecovered"
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")

  await model.connectIMessage()
  for _ in 0..<50 where model.latestChannelMissionEvent == nil {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.iMessageStatus == "connected")
  #expect(model.latestChannelMissionEvent?.eventId == "channel-event-historical-recovery")
  #expect(model.channelRouteSet?.revision == 2)
  #expect(model.confirmedMission?.missionId == mission.missionId)
  #expect(model.receipt == nil)
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func historicalMissionEventRejectsAnUnknownRoute() async {
  let core = MockCore()
  let mission = testConfirmedMission()
  let routes = testChannelRouteSetWithAdditionalRoute()
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routes)
  await core.queueChannelMissionEvent(
    ChannelMissionEvent(
      eventId: "channel-event-unknown-route",
      missionId: mission.missionId,
      missionRevision: 12,
      missionAnchorHash: String(repeating: "a", count: 64),
      routeId: "route-unknown",
      routeSetRevision: 1,
      messageClass: .missionParticipation,
      channel: .iMessage,
      sourceMessageId: "message-unknown-route",
      contentSha256: String(repeating: "b", count: 64),
      recordedAtMs: 20
    ),
    eventStatus: "missionUpdateRecovered"
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")

  await model.connectIMessage()
  for _ in 0..<50 where model.iMessageStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.iMessageStatus == "faulted")
  #expect(model.latestChannelMissionEvent == nil)
  #expect(model.errorMessage == nil)
  #expect(model.channelListenerFeedback[.iMessage] != nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func historicalMissionEventRejectsARouteCreatedAfterTheEventRevision() async {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  let mission = testConfirmedMission()
  let routes = testChannelRouteSetWithAdditionalRoute()
  await core.setChannelPairing(testDiscordPairing())
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routes)
  await core.queueChannelMissionEvent(
    ChannelMissionEvent(
      eventId: "channel-event-before-route-existed",
      missionId: mission.missionId,
      missionRevision: 12,
      missionAnchorHash: String(repeating: "a", count: 64),
      routeId: "route-additional-discord",
      routeSetRevision: 1,
      messageClass: .missionParticipation,
      channel: .discord,
      sourceMessageId: "message-before-route-existed",
      contentSha256: String(repeating: "b", count: 64),
      recordedAtMs: 20
    ),
    eventStatus: "missionUpdateRecovered"
  )
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore
  ) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()
  model.discordTokenDraft = "test-only-discord-token"

  await model.connectDiscord()
  for _ in 0..<50 where model.discordStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(20))
  }

  #expect(model.discordStatus == "faulted")
  #expect(model.latestChannelMissionEvent == nil)
  #expect(model.errorMessage == nil)
  #expect(model.channelListenerFeedback[.discord] != nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func iMessageActivationFailureStopsPreparedChildAndRetryConnects() async {
  let core = MockCore()
  await core.failNextIMessageActivation()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")

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
  model.selectIMessageChat("42")
  model.selectIMessageOwner("owner@example.invalid")

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
  await core.setChannelPairing(testIMessagePairing())
  await core.restoreFromDashboard(
    mission: testConfirmedMission(),
    receipt: nil,
    channelRouteSet: testChannelRouteSet(
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid"
    )
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await connectTestIMessage(model)
  await model.refreshDashboard()
  model.channelMessageDraft = "Working on it — the bounded Mission is active."
  let earliestApproval = Int64((Date().timeIntervalSince1970 * 1_000).rounded(.down))

  await model.sendChannelProgress()

  let latestApproval = Int64((Date().timeIntervalSince1970 * 1_000).rounded(.down))
  let sends = await core.channelSends
  #expect(sends.count == 1)
  #expect(sends.first?.missionId == "mission-1")
  #expect(sends.first?.routeId == "route-primary")
  #expect(sends.first?.kind == .progress)
  #expect(sends.first?.content == "Working on it — the bounded Mission is active.")
  #expect((sends.first?.approvedAtMs ?? 0) >= earliestApproval)
  #expect((sends.first?.approvedAtMs ?? 0) <= latestApproval)
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func additionalMissionRouteNamesExactPairingAndDefaultsEveryOutboundClassOff() async {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  try? tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(testDiscordPairing())
  await core.restoreFromDashboard(
    mission: testConfirmedMission(),
    receipt: nil,
    channelRouteSet: testChannelRouteSet(
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid"
    )
  )
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore
  ) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()

  await model.prepareAdditionalRoute(.discord)

  #expect(model.pendingAdditionalRoute?.pairing.channel == .discord)
  #expect(model.pendingAdditionalRoute?.pairing.conversationId == "2002")
  #expect(model.pendingAdditionalRoute?.pairing.ownerSenderId == "1001")
  #expect(model.routeAllowsNeedYou == false)
  #expect(model.routeAllowsProgress == false)
  #expect(model.routeAllowsReceipt == false)

  await model.confirmAdditionalRoute()

  let approvals = await core.channelRouteApprovals
  #expect(approvals.count == 1)
  #expect(approvals[0].allowedInboundClasses == [.missionParticipation, .needYouResponse])
  #expect(approvals[0].allowedOutboundClasses.isEmpty)
  #expect(approvals[0].providerIdentity == "4004")
  #expect(model.channelRouteSet?.revision == 2)
  #expect(model.channelRouteSet?.routes.last?.role == .additional)
  #expect(model.channelRouteSet?.routes.last?.allowedOutboundClasses.isEmpty == true)
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func explicitAdditionalRouteClassAndSelectionDriveTheExactOutboundRouteId() async {
  let core = MockCore()
  let tokenStore = MockDiscordTokenStore()
  try? tokenStore.save("test-only-discord-token")
  await core.setChannelPairing(testDiscordPairing())
  await core.restoreFromDashboard(
    mission: testConfirmedMission(),
    receipt: nil,
    channelRouteSet: testChannelRouteSet(
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid"
    )
  )
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    discordTokenStore: tokenStore
  ) {}
  await model.updateEnabled(true)
  await model.connectDiscord()
  await model.refreshDashboard()
  await model.prepareAdditionalRoute(.discord)
  model.routeAllowsProgress = true

  await model.confirmAdditionalRoute()
  model.selectedChannelRouteId = "route-additional-discord"
  model.channelMessageDraft = "Exact approved Discord progress."
  await model.sendChannelProgress()

  let approvals = await core.channelRouteApprovals
  let sends = await core.channelSends
  #expect(approvals.first?.allowedOutboundClasses == [.progress])
  #expect(sends.count == 1)
  #expect(sends.first?.routeId == "route-additional-discord")
  #expect(sends.first?.kind == .progress)
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func channelNeedYouSendUsesOnlyTheExactRestoredPrompt() async {
  let core = MockCore()
  await core.setChannelPairing(testIMessagePairing())
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelRouteSet: testChannelRouteSet(
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid"
    ),
    needsYou: MissionNeedsYou(
      missionId: "mission-1",
      title: "Plan the day",
      prompt: "Choose the one approved destination.",
      createdAtMs: 2
    )
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await connectTestIMessage(model)
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
func needYouCannotUseAnotherMissionsRouteOrAdmitAnotherOutcome() async {
  let core = MockCore()
  await core.restoreFromDashboard(
    mission: testConfirmedMission(missionId: "mission-b", title: "Second Mission"),
    receipt: nil,
    channelRouteSet: testChannelRouteSet(missionId: "mission-b"),
    needsYou: MissionNeedsYou(
      missionId: "mission-a",
      title: "First Mission",
      prompt: "Finish the first exact boundary.",
      createdAtMs: 2
    )
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()
  model.prompt = "This second Outcome must remain blocked."

  #expect(!model.selectedRouteAllowsNeedYou)
  #expect(!model.dashboardControls.outcomeInputEnabled)
  #expect(!model.dashboardControls.outcomeSubmitEnabled)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  await model.sendChannelNeedYou()
  await model.submitPrompt()
  #expect(await core.channelSends.isEmpty)
  #expect(await core.proposalCount == 0)
  await model.updateEnabled(false)
}

@MainActor
@Test
func progressAndReceiptControlsRequireTheExactRouteMissionIdentity() async {
  let core = MockCore()
  let missionB = testConfirmedMission(missionId: "mission-b", title: "Second Mission")
  let routesA = testChannelRouteSet(missionId: "mission-a")
  await core.restoreFromDashboard(
    mission: missionB,
    receipt: nil,
    channelRouteSet: routesA
  )
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()
  model.channelMessageDraft = "This must not cross Mission identity."
  #expect(!model.selectedRouteAllowsProgress)
  await model.sendChannelProgress()
  #expect(await core.channelSends.isEmpty)

  let receiptB = MissionReceipt(
    id: "receipt-b",
    missionId: missionB.missionId,
    summary: "Completed Second Mission",
    actualModel: "gpt-test-model",
    evidenceIds: ["evidence-b"],
    outputHashes: [],
    completedAtMs: 20
  )
  await core.restoreFromDashboard(
    mission: nil,
    receipt: receiptB,
    channelRouteSet: routesA
  )
  await model.refreshDashboard()
  #expect(!model.selectedRouteAllowsReceipt)
  await model.sendChannelReceipt()
  #expect(await core.channelSends.isEmpty)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  await model.updateEnabled(false)
}

@MainActor
@Test
func channelMissionCompletionAuthorizesAndReturnsTheExactEvidenceReceipt() async {
  let core = MockCore()
  let reminders = MockReminders()
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
  await core.setChannelPairing(testIMessagePairing())
  await core.restoreFromDashboard(
    mission: mission,
    receipt: nil,
    channelRouteSet: testChannelRouteSet(
      missionId: mission.missionId,
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid"
    )
  )
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  await connectTestIMessage(model)
  await model.refreshDashboard()
  await model.checkMissionProgress()

  let approvals = await core.receiptReturnApprovals
  let receiptRouteIds = await core.receiptReturnRouteIds
  let sends = await core.channelSends
  #expect(approvals.count == 1)
  #expect(approvals[0] != nil)
  #expect(receiptRouteIds == ["route-primary"])
  #expect(sends.count == 1)
  #expect(sends.first?.kind == .receipt)
  #expect(
    sends.first?.content
      == "Done: Completed Plan the day\nEvidence: 1 verified completion\nModel: gpt-test-model"
  )
  #expect(model.receipt?.missionId == "mission-1")
  #expect(model.channelRouteSet?.missionId == "mission-1")
  #expect(model.errorMessage == nil)
  await model.updateEnabled(false)
}

@MainActor
@Test
func progressResponseLossRecoversAfterAppRestartWithoutDuplicateSend() async {
  let core = MockCore()
  let broker = MockBroker()
  let mission = testConfirmedMission()
  await core.setChannelPairing(testIMessagePairing())
  await core.restoreFromDashboard(
    mission: mission,
    receipt: nil,
    channelRouteSet: testChannelRouteSet(
      missionId: mission.missionId,
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid"
    )
  )
  await core.loseNextChannelSendResponseAfterCommit()
  let first = AppModel(core: core, broker: broker) {}
  await first.updateEnabled(true)
  await connectTestIMessage(first)
  await first.refreshDashboard()
  first.channelMessageDraft = "One exact progress update."
  await first.sendChannelProgress()
  #expect(await core.channelSends.count == 1)
  #expect(await core.channelSendAttemptCount == 1)
  await first.updateEnabled(false)

  try? await Task.sleep(for: .milliseconds(5))
  let restarted = AppModel(core: core, broker: broker) {}
  await restarted.updateEnabled(true)
  await connectTestIMessage(restarted)
  await restarted.refreshDashboard()
  #expect(restarted.iMessageIsConnected)
  #expect(restarted.selectedRouteAllowsProgress)
  restarted.channelMessageDraft = "One exact progress update."
  await restarted.sendChannelProgress()

  #expect(await core.channelSendAttemptCount == 2)
  #expect(await core.channelSends.count == 1)
  #expect(restarted.errorMessage == nil)
}

@MainActor
@Test
func needYouUncertainSendRecoversAfterAppRestartWithoutDuplicateSend() async {
  let core = MockCore()
  let broker = MockBroker()
  let needsYou = MissionNeedsYou(
    missionId: "mission-1",
    title: "Plan the day",
    prompt: "Choose the exact approved option.",
    createdAtMs: 2
  )
  await core.setChannelPairing(testIMessagePairing())
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelRouteSet: testChannelRouteSet(
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid"
    ),
    needsYou: needsYou
  )
  await core.returnNextChannelSendUncertainAfterCommit()
  let first = AppModel(core: core, broker: broker) {}
  await first.updateEnabled(true)
  await connectTestIMessage(first)
  await first.refreshDashboard()
  await first.sendChannelNeedYou()
  #expect(await core.channelSends.count == 1)
  #expect(await core.channelSendAttemptCount == 1)
  await first.updateEnabled(false)

  try? await Task.sleep(for: .milliseconds(5))
  let restarted = AppModel(core: core, broker: broker) {}
  await restarted.updateEnabled(true)
  await connectTestIMessage(restarted)
  await restarted.refreshDashboard()
  #expect(restarted.iMessageIsConnected)
  #expect(restarted.selectedRouteAllowsNeedYou)
  await restarted.sendChannelNeedYou()

  #expect(await core.channelSendAttemptCount == 2)
  #expect(await core.channelSends.count == 1)
  #expect(restarted.errorMessage == nil)
}

@MainActor
@Test
func automaticReceiptResponseLossRecoversAfterAppRestartWithoutDuplicateSend() async {
  let core = MockCore()
  let broker = MockBroker()
  let reminders = MockReminders()
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
  await core.setChannelPairing(testIMessagePairing())
  await core.restoreFromDashboard(
    mission: mission,
    receipt: nil,
    channelRouteSet: testChannelRouteSet(
      missionId: mission.missionId,
      channel: .iMessage,
      conversationId: "42",
      ownerSenderId: "owner@example.invalid"
    )
  )
  await core.loseNextChannelSendResponseAfterCommit()
  let first = AppModel(core: core, broker: broker, reminders: reminders) {}
  await first.updateEnabled(true)
  await connectTestIMessage(first)
  await first.refreshDashboard()
  await first.checkMissionProgress()
  #expect(first.receipt?.missionId == mission.missionId)
  #expect(await core.channelSends.count == 1)
  #expect(await core.channelSendAttemptCount == 1)
  await first.updateEnabled(false)

  try? await Task.sleep(for: .milliseconds(5))
  let restarted = AppModel(core: core, broker: broker, reminders: reminders) {}
  await restarted.updateEnabled(true)
  await connectTestIMessage(restarted)
  await restarted.refreshDashboard()
  #expect(restarted.iMessageIsConnected)
  #expect(restarted.selectedRouteAllowsReceipt)
  await restarted.sendChannelReceipt()

  #expect(await core.channelSendAttemptCount == 2)
  #expect(await core.channelSends.count == 1)
  #expect(restarted.receipt?.missionId == mission.missionId)
  #expect(restarted.errorMessage == nil)
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
func visibleRuntimeToggleCanCancelASlowOnBeforeItCommits() async {
  let core = MockCore()
  let broker = MockBroker()
  await broker.delayAndFailNextOn()
  let model = AppModel(core: core, broker: broker) {}

  // This is the same get/set seam used by both visible SwiftUI Toggles.
  model.requestEnabled(true)
  for _ in 0..<100 where model.runtimeDisplayState != .turningOn {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.runtimeToggleValue)
  #expect(model.runtimeDisplayState == .turningOn)

  // A second click must read the requested On state and therefore emit Off,
  // even though the protected Store has not yet committed On.
  model.requestEnabled(!model.runtimeToggleValue)
  #expect(!model.runtimeToggleValue)

  for _ in 0..<200 where model.runtimeDisplayState != .off {
    try? await Task.sleep(for: .milliseconds(5))
  }
  #expect(!model.runtimeToggleValue)
  #expect(!model.enabled)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.runtimeRecoveryState == .ready)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  let finalStatus = await broker.status(challenge: String(repeating: "a", count: 64))
  #expect(finalStatus?.authorization.enabled == false)
}

@MainActor
@Test
func firstUnknownHydrationToggleIsAlwaysASafeOffAction() async {
  let core = MockCore()
  let broker = MockBroker()
  do {
    let seeded = AppModel(core: core, broker: broker) {}
    await seeded.updateEnabled(true)
    #expect(seeded.enabled)
  }

  // A newly launched UI has not hydrated the protected On row yet. Its first
  // visible switch value must conservatively offer Off, never a false Off
  // placeholder whose click would start listeners/model restoration.
  let restarted = AppModel(core: core, broker: broker) {}
  #expect(restarted.runtimeDisplayState == .unknown)
  #expect(restarted.runtimeToggleValue)
  restarted.requestEnabled(!restarted.runtimeToggleValue)

  for _ in 0..<200 where restarted.runtimeDisplayState != .off {
    try? await Task.sleep(for: .milliseconds(5))
  }
  #expect(!restarted.runtimeToggleValue)
  #expect(!restarted.enabled)
  #expect(restarted.runtimeDisplayState == .off)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  let finalStatus = await broker.status(challenge: String(repeating: "b", count: 64))
  #expect(finalStatus?.authorization.enabled == false)
}

@MainActor
@Test
func mismatchedRecoveredTimestampCannotAuthorizeModelEntry() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshAccountAndModels()
  await model.refreshDashboard()
  #expect(model.choiceLoopContinuityState == .empty)
  let accepted = ChoiceBeginAccepted(
    requestId: "choice-request-recovery-mismatch",
    operationId: "choice-operation-recovery-mismatch",
    choiceSessionId: "session-choice-1",
    acceptedSessionRevision: 1,
    sourceEnvelopeId: "source-envelope-recovery-mismatch",
    conversationTurnBatchId: "batch-choice-recovery-mismatch",
    state: "interpreting"
  )
  await core.setChoiceLoopSnapshot(testInterpretingChoiceLoopSnapshot())
  await core.setChoiceBeginAccepted(accepted)
  await core.returnMismatchedRecoveryTimestamp()
  model.choiceQuestion = "must stay local"
  await model.submitChoiceQuestion()
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
func dashboardStateMatrixAlwaysLeavesProtectedControlsReachable() {
  let displayStates: [RuntimeDisplayState] = [
    .off, .on, .turningOn, .turningOff, .unknown,
  ]
  let recoveryStates: [RuntimeRecoveryState] = [
    .ready, .recovering, .awaitingAccount, .paused,
  ]

  for displayState in displayStates {
    for recoveryState in recoveryStates {
      for storeControlEnabled in [false, true] {
        for isBusy in [false, true] {
          for hasMission in [false, true] {
            for hasNeedsYou in [false, true] {
              for hasIncident in [false, true] {
                let controls = DashboardControlState.evaluate(
                  runtimeDisplayState: displayState,
                  runtimeRecoveryState: recoveryState,
                  modelEntryEnabled: displayState == .on && recoveryState == .ready,
                  storeControlEnabled: storeControlEnabled,
                  isBusy: isBusy,
                  hasConfirmedMission: hasMission,
                  hasCancellableMission: hasMission || hasNeedsYou,
                  hasNeedsYou: hasNeedsYou,
                  hasSuggestion: !hasMission && !hasNeedsYou,
                  suggestionMatchesConfirmedMission: !hasMission,
                  hasReceipt: false,
                  terminalIncidentCount: hasIncident ? 1 : 0,
                  localFeedbackPresent: hasIncident,
                  prompt: hasIncident ? "Draft remains editable." : "Prepare one outcome."
                )
                #expect(controls.globalToggleEnabled)
                #expect(controls.settingsEnabled)
                if displayState != .on || recoveryState != .ready || isBusy || hasMission
                  || hasNeedsYou
                {
                  #expect(!controls.outcomeInputEnabled)
                }
                if !isBusy, hasMission || hasNeedsYou {
                  #expect(controls.missionCancellationEnabled == storeControlEnabled)
                }
                #expect(
                  controls.missionProgressEnabled
                    == (storeControlEnabled && !isBusy && hasMission)
                )
              }
            }
          }
        }
      }
    }
  }
}

@Test
func dashboardRejectsVisibleDoneWithoutBoundedEvidence() {
  let invalidReceipt = MissionReceipt(
    id: "receipt-1",
    missionId: "mission-1",
    summary: "Completed Plan the day",
    actualModel: "gpt-test-model",
    evidenceIds: [],
    outputHashes: [],
    completedAtMs: 10
  )
  let dashboard = DashboardState(
    activeCards: [],
    microphone: MicrophoneState(available: false, reason: "Unavailable"),
    runtime: RuntimeControl(enabled: true, revision: 1, updatedAtMs: 1),
    suggestion: nil,
    receipt: invalidReceipt
  )
  #expect(throws: CoreClientError.self) {
    try dashboard.validated()
  }
}

@Test
func dashboardRejectsUnreachableOrContradictoryMissionStates() {
  let mission = testConfirmedMission()
  let microphone = MicrophoneState(available: false, reason: "Unavailable")
  let runtime = RuntimeControl(enabled: true, revision: 1, updatedAtMs: 1)

  let missingCard = DashboardState(
    activeCards: [],
    microphone: microphone,
    runtime: runtime,
    suggestion: nil,
    confirmedMission: mission
  )
  #expect(throws: CoreClientError.self) { try missingCard.validated() }

  let conflictingSuggestion = DashboardState(
    activeCards: [
      ActiveOutcomeCard(id: mission.missionId, title: mission.title, state: "working")
    ],
    microphone: microphone,
    runtime: runtime,
    suggestion: OutcomeSuggestion(
      id: "suggestion-conflict",
      title: "Different work",
      whyNow: "This must not replace the active Mission.",
      proposedSteps: ["Different step"],
      sourceRefs: []
    ),
    confirmedMission: mission
  )
  #expect(throws: CoreClientError.self) { try conflictingSuggestion.validated() }

  let pausedCardWithUnrelatedSuggestion = DashboardState(
    activeCards: [
      ActiveOutcomeCard(id: "mission-paused", title: "Paused work", state: "Paused")
    ],
    microphone: microphone,
    runtime: runtime,
    suggestion: OutcomeSuggestion(
      id: "suggestion-unrelated",
      title: "Different work",
      whyNow: "This must wait behind the paused Mission.",
      proposedSteps: ["Different step"],
      sourceRefs: []
    )
  )
  #expect(throws: CoreClientError.self) {
    try pausedCardWithUnrelatedSuggestion.validated()
  }

  let needYou = MissionNeedsYou(
    missionId: "mission-needs-you",
    title: "Need one owner action",
    prompt: "Complete the exact approved action.",
    createdAtMs: 2
  )
  let missingNeedYouCard = DashboardState(
    activeCards: [],
    microphone: microphone,
    runtime: runtime,
    suggestion: nil,
    needsYou: needYou
  )
  #expect(throws: CoreClientError.self) { try missingNeedYouCard.validated() }

  let historicalReceipt = MissionReceipt(
    id: "receipt-old",
    missionId: "mission-old",
    summary: "Completed the previous Mission",
    actualModel: "gpt-test-model",
    evidenceIds: ["evidence-old"],
    outputHashes: [],
    completedAtMs: 1
  )
  let activeMissionWithHistoricalReceipt = DashboardState(
    activeCards: [
      ActiveOutcomeCard(id: mission.missionId, title: mission.title, state: "working")
    ],
    microphone: microphone,
    runtime: runtime,
    suggestion: nil,
    confirmedMission: mission,
    receipt: historicalReceipt
  )
  #expect(throws: CoreClientError.self) {
    try activeMissionWithHistoricalReceipt.validated()
  }

  let sameMissionReceipt = MissionReceipt(
    id: "receipt-current",
    missionId: mission.missionId,
    summary: "Must not be Done while work is active",
    actualModel: "gpt-test-model",
    evidenceIds: ["evidence-current"],
    outputHashes: [],
    completedAtMs: 2
  )
  let activeMissionWithSameReceipt = DashboardState(
    activeCards: [
      ActiveOutcomeCard(id: mission.missionId, title: mission.title, state: "working")
    ],
    microphone: microphone,
    runtime: runtime,
    suggestion: nil,
    confirmedMission: mission,
    receipt: sameMissionReceipt
  )
  #expect(throws: CoreClientError.self) {
    try activeMissionWithSameReceipt.validated()
  }

  let needYouWithReceipt = DashboardState(
    activeCards: [
      ActiveOutcomeCard(id: needYou.missionId, title: needYou.title, state: "Need you")
    ],
    microphone: microphone,
    runtime: runtime,
    suggestion: nil,
    needsYou: needYou,
    receipt: historicalReceipt
  )
  #expect(throws: CoreClientError.self) {
    try needYouWithReceipt.validated()
  }

  let suggestionWithHistoricalReceipt = DashboardState(
    activeCards: [],
    microphone: microphone,
    runtime: runtime,
    suggestion: OutcomeSuggestion(
      id: "suggestion-next",
      title: "Plan tomorrow",
      whyNow: "The owner asked for a later Outcome.",
      proposedSteps: ["Plan tomorrow"],
      sourceRefs: []
    ),
    receipt: historicalReceipt
  )
  #expect(throws: CoreClientError.self) {
    try suggestionWithHistoricalReceipt.validated()
  }
}

@MainActor
@Test
func dashboardGoldenPathKeepsControlsUsableUntilEvidenceBackedDone() async {
  let incident = testChannelFailureIncident(acknowledged: true)
  let core = MockCore()
  let reminders = MockReminders()
  let link = ReminderLink(
    missionId: "mission-1", workItemId: "work-1",
    sourceIdentifier: "source-1", calendarIdentifier: "calendar-1",
    calendarItemIdentifier: "reminder-work-1", dispatchToken: "dispatch-work-1",
    title: "Pick one priority"
  )
  let mission = testConfirmedMission(
    writeDisposition: .recoverOnly,
    reminderDispatch: [ConfirmedReminderDispatch(workItemId: "work-1", token: "dispatch-work-1")],
    reminderLinks: [link]
  )
  await core.restoreFromDashboard(
    mission: mission,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.refreshDashboard()
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  #expect(!model.dashboardControls.outcomeInputEnabled)

  await model.updateEnabled(true)
  await model.refreshDashboard()
  #expect(model.channelFailureIncidents == [incident])
  #expect(model.confirmedMission == mission)
  #expect(model.dashboardControls.missionProgressEnabled)
  #expect(!model.dashboardControls.doneVisible)

  await model.checkMissionProgress()
  #expect(model.receipt?.evidenceIds == ["evidence-work-1"])
  #expect(model.dashboardControls.doneVisible)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
}

@MainActor
@Test
func hostedDashboardActivatesTheEvidenceGatedGoldenPathAndRestoresDone() async throws {
  let incident = testChannelFailureIncident(acknowledged: true)
  let core = MockCore()
  let reminders = MockReminders()
  let link = ReminderLink(
    missionId: "mission-1", workItemId: "work-1",
    sourceIdentifier: "source-1", calendarIdentifier: "calendar-1",
    calendarItemIdentifier: "reminder-work-1", dispatchToken: "dispatch-work-1",
    title: "Pick one priority"
  )
  let mission = testConfirmedMission(
    writeDisposition: .recoverOnly,
    reminderDispatch: [ConfirmedReminderDispatch(workItemId: "work-1", token: "dispatch-work-1")],
    reminderLinks: [link]
  )
  await core.restoreFromDashboard(
    mission: mission,
    receipt: nil,
    channelFailureIncidents: [incident]
  )
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}

  _ = NSApplication.shared
  let hosting = NSHostingView(rootView: OpenOpenRootView(model: model))
  let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: 900, height: 760),
    styleMask: [.titled, .closable],
    backing: .buffered,
    defer: false
  )
  window.animationBehavior = .none
  window.isReleasedWhenClosed = false
  window.contentView = hosting
  window.makeKeyAndOrderFront(nil)
  for _ in 0..<500 {
    if model.channelFailureIncidents == [incident] { break }
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.channelFailureIncidents == [incident])
  await model.updateEnabled(true)
  for _ in 0..<500 where !model.modelEntryEnabled || model.isBusy {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.modelEntryEnabled)
  hosting.layoutSubtreeIfNeeded()
  #expect(model.confirmedMission == mission)
  #expect(model.reminderLinks == [link])
  #expect(!model.dashboardControls.doneVisible)
  hosting.layoutSubtreeIfNeeded()

  #expect(model.dashboardControls.missionProgressEnabled)
  let progress = try #require(
    dashboardInteractionAnchor(in: hosting, identifier: "openopen-dashboard-check-progress")
  )
  try clickDashboardInteractionAnchor(progress, in: window)
  for _ in 0..<500 {
    if model.receipt != nil && !model.isBusy { break }
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.receipt?.evidenceIds == ["evidence-work-1"])
  #expect(model.dashboardControls.doneVisible)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
  #expect(window.attachedSheet == nil)
  hosting.layoutSubtreeIfNeeded()
  #expect(
    dashboardInteractionAnchor(in: hosting, identifier: "openopen-dashboard-done") != nil
  )

  await model.updateEnabled(false)
  window.makeFirstResponder(nil)
  window.contentView = nil
  window.orderOut(nil)
  try? await Task.sleep(for: .milliseconds(30))

  let restarted = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await restarted.refreshDashboard()
  let restartedHosting = NSHostingView(rootView: OpenOpenRootView(model: restarted))
  let restartedWindow = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: 900, height: 760),
    styleMask: [.titled, .closable],
    backing: .buffered,
    defer: false
  )
  restartedWindow.animationBehavior = .none
  restartedWindow.isReleasedWhenClosed = false
  restartedWindow.contentView = restartedHosting
  restartedWindow.makeKeyAndOrderFront(nil)
  try? await Task.sleep(for: .milliseconds(30))
  restartedHosting.layoutSubtreeIfNeeded()

  #expect(restarted.receipt?.evidenceIds == ["evidence-work-1"])
  #expect(restarted.dashboardControls.doneVisible)
  #expect(restarted.dashboardControls.globalToggleEnabled)
  #expect(restarted.dashboardControls.settingsEnabled)
  #expect(
    dashboardInteractionAnchor(
      in: restartedHosting,
      identifier: "openopen-dashboard-done"
    ) != nil
  )
  #expect(restartedWindow.attachedSheet == nil)

  restartedWindow.makeFirstResponder(nil)
  restartedWindow.contentView = nil
  restartedWindow.orderOut(nil)
  try? await Task.sleep(for: .milliseconds(30))
}

@MainActor
@Test
func frozenEditorialShellKeepsTheChoiceEntryAndOffReachableAtNarrowWidth() async {
  let core = MockCore()
  let model = AppModel(core: core, broker: MockBroker(), reminders: MockReminders()) {}

  _ = NSApplication.shared
  let hosting = NSHostingView(rootView: OpenOpenRootView(model: model))
  let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: 390, height: 640),
    styleMask: [.titled, .closable],
    backing: .buffered,
    defer: false
  )
  window.animationBehavior = .none
  window.isReleasedWhenClosed = false
  window.contentView = hosting
  window.makeKeyAndOrderFront(nil)

  for _ in 0..<500 where model.runtimeDisplayState == .unknown {
    try? await Task.sleep(for: .milliseconds(2))
  }
  hosting.layoutSubtreeIfNeeded()

  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(dashboardOutcomeField(in: hosting) != nil)
  #expect(
    dashboardInteractionAnchor(
      in: hosting,
      identifier: "openopen-dashboard-outcome-submit"
    ) != nil
  )
  #expect(window.attachedSheet == nil)

  await model.updateEnabled(false)
  #expect(model.runtimeDisplayState == .off)
  #expect(model.dashboardControls.globalToggleEnabled)

  window.makeFirstResponder(nil)
  window.contentView = nil
  window.orderOut(nil)
  try? await Task.sleep(for: .milliseconds(30))
}

@MainActor
@Test
func goldenPathBoundarySnapshotsRemainActionableAfterAppRestart() async {
  let suggestion = OutcomeSuggestion(
    id: testRestartSuggestionId,
    title: "Plan the day",
    whyNow: "The owner requested it.",
    proposedSteps: ["Pick one priority"],
    sourceRefs: []
  )
  let mission = testConfirmedMission()
  let needsYou = MissionNeedsYou(
    missionId: mission.missionId,
    title: "Finish the approved reminder",
    prompt: "Complete the exact OpenOpen reminder, then check progress.",
    createdAtMs: 5
  )
  let receipt = MissionReceipt(
    id: "receipt-restart",
    missionId: mission.missionId,
    summary: "Completed Plan the day",
    actualModel: "gpt-test-model",
    evidenceIds: ["evidence-work-1"],
    outputHashes: [],
    completedAtMs: 10
  )
  let incident = testChannelFailureIncident()
  let snapshots:
    [(
      mission: ConfirmedMission?, receipt: MissionReceipt?, suggestion: OutcomeSuggestion?,
      needsYou: MissionNeedsYou?, incidents: [ChannelFailureIncident]
    )] = [
      (nil, nil, nil, nil, []),
      (nil, nil, nil, nil, [incident]),
      (nil, nil, suggestion, nil, [incident]),
      (mission, nil, nil, nil, [incident]),
      (nil, nil, nil, needsYou, [incident]),
      (nil, receipt, nil, nil, [incident]),
    ]

  for snapshot in snapshots {
    let core = MockCore()
    await core.restoreFromDashboard(
      mission: snapshot.mission,
      receipt: snapshot.receipt,
      needsYou: snapshot.needsYou,
      suggestion: snapshot.suggestion,
      channelFailureIncidents: snapshot.incidents
    )
    let restarted = AppModel(core: core, broker: MockBroker()) {}
    await restarted.refreshDashboard()

    #expect(restarted.confirmedMission == snapshot.mission)
    #expect(restarted.receipt == snapshot.receipt)
    #expect(restarted.suggestion == snapshot.suggestion)
    #expect(restarted.needsYou == snapshot.needsYou)
    #expect(restarted.channelFailureIncidents == snapshot.incidents)
    #expect(restarted.dashboardControls.globalToggleEnabled)
    #expect(restarted.dashboardControls.settingsEnabled)
  }
}

@MainActor
@Test
func heroAConfirmCreatesRemindersAndCompletedReadbackProducesReceipt() async {
  let core = MockCore()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  await restoreLegacySuggestionForRecovery(core, model: model)
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 1)
  #expect(model.suggestion == nil)
  #expect(model.confirmedMission?.missionId == "mission-1")
  #expect(model.reminderLinks.map(\.calendarItemIdentifier) == ["reminder-work-1"])
  #expect(model.activeCards.count == 1)

  await model.checkMissionProgress()
  #expect(model.errorMessage == nil)
  #expect(model.receipt?.missionId == "mission-1")
  #expect(model.receipt?.actualModel == "gpt-test-model")
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
func completionCannotPublishDoneWhenCoreOmitsEvidence() async {
  let core = MockCore()
  let reminders = MockReminders()
  await core.returnInvalidCompletionReceipt()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  await restoreLegacySuggestionForRecovery(core, model: model)
  await model.confirmSuggestion()

  await model.checkMissionProgress()

  #expect(model.receipt == nil)
  #expect(!model.dashboardControls.doneVisible)
  #expect(model.confirmedMission?.missionId == "mission-1")
  #expect(model.errorMessage != nil)
  #expect(model.dashboardControls.globalToggleEnabled)
  #expect(model.dashboardControls.settingsEnabled)
}

@MainActor
@Test
func heroASecondOutcomeCannotReuseTheCompletedMission() async {
  let core = MockCore()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)

  await restoreLegacySuggestionForRecovery(core, model: model)
  await model.confirmSuggestion()
  await model.checkMissionProgress()

  await restoreLegacySuggestionForRecovery(
    core, model: model, identifier: testSuggestionTwoId)
  #expect(model.suggestion?.id == testSuggestionTwoId)
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 2)
  #expect(model.confirmedMission?.missionId == "mission-2")
  #expect(model.errorMessage == nil)
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
  await restoreLegacySuggestionForRecovery(core, model: model)
  await model.confirmSuggestion()

  #expect(reminders.executeCount == 0)
  #expect(model.reminderLinks.isEmpty)
  #expect(
    model.errorMessage
      == "Core returned an incomplete or contradictory confirmed Mission."
  )
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

@MainActor
@Test
func choiceReminderRecoveryRejectsMalformedOrRemovedOwnershipMetadata() throws {
  let confirmation = testChoiceConfirmation(
    deliveryBindingId: "binding-1", recipient: "owner-1", deliveryScope: "same-surface")
  let mission = testChoiceConfirmedMission(
    confirmation: confirmation,
    dispatch: [
      ConfirmedReminderDispatch(
        workItemId: confirmation.reminderItems[0].id,
        token: "dispatch-choice-work-1")
    ])
  let expectedDue = DateComponents(
    timeZone: TimeZone(identifier: "Etc/UTC"), year: 1970, month: 1, day: 1,
    hour: 0, minute: 0, second: 0)

  #expect(throws: RemindersClientError.reminderChanged("damaged")) {
    try RemindersClient.rejectPlausibleUnverifiableReminder(
      title: "damaged", notes: "Created by OpenOpen.\nOpenOpen metadata:not-base64",
      dueDateComponents: nil, mission: mission)
  }
  #expect(throws: RemindersClientError.reminderChanged("Review the prepared plan")) {
    try RemindersClient.rejectPlausibleUnverifiableReminder(
      title: "Review the prepared plan", notes: nil,
      dueDateComponents: expectedDue, mission: mission)
  }
  #expect(throws: RemindersClientError.reminderChanged("Unrelated owner reminder")) {
    try RemindersClient.rejectPlausibleUnverifiableReminder(
      title: "Unrelated owner reminder", notes: nil,
      dueDateComponents: expectedDue, mission: mission)
  }
  #expect(throws: RemindersClientError.reminderChanged("Review the prepared plan")) {
    try RemindersClient.rejectPlausibleOtherMissionReminder(
      title: "Review the prepared plan", dueDateComponents: expectedDue,
      markerWorkItemId: confirmation.reminderItems[0].id,
      markerDispatchToken: "dispatch-choice-work-1", mission: mission)
  }
  #expect(throws: Never.self) {
    try RemindersClient.rejectPlausibleOtherMissionReminder(
      title: "Historical unrelated item",
      dueDateComponents: DateComponents(
        timeZone: TimeZone(identifier: "Etc/UTC"), year: 2030, month: 1, day: 1,
        hour: 12, minute: 0, second: 0),
      markerWorkItemId: "other-work", markerDispatchToken: "other-token", mission: mission)
  }
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
    #"{"activeCards":[{"id":"mission-1","state":"working","title":"Plan the day"}],"channelFailureIncidents":[],"channelRouteSet":null,"confirmedMission":{"missionId":"mission-1","reminderAuthorization":{"approvalDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","approvalId":"approval-reminders-1","listId":"openopen.default-reminders","missionId":"mission-1","payloadSha256":"188605fc48e5a3bc42efee3820582cb016a84685869bfbb6688daf79b055fab0","target":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"},"writeDisposition":"recoverOnly"},"reminderDispatch":[{"token":"dispatch-work-1","workItemId":"work-1"}],"reminderLinks":[],"title":"Plan the day","workItems":[{"id":"work-1","title":"Pick one priority"}]},"microphone":{"available":false,"reason":"Microphone unavailable until Voice setup"},"needsYou":null,"receipt":null,"runtime":{"enabled":true,"revision":1,"updatedAtMs":2},"suggestion":null}"#
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

@Test
func acknowledgedIncidentDecodesFromTheExactHostDashboardShape() throws {
  let data = Data(
    #"{"activeCards":[],"channelFailureIncidents":[{"acknowledgement":{"acknowledgedAtMs":11,"auditAnchor":{"entryHash":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","sequence":3,"signatureHex":"33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333"},"runtimeRevision":5},"channel":"iMessage","dispatchStateHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","failureClass":"modelResultUnavailable","incidentAuditAnchor":{"entryHash":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","sequence":2,"signatureHex":"22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222"},"incidentId":"channel-failure-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","occurredAtMs":10,"runtimeRevision":5,"sourceAuditAnchor":{"entryHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","sequence":1,"signatureHex":"11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111"}}],"channelRouteSet":null,"confirmedMission":null,"microphone":{"available":false,"reason":"Microphone unavailable until Voice setup"},"needsYou":null,"receipt":null,"runtime":{"enabled":true,"revision":5,"updatedAtMs":12},"suggestion":null}"#
      .utf8
  )
  let dashboard = try JSONDecoder().decode(DashboardState.self, from: data)
  #expect(try dashboard.validated() == dashboard)
  #expect(dashboard.channelFailureIncidents.count == 1)
  #expect(dashboard.channelFailureIncidents[0].acknowledgement?.runtimeRevision == 5)
  #expect(dashboard.channelFailureIncidents[0].acknowledgement?.auditAnchor.sequence == 3)
}

@Test
func heroASecondMissionHostShapeSuppressesTheHistoricalReceipt() throws {
  let payloadHash = ReminderWriteAuthorization.payloadSha256(
    missionId: "mission-2",
    target: ReminderTarget(
      sourceIdentifier: "source-1", calendarIdentifier: "calendar-1"),
    workItems: [MissionWorkItem(id: "work-2", title: "Plan tomorrow")]
  )
  let data = Data(
    #"{"activeCards":[{"id":"mission-2","state":"working","title":"Plan tomorrow"}],"channelFailureIncidents":[],"channelRouteSet":null,"confirmedMission":{"missionId":"mission-2","reminderAuthorization":{"approvalDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","approvalId":"approval-reminders-2","listId":"openopen.default-reminders","missionId":"mission-2","payloadSha256":"\#(payloadHash)","target":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"},"writeDisposition":"recoverOnly"},"reminderDispatch":[{"token":"dispatch-work-2","workItemId":"work-2"}],"reminderLinks":[],"title":"Plan tomorrow","workItems":[{"id":"work-2","title":"Plan tomorrow"}]},"microphone":{"available":false,"reason":"Microphone unavailable until Voice setup"},"needsYou":null,"receipt":null,"runtime":{"enabled":true,"revision":2,"updatedAtMs":3},"suggestion":null}"#
      .utf8
  )
  let dashboard = try JSONDecoder().decode(DashboardState.self, from: data)
  #expect(try dashboard.validated() == dashboard)
  #expect(dashboard.confirmedMission?.missionId == "mission-2")
  #expect(dashboard.receipt == nil)

  var contradictoryShape = try #require(
    JSONSerialization.jsonObject(with: data) as? [String: Any]
  )
  contradictoryShape["receipt"] = [
    "id": "receipt-old",
    "missionId": "mission-old",
    "summary": "Completed the previous Mission",
    "actualModel": "gpt-test-model",
    "evidenceIds": ["evidence-old"],
    "outputHashes": [],
    "completedAtMs": 1,
  ]
  let contradictory = try JSONDecoder().decode(
    DashboardState.self,
    from: JSONSerialization.data(withJSONObject: contradictoryShape)
  )
  #expect(throws: CoreClientError.self) {
    try contradictory.validated()
  }
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
    actualModel: "gpt-test-model",
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
  await restoreLegacySuggestionForRecovery(core, model: model)
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
  await restoreLegacySuggestionForRecovery(core, model: model)
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 1)
  #expect(model.confirmedMission?.missionId == "mission-1")
  #expect(model.suggestion?.id == testSuggestionOneId)
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
  await restoreLegacySuggestionForRecovery(core, model: model)
  await model.confirmSuggestion()

  reminders.mode = .complete
  await model.confirmSuggestion()

  #expect(await core.confirmationCount == 1)
  #expect(await core.dispatchBeginCount == 2)
  #expect(reminders.executeCount == 1)
  #expect(reminders.recoverCount == 1)
  #expect(model.reminderLinks.isEmpty)
  #expect(model.errorMessage?.contains("No Reminder exists") == true)
}

@MainActor
@Test
func strandedReminderMissionCanBeCancelledWithoutASecondExternalWrite() async {
  let core = MockCore()
  let reminders = MockReminders(mode: .failBeforeCommit)
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  await restoreLegacySuggestionForRecovery(core, model: model)
  await model.confirmSuggestion()

  #expect(model.activeCards.map(\.id) == ["mission-1"])
  #expect(model.dashboardControls.missionCancellationEnabled)
  await model.cancelMission(identifier: "mission-1")

  #expect(await core.missionCancellationCount == 1)
  #expect(await core.dispatchedMissions["mission-1"] != nil)
  #expect(reminders.executeCount == 1)
  #expect(model.confirmedMission == nil)
  #expect(model.activeCards.isEmpty)
  #expect(model.receipt == nil)
  #expect(model.dashboardControls.outcomeInputEnabled)
  #expect(model.errorMessage?.contains("did not retry or remove any Reminders") == true)
}

@MainActor
@Test
func protectedOffNeverEnablesOrCallsMissionCancellation() async {
  let core = MockCore()
  let mission = testConfirmedMission()
  await core.restoreFromDashboard(mission: mission, receipt: nil)
  let model = AppModel(core: core, broker: MockBroker()) {}

  await model.refreshDashboard()

  #expect(model.runtimeDisplayState == .off)
  #expect(model.activeCards.map(\.id) == [mission.missionId])
  #expect(!model.storeControlEnabled)
  #expect(!model.dashboardControls.missionCancellationEnabled)
  await model.cancelMission(identifier: mission.missionId)
  #expect(await core.missionCancellationCount == 0)
  #expect(model.activeCards.map(\.id) == [mission.missionId])
}

@MainActor
@Test
func awaitingAccountKeepsProtectedMissionCancellationAvailableWithoutModelWork() async {
  let core = MockCore(loginCompleted: false)
  let mission = testConfirmedMission()
  await core.restoreFromDashboard(mission: mission, receipt: nil)
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream
  )

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(!model.modelEntryEnabled)
  #expect(model.storeControlEnabled)
  #expect(model.dashboardControls.missionCancellationEnabled)
  await model.cancelMission(identifier: mission.missionId)

  #expect(await core.missionCancellationCount == 1)
  #expect(model.activeCards.isEmpty)
  #expect(model.confirmedMission == nil)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.runtimeDisplayState == .turningOn)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func awaitingAccountPollsRouteBoundMissionEventWithoutModelWork() async {
  let core = MockCore(loginCompleted: false)
  let mission = testConfirmedMission()
  let routes = testChannelRouteSet(
    channel: .iMessage,
    conversationId: "42",
    ownerSenderId: "owner@example.invalid"
  )
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routes)
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    )
  )
  let event = ChannelMissionEvent(
    eventId: "channel-event-awaiting-account",
    missionId: mission.missionId,
    missionRevision: 12,
    missionAnchorHash: String(repeating: "a", count: 64),
    routeId: routes.primaryRouteId,
    routeSetRevision: routes.revision,
    messageClass: .missionParticipation,
    channel: .iMessage,
    sourceMessageId: "message-awaiting-account",
    contentSha256: String(repeating: "b", count: 64),
    recordedAtMs: 20
  )
  await core.queueChannelMissionEvent(event)
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    channelPollInterval: .milliseconds(1)
  )

  await model.updateEnabled(true)
  for _ in 0..<200 where model.latestChannelMissionEvent == nil {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(!model.modelEntryEnabled)
  #expect(model.channelEffectEntryEnabled)
  #expect(model.iMessageStatus == "connected")
  #expect(model.latestChannelMissionEvent == event)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func awaitingAccountDefersUnboundChannelInputWithoutModelWork() async {
  let core = MockCore(loginCompleted: false)
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    )
  )
  await core.queueChannelPollResponses([
    ChannelPollResponse(
      connectionStatus: "connected",
      eventStatus: "deferred",
      suggestion: nil
    )
  ])
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    channelPollInterval: .milliseconds(1)
  )

  await model.updateEnabled(true)
  for _ in 0..<200 where await core.channelPollCount == 0 {
    try? await Task.sleep(for: .milliseconds(2))
  }

  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.iMessageStatus == "connected")
  #expect(await core.channelPollCount >= 1)
  #expect(await core.proposalCount == 0)
  #expect(model.suggestion == nil)
  #expect(await core.channelSends.isEmpty)
  await model.updateEnabled(false)
}

@MainActor
@Test
func awaitingAccountSendsExactMissionChannelEffectsWithoutModelWork() async {
  let core = MockCore(loginCompleted: false)
  let mission = testConfirmedMission()
  let routes = testChannelRouteSet(
    channel: .iMessage,
    conversationId: "42",
    ownerSenderId: "owner@example.invalid"
  )
  await core.restoreFromDashboard(mission: mission, receipt: nil, channelRouteSet: routes)
  await core.setChannelPairing(
    ChannelPairing(
      channel: .iMessage,
      ownerSenderId: "owner@example.invalid",
      conversationId: "42",
      pairedAtMs: 1
    )
  )
  let events = CoreTerminationEmitter()
  let model = AppModel(
    core: core,
    broker: MockBroker(),
    registerLoginItem: {},
    coreTerminationEvents: events.stream,
    channelPollInterval: .milliseconds(1)
  )

  await model.updateEnabled(true)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(model.iMessageStatus == "connected")
  #expect(model.selectedRouteAllowsProgress)
  model.channelMessageDraft = "Exact protected progress."
  await model.sendChannelProgress()

  let needsYou = MissionNeedsYou(
    missionId: mission.missionId,
    title: mission.title,
    prompt: "Choose the exact bounded option.",
    createdAtMs: 2
  )
  await core.restoreFromDashboard(
    mission: nil,
    receipt: nil,
    channelRouteSet: routes,
    needsYou: needsYou
  )
  await model.refreshDashboard()
  #expect(model.selectedRouteAllowsNeedYou)
  await model.sendChannelNeedYou()

  let receipt = MissionReceipt(
    id: "receipt-awaiting-account",
    missionId: mission.missionId,
    summary: "Completed Plan the day",
    actualModel: "gpt-test-model",
    evidenceIds: ["evidence-work-1"],
    outputHashes: [],
    completedAtMs: 20
  )
  await core.restoreFromDashboard(
    mission: nil,
    receipt: receipt,
    channelRouteSet: routes
  )
  await model.refreshDashboard()
  #expect(model.selectedRouteAllowsReceipt)
  await model.sendChannelReceipt()

  let sends = await core.channelSends
  #expect(sends.map(\.kind) == [.progress, .needYou, .receipt])
  #expect(await core.proposalCount == 0)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  await model.updateEnabled(false)
}

@MainActor
@Test
func nonConnectedProviderStatusRevokesOnlyEffectReadinessUntilExactReconnect() async throws {
  let core = MockCore()
  let mission = testConfirmedMission()
  await core.restoreFromDashboard(
    mission: mission,
    receipt: nil,
    channelRouteSet: testChannelRouteSet(missionId: mission.missionId)
  )
  let (model, _) = try await makePairedRecoveryModel(
    core: core,
    channelPollInterval: .milliseconds(2)
  )
  await model.updateEnabled(true)
  #expect(model.discordStatus == "connected")

  for status in ["connecting", "reconnecting", "disconnected", "faulted"] {
    await core.setChannelPollConnectionStatus(status)
    for _ in 0..<200 where model.discordStatus != status {
      try? await Task.sleep(for: .milliseconds(2))
    }
    #expect(model.discordStatus == status)
    #expect(!model.selectedRouteAllowsProgress)
    #expect(!model.selectedRouteAllowsNeedYou)
    #expect(!model.selectedRouteAllowsReceipt)
    let attempts = await core.channelSendAttemptCount
    model.channelMessageDraft = "This effect must remain local while Discord is (status)."
    await model.sendChannelProgress()
    #expect(await core.channelSendAttemptCount == attempts)
    #expect(await core.channelSends.isEmpty)
    #expect(model.dashboardControls.globalToggleEnabled)
    #expect(model.dashboardControls.settingsEnabled)
  }

  await core.setChannelPollConnectionStatus("connected")
  for _ in 0..<200 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.discordStatus == "connected")
  #expect(model.selectedRouteAllowsProgress)
  model.channelMessageDraft = "Exact approved progress after reconnect."
  await model.sendChannelProgress()
  #expect((await core.channelSends).map(\.kind) == [.progress])
  await model.updateEnabled(false)
}

@MainActor
@Test
func awaitingAccountReconnectsBothDurableListenersWithoutModelOrOutboundWork() async throws {
  let core = MockCore(loginCompleted: false)
  let (model, _) = try await makePairedRecoveryModel(
    core: core,
    channelPollInterval: .milliseconds(2)
  )
  await model.updateEnabled(true)
  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(!model.modelEntryEnabled)
  #expect(model.iMessageStatus == "connected")
  #expect(model.discordStatus == "connected")

  await core.failNextChannelPoll(
    .iMessage,
    with: .contractViolation("Messages permission was revoked.")
  )
  for _ in 0..<200 where model.iMessageStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.iMessageStatus == "faulted")
  #expect(model.iMessageConnectionActionEnabled)
  #expect(model.discordStatus == "connected")
  await model.connectIMessage()
  #expect(model.iMessageStatus == "connected")

  await core.failNextChannelPoll(
    .discord,
    with: .contractViolation("Discord transport disconnected.")
  )
  for _ in 0..<200 where model.discordStatus != "faulted" {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.discordStatus == "faulted")
  #expect(model.discordConnectionActionEnabled)
  #expect(model.iMessageStatus == "connected")
  await model.connectDiscord()
  for _ in 0..<200 where model.discordStatus != "connected" {
    try? await Task.sleep(for: .milliseconds(2))
  }
  #expect(model.discordStatus == "connected")

  #expect(model.runtimeRecoveryState == .awaitingAccount)
  #expect(!model.modelEntryEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
  let pollAllowances = await core.channelPollModelWorkAllowances
  #expect(!pollAllowances.isEmpty)
  #expect(pollAllowances.allSatisfy { !$0 })
  await model.updateEnabled(false)
}

@MainActor
@Test
func pausedCancellationProofDoesNotPublishFalseOn() async throws {
  let core = MockCore()
  let mission = testConfirmedMission()
  await core.restoreFromDashboard(
    mission: mission,
    receipt: nil,
    channelRouteSet: testChannelRouteSet(missionId: mission.missionId)
  )
  await core.queueDiscordStatusResponses(["connecting", "connecting", "faulted"])
  await core.failNextChannelPoll()
  let (model, _) = try await makePairedRecoveryModel(core: core)

  await model.updateEnabled(true)

  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(model.storeControlEnabled)
  #expect(!model.channelEffectEntryEnabled)
  #expect(model.dashboardControls.missionCancellationEnabled)
  #expect(!model.selectedRouteAllowsProgress)
  model.channelMessageDraft = "This paused provider effect must stay blocked."
  await model.sendChannelProgress()
  #expect(await core.channelSends.isEmpty)
  await model.cancelMission(identifier: mission.missionId)

  #expect(await core.missionCancellationCount == 1)
  #expect(model.activeCards.isEmpty)
  #expect(model.runtimeRecoveryState == .paused)
  #expect(model.runtimeDisplayState == .unknown)
  #expect(!model.modelEntryEnabled)
  #expect(await core.proposalCount == 0)
  #expect(await core.channelSends.isEmpty)
}

@MainActor
@Test
func missingAccountOrRequiredModelStillCompletesEvidenceLocallyWithoutOutbound() async {
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
  let cores = [
    MockCore(loginCompleted: false),
    MockCore(
      modelCatalog: [
        GptModel(
          id: "gpt-5.6-terra", displayName: "GPT-5.6 Terra",
          supportedReasoningEfforts: ["high"])
      ]),
  ]

  for core in cores {
    await core.clearPersistedModelSelection()
    await core.restoreFromDashboard(
      mission: mission,
      receipt: nil,
      channelRouteSet: testChannelRouteSet(missionId: mission.missionId)
    )
    let reminders = MockReminders()
    let events = CoreTerminationEmitter()
    let model = AppModel(
      core: core,
      broker: MockBroker(),
      reminders: reminders,
      registerLoginItem: {},
      coreTerminationEvents: events.stream
    )

    await model.updateEnabled(true)

    #expect(model.runtimeRecoveryState == .awaitingAccount)
    #expect(!model.modelEntryEnabled)
    #expect(model.storeControlEnabled)
    #expect(model.dashboardControls.missionProgressEnabled)
    await model.checkMissionProgress()

    let approvals = await core.receiptReturnApprovals
    let routeIds = await core.receiptReturnRouteIds
    #expect(approvals.count == 1)
    #expect(approvals[0] == nil)
    #expect(routeIds.count == 1)
    #expect(routeIds[0] == nil)
    #expect(model.receipt?.missionId == mission.missionId)
    #expect(model.activeCards.isEmpty)
    #expect(model.runtimeRecoveryState == .awaitingAccount)
    #expect(model.runtimeDisplayState == .turningOn)
    #expect(await core.proposalCount == 0)
    #expect(await core.channelSends.isEmpty)
  }
}

@MainActor
@Test
func completedFocusImmediatelyHandsOffToTheNextTwoActiveMissions() async {
  let link = ReminderLink(
    missionId: "mission-1", workItemId: "work-1",
    sourceIdentifier: "source-1", calendarIdentifier: "calendar-1",
    calendarItemIdentifier: "reminder-work-1", dispatchToken: "dispatch-work-1",
    title: "Pick one priority"
  )
  let completed = testConfirmedMission(
    writeDisposition: .recoverOnly,
    reminderDispatch: [
      ConfirmedReminderDispatch(workItemId: "work-1", token: "dispatch-work-1")
    ],
    reminderLinks: [link]
  )
  let next = testConfirmedMission(missionId: "mission-2", title: "Prepare the backup")
  let core = MockCore()
  await core.restoreFromDashboard(mission: completed, receipt: nil)
  await core.returnDashboardAfterNextCompletion(
    DashboardState(
      activeCards: [
        ActiveOutcomeCard(id: next.missionId, title: next.title, state: "Active"),
        ActiveOutcomeCard(id: "mission-3", title: "Rehearse once", state: "Proposed"),
      ],
      microphone: MicrophoneState(available: false, reason: "Unavailable"),
      runtime: RuntimeControl(enabled: true, revision: 1, updatedAtMs: 1),
      suggestion: nil,
      confirmedMission: next
    ))
  let model = AppModel(core: core, broker: MockBroker(), reminders: MockReminders()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()

  await model.checkMissionProgress()

  #expect(model.activeCards.map(\.id) == ["mission-2", "mission-3"])
  #expect(model.confirmedMission?.missionId == "mission-2")
  #expect(model.receipt == nil)
  #expect(!model.dashboardControls.doneVisible)
  #expect(model.dashboardControls.missionProgressEnabled)
}

@MainActor
@Test
func completedFocusImmediatelyHandsOffToActiveNeedsYou() async {
  let link = ReminderLink(
    missionId: "mission-1", workItemId: "work-1",
    sourceIdentifier: "source-1", calendarIdentifier: "calendar-1",
    calendarItemIdentifier: "reminder-work-1", dispatchToken: "dispatch-work-1",
    title: "Pick one priority"
  )
  let completed = testConfirmedMission(
    writeDisposition: .recoverOnly,
    reminderDispatch: [
      ConfirmedReminderDispatch(workItemId: "work-1", token: "dispatch-work-1")
    ],
    reminderLinks: [link]
  )
  let next = MissionNeedsYou(
    missionId: "mission-2",
    title: "Choose the backup",
    prompt: "Choose the exact approved backup before OpenOpen continues.",
    createdAtMs: 12
  )
  let core = MockCore()
  await core.restoreFromDashboard(mission: completed, receipt: nil)
  await core.returnDashboardAfterNextCompletion(
    DashboardState(
      activeCards: [
        ActiveOutcomeCard(id: next.missionId, title: next.title, state: "Need you"),
        ActiveOutcomeCard(id: "mission-3", title: "Rehearse once", state: "Active"),
      ],
      microphone: MicrophoneState(available: false, reason: "Unavailable"),
      runtime: RuntimeControl(enabled: true, revision: 1, updatedAtMs: 1),
      suggestion: nil,
      needsYou: next
    ))
  let model = AppModel(core: core, broker: MockBroker(), reminders: MockReminders()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()

  await model.checkMissionProgress()

  #expect(model.activeCards.map(\.id) == ["mission-2", "mission-3"])
  #expect(model.needsYou?.missionId == "mission-2")
  #expect(model.confirmedMission == nil)
  #expect(model.receipt == nil)
  #expect(!model.dashboardControls.doneVisible)
  #expect(model.dashboardControls.missionCancellationEnabled)
}

@MainActor
@Test
func missionCancellationResponseLossReconcilesFromTheDurableDashboard() async {
  let core = MockCore()
  let mission = testConfirmedMission(
    writeDisposition: .recoverOnly,
    reminderDispatch: [
      ConfirmedReminderDispatch(workItemId: "work-1", token: "dispatch-work-1")
    ]
  )
  await core.restoreFromDashboard(mission: mission, receipt: nil)
  await core.loseNextMissionCancellationResponseAfterCommit()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()

  await model.cancelMission(identifier: mission.missionId)

  #expect(await core.missionCancellationCount == 1)
  #expect(model.activeCards.isEmpty)
  #expect(model.confirmedMission == nil)
  #expect(model.receipt == nil)
  #expect(model.dashboardControls.outcomeInputEnabled)
  #expect(model.errorMessage?.contains("no longer active") == true)
}

@MainActor
@Test
func missionCompletionWinsACancellationRaceWithoutFalseCancelledState() async {
  let core = MockCore()
  let mission = testConfirmedMission(
    writeDisposition: .recoverOnly,
    reminderDispatch: [
      ConfirmedReminderDispatch(workItemId: "work-1", token: "dispatch-work-1")
    ]
  )
  await core.restoreFromDashboard(mission: mission, receipt: nil)
  await core.completeMissionBeforeNextCancellation()
  let model = AppModel(core: core, broker: MockBroker()) {}
  await model.updateEnabled(true)
  await model.refreshDashboard()

  await model.cancelMission(identifier: mission.missionId)

  #expect(await core.missionCancellationCount == 1)
  #expect(model.activeCards.isEmpty)
  #expect(model.confirmedMission == nil)
  #expect(model.receipt?.missionId == mission.missionId)
  #expect(model.dashboardControls.doneVisible)
  #expect(model.errorMessage?.contains("already finished with verified Evidence") == true)
}

@MainActor
@Test
func heroADispatchResponseLossFailsClosedWithoutAnyExternalWrite() async {
  let core = MockCore()
  await core.loseNextReminderDispatchResponse()
  let reminders = MockReminders()
  let model = AppModel(core: core, broker: MockBroker(), reminders: reminders) {}
  await model.updateEnabled(true)
  await restoreLegacySuggestionForRecovery(core, model: model)
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
  await restoreLegacySuggestionForRecovery(core, model: model)
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
func receiptCleanupNeverLaunchesAQuiescedCore() async throws {
  let launchAttempts = LockIsolated(0)
  let client = CoreProcessClient(
    executableResolver: {
      launchAttempts.withLock { $0 += 1 }
      throw CoreClientError.processUnavailable
    },
    staticCodeValidator: { _ in },
    runningCodeValidator: { _ in },
    masterKeyLoader: { Data(repeating: 7, count: 32) }
  )
  do {
    _ = try await client.cleanupChoiceMarkdownReceipt()
    Issue.record("post-Off receipt cleanup must not launch Core")
  } catch {}
  #expect(launchAttempts.value == 0)
}

@Test
func runningCoreAuthenticationFailurePrecedesTheMasterKeyLoad() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("fake-core")
  try compileTestCExecutable(
    """
    #include <unistd.h>

    int main(void) {
      for (;;) pause();
    }
    """,
    output: executable
  )
  let masterLoads = LockIsolated(0)
  let validatedToken = LockIsolated<String?>(nil)
  let terminatedToken = LockIsolated<String?>(nil)
  let client = CoreProcessClient(
    executableResolver: { executable },
    staticCodeValidator: { _ in },
    runningCodeValidator: { token in
      validatedToken.withLock { $0 = token }
      throw CodeSigningIdentityError.invalidIdentity
    },
    masterKeyLoader: {
      masterLoads.withLock { $0 += 1 }
      return Data(repeating: 7, count: 32)
    },
    exactProcessTerminator: { token in
      terminatedToken.withLock { $0 = token }
      return terminateTestProcess(auditTokenHex: token)
    }
  )
  do {
    _ = try await client.runtime()
    Issue.record("running Core authentication must fail before bootstrap")
  } catch {}
  #expect(masterLoads.value == 0)
  #expect(validatedToken.value?.count == 64)
  #expect(terminatedToken.value == validatedToken.value)
  let failedChildPID = try #require(
    validatedToken.value.flatMap(testProcessIdentifier(auditTokenHex:)))
  errno = 0
  #expect(Darwin.kill(failedChildPID, 0) == -1)
  #expect(errno == ESRCH)
  client.shutdown()
}

@Test
func acceptedDelayedFailedLaunchTerminationBlocksAReplacementUntilExactExit() async throws {
  let root = try TemporaryDirectory()
  let failedExecutable = root.url.appendingPathComponent("delayed-failed-launch-core")
  let replacementExecutable = root.url.appendingPathComponent("replacement-core")
  try compileTestCExecutable(
    """
    #include <unistd.h>

    int main(void) {
      for (;;) pause();
    }
    """,
    output: failedExecutable
  )
  try compileTestCExecutable(
    """
    #include <string.h>
    #include <unistd.h>

    int main(void) {
      unsigned char bootstrap[55];
      size_t offset = 0;
      while (offset < sizeof(bootstrap)) {
        ssize_t length = read(STDIN_FILENO, bootstrap + offset, sizeof(bootstrap) - offset);
        if (length <= 0) return 2;
        offset += (size_t)length;
      }
      char byte = 0;
      do {
        if (read(STDIN_FILENO, &byte, 1) != 1) return 3;
      } while (byte != '\\n');
      const char *response = "{\\"jsonrpc\\":\\"2.0\\",\\"id\\":1,\\"result\\":{\\"enabled\\":false,\\"revision\\":0,\\"updatedAtMs\\":0}}\\n";
      if (write(STDOUT_FILENO, response, strlen(response)) != (ssize_t)strlen(response)) return 4;
      for (;;) pause();
    }
    """,
    output: replacementExecutable
  )
  let resolutions = LockIsolated(0)
  let validations = LockIsolated(0)
  let terminationRequests = LockIsolated(0)
  let releaseFirstTermination = DispatchSemaphore(value: 0)
  let client = CoreProcessClient(
    executableResolver: {
      var resolution = 0
      resolutions.withLock {
        $0 += 1
        resolution = $0
      }
      return resolution == 1 ? failedExecutable : replacementExecutable
    },
    staticCodeValidator: { _ in },
    runningCodeValidator: { _ in
      var reject = false
      validations.withLock {
        $0 += 1
        reject = $0 == 1
      }
      if reject { throw CodeSigningIdentityError.invalidIdentity }
    },
    masterKeyLoader: { Data(repeating: 7, count: 32) },
    exactProcessTerminator: { token in
      var requestNumber = 0
      terminationRequests.withLock {
        $0 += 1
        requestNumber = $0
      }
      guard requestNumber == 1 else {
        return terminateTestProcess(auditTokenHex: token)
      }
      DispatchQueue.global(qos: .utility).async {
        releaseFirstTermination.wait()
        _ = terminateTestProcess(auditTokenHex: token)
      }
      return true
    }
  )
  defer { client.shutdown() }

  let first = Task { try await client.runtime() }
  for _ in 0..<200 where terminationRequests.value == 0 {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(terminationRequests.value == 1)
  let second = Task { try await client.runtime() }
  try? await Task.sleep(for: .milliseconds(100))
  #expect(validations.value == 1)
  #expect(resolutions.value == 1)
  releaseFirstTermination.signal()

  switch await first.result {
  case .success:
    Issue.record("a rejected first Core must not complete")
  case .failure(let error):
    #expect(error as? CoreClientError == .processUnavailable)
  }
  let replacement = try await second.value
  #expect(replacement == RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0))
  #expect(validations.value == 2)
  #expect(resolutions.value == 2)
}

@Test
func rejectedFailedLaunchTerminationQuarantinesTheChildAndForbidsAReplacement() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("quarantined-failed-launch-core")
  let source = """
    #include <unistd.h>

    int main(void) {
      for (;;) pause();
    }
    """
  try compileTestCExecutable(source, output: executable)
  let validations = LockIsolated(0)
  let terminationAttempts = LockIsolated(0)
  let retainedToken = LockIsolated<String?>(nil)
  let client = CoreProcessClient(
    executableResolver: { executable },
    staticCodeValidator: { _ in },
    runningCodeValidator: { _ in
      validations.withLock { $0 += 1 }
      throw CodeSigningIdentityError.invalidIdentity
    },
    masterKeyLoader: { Data(repeating: 7, count: 32) },
    exactProcessTerminator: { token in
      retainedToken.withLock { $0 = token }
      terminationAttempts.withLock { $0 += 1 }
      return false
    }
  )
  defer {
    if let token = retainedToken.value {
      _ = terminateTestProcess(auditTokenHex: token)
    }
  }

  do {
    _ = try await client.runtime()
    Issue.record("a running-Code authentication failure must reject the first Core")
  } catch let error as CoreClientError {
    #expect(error == .processUnavailable)
  }
  do {
    _ = try await client.runtime()
    Issue.record("a quarantined failed Core must forbid a replacement launch")
  } catch let error as CoreClientError {
    #expect(error == .processUnavailable)
  }

  #expect(validations.value == 1)
  #expect(terminationAttempts.value == 1)
  #expect(client.shutdown() == false)
  #expect(terminationAttempts.value == 2)
  let failedChildPID = try #require(
    retainedToken.value.flatMap(testProcessIdentifier(auditTokenHex:)))
  #expect(terminateTestProcess(auditTokenHex: try #require(retainedToken.value)))
  for _ in 0..<200 {
    errno = 0
    if Darwin.kill(failedChildPID, 0) == -1, errno == ESRCH { break }
    try? await Task.sleep(for: .milliseconds(10))
  }
  errno = 0
  #expect(Darwin.kill(failedChildPID, 0) == -1)
  #expect(errno == ESRCH)
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
  do {
    _ = try await client.runtime()
    Issue.record("first fake Core invocation should terminate")
  } catch {}
  let replacement = try await client.runtime()
  #expect(replacement == RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0))
  client.shutdown()
}

@Test
func cleanupStopNeverLaunchesAReplacementCoreWhenNoGenerationIsRunning() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("existing-only-core")
  let countFile = root.url.appendingPathComponent("existing-only-core.count")
  let script = """
    #!/bin/sh
    count_file="$0.count"
    count=0
    if [ -f "$count_file" ]; then count=$(/bin/cat "$count_file"); fi
    count=$((count + 1))
    printf '%s' "$count" > "$count_file"
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

  _ = try await client.runtime()
  client.shutdown()
  do {
    _ = try await client.stopChannelIfRunning(.discord)
    Issue.record("cleanup-only channel stop must not launch a replacement Core")
  } catch let error as CoreClientError {
    #expect(error == .processUnavailable)
  }
  try? await Task.sleep(for: .milliseconds(100))
  #expect(try String(contentsOf: countFile, encoding: .utf8) == "1")
}

@Test
func boundedRecoveryFenceRejectsAnImplicitCoreReplacementBetweenRPCs() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("generation-fenced-core")
  let countFile = root.url.appendingPathComponent("generation-fenced-core.count")
  let script = """
    #!/bin/sh
    count_file="$0.count"
    count=0
    if [ -f "$count_file" ]; then count=$(/bin/cat "$count_file"); fi
    count=$((count + 1))
    printf '%s' "$count" > "$count_file"
    /bin/dd bs=55 count=1 of=/dev/null 2>/dev/null
    IFS= read -r request
    printf '{"jsonrpc":"2.0","id":%s,"result":{"enabled":false,"revision":0,"updatedAtMs":0}}\n' "$count"
    /bin/sleep 0.2
    exit 0
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
  let eventTask = Task { () -> CoreTerminationEvent? in
    for await event in client.terminationEvents() { return event }
    return nil
  }

  let fence = try await client.beginCoreGenerationFence()
  let first = try await client.runtime()
  #expect(first == RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0))
  #expect(await eventTask.value?.generation == fence.generation)

  do {
    _ = try await client.runtime()
    Issue.record("a fenced recovery RPC must not launch a replacement generation")
  } catch let error as CoreClientError {
    #expect(error == .processUnavailable)
  }
  #expect(try String(contentsOf: countFile, encoding: .utf8) == "1")
  #expect(await client.closeCoreGenerationFence(fence) == false)

  let replacement = try await client.runtime()
  #expect(replacement == RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0))
  #expect(try String(contentsOf: countFile, encoding: .utf8) == "2")
}

@Test
func cancellationAfterInstallButBeforeFrameWriteRemovesTheUnreachableTombstone() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("cancel-before-write-core")
  let countFile = root.url.appendingPathComponent("cancel-before-write-core.count")
  let script = """
    #!/bin/sh
    count_file="$0.count"
    count=0
    if [ -f "$count_file" ]; then count=$(/bin/cat "$count_file"); fi
    count=$((count + 1))
    printf '%s' "$count" > "$count_file"
    /bin/dd bs=55 count=1 of=/dev/null 2>/dev/null
    IFS= read -r request
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"enabled":false,"revision":0,"updatedAtMs":0}}'
    /bin/cat >/dev/null
    """
  try Data(script.utf8).write(to: executable)
  try FileManager.default.setAttributes(
    [.posixPermissions: 0o700],
    ofItemAtPath: executable.path
  )
  let installed = LockIsolated(false)
  let releaseWrite = DispatchSemaphore(value: 0)
  let shouldBlock = LockIsolated(true)
  let client = CoreProcessClient(
    executableResolver: { executable },
    staticCodeValidator: { _ in },
    runningCodeValidator: { _ in },
    masterKeyLoader: { Data(repeating: 7, count: 32) },
    requestInstalledBeforeWriteHook: {
      var block = false
      shouldBlock.withLock {
        block = $0
        $0 = false
      }
      if block {
        installed.withLock { $0 = true }
        releaseWrite.wait()
      }
    }
  )
  defer { client.shutdown() }

  let first = Task { try await client.runtime() }
  for _ in 0..<500 where !installed.value {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(installed.value)
  first.cancel()
  releaseWrite.signal()
  switch await first.result {
  case .success:
    Issue.record("the request cancelled before its frame write must not complete")
  case .failure(let error):
    #expect(error as? CoreClientError == .requestCancelled)
  }

  let second = try await client.runtime()
  #expect(second == RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0))
  #expect(try String(contentsOf: countFile, encoding: .utf8) == "1")
}

@Test
func stdoutEOFRevokesAnInstalledRequestBeforeItsFrameCanBeWritten() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("eof-before-write-core")
  let requestFile = root.url.appendingPathComponent("eof-before-write-core.request")
  let script = """
    #!/bin/sh
    /bin/dd bs=55 count=1 of=/dev/null 2>/dev/null
    exec 1>&-
    if IFS= read -r request; then
      printf '%s' "$request" > "$0.request"
    fi
    /bin/sleep 5
    """
  try Data(script.utf8).write(to: executable)
  try FileManager.default.setAttributes(
    [.posixPermissions: 0o700],
    ofItemAtPath: executable.path
  )
  let requestInstalled = LockIsolated(false)
  let allowWrite = DispatchSemaphore(value: 0)
  let inputRevoked = LockIsolated(false)
  let allowEOFCompletion = DispatchSemaphore(value: 0)
  let client = CoreProcessClient(
    executableResolver: { executable },
    staticCodeValidator: { _ in },
    runningCodeValidator: { _ in },
    masterKeyLoader: { Data(repeating: 7, count: 32) },
    requestInstalledBeforeWriteHook: {
      requestInstalled.withLock { $0 = true }
      allowWrite.wait()
    },
    inputRevokedHook: {
      inputRevoked.withLock { $0 = true }
      allowEOFCompletion.wait()
    }
  )
  defer {
    allowWrite.signal()
    allowEOFCompletion.signal()
    client.shutdown()
  }

  let request = Task { try await client.runtime() }
  for _ in 0..<500 where !requestInstalled.value {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(requestInstalled.value)
  for _ in 0..<500 where !inputRevoked.value {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(inputRevoked.value)
  allowWrite.signal()

  switch await request.result {
  case .success:
    Issue.record("a request whose input authority was revoked at EOF must not succeed")
  case .failure(let error):
    #expect(error as? CoreClientError == .processUnavailable)
  }
  #expect(!FileManager.default.fileExists(atPath: requestFile.path))
  allowEOFCompletion.signal()
}

@Test
func cancellingOneRpcDoesNotTerminateTheSharedCoreOrRejectItsLateResponse() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("cancel-safe-core")
  let countFile = root.url.appendingPathComponent("cancel-safe-core.count")
  let firstRequestFile = root.url.appendingPathComponent("cancel-safe-core.first-request")
  let releaseFirstResponseFile = root.url.appendingPathComponent("cancel-safe-core.release-first")
  let script = """
    #!/bin/sh
    count_file="$0.count"
    first_request_file="$0.first-request"
    release_first_response_file="$0.release-first"
    count=0
    if [ -f "$count_file" ]; then count=$(/bin/cat "$count_file"); fi
    count=$((count + 1))
    printf '%s' "$count" > "$count_file"
    /bin/dd bs=55 count=1 of=/dev/null 2>/dev/null
    request_number=0
    while IFS= read -r request; do
      request_number=$((request_number + 1))
      if [ "$request_number" -eq 1 ]; then
        : > "$first_request_file"
        while [ ! -f "$release_first_response_file" ]; do /bin/sleep 0.01; done
      fi
      printf '{"jsonrpc":"2.0","id":%s,"result":{"enabled":false,"revision":0,"updatedAtMs":0}}\n' "$request_number"
    done
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

  let first = Task { try await client.runtime() }
  for _ in 0..<500 where !FileManager.default.fileExists(atPath: firstRequestFile.path) {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(FileManager.default.fileExists(atPath: firstRequestFile.path))
  first.cancel()
  switch await first.result {
  case .success:
    Issue.record("a cancelled RPC must not publish its late response")
  case .failure(let error):
    #expect(error as? CoreClientError == .requestCancelled)
  }
  try Data().write(to: releaseFirstResponseFile)

  let second = try await client.runtime()
  #expect(second == RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0))
  #expect(try String(contentsOf: countFile, encoding: .utf8) == "1")
}

@Test
func explicitShutdownWaitsForTheOldGenerationBeforeStartingAReplacement() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("restart-core")
  let countFile = root.url.appendingPathComponent("restart-core.count")
  let activeFile = root.url.appendingPathComponent("restart-core.active")
  let script = """
    #!/bin/sh
    count_file="$0.count"
    active_file="$0.active"
    if [ -f "$active_file" ]; then
      old_pid=$(/bin/cat "$active_file")
      if /bin/kill -0 "$old_pid" 2>/dev/null; then exit 99; fi
    fi
    printf '%s' "$$" > "$active_file"
    count=0
    if [ -f "$count_file" ]; then count=$(/bin/cat "$count_file"); fi
    count=$((count + 1))
    printf '%s' "$count" > "$count_file"
    /bin/dd bs=55 count=1 of=/dev/null 2>/dev/null
    IFS= read -r request
    printf '{"jsonrpc":"2.0","id":%s,"result":{"enabled":false,"revision":0,"updatedAtMs":0}}\n' "$count"
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

  _ = try await client.runtime()
  client.shutdown()
  let replacement = try await client.runtime()

  #expect(replacement == RuntimeControl(enabled: false, revision: 0, updatedAtMs: 0))
  #expect(try String(contentsOf: countFile, encoding: .utf8) == "2")
  #expect(FileManager.default.fileExists(atPath: activeFile.path))
}

@Test
func explicitShutdownRetriesExactTerminatorForSameNonCooperativeCoreGeneration() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("noncooperative-core")
  let startedFile = root.url.appendingPathComponent("noncooperative-core.started")
  let completedFile = root.url.appendingPathComponent("noncooperative-core.completed")
  let pidFile = root.url.appendingPathComponent("noncooperative-core.pid")
  let source = """
    #include <errno.h>
    #include <fcntl.h>
    #include <limits.h>
    #include <stdio.h>
    #include <string.h>
    #include <unistd.h>

    static int write_text(const char *path, const char *text) {
      int descriptor = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0600);
      if (descriptor < 0) return -1;
      size_t length = strlen(text);
      ssize_t written = write(descriptor, text, length);
      int saved_errno = errno;
      (void)close(descriptor);
      errno = saved_errno;
      return written == (ssize_t)length ? 0 : -1;
    }

    int main(int argc, char **argv) {
      if (argc < 1) return 2;
      char pid_path[PATH_MAX];
      char started_path[PATH_MAX];
      char completed_path[PATH_MAX];
      if (snprintf(pid_path, sizeof(pid_path), "%s.pid", argv[0]) >= (int)sizeof(pid_path)) return 3;
      if (snprintf(started_path, sizeof(started_path), "%s.started", argv[0]) >= (int)sizeof(started_path)) return 4;
      if (snprintf(completed_path, sizeof(completed_path), "%s.completed", argv[0]) >= (int)sizeof(completed_path)) return 5;
      char pid_text[32];
      if (snprintf(pid_text, sizeof(pid_text), "%d", getpid()) >= (int)sizeof(pid_text)) return 6;
      if (write_text(pid_path, pid_text) != 0) return 7;

      unsigned char bootstrap[55];
      size_t offset = 0;
      while (offset < sizeof(bootstrap)) {
        ssize_t count = read(STDIN_FILENO, bootstrap + offset, sizeof(bootstrap) - offset);
        if (count <= 0) return 8;
        offset += (size_t)count;
      }
      char byte = 0;
      do {
        if (read(STDIN_FILENO, &byte, 1) != 1) return 9;
      } while (byte != '\\n');
      if (write_text(started_path, "") != 0) return 10;
      sleep(5);
      if (write_text(completed_path, "") != 0) return 11;
      return 0;
    }
    """
  try compileTestCExecutable(source, output: executable)
  let validatedToken = LockIsolated<String?>(nil)
  let terminatedToken = LockIsolated<String?>(nil)
  let terminationAttempts = LockIsolated(0)
  let client = CoreProcessClient(
    executableResolver: { executable },
    staticCodeValidator: { _ in },
    runningCodeValidator: { token in
      validatedToken.withLock { $0 = token }
    },
    masterKeyLoader: { Data(repeating: 7, count: 32) },
    exactProcessTerminator: { token in
      terminatedToken.withLock { $0 = token }
      terminationAttempts.withLock { $0 += 1 }
      if terminationAttempts.value == 1 { return false }
      return terminateTestProcess(auditTokenHex: token)
    }
  )
  let observedEvent = LockIsolated<CoreTerminationEvent?>(nil)
  let events = client.terminationEvents()
  let observer = Task {
    for await event in events {
      observedEvent.withLock { $0 = event }
      return
    }
  }
  defer {
    observer.cancel()
    client.shutdown()
  }
  await Task.yield()
  try? await Task.sleep(for: .milliseconds(10))

  let request = Task { try await client.runtime() }
  for _ in 0..<500 where !FileManager.default.fileExists(atPath: startedFile.path) {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(FileManager.default.fileExists(atPath: startedFile.path))

  let childPID = try #require(
    Int32(try String(contentsOf: pidFile, encoding: .utf8)))
  let firstExactTerminationAccepted = client.shutdown()
  #expect(!firstExactTerminationAccepted)
  #expect(validatedToken.value?.count == 64)
  #expect(terminatedToken.value == validatedToken.value)
  #expect(terminationAttempts.value == 1)
  errno = 0
  #expect(Darwin.kill(childPID, 0) == 0)
  #expect(!FileManager.default.fileExists(atPath: completedFile.path))

  let retryExactTerminationAccepted = client.shutdown()
  #expect(retryExactTerminationAccepted)
  #expect(terminationAttempts.value == 2)
  #expect(terminatedToken.value == validatedToken.value)
  for _ in 0..<100 where observedEvent.value == nil {
    try? await Task.sleep(for: .milliseconds(10))
  }
  #expect(observedEvent.value?.reason == .explicitShutdown)
  #expect(!FileManager.default.fileExists(atPath: completedFile.path))
  errno = 0
  #expect(Darwin.kill(childPID, 0) == -1)
  #expect(errno == ESRCH)
  switch await request.result {
  case .success:
    Issue.record("an explicitly terminated Core request must not complete")
  case .failure(let error):
    #expect(error as? CoreClientError == .processTerminated)
  }
}

@Test
func unexpectedCoreExitPublishesOneBoundedLifecycleEvent() async throws {
  let root = try TemporaryDirectory()
  let executable = root.url.appendingPathComponent("exiting-core")
  let script = """
    #!/bin/sh
    /bin/dd bs=55 count=1 of=/dev/null 2>/dev/null
    IFS= read -r request
    exit 17
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
  let events = client.terminationEvents()
  let eventTask = Task { () -> CoreTerminationEvent? in
    for await event in events { return event }
    return nil
  }

  do {
    _ = try await client.runtime()
    Issue.record("the fake Core must exit before returning a response")
  } catch {}
  let event = await eventTask.value

  #expect(event?.generation == 1)
  #expect(event?.reason == .exited)
  #expect(event?.exitStatus == 17)
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

private enum TestFixtureError: Error {
  case compilerFailed(String)
}

private func compileTestCExecutable(_ source: String, output: URL) throws {
  let sourceURL = output.appendingPathExtension("c")
  try Data(source.utf8).write(to: sourceURL)
  let process = Process()
  process.executableURL = URL(fileURLWithPath: "/usr/bin/clang")
  process.arguments = [
    "-std=c11", "-D_DARWIN_C_SOURCE", "-O0", "-Wall", "-Wextra", "-Werror",
    sourceURL.path, "-o", output.path,
  ]
  let standardError = Pipe()
  process.standardError = standardError
  try process.run()
  process.waitUntilExit()
  guard process.terminationStatus == 0 else {
    let data = standardError.fileHandleForReading.readDataToEndOfFile()
    throw TestFixtureError.compilerFailed(String(decoding: data, as: UTF8.self))
  }
}

private func terminateTestProcess(auditTokenHex: String) -> Bool {
  guard auditTokenHex.count == MemoryLayout<audit_token_t>.size * 2 else { return false }
  var bytes = [UInt8]()
  bytes.reserveCapacity(MemoryLayout<audit_token_t>.size)
  var index = auditTokenHex.startIndex
  while index < auditTokenHex.endIndex {
    let next = auditTokenHex.index(index, offsetBy: 2)
    guard let byte = UInt8(auditTokenHex[index..<next], radix: 16) else { return false }
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

private func testProcessIdentifier(auditTokenHex: String) -> Int32? {
  guard auditTokenHex.count == MemoryLayout<audit_token_t>.size * 2 else { return nil }
  var bytes = [UInt8]()
  bytes.reserveCapacity(MemoryLayout<audit_token_t>.size)
  var index = auditTokenHex.startIndex
  while index < auditTokenHex.endIndex {
    let next = auditTokenHex.index(index, offsetBy: 2)
    guard let byte = UInt8(auditTokenHex[index..<next], radix: 16) else { return nil }
    bytes.append(byte)
    index = next
  }
  var token = audit_token_t()
  withUnsafeMutableBytes(of: &token) { destination in
    destination.copyBytes(from: bytes)
  }
  let processIdentifier = audit_token_to_pid(token)
  return processIdentifier > 0 ? processIdentifier : nil
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
