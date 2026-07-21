import AppKit
import Combine
import EffectBrokerBridge
import Foundation

public struct CoreGenerationFence: Equatable, Sendable {
  public let identifier: UUID
  public let generation: UInt64
}

public protocol CoreServing: Sendable {
  /// The production Core never enables deferred channel stages during PR1.
  /// The explicit test double override keeps historical recovery fixtures
  /// covered without making the shipped App surface writable channel routes.
  var permitsDeferredChannelTestRoutes: Bool { get }
  var permitsIMessageSelfChatRoutes: Bool { get }
  func beginCoreGenerationFence() async throws -> CoreGenerationFence
  func closeCoreGenerationFence(_ fence: CoreGenerationFence) async -> Bool
  func runtime() async throws -> RuntimeControl
  func effectIdentity() async throws -> CoreEffectIdentity
  func signBrokerEnrollment(_ anchor: EnrolledBrokerTrustAnchor) async throws -> Data
  func prepareCodexRuntime() async throws -> Int32
  func prepareCodexLoginRuntime() async throws -> Int32
  func bindCodexCandidateForBroker() async throws
  func initializeCodexRuntime() async throws
  func abortCodexCandidate() async throws
  func runtimeChallenge() async throws -> String
  func prepareRuntime(_ enabled: Bool) async throws -> RuntimeControlAuthorization
  func commitRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt: RuntimeControlReceipt
  ) async throws -> RuntimeControl
  func recoverRuntime(
    _ authorization: RuntimeControlAuthorization,
    brokerReceipt: RuntimeControlReceipt
  ) async throws -> RuntimeControl
  func installBrokerEnrollment(_ recordJSON: Data) async throws
  func installCoreLease(_ leaseJSON: Data) async throws
  func dashboard() async throws -> DashboardState
  func pairChannel(_ pairing: ChannelPairing, proof: BrokerRuntimeState) async throws
  func channelPairing(_ channel: ChannelKind) async throws -> ChannelPairing?
  func startDiscordSetup(token: String, proof: BrokerRuntimeState) async throws
    -> DiscordSetupStart
  func pollDiscordSetup(proof: BrokerRuntimeState) async throws -> DiscordSetupPollResponse
  func confirmDiscordSetup(
    candidateId: String, confirmedAtMs: Int64, proof: BrokerRuntimeState
  ) async throws
  func startDiscord(
    token: String, proof: BrokerRuntimeState
  ) async throws -> ChannelStatusResponse
  func prepareIMessageChatDiscovery(proof: BrokerRuntimeState) async throws
  func listPreparedIMessageChats(proof: BrokerRuntimeState) async throws -> [IMessageChat]
  func prepareIMessage(proof: BrokerRuntimeState) async throws
  func activateIMessage(proof: BrokerRuntimeState) async throws -> ChannelStatusResponse
  func channelStatus(_ channel: ChannelKind) async throws -> ChannelStatusResponse
  func stopChannel(_ channel: ChannelKind) async throws -> ChannelStatusResponse
  func stopChannelIfRunning(_ channel: ChannelKind) async throws -> ChannelStatusResponse
  func pollChannel(
    _ channel: ChannelKind,
    modelWorkAllowed: Bool,
    proof: BrokerRuntimeState
  ) async throws -> ChannelPollResponse
  func acknowledgeChannelFailure(
    _ incident: ChannelFailureIncident,
    acknowledgedAtMs: Int64,
    proof: BrokerRuntimeState
  ) async throws -> ChannelFailureIncident
  func bindChannelRoute(
    _ approval: ChannelRouteApproval, proof: BrokerRuntimeState
  ) async throws -> ChannelRouteSet
  func sendChannelMessage(
    missionId: String,
    routeId: String,
    kind: ChannelMessageKind,
    content: String,
    approvedAtMs: Int64,
    proof: BrokerRuntimeState
  ) async throws -> ChannelSendResponse
  func account(proof: BrokerRuntimeState) async throws -> AccountState
  func beginLogin(proof: BrokerRuntimeState) async throws -> ChatGptLogin
  func awaitLogin(identifier: String, proof: BrokerRuntimeState) async throws
  func models(proof: BrokerRuntimeState) async throws -> [GptModel]
  func modelSetup(proof: BrokerRuntimeState) async throws -> ModelSetup
  func selectedModel(proof: BrokerRuntimeState) async throws -> ModelSelection?
  func personaStatus() async throws -> PersonaStatusView?
  func selectModel(
    modelId: String,
    requestedEffort: String,
    catalogSnapshotId: String,
    catalogFingerprint: String,
    catalogRevision: UInt64,
    proof: BrokerRuntimeState
  ) async throws -> ModelSelection
  func choiceLoop() async throws -> ChoiceLoopSnapshot?
  func choiceReminderSchedule() async throws -> ChoiceReminderSchedule?
  func beginChoice(_ parameters: ChoiceBeginParameters) async throws -> ChoiceBeginAccepted
  func selectChoice(_ selection: ChoiceSelection, proof: BrokerRuntimeState) async throws
    -> ChoiceLoopSnapshot
  func selectChoiceD(
    _ input: ChoiceDInput, proof: BrokerRuntimeState
  ) async throws -> ChoiceLoopSnapshot
  func resumeChoice(proof: BrokerRuntimeState) async throws -> ChoiceLoopSnapshot
  func confirmChoice(
    _ confirmation: ChoiceConsolidatedConfirmation,
    proof: BrokerRuntimeState
  ) async throws -> ChoiceLoopSnapshot
  func prepareChoiceConfirmation(
    proof: BrokerRuntimeState
  ) async throws -> ChoiceConsolidatedConfirmation
  func prepareChoiceIMessageReply(proof: BrokerRuntimeState) async throws
    -> ChoiceIMessageReplyPrepareResponse
  func authorizeChoiceIMessageReply(
    _ preview: ChoiceIMessageReplyPreview, proof: BrokerRuntimeState
  ) async throws -> ChoiceIMessageReplyResponse
  func recordChoiceReminderSchedule(
    _ input: ChoiceReminderScheduleInput, proof: BrokerRuntimeState
  ) async throws -> ChoiceReminderSchedule
  func authorizeChoiceReminders(
    confirmationId: String, reminderTarget: ReminderTarget, proof: BrokerRuntimeState
  ) async throws -> ConfirmedMission
  func beginChoiceReminderDispatch(
    confirmationId: String, proof: BrokerRuntimeState
  ) async throws -> ReminderDispatchStart
  func abortChoiceReminderDispatchBeforeCommit(
    confirmationId: String, proof: BrokerRuntimeState
  ) async throws -> ConfirmedMission
  func recordChoiceReminderMirror(
    confirmationId: String, links: [ReminderLink], proof: BrokerRuntimeState
  ) async throws -> ConfirmedMission
  func completeChoiceReminders(
    confirmationId: String, completions: [ReminderCompletionInput], proof: BrokerRuntimeState
  ) async throws -> ChoiceReminderCompletion
  func cancelChoice(proof: BrokerRuntimeState) async throws -> ChoiceLoopSnapshot
  func reconcileChoiceMarkdown(proof: BrokerRuntimeState) async throws -> ChoiceLoopSnapshot
  func cleanupChoiceMarkdownReceipt() async throws -> ChoiceLoopSnapshot
  func choiceMarkdownReceiptCleanupAvailability() async throws
    -> ChoiceMarkdownReceiptCleanupAvailability
  func confirmSuggestion(
    identifier: String, reminderTarget: ReminderTarget
  ) async throws -> ConfirmedMission
  func cancelMission(
    identifier: String, proof: BrokerRuntimeState
  ) async throws -> MissionCancellation
  func beginReminderDispatch(identifier: String) async throws -> ReminderDispatchStart
  func recordReminderMirror(
    identifier: String, links: [ReminderLink]
  ) async throws -> ConfirmedMission
  func completeReminderMission(
    identifier: String,
    completions: [ReminderCompletionInput],
    receiptReturnApprovedAtMs: Int64?,
    receiptReturnRouteId: String?
  ) async throws -> MissionReceipt
}

extension CoreServing {
  public func authorizeChoiceReminders(
    confirmationId _: String, reminderTarget _: ReminderTarget, proof _: BrokerRuntimeState
  ) async throws -> ConfirmedMission {
    throw CoreClientError.contractViolation("Choice Reminder authorization is unavailable.")
  }

  public func beginChoiceReminderDispatch(
    confirmationId _: String, proof _: BrokerRuntimeState
  ) async throws -> ReminderDispatchStart {
    throw CoreClientError.contractViolation("Choice Reminder dispatch is unavailable.")
  }

  public func abortChoiceReminderDispatchBeforeCommit(
    confirmationId _: String, proof _: BrokerRuntimeState
  ) async throws -> ConfirmedMission {
    throw CoreClientError.contractViolation("Choice Reminder dispatch recovery is unavailable.")
  }

  public func recordChoiceReminderMirror(
    confirmationId _: String, links _: [ReminderLink], proof _: BrokerRuntimeState
  ) async throws -> ConfirmedMission {
    throw CoreClientError.contractViolation("Choice Reminder Evidence is unavailable.")
  }

  public func completeChoiceReminders(
    confirmationId _: String, completions _: [ReminderCompletionInput],
    proof _: BrokerRuntimeState
  ) async throws -> ChoiceReminderCompletion {
    throw CoreClientError.contractViolation("Choice Reminder completion is unavailable.")
  }

  public var permitsDeferredChannelTestRoutes: Bool { false }
  public var permitsIMessageSelfChatRoutes: Bool { false }
  public func selectChoice(_: ChoiceSelection, proof _: BrokerRuntimeState) async throws
    -> ChoiceLoopSnapshot
  {
    throw CoreClientError.contractViolation("Choice selection is unavailable from this Core.")
  }

  public func selectChoiceD(
    _: ChoiceDInput, proof _: BrokerRuntimeState
  ) async throws -> ChoiceLoopSnapshot {
    throw CoreClientError.contractViolation("Choice D selection is unavailable from this Core.")
  }

  public func resumeChoice(proof _: BrokerRuntimeState) async throws -> ChoiceLoopSnapshot {
    throw CoreClientError.contractViolation("Choice resume is unavailable from this Core.")
  }

  public func confirmChoice(
    _: ChoiceConsolidatedConfirmation, proof _: BrokerRuntimeState
  ) async throws -> ChoiceLoopSnapshot {
    throw CoreClientError.contractViolation("Choice confirmation is unavailable from this Core.")
  }

  public func prepareChoiceConfirmation(
    proof _: BrokerRuntimeState
  ) async throws -> ChoiceConsolidatedConfirmation {
    throw CoreClientError.contractViolation("Choice confirmation is unavailable from this Core.")
  }

  public func prepareChoiceIMessageReply(proof _: BrokerRuntimeState) async throws
    -> ChoiceIMessageReplyPrepareResponse
  {
    throw CoreClientError.contractViolation("Choice iMessage reply is unavailable from this Core.")
  }

  public func authorizeChoiceIMessageReply(
    _: ChoiceIMessageReplyPreview, proof _: BrokerRuntimeState
  ) async throws -> ChoiceIMessageReplyResponse {
    throw CoreClientError.contractViolation("Choice iMessage reply is unavailable from this Core.")
  }

  public func recordChoiceReminderSchedule(
    _: ChoiceReminderScheduleInput, proof _: BrokerRuntimeState
  ) async throws -> ChoiceReminderSchedule {
    throw CoreClientError.contractViolation(
      "Choice Reminder scheduling is unavailable from this Core.")
  }

  public func cancelChoice(proof _: BrokerRuntimeState) async throws -> ChoiceLoopSnapshot {
    throw CoreClientError.contractViolation("Choice cancellation is unavailable from this Core.")
  }

  public func reconcileChoiceMarkdown(proof _: BrokerRuntimeState) async throws
    -> ChoiceLoopSnapshot
  {
    throw CoreClientError.contractViolation(
      "Choice Markdown recovery is unavailable from this Core.")
  }

  public func cleanupChoiceMarkdownReceipt() async throws -> ChoiceLoopSnapshot {
    throw CoreClientError.contractViolation(
      "Choice Markdown receipt cleanup is unavailable from this Core.")
  }

  public func choiceMarkdownReceiptCleanupAvailability() async throws
    -> ChoiceMarkdownReceiptCleanupAvailability
  {
    ChoiceMarkdownReceiptCleanupAvailability(available: false)
  }

  public func beginCoreGenerationFence() async throws -> CoreGenerationFence {
    throw CoreClientError.contractViolation(
      "This Core client cannot bind a recovery attempt to one process generation.")
  }

  public func closeCoreGenerationFence(_ fence: CoreGenerationFence) async -> Bool { false }

  public func prepareCodexLoginRuntime() async throws -> Int32 {
    throw CoreClientError.contractViolation("Login-only Codex is unavailable in this test client.")
  }

  public func cancelMission(
    identifier _: String, proof _: BrokerRuntimeState
  ) async throws -> MissionCancellation {
    throw CoreClientError.contractViolation("Mission cancellation is unavailable in this client.")
  }

  public func pairChannel(_ pairing: ChannelPairing, proof: BrokerRuntimeState) async throws {
    throw CoreClientError.contractViolation("Channel pairing is unavailable in this test client.")
  }

  public func channelPairing(_ channel: ChannelKind) async throws -> ChannelPairing? { nil }

  public func startDiscordSetup(
    token: String, proof: BrokerRuntimeState
  ) async throws -> DiscordSetupStart {
    throw CoreClientError.contractViolation("Discord setup is unavailable in this test client.")
  }

  public func pollDiscordSetup(proof: BrokerRuntimeState) async throws -> DiscordSetupPollResponse {
    throw CoreClientError.contractViolation("Discord setup is unavailable in this test client.")
  }

  public func confirmDiscordSetup(
    candidateId: String, confirmedAtMs: Int64, proof: BrokerRuntimeState
  ) async throws {
    throw CoreClientError.contractViolation("Discord setup is unavailable in this test client.")
  }

  public func startDiscord(
    token: String, proof: BrokerRuntimeState
  ) async throws -> ChannelStatusResponse {
    throw CoreClientError.contractViolation("Discord is unavailable in this test client.")
  }

  public func prepareIMessage(proof: BrokerRuntimeState) async throws {
    throw CoreClientError.contractViolation("iMessage is unavailable in this test client.")
  }

  public func prepareIMessageChatDiscovery(proof: BrokerRuntimeState) async throws {
    throw CoreClientError.contractViolation(
      "iMessage discovery is unavailable in this test client.")
  }

  public func listPreparedIMessageChats(proof: BrokerRuntimeState) async throws -> [IMessageChat] {
    throw CoreClientError.contractViolation(
      "iMessage discovery is unavailable in this test client.")
  }

  public func activateIMessage(proof: BrokerRuntimeState) async throws -> ChannelStatusResponse {
    throw CoreClientError.contractViolation("iMessage is unavailable in this test client.")
  }

  public func stopChannelIfRunning(_ channel: ChannelKind) async throws -> ChannelStatusResponse {
    try await stopChannel(channel)
  }

  public func channelStatus(_ channel: ChannelKind) async throws -> ChannelStatusResponse {
    throw CoreClientError.contractViolation("Channel status is unavailable in this test client.")
  }

  public func selectedModel(proof _: BrokerRuntimeState) async throws -> ModelSelection? { nil }

  /// Non-production clients may omit this diagnostic projection. They never
  /// receive a mutable Persona lifecycle route through the fallback.
  public func personaStatus() async throws -> PersonaStatusView? { nil }

  /// Test-only/default clients do not have the production atomic setup RPC.
  /// They must never grant model readiness from this fallback: shipped
  /// `CoreProcessClient` overrides it with `models.setup.read`.
  public func modelSetup(proof: BrokerRuntimeState) async throws -> ModelSetup {
    let account = try await account(proof: proof)
    let models = try await models(proof: proof)
    let selection = try await selectedModel(proof: proof)
    return ModelSetup(
      account: account,
      models: models,
      selection: selection,
      selectionStatus: selection == nil ? .unselected : .unavailable,
      catalogSnapshotId: "",
      catalogFingerprint: "",
      catalogRevision: 0
    )
  }

  public func selectModel(
    modelId _: String,
    requestedEffort _: String,
    catalogSnapshotId _: String,
    catalogFingerprint _: String,
    catalogRevision _: UInt64,
    proof _: BrokerRuntimeState
  ) async throws -> ModelSelection {
    throw CoreClientError.contractViolation("Model selection is unavailable in this test client.")
  }

  public func choiceLoop() async throws -> ChoiceLoopSnapshot? { nil }
  public func choiceReminderSchedule() async throws -> ChoiceReminderSchedule? { nil }

  public func beginChoice(_ parameters: ChoiceBeginParameters) async throws -> ChoiceBeginAccepted {
    _ = parameters
    throw CoreClientError.contractViolation("Choice intake is unavailable in this test client.")
  }

  public func stopChannel(_ channel: ChannelKind) async throws -> ChannelStatusResponse {
    throw CoreClientError.contractViolation("Channel stop is unavailable in this test client.")
  }

  public func pollChannel(
    _ channel: ChannelKind,
    modelWorkAllowed: Bool,
    proof: BrokerRuntimeState
  ) async throws -> ChannelPollResponse {
    throw CoreClientError.contractViolation("Channel polling is unavailable in this test client.")
  }

  public func acknowledgeChannelFailure(
    _ incident: ChannelFailureIncident,
    acknowledgedAtMs: Int64,
    proof: BrokerRuntimeState
  ) async throws -> ChannelFailureIncident {
    throw CoreClientError.contractViolation(
      "Channel failure acknowledgement is unavailable in this test client.")
  }

  public func sendChannelMessage(
    missionId: String,
    routeId: String,
    kind: ChannelMessageKind,
    content: String,
    approvedAtMs: Int64,
    proof: BrokerRuntimeState
  ) async throws -> ChannelSendResponse {
    throw CoreClientError.contractViolation("Channel sending is unavailable in this test client.")
  }

  public func bindChannelRoute(
    _ approval: ChannelRouteApproval, proof: BrokerRuntimeState
  ) async throws -> ChannelRouteSet {
    throw CoreClientError.contractViolation(
      "Channel route binding is unavailable in this test client.")
  }
}

public protocol BrokerRuntimeServing: Sendable {
  func provision(coreIdentity: CoreEffectIdentity) async throws -> EnrolledBrokerTrustAnchor
  func prepareCodexRuntimeHome() async throws -> String
  func acquireCoreLease(
    coreIdentity: CoreEffectIdentity, codexProcessIdentifier: Int32
  ) async throws -> Data
  func status(challenge: String) async throws -> BrokerRuntimeState?
  func apply(_ authorization: RuntimeControlAuthorization) async throws -> RuntimeControlReceipt
}

public struct BrokerRuntimeState: Equatable, Sendable {
  public let authorization: RuntimeControlAuthorization
  public let receipt: RuntimeControlReceipt
}

public enum RuntimeDisplayState: Equatable, Sendable {
  case off
  case on
  case turningOn
  case turningOff
  case unknown

  public var label: String {
    switch self {
    case .off: "OpenOpen is Off"
    case .on: "OpenOpen is On"
    case .turningOn: "Turning OpenOpen On…"
    case .turningOff: "Turning OpenOpen Off…"
    case .unknown: "OpenOpen state is Unknown"
    }
  }

  public var menuBarSymbol: String {
    switch self {
    case .on: "circle.fill"
    case .turningOn, .turningOff: "circle.dotted"
    case .off: "circle"
    case .unknown: "questionmark.circle"
    }
  }
}

public enum RuntimeRecoveryState: Equatable, Sendable {
  case ready
  case recovering
  case awaitingAccount
  case paused

  public var message: String? {
    switch self {
    case .ready:
      nil
    case .recovering:
      "OpenOpen is restoring its verified local runtime and approved connections."
    case .awaitingAccount:
      "Need you: review your account and model setup before OpenOpen finishes turning on."
    case .paused:
      "Need you: OpenOpen paused after Core stopped. No listener, model, or outbound work is running."
    }
  }
}

/// The foreground Choice session is useful local continuity, but it is never
/// authority for a model turn or an external effect. A failed read must remain
/// visible without making protected Off, Settings, or Dashboard controls
/// unreachable.
public enum ChoiceLoopContinuityState: Equatable, Sendable {
  case empty
  case current
  case needsYou(ChoiceLoopContinuityIssue)

  public var message: String? {
    switch self {
    case .empty, .current:
      nil
    case .needsYou:
      "Need you: OpenOpen could not verify your local session continuity. Review it before relying on it."
    }
  }
}

public enum ChoiceLoopContinuityIssue: Equatable, Sendable {
  case readFailed
  case invalidContract
  case blocked
  case clockUncertain
  case refreshRequired
}

/// The functional Dashboard surface derived from product state.
///
/// SwiftUI and the state-matrix tests consume this same projection so a
/// background failure cannot accidentally disable protected Off, Settings, or
/// an otherwise-authorized local Outcome request.
public struct DashboardControlState: Equatable, Sendable {
  public let globalToggleEnabled: Bool
  public let settingsEnabled: Bool
  public let outcomeInputEnabled: Bool
  public let outcomeSubmitEnabled: Bool
  public let suggestionConfirmationEnabled: Bool
  public let missionProgressEnabled: Bool
  public let missionCancellationEnabled: Bool
  public let doneVisible: Bool

  public static func evaluate(
    runtimeDisplayState _: RuntimeDisplayState,
    runtimeRecoveryState _: RuntimeRecoveryState,
    modelEntryEnabled: Bool,
    storeControlEnabled: Bool,
    isBusy: Bool,
    hasConfirmedMission: Bool,
    hasCancellableMission: Bool,
    hasNeedsYou: Bool,
    hasSuggestion: Bool,
    suggestionMatchesConfirmedMission: Bool,
    hasReceipt: Bool,
    terminalIncidentCount _: Int,
    localFeedbackPresent _: Bool,
    prompt: String,
    hasActiveForegroundChoiceSession: Bool = false,
    permitsFocusedChoiceDComposer: Bool = false
  ) -> Self {
    let inputEnabled =
      modelEntryEnabled && !isBusy && !hasConfirmedMission && !hasNeedsYou
      && (!hasActiveForegroundChoiceSession || permitsFocusedChoiceDComposer)
    let normalizedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
    return Self(
      // Runtime-control and navigation must remain reachable independently of
      // model, listener, incident, Mission, or recovery state.
      globalToggleEnabled: true,
      settingsEnabled: true,
      outcomeInputEnabled: inputEnabled,
      outcomeSubmitEnabled: inputEnabled && !normalizedPrompt.isEmpty
        && normalizedPrompt.utf8.count <= 4_096,
      suggestionConfirmationEnabled: modelEntryEnabled && !isBusy && hasSuggestion
        && (!hasConfirmedMission || suggestionMatchesConfirmedMission),
      // Reminder readback/completion and Mission cancellation are bounded
      // Store-control actions. They require protected On, but not ChatGPT or
      // model-catalog readiness, and they never grant model/outbound entry.
      missionProgressEnabled: storeControlEnabled && !isBusy && hasConfirmedMission,
      missionCancellationEnabled: storeControlEnabled && !isBusy && hasCancellableMission,
      doneVisible: hasReceipt
    )
  }
}

private struct DiscordRecoveryProgress {
  var cursorClosed = false
  var sawRecoveredEvent = false
  var latestMissionEvent: ChannelMissionEvent?
}

private enum DurableChannelRestoreError: Error {
  case unavailable(ChannelKind)
}

extension CoreProcessClient: CoreServing {}

@MainActor
public final class AppModel: ObservableObject {
  /// Channel setup belongs to PR2/PR3, not the PR1 local Choice Core. The
  /// historical connection state remains readable, but this stage never
  /// exposes setup, pairing, token, or listener actions.
  public var choiceCoreConnectionsAvailable: Bool { core.permitsIMessageSelfChatRoutes }
  public var discordSetupVisible: Bool { core.permitsDeferredChannelTestRoutes }

  /// PR1 renders durable historical Mission/Receipt state read-only. The
  /// shipped Core never permits the pre-Choice Mission effect family; the
  /// explicit test double preserves recovery regression coverage without
  /// leaving a production AppModel path that can start Reminder work.
  private var historicalMissionEffectsAvailable: Bool { core.permitsDeferredChannelTestRoutes }
  @Published public private(set) var enabled = false
  @Published public private(set) var runtimeDisplayState: RuntimeDisplayState = .unknown
  @Published public var prompt = ""
  @Published public private(set) var suggestion: OutcomeSuggestion?
  @Published public private(set) var activeCards: [ActiveOutcomeCard] = []
  @Published public private(set) var confirmedMission: ConfirmedMission?
  @Published public private(set) var reminderLinks: [ReminderLink] = []
  @Published public private(set) var receipt: MissionReceipt?
  @Published public private(set) var needsYou: MissionNeedsYou?
  @Published public private(set) var channelFailureIncidents: [ChannelFailureIncident] = []
  @Published public private(set) var channelFailureFeedback: String?
  @Published public private(set) var channelFailureAcknowledgementFeedback: [String: String] = [:]
  @Published public private(set) var channelListenerFeedback: [ChannelKind: String] = [:]
  @Published public private(set) var channelRouteSet: ChannelRouteSet?
  @Published public private(set) var latestChannelMissionEvent: ChannelMissionEvent?
  @Published public var selectedChannelRouteId = ""
  @Published public private(set) var pendingAdditionalRoute: ChannelRouteDraft?
  @Published public var routeAllowsProgress = false
  @Published public var routeAllowsNeedYou = false
  @Published public var routeAllowsReceipt = false
  @Published public private(set) var iMessageStatus = "disconnected"
  @Published public private(set) var discordStatus = "disconnected"
  @Published public private(set) var iMessageChats: [IMessageChat] = []
  @Published public private(set) var iMessageChatId = ""
  @Published public private(set) var iMessageOwnerSender = ""
  @Published public var discordTokenDraft = ""
  @Published public private(set) var discordSetup: DiscordSetupStart?
  @Published public private(set) var discordPairingCandidate: DiscordPairingCandidate?
  @Published public private(set) var discordSetupFeedback: String?
  @Published public var channelMessageDraft =
    "Working on it — I’ll return when the Reminders evidence is complete."
  @Published public private(set) var microphone = MicrophoneState(
    available: false,
    reason: "Microphone unavailable until Voice setup"
  )
  @Published public private(set) var accountState: AccountState = .notConnected
  @Published public private(set) var availableModels: [GptModel] = []
  @Published public var selectedModelId = ""
  @Published public var selectedModelEffort = ""
  @Published public private(set) var persistedModelSelection: ModelSelection?
  @Published public private(set) var personaStatus: PersonaStatusView?
  @Published public private(set) var modelSelectionStatus: ModelSelectionStatus = .unselected
  @Published public private(set) var catalogSnapshotId = ""
  @Published public private(set) var catalogFingerprint = ""
  @Published public private(set) var catalogRevision: UInt64 = 0
  @Published public private(set) var choiceLoopSnapshot: ChoiceLoopSnapshot?
  @Published public private(set) var choiceMarkdownReceiptCleanupAvailable = false
  @Published public private(set) var choiceLoopContinuityState: ChoiceLoopContinuityState = .empty
  @Published public var choiceQuestion = ""
  /// A one-shot local routing fence for the frozen D card.  It is deliberately
  /// not durable authority: only the Host validates the matching active
  /// ChoiceSet before accepting composer text as D input.
  @Published public private(set) var choiceDComposerFocusRequested = false
  // Empty means no user-provided schedule.  The Mac never manufactures a
  // date, zone, or list from the question timestamp, current clock, or a
  // hidden default; the Host independently validates the submitted instant.
  @Published public var choiceReminderDateTime = ""
  @Published public var choiceReminderTimeZone = ""
  @Published public var choiceReminderListId = ""
  @Published public var choiceReminderCount = ""
  @Published public private(set) var choiceReminderPickerDate = Date()
  @Published public private(set) var choiceReminderPickerIsPresented = false
  @Published public private(set) var choiceReminderScheduleIsVisible = true
  @Published public private(set) var choiceConfirmationPreview: ChoiceConsolidatedConfirmation?
  @Published public private(set) var choiceIMessageReplyPreview: ChoiceIMessageReplyPreview?
  @Published public private(set) var choiceIMessageReplyStatus: String?
  // These identities survive an ambiguous in-process RPC response. They are
  // cleared only after Host continuity proves the corresponding durable
  // transition, never when transport delivery is uncertain.
  private var pendingChoiceBeginRequest: (question: String, requestId: String)?
  private var pendingChoiceOptionSelection: ChoiceSelection?
  private var pendingChoiceDRequest: ChoiceDInput?
  // A terminal D snapshot does not expose the private request identity. Only
  // an ambiguous transport result may use the exact revision/session shape as
  // a body-retirement witness; a known rejection preserves the owner's draft.
  private var pendingChoiceDResponseMayBeAmbiguous = false
  private var choiceDComposerTarget: (sessionId: String, choiceSetId: String, revision: UInt64)?
  // Once the frozen D card has focused the shared composer, text remains D
  // intent until Host acceptance or an explicit user clear. A stale target
  // must never make the same retained text fall through to `choice.begin`.
  private var choiceDComposerTextIsBound = false
  // The suppression key is an observed durable state within one exact Core
  // generation, not a session-wide latch. A replacement Core may recover the
  // same Store-owned pending resume, and a failed resume returns a newer
  // idle/stale revision that may receive one later genuine owner return.
  private var automaticResumeAttemptedStates: Set<String> = []
  private var pendingChoiceReminderSchedule:
    (
      dateTime: String, timeZone: String, listId: String, count: String, sessionId: String,
      sessionRevision: UInt64, requestId: String
    )?
  // A visible schedule edit is not merely cosmetic: it invalidates any
  // in-flight preview and prevents a blank draft from silently reusing an
  // older sealed schedule after a restart or lost response.
  private var choiceReminderScheduleDraftRevision: UInt64 = 0
  private var choiceReminderScheduleDraftIsDirty = false
  private var choiceReminderPickerDateIsExplicit = false
  private var applyingChoiceReminderScheduleHydration = false
  @Published public private(set) var isBusy = false
  @Published public private(set) var errorMessage: String?
  @Published public var showsSettings = false
  @Published public private(set) var runtimeRecoveryState: RuntimeRecoveryState = .ready

  /// A completed Choice Receipt remains typed after restart even though the
  /// dashboard intentionally has no nonterminal Mission focus. Both values
  /// come from authenticated Store projections; presentation never relies on
  /// transient Mission focus to classify durable history.
  public var receiptIsForCurrentChoice: Bool {
    guard let receipt, let confirmation = choiceLoopSnapshot?.confirmation else { return false }
    return receipt.outputHashes.contains(confirmation.payloadDigest)
  }

  /// Once authenticated continuity advances beyond the receipted Choice, the
  /// old Receipt remains in Activity history but must not fall through to the
  /// retired generic Done card beside a new/refining ChoiceSet.
  public var receiptIsPresentableOnHome: Bool {
    guard receipt != nil else { return false }
    guard choiceLoopSnapshot != nil else { return true }
    return receiptIsForCurrentChoice
  }

  private let core: any CoreServing
  private let broker: any BrokerRuntimeServing
  private let reminders: any RemindersServing
  private let discordTokenStore: any DiscordTokenStoring
  private let registerLoginItem: @Sendable () throws -> Void
  private let openOfficialURL: @Sendable (URL) -> Bool
  private let shutdownCore: @Sendable () -> Bool
  private var confirmedEnabled = false
  @Published private var desiredEnabled = false
  private var pendingRuntimeIntent: Bool?
  private var switchTask: Task<Void, Never>?
  private var dashboardRefreshTask: Task<Void, Never>?
  private var dashboardRefreshIdentifier: UUID?
  private var dashboardRefreshAuthenticatedHomeForeground = false
  private var heroTask: Task<Void, Never>?
  private var channelTask: Task<Void, Never>?
  private var choiceResultTask: Task<Void, Never>?
  private var choiceReminderWriteTask: Task<Void, Never>?
  private var choiceReminderWriteTaskID: UUID?
  private var channelFailureAcknowledgementTasks: [String: Task<Void, Never>] = [:]
  private var channelFailureAcknowledgementTokens: [String: UUID] = [:]
  private var recoveringTerminalChannelFailure = false
  private var coreLifecycleTask: Task<Void, Never>?
  private var coreRecoveryTask: Task<Void, Never>?
  private var connectedChannels: Set<ChannelKind> = []
  private var durablePairings: [ChannelKind: ChannelPairing] = [:]
  private var loginItemRegistered = false
  private var runtimeGeneration: UInt64 = 0
  private var choiceLoopRefreshSequence: UInt64 = 0
  private var hasVerifiedChoiceContinuity = false
  private var protectedRuntime: BrokerRuntimeState?
  private var authoritativeStateCertain = false
  private var brokerTrustCoreInstanceNonce: String?
  private var codexReadyCoreInstanceNonce: String?
  private var offCoreInterruptionFailed = false
  private var offRequiresReplacementCoreProvisioning = false
  private var onRequiresReplacementCoreRestoration = false
  private var onRequiresAccountSetup = false
  private var managedLoginMayHaveCompleted = false
  private let coreTerminationEvents: AsyncStream<CoreTerminationEvent>?
  private let channelPollInterval: Duration
  private var lastCoreTerminationGeneration: UInt64 = 0

  public var modelEntryEnabled: Bool {
    enabled && desiredEnabled && runtimeDisplayState == .on && runtimeRecoveryState == .ready
      && requiredAccountAndModelReady
  }

  public var runtimeRecoveryMessage: String? {
    guard runtimeRecoveryState == .awaitingAccount else { return runtimeRecoveryState.message }
    switch modelSelectionStatus {
    case .unselected:
      return
        "Need you: choose a compatible model and its supported effort before OpenOpen finishes turning on."
    case .unavailable:
      return
        "Need you: your saved model selection is no longer available. Review the current model and effort."
    case .current:
      return "Need you: review the current account setup before OpenOpen finishes turning on."
    }
  }

  public var choiceLoopContinuityMessage: String? {
    choiceLoopContinuityState.message
  }

  /// A preserved last-known-good snapshot is recovery evidence, not current
  /// authority for another Choice/model call. Empty is valid only for a new
  /// first question; every existing-session action requires a current read.
  private var choiceContinuityAllowsBegin: Bool {
    hasVerifiedChoiceContinuity
      && (choiceLoopContinuityState == .empty || choiceLoopContinuityState == .current)
  }

  public var choiceSessionActionEnabled: Bool {
    modelEntryEnabled && !isBusy && hasVerifiedChoiceContinuity
      && choiceLoopContinuityState == .current && choiceLoopSnapshot?.session.state == "active"
  }

  private func requireChoiceContinuity(
    expectedSequence: UInt64, permitsEmpty: Bool = false
  ) throws {
    let stateIsCurrent = choiceLoopContinuityState == .current
    let stateIsPermittedEmpty = permitsEmpty && choiceLoopContinuityState == .empty
    guard hasVerifiedChoiceContinuity, expectedSequence == choiceLoopRefreshSequence,
      stateIsCurrent || stateIsPermittedEmpty
    else {
      throw CoreClientError.contractViolation(
        "Refresh local Choice continuity before continuing."
      )
    }
  }

  public var selectedCatalogModel: GptModel? {
    availableModels.first(where: { $0.id == selectedModelId })
  }

  public var selectedCatalogModelEfforts: [String] {
    selectedCatalogModel?.supportedReasoningEfforts ?? []
  }

  public var modelSelectionCanBeSaved: Bool {
    guard accountSetupEnabled,
      !isBusy,
      isLowerSHA256(catalogSnapshotId),
      isLowerSHA256(catalogFingerprint),
      catalogRevision > 0,
      let model = selectedCatalogModel
    else { return false }
    if model.supportedReasoningEfforts.isEmpty {
      return selectedModelEffort == "not_applicable"
    }
    return model.supportedReasoningEfforts.contains(selectedModelEffort)
  }

  public func modelEffortLabel(_ effort: String) -> String {
    switch effort {
    case "low": "Faster (low)"
    case "medium": "More thoughtful (medium)"
    case "high": "Deepest (high)"
    case "xhigh": "Deepest (xhigh)"
    case "max": "Deepest (max)"
    default: "Available effort (\(effort))"
    }
  }

  /// A bounded local/Store-control route for an already-confirmed Mission.
  ///
  /// The last broker-protected state must be On and the Owner must still want
  /// On. Account/model readiness is deliberately excluded so a stranded
  /// Mission can be cancelled or completed while sign-in is awaiting action.
  /// Every call still revalidates a fresh broker proof before mutation. A
  /// recovering generation is excluded to avoid racing its fenced restore;
  /// paused may recover a replacement Core but remains visibly paused.
  public var storeControlEnabled: Bool {
    enabled && desiredEnabled && confirmedEnabled
      && protectedRuntime?.authorization.enabled == true
      && !offCoreInterruptionFailed && runtimeRecoveryState != .recovering
  }

  /// Existing route-bound polling and exact approved channel effects require
  /// protected Store control, but never grant new model authority.
  public var channelEffectEntryEnabled: Bool {
    storeControlEnabled
      && (runtimeRecoveryState == .ready || runtimeRecoveryState == .awaitingAccount)
  }

  public var discordSetupCheckEnabled: Bool {
    modelEntryEnabled && !isBusy && discordSetup != nil
  }

  public var discordSetupConfirmationEnabled: Bool {
    modelEntryEnabled && !isBusy && discordPairingCandidate != nil
  }

  /// The latest owner-requested Global state, including an in-flight
  /// transition. Visible switches bind to this value rather than the last
  /// authoritative Store state so a slow On can always be cancelled by an
  /// immediate Off request.
  public var runtimeToggleValue: Bool {
    if let pendingRuntimeIntent { return pendingRuntimeIntent }
    // Unknown must never render a false Off placeholder. Conservatively
    // presenting On makes the first safety interaction an Off request whether
    // the protected Store is actually On or already Off.
    return authoritativeStateCertain ? desiredEnabled : true
  }

  public var dashboardControls: DashboardControlState {
    DashboardControlState.evaluate(
      runtimeDisplayState: runtimeDisplayState,
      runtimeRecoveryState: runtimeRecoveryState,
      modelEntryEnabled: modelEntryEnabled && choiceContinuityAllowsBegin,
      storeControlEnabled: storeControlEnabled,
      isBusy: isBusy,
      hasConfirmedMission: hasNonterminalMission,
      hasCancellableMission: hasNonterminalMission,
      hasNeedsYou: needsYou != nil,
      hasSuggestion: suggestion != nil,
      suggestionMatchesConfirmedMission: suggestion.map { suggestion in
        confirmedMission.map { Self.matches($0, suggestion: suggestion) } ?? true
      } ?? false,
      hasReceipt: receipt != nil,
      terminalIncidentCount: channelFailureIncidents.count,
      localFeedbackPresent: errorMessage != nil || channelFailureFeedback != nil,
      prompt: choiceQuestion,
      hasActiveForegroundChoiceSession: hasActiveForegroundChoiceSession,
      permitsFocusedChoiceDComposer: focusedChoiceDComposerIsCurrent
    )
  }

  /// The shared composer remains disabled throughout an active Choice except
  /// when the frozen D card has installed this exact, transient target. This
  /// is presentation-only: Host still validates the target before accepting
  /// the text as D input.
  private var focusedChoiceDComposerIsCurrent: Bool {
    guard choiceDComposerTextIsBound, let target = choiceDComposerTarget,
      let snapshot = choiceLoopSnapshot, let choiceSet = snapshot.activeChoiceSet
    else { return false }
    return snapshot.session.state == "active"
      && snapshot.session.id == target.sessionId
      && snapshot.session.revision == target.revision
      && choiceSet.id == target.choiceSetId
      && choiceSet.sessionRevision == target.revision
      && choiceSet.dAvailable
  }

  private var hasActiveForegroundChoiceSession: Bool {
    guard let snapshot = choiceLoopSnapshot else { return false }
    // A completed local Markdown journal has no pending effect authority.
    // Starting a new explicit question is the only way to supersede it; the
    // Host verifies that transition atomically before accepting any intake.
    return !["completed", "cancelled", "executing"].contains(snapshot.session.state)
  }

  public var accountSetupEnabled: Bool {
    enabled && desiredEnabled && confirmedEnabled && authoritativeStateCertain
      && protectedRuntime?.authorization.enabled == true
      && !offCoreInterruptionFailed && !offRequiresReplacementCoreProvisioning
      && (runtimeRecoveryState == .ready || runtimeRecoveryState == .awaitingAccount)
  }

  private var hasNonterminalMission: Bool {
    !activeCards.isEmpty || confirmedMission != nil || needsYou != nil
  }

  private var channelPollingEnabled: Bool {
    modelEntryEnabled || (channelEffectEntryEnabled && !connectedChannels.isEmpty)
  }

  public var iMessageOwnerOptions: [String] {
    iMessageChats.first(where: { $0.chatId == iMessageChatId })?.participants ?? []
  }

  public var iMessageIsConnected: Bool {
    iMessageStatus == "connected"
  }

  public var iMessagePairingSelectionComplete: Bool {
    guard let chatId = Int64(iMessageChatId), chatId > 0 else { return false }
    return !iMessageOwnerSender.isEmpty
      && iMessageOwnerSender == iMessageOwnerSender.trimmingCharacters(in: .whitespacesAndNewlines)
      && iMessageOwnerOptions.contains(iMessageOwnerSender)
  }

  public var iMessageConnectionActionEnabled: Bool {
    guard !isBusy, !iMessageIsConnected else { return false }
    if durablePairings[.iMessage] != nil { return channelEffectEntryEnabled }
    return modelEntryEnabled && iMessagePairingSelectionComplete
  }

  public var iMessageHasDurablePairing: Bool {
    durablePairings[.iMessage] != nil
  }

  public var discordConnectionActionEnabled: Bool {
    guard !isBusy, discordStatus != "connected" else { return false }
    return durablePairings[.discord] == nil ? modelEntryEnabled : channelEffectEntryEnabled
  }

  public var outboundChannelRoutes: [ChannelRoute] {
    channelRouteSet?.routes.filter { !$0.allowedOutboundClasses.isEmpty } ?? []
  }

  public var selectedRouteAllowsProgress: Bool {
    guard let confirmedMission, channelRouteSet?.missionId == confirmedMission.missionId else {
      return false
    }
    return selectedChannelRoute(for: .progress) != nil
  }

  public var selectedRouteAllowsNeedYou: Bool {
    guard let needsYou, channelRouteSet?.missionId == needsYou.missionId else { return false }
    return selectedChannelRoute(for: .needYou) != nil
  }

  public var selectedRouteAllowsReceipt: Bool {
    guard let receipt, channelRouteSet?.missionId == receipt.missionId else { return false }
    return selectedChannelRoute(for: .receipt) != nil
  }

  public init(core: any CoreServing = CoreProcessClient()) {
    self.core = core
    broker = PrivilegedBrokerRuntimeClient()
    reminders = RemindersClient()
    discordTokenStore = DiscordTokenKeychain()
    registerLoginItem = { try LoginItemController.registerAfterOnboarding() }
    openOfficialURL = { NSWorkspace.shared.open($0) }
    shutdownCore = Self.shutdownHandler(for: core)
    coreTerminationEvents = (core as? any CoreLifecycleMonitoring)?.terminationEvents()
    channelPollInterval = .seconds(1)
    startCoreLifecycleMonitoring()
  }

  init(
    core: any CoreServing,
    broker: any BrokerRuntimeServing = PrivilegedBrokerRuntimeClient(),
    reminders: any RemindersServing = RemindersClient(),
    discordTokenStore: any DiscordTokenStoring = DiscordTokenKeychain(),
    registerLoginItem: @escaping @Sendable () throws -> Void,
    openOfficialURL: @escaping @Sendable (URL) -> Bool = { NSWorkspace.shared.open($0) },
    coreTerminationEvents: AsyncStream<CoreTerminationEvent>? = nil,
    shutdownCore: (@Sendable () -> Bool)? = nil,
    channelPollInterval: Duration = .seconds(1)
  ) {
    self.core = core
    self.broker = broker
    self.reminders = reminders
    self.discordTokenStore = discordTokenStore
    self.registerLoginItem = registerLoginItem
    self.openOfficialURL = openOfficialURL
    self.shutdownCore = shutdownCore ?? Self.shutdownHandler(for: core)
    self.coreTerminationEvents =
      coreTerminationEvents ?? (core as? any CoreLifecycleMonitoring)?.terminationEvents()
    self.channelPollInterval = channelPollInterval
    startCoreLifecycleMonitoring()
  }

  /// Refreshes durable dashboard state.  A generic refresh (including
  /// Settings/recovery presentation) is read-only; only the Home surface may
  /// explicitly mark one foreground return as eligible for a private resume.
  public func refreshDashboard(authenticatedHomeForeground: Bool = false) async {
    if let dashboardRefreshTask {
      let existingRefreshIsHomeForeground = dashboardRefreshAuthenticatedHomeForeground
      await dashboardRefreshTask.value
      // A root/settings refresh can race the real Home appearance.  Let that
      // one Home appearance run exactly one follow-up refresh after the
      // non-authorizing task, rather than promoting the in-flight task.
      if authenticatedHomeForeground, !existingRefreshIsHomeForeground {
        await refreshDashboard(authenticatedHomeForeground: true)
      }
      return
    }
    let identifier = UUID()
    let task = Task { [weak self] in
      guard let self else { return }
      await performDashboardRefresh(authenticatedHomeForeground: authenticatedHomeForeground)
      finishDashboardRefresh(identifier)
    }
    dashboardRefreshIdentifier = identifier
    dashboardRefreshAuthenticatedHomeForeground = authenticatedHomeForeground
    dashboardRefreshTask = task
    await task.value
  }

  private func finishDashboardRefresh(_ identifier: UUID) {
    guard dashboardRefreshIdentifier == identifier else { return }
    dashboardRefreshIdentifier = nil
    dashboardRefreshAuthenticatedHomeForeground = false
    dashboardRefreshTask = nil
  }

  /// Reads the durable foreground Choice session without turning a read or
  /// validation failure into a false empty state. The sequence token fences a
  /// late refresh even when the runtime generation itself has not changed.
  private func refreshChoiceLoopContinuity(
    expectedGeneration: UInt64, authenticatedOwnerReturn: Bool = false
  ) async {
    choiceLoopRefreshSequence &+= 1
    let sequence = choiceLoopRefreshSequence

    do {
      let snapshot = try await core.choiceLoop()
      guard expectedGeneration == runtimeGeneration, sequence == choiceLoopRefreshSequence,
        !Task.isCancelled
      else { return }

      guard let snapshot else {
        choiceLoopSnapshot = nil
        choiceMarkdownReceiptCleanupAvailable = false
        hasVerifiedChoiceContinuity = true
        choiceLoopContinuityState = .empty
        return
      }

      do {
        let validated = try snapshot.validated()
        let priorSessionID = choiceLoopSnapshot?.session.id
        if priorSessionID != nil && priorSessionID != validated.session.id {
          clearChoiceReminderScheduleDraft()
        }
        let cleanupAvailable: Bool
        if validated.session.state == "cancelled" {
          cleanupAvailable = try await core.choiceMarkdownReceiptCleanupAvailability().available
        } else {
          cleanupAvailable = false
        }
        let schedule = try await core.choiceReminderSchedule()
        guard expectedGeneration == runtimeGeneration, sequence == choiceLoopRefreshSequence,
          !Task.isCancelled
        else { return }
        if let schedule {
          guard schedule.validated(),
            schedule.input.choiceSessionId == validated.session.id,
            schedule.input.expectedSessionRevision == validated.session.revision
          else {
            // A non-nil schedule is durable continuity data, not optional
            // decoration. Never publish a fresh Choice snapshot as healthy
            // while its paired schedule is malformed or belongs to another
            // session/revision.
            hasVerifiedChoiceContinuity = false
            choiceLoopContinuityState = .needsYou(.invalidContract)
            return
          }
          if choiceReminderDateTime.isEmpty, choiceReminderTimeZone.isEmpty,
            choiceReminderListId.isEmpty, choiceReminderCount.isEmpty
          {
            hydrateChoiceReminderSchedule(schedule)
          }
        }
        if let target = choiceDComposerTarget,
          validated.session.state != "active"
            || validated.session.id != target.sessionId
            || validated.activeChoiceSet?.id != target.choiceSetId
            || validated.session.revision != target.revision
        {
          invalidateChoiceDComposerTarget()
        }
        retirePrivateChoiceIntakeBodies(after: validated)
        choiceLoopSnapshot = validated
        choiceMarkdownReceiptCleanupAvailable = cleanupAvailable
        // A durable confirmation is recovery metadata, not a fresh review
        // preview. Only `choice.confirm.prepare` may populate the actionable
        // card for the exact current Active revision.
        choiceConfirmationPreview = nil
        hasVerifiedChoiceContinuity = true
        choiceLoopContinuityState =
          validated.session.state == "blocked" ? .needsYou(.blocked) : .current
        if authenticatedOwnerReturn,
          ["softIdle", "staleReview"].contains(validated.session.state)
            || (validated.session.state == "refining"
              && validated.pendingRefinementOperation?.isOwnerResume == true)
        {
          await resumeChoiceIfNeeded(expectedGeneration: expectedGeneration, snapshot: validated)
        }
      } catch CoreClientError.remote(let code, _) where code == -32_025 {
        guard expectedGeneration == runtimeGeneration, sequence == choiceLoopRefreshSequence,
          !Task.isCancelled
        else { return }
        hasVerifiedChoiceContinuity = false
        choiceLoopContinuityState = .needsYou(.clockUncertain)
      } catch CoreClientError.remote(let code, _) where code == -32_026 {
        guard expectedGeneration == runtimeGeneration, sequence == choiceLoopRefreshSequence,
          !Task.isCancelled
        else { return }
        hasVerifiedChoiceContinuity = false
        choiceLoopContinuityState = .needsYou(.refreshRequired)
      } catch {
        guard expectedGeneration == runtimeGeneration, sequence == choiceLoopRefreshSequence,
          !Task.isCancelled
        else { return }
        // Keep the last valid snapshot. An invalid current response is a
        // fail-closed continuity incident, not proof that the session vanished.
        hasVerifiedChoiceContinuity = false
        choiceLoopContinuityState = .needsYou(.invalidContract)
      }
    } catch CoreClientError.remote(let code, _) where code == -32_025 {
      guard expectedGeneration == runtimeGeneration, sequence == choiceLoopRefreshSequence,
        !Task.isCancelled
      else { return }
      hasVerifiedChoiceContinuity = false
      choiceLoopContinuityState = .needsYou(.clockUncertain)
    } catch {
      guard expectedGeneration == runtimeGeneration, sequence == choiceLoopRefreshSequence,
        !Task.isCancelled
      else { return }
      // Keep the last valid snapshot and expose a non-blocking recovery path.
      // In particular, do not feed this into errorMessage, which is displayed
      // as dismissible local-operation feedback and can steal the next action.
      hasVerifiedChoiceContinuity = false
      choiceLoopContinuityState = .needsYou(.readFailed)
    }
  }

  /// Exactly one automatic authenticated owner-return attempt per observed
  /// session state and revision. A model failure gets a newer idle/stale
  /// revision, which may receive one later foreground attempt; repeated reads
  /// of that exact revision can never create a retry loop.
  private func resumeChoiceIfNeeded(
    expectedGeneration: UInt64, snapshot observed: ChoiceLoopSnapshot
  ) async {
    let key =
      "\(expectedGeneration)|\(observed.session.id)|\(observed.session.state)|\(observed.session.revision)"
    guard !automaticResumeAttemptedStates.contains(key), !isBusy,
      expectedGeneration == runtimeGeneration,
      let snapshot = choiceLoopSnapshot,
      snapshot.session.id == observed.session.id,
      snapshot.session.revision == observed.session.revision,
      snapshot.session.state == observed.session.state,
      ["softIdle", "staleReview"].contains(snapshot.session.state)
        || (snapshot.session.state == "refining"
          && snapshot.pendingRefinementOperation?.isOwnerResume == true)
    else { return }
    automaticResumeAttemptedStates.insert(key)
    do {
      let proof = try await currentEnabledProof(expectedGeneration: expectedGeneration)
      let next = try await core.resumeChoice(proof: proof).validated()
      guard expectedGeneration == runtimeGeneration,
        next.session.id == observed.session.id, next.session.state == "refining"
      else { return }
      automaticResumeAttemptedStates.insert(
        "\(expectedGeneration)|\(next.session.id)|\(next.session.state)|\(next.session.revision)")
      adoptCommittedChoiceLoopSnapshot(next)
      awaitChoiceRefinementResult(
        expectedGeneration: expectedGeneration,
        sessionID: observed.session.id)
    } catch {
      guard expectedGeneration == runtimeGeneration else { return }
      reconcileOwnerResumeAfterAmbiguousTransport(
        expectedGeneration: expectedGeneration, sessionID: observed.session.id)
    }
  }

  /// An accepted resume can lose its RPC response after the Host persists the
  /// operation. Re-read from an independent task and resume result polling
  /// only for that exact Store-owned operation; no second resume RPC is ever
  /// issued for the same owner return.
  private func reconcileOwnerResumeAfterAmbiguousTransport(
    expectedGeneration: UInt64, sessionID: String
  ) {
    Task { [weak self] in
      guard let self else { return }
      await self.refreshChoiceLoopContinuity(expectedGeneration: expectedGeneration)
      guard expectedGeneration == self.runtimeGeneration,
        let snapshot = self.choiceLoopSnapshot,
        snapshot.session.id == sessionID,
        snapshot.session.state == "refining",
        snapshot.pendingRefinementOperation?.isOwnerResume == true
      else { return }
      self.awaitChoiceRefinementResult(
        expectedGeneration: expectedGeneration, sessionID: sessionID)
    }
  }

  /// Adopts a Store-confirmed Choice transition and invalidates every older
  /// continuity read. A late dashboard refresh must not replace a newer
  /// select, cancel, or begin result with an earlier session revision.
  private func adoptCommittedChoiceLoopSnapshot(_ snapshot: ChoiceLoopSnapshot) {
    let priorChoiceRevision = choiceLoopSnapshot?.session.revision
    choiceLoopRefreshSequence &+= 1
    if choiceLoopSnapshot?.session.id != nil
      && choiceLoopSnapshot?.session.id != snapshot.session.id
    {
      clearChoiceReminderScheduleDraft()
    }
    if let target = choiceDComposerTarget,
      snapshot.session.state != "active"
        || snapshot.session.id != target.sessionId
        || snapshot.activeChoiceSet?.id != target.choiceSetId
        || snapshot.session.revision != target.revision
    {
      invalidateChoiceDComposerTarget()
    }
    retirePrivateChoiceIntakeBodies(after: snapshot)
    choiceLoopSnapshot = snapshot
    if choiceIMessageReplyPreview?.previewRevision != snapshot.session.revision
      || snapshot.session.state != "active"
    {
      choiceIMessageReplyPreview = nil
      choiceIMessageReplyStatus = nil
    }
    if priorChoiceRevision != snapshot.session.revision {
      choiceReminderScheduleIsVisible = true
    }
    // This is a transition result, not an independently authenticated
    // receipt-cleanup availability read.  Never carry an availability bit
    // from a previous cancelled session into a newer snapshot.
    choiceMarkdownReceiptCleanupAvailable = false
    choiceConfirmationPreview = nil
    hasVerifiedChoiceContinuity = true
    choiceLoopContinuityState = .current
  }

  /// Bounded local polling is only a continuity read: the accepted Host
  /// operation owns generation and result commits. It never starts a second
  /// model turn, writes an effect, or presents a blocking alert.
  private func awaitInitialChoiceResult(
    expectedGeneration: UInt64, sessionID: String, prepareIMessageReply: Bool = false
  ) {
    choiceResultTask?.cancel()
    choiceResultTask = Task { [weak self] in
      guard let self else { return }
      for _ in 0..<240 {
        guard !Task.isCancelled, expectedGeneration == runtimeGeneration else { return }
        try? await Task.sleep(for: .seconds(1))
        guard !Task.isCancelled, expectedGeneration == runtimeGeneration else { return }
        await refreshChoiceLoopContinuity(expectedGeneration: expectedGeneration)
        guard !Task.isCancelled, expectedGeneration == runtimeGeneration else { return }
        guard let snapshot = choiceLoopSnapshot, snapshot.session.id == sessionID else { return }
        if snapshot.session.state != "interpreting" {
          if prepareIMessageReply, snapshot.session.state == "active" {
            await prepareCurrentChoiceIMessageReply(expectedGeneration: expectedGeneration)
          }
          choiceResultTask = nil
          return
        }
      }
      guard expectedGeneration == runtimeGeneration,
        choiceLoopSnapshot?.session.id == sessionID,
        choiceLoopSnapshot?.session.state == "interpreting"
      else { return }
      choiceLoopContinuityState = .needsYou(.readFailed)
      choiceResultTask = nil
    }
  }

  private func performDashboardRefresh(authenticatedHomeForeground: Bool) async {
    let generation = runtimeGeneration
    runtimeRecoveryState = .recovering
    for (attempt, delay) in [Duration.zero, .milliseconds(250), .seconds(1)].enumerated() {
      guard !Task.isCancelled, generation == runtimeGeneration else { return }
      if delay > .zero {
        try? await Task.sleep(for: delay)
        guard !Task.isCancelled, generation == runtimeGeneration else { return }
      }
      do {
        let accountReady = try await withCoreGenerationFence {
          var accountReady = true
          let identity = try await provisionBrokerTrust()
          let dashboard = try await core.dashboard()
          let protected = try await readProtectedRuntime()
          let runtime: RuntimeControl
          if let protected {
            if protected.authorization.enabled, pendingRuntimeIntent != false {
              try await ensureCodexReady(coreIdentity: identity)
            }
            runtime = try await core.recoverRuntime(
              protected.authorization,
              brokerReceipt: protected.receipt
            )
          } else {
            runtime = dashboard.runtime
          }
          guard generation == runtimeGeneration, switchTask == nil else {
            throw CoreClientError.requestCancelled
          }
          try applyDashboard(dashboard)
          await refreshPersonaStatus(expectedGeneration: generation)
          protectedRuntime = protected
          let protectedMatchesRuntime =
            protected.map {
              $0.authorization.enabled == runtime.enabled
                && $0.authorization.revision == runtime.revision
                && $0.authorization.updatedAtMs == runtime.updatedAtMs
            } ?? false
          if protectedMatchesRuntime || (protected == nil && isExplicitDefaultOff(runtime)) {
            confirmedEnabled = runtime.enabled
            enabled = runtime.enabled
            authoritativeStateCertain = true
            if let pendingRuntimeIntent {
              desiredEnabled = pendingRuntimeIntent
            } else {
              desiredEnabled = runtime.enabled
            }
            runtimeDisplayState =
              runtime.enabled && desiredEnabled
              ? .turningOn
              : displayState(forAuthoritativeEnabled: runtime.enabled, desired: desiredEnabled)
            if runtime.enabled == desiredEnabled {
              pendingRuntimeIntent = nil
            }
          } else {
            authoritativeStateCertain = false
            runtimeDisplayState = .unknown
          }
          // Refresh the bounded Host-owned model catalog before an owner
          // foreground return can consume a soft-idle/stale resume. The idle
          // window is intentionally longer than catalog eligibility; doing
          // this after `choice.resume` would permanently spend that exact
          // owner-return revision on a predictable stale-catalog failure.
          if runtime.enabled, protectedMatchesRuntime, desiredEnabled {
            accountReady = try await refreshRecoveredAccountAndModels(
              expectedGeneration: generation)
            try requireCurrentOnGeneration(generation)
          }
          if runtime.enabled, coreTerminationEvents != nil {
            let fencedDashboard = try await core.dashboard()
            try requireCurrentOnGeneration(generation)
            try applyDashboard(fencedDashboard)
            try await restoreDurableConnections(expectedGeneration: generation)
            try requireCurrentOnGeneration(generation)
          }
          await refreshChoiceLoopContinuity(
            expectedGeneration: generation,
            authenticatedOwnerReturn: authenticatedHomeForeground && attempt == 0
              && runtime.enabled && protectedMatchesRuntime && desiredEnabled && accountReady)
          if choiceIMessageReplyPreview == nil,
            choiceLoopSnapshot?.session.state == "active", iMessageIsConnected
          {
            await prepareCurrentChoiceIMessageReply(expectedGeneration: generation)
          }
          guard generation == runtimeGeneration, switchTask == nil else {
            throw CoreClientError.requestCancelled
          }
          return accountReady
        }
        if enabled, desiredEnabled {
          if accountReady {
            onRequiresAccountSetup = false
            try finishRecoveredOn(expectedGeneration: generation)
          } else {
            enterAccountSetupRequired()
          }
        } else {
          runtimeRecoveryState = .ready
        }
        errorMessage = nil
        return
      } catch CoreClientError.requestCancelled {
        return
      } catch {
        guard generation == runtimeGeneration, switchTask == nil, !Task.isCancelled else { return }
        authoritativeStateCertain = false
        runtimeDisplayState = .unknown
        let processFailure: Bool
        switch error as? CoreClientError {
        case .processUnavailable, .processTerminated, .requestTimedOut:
          processFailure = true
        default:
          processFailure = false
        }
        let mayRetry = processFailure || (desiredEnabled && confirmedEnabled)
        guard mayRetry else {
          runtimeRecoveryState = .ready
          errorMessage = userMessage(for: error)
          return
        }
        brokerTrustCoreInstanceNonce = nil
        codexReadyCoreInstanceNonce = nil
        guard shutdownCore() else {
          pauseAfterRecoveryFailure()
          errorMessage =
            "Need you: OpenOpen could not verify that Core stopped. Provider activity is unknown."
          return
        }
        clearLiveConnectionState(status: "paused")
        clearTransientModelSetup()
        if attempt == 2 {
          pauseAfterRecoveryFailure()
          errorMessage = RuntimeRecoveryState.paused.message ?? userMessage(for: error)
          return
        }
      }
    }
  }

  public func requestEnabled(_ requested: Bool) {
    var coreInterruptionFailed = false
    let reminderTaskToQuiesce = requested ? nil : choiceReminderWriteTask
    let mustInterruptActiveCore =
      !requested
      && (dashboardRefreshTask != nil || heroTask != nil || channelTask != nil
        || coreRecoveryTask != nil || choiceReminderWriteTask != nil
        || runtimeRecoveryState == .recovering || runtimeRecoveryState == .paused || isBusy
        || offCoreInterruptionFailed
        || onRequiresReplacementCoreRestoration)
    runtimeGeneration &+= 1
    hasVerifiedChoiceContinuity = false
    desiredEnabled = requested
    pendingRuntimeIntent = requested
    if requested {
      offCoreInterruptionFailed = false
    }
    if !requested {
      // Off is a hard local-intent boundary. Preserve the owner-authored
      // draft, but revoke the transient D tuple so it cannot survive a new
      // protected runtime generation or fall through to `choice.begin`.
      invalidateChoiceDComposerTarget()
      cancelChannelFailureAcknowledgements()
      recoveringTerminalChannelFailure = false
      dashboardRefreshTask?.cancel()
      dashboardRefreshTask = nil
      dashboardRefreshIdentifier = nil
      coreRecoveryTask?.cancel()
      coreRecoveryTask = nil
      heroTask?.cancel()
      heroTask = nil
      channelTask?.cancel()
      channelTask = nil
      choiceResultTask?.cancel()
      choiceResultTask = nil
      choiceReminderWriteTask?.cancel()
      // Keep the exact task handle until it has either recorded a definite
      // pre-commit abort or crossed into read-only ambiguous recovery. Core
      // must remain alive for that durable transition before protected Off.
      if reminderTaskToQuiesce == nil {
        choiceReminderWriteTask = nil
        choiceReminderWriteTaskID = nil
      }
      connectedChannels.removeAll()
      channelListenerFeedback.removeAll()
      latestChannelMissionEvent = nil
      iMessageStatus = "disconnected"
      discordStatus = "disconnected"
      discordSetup = nil
      discordPairingCandidate = nil
      discordSetupFeedback = nil
      discardDiscordTokenDraft()
      runtimeRecoveryState = .ready
      onRequiresAccountSetup = false
      if mustInterruptActiveCore, reminderTaskToQuiesce == nil,
        !interruptActiveCoreForOff()
      {
        coreInterruptionFailed = true
      }
    }
    if requested {
      runtimeDisplayState =
        authoritativeStateCertain && enabled && !offRequiresReplacementCoreProvisioning
          && !onRequiresReplacementCoreRestoration && !onRequiresAccountSetup
          && runtimeRecoveryState == .ready && requiredAccountAndModelReady
        ? .on : .turningOn
    } else {
      runtimeDisplayState = authoritativeStateCertain && !enabled ? .off : .turningOff
    }
    if coreInterruptionFailed {
      runtimeDisplayState = .unknown
      return
    }
    guard switchTask == nil else { return }
    switchTask = Task { [weak self] in
      guard let self else { return }
      if let reminderTaskToQuiesce {
        await reminderTaskToQuiesce.value
        guard self.interruptActiveCoreForOff() else {
          self.switchTask = nil
          return
        }
      }
      await self.reconcileEnabledState()
    }
  }

  private func interruptActiveCoreForOff() -> Bool {
    if shutdownCore() {
      // The next Core is a new process generation and therefore has no
      // in-memory broker enrollment. The exact old Core is already proven
      // stopped before protected Off proceeds.
      brokerTrustCoreInstanceNonce = nil
      codexReadyCoreInstanceNonce = nil
      offCoreInterruptionFailed = false
      offRequiresReplacementCoreProvisioning = true
      onRequiresReplacementCoreRestoration = false
      onRequiresAccountSetup = false
      return true
    }
    authoritativeStateCertain = false
    runtimeDisplayState = .unknown
    errorMessage = "OpenOpen could not verify that the previous Core stopped."
    offCoreInterruptionFailed = true
    onRequiresReplacementCoreRestoration = true
    return false
  }

  public func updateEnabled(_ requested: Bool) async {
    requestEnabled(requested)
    await switchTask?.value
  }

  private func reconcileEnabledState() async {
    while !runtimeIsConverged(with: desiredEnabled) {
      let attemptGeneration = runtimeGeneration
      let completingQuiescedOff = offRequiresReplacementCoreProvisioning
      let target = completingQuiescedOff ? false : desiredEnabled
      // Any protected transition toward On needs one complete, generation-
      // fenced product restoration. A persisted Off state does not carry the
      // in-memory supersession latch, and protected Off itself terminates the
      // leased Core, so limiting this gate to the quiesced supersession path
      // could publish false On after an ordinary restart or Off -> On.
      let restoringOn =
        target && (coreTerminationEvents != nil || onRequiresReplacementCoreRestoration)
      if !target, offCoreInterruptionFailed { break }
      if restoringOn {
        // The protected state may already say On, but the exact Core and both
        // listeners were quiesced. Keep every model/provider entry disabled
        // from the first replacement RPC until complete restoration.
        runtimeRecoveryState = .recovering
        runtimeDisplayState = .turningOn
      }
      var brokerAccepted: BrokerRuntimeState?
      var brokerApplyAttempted = false
      do {
        let preparedOff: RuntimeControlAuthorization?
        let identity: CoreEffectIdentity
        if completingQuiescedOff {
          identity = try await provisionBrokerTrust()
          preparedOff = try await core.prepareRuntime(false)
        } else {
          preparedOff = target ? nil : try await core.prepareRuntime(false)
          identity = try await provisionBrokerTrust()
        }
        if target {
          try await ensureCodexReady(coreIdentity: identity)
        }
        if target, let protected = try await readProtectedRuntime() {
          let recovered = try await core.recoverRuntime(
            protected.authorization,
            brokerReceipt: protected.receipt
          )
          protectedRuntime = protected
          confirmedEnabled = recovered.enabled
          enabled = recovered.enabled
          authoritativeStateCertain = true
          runtimeDisplayState =
            recovered.enabled && !restoringOn ? .on : .turningOn
          if recovered.enabled, restoringOn {
            let restored = try await restoreEnabledRuntime(
              expectedGeneration: attemptGeneration)
            if !restored { break }
          }
          if runtimeIsConverged(with: desiredEnabled) { continue }
        }
        let authorization: RuntimeControlAuthorization
        if let preparedOff {
          authorization = preparedOff
        } else {
          authorization = try await core.prepareRuntime(true)
        }
        brokerApplyAttempted = true
        let receipt = try await broker.apply(authorization)
        let accepted = BrokerRuntimeState(authorization: authorization, receipt: receipt)
        brokerAccepted = accepted
        protectedRuntime = accepted
        authoritativeStateCertain = true
        if !target {
          enabled = false
          runtimeDisplayState = .off
          brokerTrustCoreInstanceNonce = nil
          codexReadyCoreInstanceNonce = nil
        }
        let control = try await commitOrRecover(accepted)
        confirmedEnabled = control.enabled
        enabled = control.enabled
        authoritativeStateCertain = true
        if completingQuiescedOff, !control.enabled {
          offRequiresReplacementCoreProvisioning = false
          onRequiresReplacementCoreRestoration = desiredEnabled
        }
        if !control.enabled {
          // This text describes only the transient recovery posture. Clear it
          // after the protected broker/Core transaction has durably confirmed
          // Off; the typed incident and audit history remain untouched.
          channelFailureFeedback = nil
          // `mission.runtime.prepare(false)` first retires any unreceipted
          // Choice journal while its On proof is still current. Re-read that
          // terminal state after the durable Off commit so a stale local card
          // cannot offer a publication-only recovery that is no longer legal.
          await refreshChoiceLoopContinuity(expectedGeneration: runtimeGeneration)
        }
        runtimeDisplayState =
          control.enabled && restoringOn
          ? .turningOn
          : displayState(
            forAuthoritativeEnabled: control.enabled,
            desired: desiredEnabled
          )
        if control.enabled, restoringOn {
          let restored = try await restoreEnabledRuntime(
            expectedGeneration: attemptGeneration)
          if !restored { break }
        }
        errorMessage = nil
        runtimeRecoveryState = .ready
        if control.enabled {
          // Local input is not ready until this exact Core generation proves
          // either an empty Choice Store or a valid current snapshot. Channel
          // incidents remain independent and cannot stand in for this read.
          await refreshChoiceLoopContinuity(expectedGeneration: attemptGeneration)
        }
        if control.enabled, !loginItemRegistered {
          do {
            try registerLoginItem()
            loginItemRegistered = true
          } catch {
            // Core is authoritative once it accepted On. Login-item setup is a
            // separate convenience and must never make the UI claim Core is Off.
            errorMessage = userMessage(for: error)
          }
        }
      } catch {
        // Once the protected broker has accepted either state, Core must catch
        // up before any model route is exposed. In particular, accepted Off is
        // never visually or operationally reverted to On.
        if let brokerAccepted {
          protectedRuntime = brokerAccepted
          enabled = brokerAccepted.authorization.enabled
          authoritativeStateCertain = true
          runtimeDisplayState = brokerAccepted.authorization.enabled ? .unknown : .off
        } else if attemptGeneration != runtimeGeneration {
          continue
        } else if brokerApplyAttempted {
          authoritativeStateCertain = false
          runtimeDisplayState = .unknown
        } else if restoringOn {
          runtimeDisplayState = .unknown
        } else {
          runtimeDisplayState = authoritativeStateCertain ? (enabled ? .on : .off) : .unknown
        }
        if attemptGeneration != runtimeGeneration { continue }
        if restoringOn {
          brokerTrustCoreInstanceNonce = nil
          codexReadyCoreInstanceNonce = nil
          if shutdownCore() {
            offRequiresReplacementCoreProvisioning = true
            onRequiresReplacementCoreRestoration = false
            pauseAfterRecoveryFailure()
            errorMessage = userMessage(for: error)
          } else {
            offCoreInterruptionFailed = true
            pauseAfterRecoveryFailure()
            errorMessage =
              "Need you: OpenOpen could not verify that Core stopped. Provider activity is unknown."
          }
          break
        }
        errorMessage = userMessage(for: error)
        break
      }
    }
    if runtimeIsConverged(with: desiredEnabled) {
      pendingRuntimeIntent = nil
    }
    switchTask = nil
  }

  private func runtimeIsConverged(with target: Bool) -> Bool {
    confirmedEnabled == target && enabled == target
      && protectedRuntime?.authorization.enabled == target
      && runtimeDisplayState == (target ? .on : .off)
      && runtimeRecoveryState == .ready
      && !offRequiresReplacementCoreProvisioning
      && !onRequiresReplacementCoreRestoration
      && !onRequiresAccountSetup
      && (!target || requiredAccountAndModelReady)
  }

  private func restoreEnabledRuntime(expectedGeneration: UInt64) async throws -> Bool {
    runtimeRecoveryState = .recovering
    runtimeDisplayState = .turningOn
    let accountReady = try await withCoreGenerationFence {
      let dashboard = try await core.dashboard()
      try requireCurrentOnGeneration(expectedGeneration)
      try applyDashboard(dashboard)
      try await restoreDurableConnections(expectedGeneration: expectedGeneration)
      let accountReady = try await refreshRecoveredAccountAndModels(
        expectedGeneration: expectedGeneration)
      try requireCurrentOnGeneration(expectedGeneration)
      return accountReady
    }
    onRequiresReplacementCoreRestoration = false
    guard accountReady else {
      enterAccountSetupRequired()
      return false
    }
    onRequiresAccountSetup = false
    // Listener restoration and account/catalog recovery can overlap an older
    // dashboard continuity read.  Re-read the durable Choice session only
    // after those prerequisites have settled, so a healthy empty Store is not
    // left looking like an unverified local-input failure.  Conversely, a
    // failed read stays fail-closed and cannot publish a usable local entry.
    await refreshChoiceLoopContinuity(expectedGeneration: expectedGeneration)
    try requireCurrentOnGeneration(expectedGeneration)
    guard hasVerifiedChoiceContinuity else {
      throw CoreClientError.contractViolation(
        "OpenOpen cannot publish On before Choice continuity is verified."
      )
    }
    try finishRecoveredOn(expectedGeneration: expectedGeneration)
    return true
  }

  private func finishRecoveredOn(expectedGeneration: UInt64) throws {
    try requireCurrentOnGeneration(expectedGeneration)
    guard confirmedEnabled, enabled, protectedRuntime?.authorization.enabled == true,
      !offRequiresReplacementCoreProvisioning, !onRequiresReplacementCoreRestoration,
      !onRequiresAccountSetup, requiredAccountAndModelReady
    else {
      throw CoreClientError.contractViolation(
        "OpenOpen cannot publish On before protected runtime restoration completes."
      )
    }
    runtimeRecoveryState = .ready
    runtimeDisplayState = .on
    // A completed On restoration has re-established the exact protected
    // runtime, account/model and listener prerequisites. The durable incident
    // remains visible; only its now-stale paused/restoring status is cleared.
    channelFailureFeedback = nil
  }

  private func displayState(
    forAuthoritativeEnabled authoritativeEnabled: Bool,
    desired: Bool
  ) -> RuntimeDisplayState {
    if authoritativeEnabled == desired { return authoritativeEnabled ? .on : .off }
    return desired ? .turningOn : .turningOff
  }

  private func isExplicitDefaultOff(_ runtime: RuntimeControl) -> Bool {
    !runtime.enabled && runtime.revision == 0 && runtime.updatedAtMs == 0
  }

  private func commitOrRecover(_ state: BrokerRuntimeState) async throws -> RuntimeControl {
    do {
      return try await core.commitRuntime(
        state.authorization,
        brokerReceipt: state.receipt
      )
    } catch {
      var lastError: Error = error
      for delay in [50, 150, 300] {
        do {
          // Protected Off terminates the exact leased Core before the broker
          // persists Off. A replacement Core must receive the pinned broker
          // enrollment before it can verify and recover that checkpoint.
          _ = try await provisionBrokerTrust()
          return try await core.recoverRuntime(
            state.authorization,
            brokerReceipt: state.receipt
          )
        } catch {
          lastError = error
          try? await Task.sleep(for: .milliseconds(delay))
        }
      }
      throw lastError
    }
  }

  private func currentEnabledProof(
    expectedGeneration: UInt64, prepareModelRuntime: Bool = true
  ) async throws -> BrokerRuntimeState {
    try requireCurrentOnGeneration(expectedGeneration)
    let identity = try await provisionBrokerTrust()
    if prepareModelRuntime {
      try await ensureCodexReady(coreIdentity: identity)
    }
    try requireCurrentOnGeneration(expectedGeneration)
    guard let protected = try await readProtectedRuntime() else {
      if expectedGeneration == runtimeGeneration {
        authoritativeStateCertain = false
        runtimeDisplayState = .unknown
      }
      throw CoreClientError.contractViolation("OpenOpen runtime proof is unavailable.")
    }
    guard protected.authorization.enabled else {
      if expectedGeneration == runtimeGeneration {
        protectedRuntime = protected
        confirmedEnabled = false
        enabled = false
        authoritativeStateCertain = true
        runtimeDisplayState = .off
      }
      throw CoreClientError.contractViolation("OpenOpen is off.")
    }
    try requireCurrentOnGeneration(expectedGeneration)
    let runtime = try await core.recoverRuntime(
      protected.authorization,
      brokerReceipt: protected.receipt
    )
    try requireCurrentOnGeneration(expectedGeneration)
    guard runtime.enabled,
      runtime.revision == protected.authorization.revision,
      runtime.updatedAtMs == protected.authorization.updatedAtMs
    else {
      enabled = true
      authoritativeStateCertain = true
      runtimeDisplayState = .unknown
      throw CoreClientError.contractViolation("OpenOpen runtime proof is not synchronized.")
    }
    protectedRuntime = protected
    confirmedEnabled = true
    enabled = true
    authoritativeStateCertain = true
    // Protected runtime On is necessary but is not sufficient to publish a
    // converged product state while the exact Core generation is restoring its
    // approved listeners/account/model surface. Keep the visible state honest
    // until the outer fenced restoration completes every dependency.
    if runtimeRecoveryState != .paused {
      runtimeDisplayState =
        runtimeRecoveryState == .recovering || runtimeRecoveryState == .awaitingAccount
          || onRequiresReplacementCoreRestoration || onRequiresAccountSetup
        ? .turningOn : .on
    }
    return protected
  }

  private func requireCurrentOnGeneration(_ generation: UInt64) throws {
    guard generation == runtimeGeneration, desiredEnabled else {
      throw CoreClientError.contractViolation("The runtime changed while authorizing the request.")
    }
  }

  private func provisionBrokerTrust() async throws -> CoreEffectIdentity {
    let identity = try await core.effectIdentity()
    if codexReadyCoreInstanceNonce != identity.coreInstanceNonce {
      codexReadyCoreInstanceNonce = nil
    }
    if brokerTrustCoreInstanceNonce != identity.coreInstanceNonce {
      brokerTrustCoreInstanceNonce = nil
      let anchor = try await broker.provision(coreIdentity: identity)
      let enrollment = try await core.signBrokerEnrollment(anchor)
      try await core.installBrokerEnrollment(enrollment)
      brokerTrustCoreInstanceNonce = identity.coreInstanceNonce
    }
    return identity
  }

  private func ensureCodexReady(coreIdentity identity: CoreEffectIdentity) async throws {
    if codexReadyCoreInstanceNonce == identity.coreInstanceNonce { return }
    _ = try await broker.prepareCodexRuntimeHome()
    let codexPID = try await core.prepareCodexRuntime()
    do {
      try await core.bindCodexCandidateForBroker()
      let lease = try await broker.acquireCoreLease(
        coreIdentity: identity, codexProcessIdentifier: codexPID
      )
      try await core.installCoreLease(lease)
      try await core.initializeCodexRuntime()
      codexReadyCoreInstanceNonce = identity.coreInstanceNonce
    } catch {
      codexReadyCoreInstanceNonce = nil
      try? await core.abortCodexCandidate()
      throw error
    }
  }

  private func ensureLoginCodexReady(coreIdentity identity: CoreEffectIdentity) async throws {
    codexReadyCoreInstanceNonce = nil
    _ = try await broker.prepareCodexRuntimeHome()
    let codexPID = try await core.prepareCodexLoginRuntime()
    do {
      try await core.bindCodexCandidateForBroker()
      let lease = try await broker.acquireCoreLease(
        coreIdentity: identity, codexProcessIdentifier: codexPID
      )
      try await core.installCoreLease(lease)
      try await core.initializeCodexRuntime()
    } catch {
      try? await core.abortCodexCandidate()
      throw error
    }
  }

  private func readProtectedRuntime() async throws -> BrokerRuntimeState? {
    let challenge = try await core.runtimeChallenge()
    return try await broker.status(challenge: challenge)
  }

  private func startCoreLifecycleMonitoring() {
    guard let coreTerminationEvents else { return }
    coreLifecycleTask = Task { [weak self, coreTerminationEvents] in
      for await event in coreTerminationEvents {
        guard let self else { return }
        handleCoreTermination(event)
      }
    }
  }

  private func handleCoreTermination(_ event: CoreTerminationEvent) {
    guard event.generation > lastCoreTerminationGeneration else { return }
    lastCoreTerminationGeneration = event.generation
    brokerTrustCoreInstanceNonce = nil
    codexReadyCoreInstanceNonce = nil
    if runtimeRecoveryState == .paused {
      clearLiveConnectionState(status: "paused")
      return
    }
    if runtimeRecoveryState == .recovering,
      coreRecoveryTask != nil || dashboardRefreshTask != nil
    {
      // A replacement Core may itself terminate while the current bounded
      // recovery or startup-restore call is awaiting it. The in-flight call
      // will fail and consume one of this same episode's three attempts. Do
      // not start a second recovery task or reset the budget.
      channelTask?.cancel()
      channelTask = nil
      clearLiveConnectionState(status: "paused")
      clearTransientModelSetup()
      authoritativeStateCertain = false
      runtimeDisplayState = .turningOn
      return
    }
    guard desiredEnabled, confirmedEnabled else {
      clearLiveConnectionState(status: "disconnected")
      return
    }

    beginCoreRecovery(shuttingDownCurrentCore: false)
  }

  private func beginCoreRecovery(
    shuttingDownCurrentCore: Bool,
    dueToTerminalChannelFailure: Bool = false
  ) {
    guard desiredEnabled, confirmedEnabled, coreRecoveryTask == nil else { return }
    recoveringTerminalChannelFailure = dueToTerminalChannelFailure

    // A startup restore may have partially activated a listener before it
    // failed. Close that exact Core generation synchronously before starting
    // the bounded replacement loop; cleanup RPCs must never target or launch a
    // later generation.
    if shuttingDownCurrentCore, !shutdownCore() {
      pauseAfterRecoveryFailure()
      publishCoreRecoveryFailure(
        "Need you: OpenOpen could not verify that Core stopped. Provider activity is unknown."
      )
      recoveringTerminalChannelFailure = false
      return
    }
    cancelChannelFailureAcknowledgements()
    runtimeGeneration &+= 1
    hasVerifiedChoiceContinuity = false
    let recoveryGeneration = runtimeGeneration
    switchTask?.cancel()
    switchTask = nil
    heroTask?.cancel()
    heroTask = nil
    channelTask?.cancel()
    channelTask = nil
    choiceResultTask?.cancel()
    choiceResultTask = nil
    coreRecoveryTask?.cancel()
    brokerTrustCoreInstanceNonce = nil
    codexReadyCoreInstanceNonce = nil
    connectedChannels.removeAll()
    clearLiveConnectionState(status: "paused")
    clearTransientModelSetup()
    authoritativeStateCertain = false
    runtimeDisplayState = .turningOn
    runtimeRecoveryState = .recovering
    onRequiresAccountSetup = false
    errorMessage = nil
    if dueToTerminalChannelFailure {
      channelFailureFeedback =
        "A background result failed safely. OpenOpen is restoring its verified local runtime without retrying that work."
    }

    coreRecoveryTask = Task { [weak self] in
      await self?.recoverTerminatedCore(expectedGeneration: recoveryGeneration)
    }
  }

  private func recoverTerminatedCore(expectedGeneration: UInt64) async {
    var lastError: Error = CoreClientError.processTerminated
    for delay in [Duration.zero, .milliseconds(250), .seconds(1)] {
      guard !Task.isCancelled, expectedGeneration == runtimeGeneration, desiredEnabled else {
        return
      }
      if delay > .zero {
        try? await Task.sleep(for: delay)
        guard !Task.isCancelled else { return }
      }
      do {
        let accountReady = try await withCoreGenerationFence {
          let identity = try await provisionBrokerTrust()
          guard let protected = try await readProtectedRuntime(), protected.authorization.enabled
          else {
            throw CoreClientError.contractViolation(
              "OpenOpen protected runtime is not enabled for recovery."
            )
          }
          try await ensureCodexReady(coreIdentity: identity)
          let runtime = try await core.recoverRuntime(
            protected.authorization,
            brokerReceipt: protected.receipt
          )
          guard runtime.enabled,
            runtime.revision == protected.authorization.revision,
            runtime.updatedAtMs == protected.authorization.updatedAtMs
          else {
            throw CoreClientError.contractViolation(
              "OpenOpen runtime proof is not synchronized after Core recovery."
            )
          }
          let dashboard = try await core.dashboard()
          try requireCurrentOnGeneration(expectedGeneration)
          try applyDashboard(dashboard)
          protectedRuntime = protected
          confirmedEnabled = true
          enabled = true
          authoritativeStateCertain = true
          runtimeDisplayState = .turningOn
          try await restoreDurableConnections(expectedGeneration: expectedGeneration)
          let accountReady = try await refreshRecoveredAccountAndModels(
            expectedGeneration: expectedGeneration)
          try requireCurrentOnGeneration(expectedGeneration)
          return accountReady
        }
        guard accountReady else {
          enterAccountSetupRequired()
          recoveringTerminalChannelFailure = false
          coreRecoveryTask = nil
          return
        }
        onRequiresAccountSetup = false
        // A replacement Core is not ready merely because its protected
        // runtime, listeners, and account catalog recovered. Rebind durable
        // Choice continuity on this exact recovery generation before
        // publishing On, matching the startup-restore ordering above. A
        // failed read remains a typed nonblocking continuity incident, while
        // an Off/newer recovery generation makes the late response inert.
        await refreshChoiceLoopContinuity(expectedGeneration: expectedGeneration)
        try requireCurrentOnGeneration(expectedGeneration)
        guard hasVerifiedChoiceContinuity else {
          throw CoreClientError.contractViolation(
            "OpenOpen cannot publish On before Choice continuity is verified."
          )
        }
        try finishRecoveredOn(expectedGeneration: expectedGeneration)
        errorMessage = nil
        recoveringTerminalChannelFailure = false
        coreRecoveryTask = nil
        return
      } catch CoreClientError.requestCancelled {
        return
      } catch {
        guard expectedGeneration == runtimeGeneration, desiredEnabled, !Task.isCancelled else {
          return
        }
        lastError = error
        brokerTrustCoreInstanceNonce = nil
        codexReadyCoreInstanceNonce = nil
        guard shutdownCore() else {
          pauseAfterRecoveryFailure()
          publishCoreRecoveryFailure(
            "Need you: OpenOpen could not verify that Core stopped. Provider activity is unknown."
          )
          recoveringTerminalChannelFailure = false
          coreRecoveryTask = nil
          return
        }
        clearLiveConnectionState(status: "paused")
      }
    }

    guard expectedGeneration == runtimeGeneration, desiredEnabled else { return }
    pauseAfterRecoveryFailure()
    publishCoreRecoveryFailure(
      RuntimeRecoveryState.paused.message ?? userMessage(for: lastError)
    )
    recoveringTerminalChannelFailure = false
    coreRecoveryTask = nil
    _ = shutdownCore()
  }

  private func publishCoreRecoveryFailure(_ message: String) {
    if recoveringTerminalChannelFailure {
      errorMessage = nil
      channelFailureFeedback =
        "A background result failed safely. OpenOpen paused its local runtime without retrying or sending anything. Use the Global switch when you are ready."
    } else {
      errorMessage = message
    }
  }

  private func withCoreGenerationFence<Result: Sendable>(
    _ operation: () async throws -> Result
  ) async throws -> Result {
    let fence = try await core.beginCoreGenerationFence()
    let result: Result
    do {
      result = try await operation()
    } catch {
      _ = await core.closeCoreGenerationFence(fence)
      throw error
    }
    guard await core.closeCoreGenerationFence(fence) else {
      throw CoreClientError.processTerminated
    }
    return result
  }

  private func restoreDurableConnections(expectedGeneration: UInt64) async throws {
    guard choiceCoreConnectionsAvailable else {
      // PR1 preserves historical channel state only as dashboard data. It
      // never starts, repairs, polls, or faults a listener before the
      // separately reviewed channel stages own those routes.
      connectedChannels.removeAll()
      durablePairings.removeAll()
      iMessageStatus = "disconnected"
      updateDiscordConnectionFeedback("disconnected")
      channelListenerFeedback.removeAll()
      return
    }
    try requireCurrentOnGeneration(expectedGeneration)
    let storedIMessagePairing = try await core.channelPairing(.iMessage)
    let iMessagePairing =
      storedIMessagePairing?.imessage == nil
        && !core.permitsDeferredChannelTestRoutes ? nil : storedIMessagePairing
    try requireCurrentOnGeneration(expectedGeneration)
    let discordPairing =
      core.permitsDeferredChannelTestRoutes
      ? try await core.channelPairing(.discord) : nil
    try requireCurrentOnGeneration(expectedGeneration)

    var validatedPairings: [ChannelKind: ChannelPairing] = [:]
    if let iMessagePairing {
      try validateDurablePairing(iMessagePairing, expected: .iMessage)
      validatedPairings[.iMessage] = iMessagePairing
    }
    if let discordPairing {
      try validateDurablePairing(discordPairing, expected: .discord)
      validatedPairings[.discord] = discordPairing
    }
    durablePairings = validatedPairings

    var restored: Set<ChannelKind> = []
    if iMessagePairing != nil {
      do {
        let status = try (await core.channelStatus(.iMessage)).validated()
        try requireCurrentOnGeneration(expectedGeneration)
        if status.status == "connected" {
          iMessageStatus = status.status
        } else {
          guard ["disconnected", "faulted"].contains(status.status) else {
            throw CoreClientError.contractViolation(
              "Core returned an invalid iMessage listener state."
            )
          }
          _ = try await core.stopChannel(.iMessage)
          let prepareProof = try await currentEnabledProof(
            expectedGeneration: expectedGeneration,
            prepareModelRuntime: false
          )
          try await core.prepareIMessage(proof: prepareProof)
          let activationProof = try await currentEnabledProof(
            expectedGeneration: expectedGeneration,
            prepareModelRuntime: false
          )
          let activated = try (await core.activateIMessage(proof: activationProof)).validated()
          try requireCurrentOnGeneration(expectedGeneration)
          guard activated.status == "connected" else {
            if ["disconnected", "faulted"].contains(activated.status) {
              throw DurableChannelRestoreError.unavailable(.iMessage)
            }
            throw CoreClientError.contractViolation(
              "Core returned an invalid iMessage activation state."
            )
          }
          iMessageStatus = activated.status
        }
        channelListenerFeedback.removeValue(forKey: .iMessage)
        restored.insert(.iMessage)
      } catch {
        guard isDurableChannelAvailabilityFailure(error, channel: .iMessage) else {
          throw error
        }
        _ = try await core.stopChannel(.iMessage)
        try requireCurrentOnGeneration(expectedGeneration)
        publishListenerRestoreFailure(.iMessage)
      }
    } else {
      iMessageStatus = "disconnected"
      channelListenerFeedback.removeValue(forKey: .iMessage)
    }

    if let pairing = discordPairing {
      do {
        guard let token = try discordTokenStore.load() else {
          throw DurableChannelRestoreError.unavailable(.discord)
        }
        let proof = try await currentEnabledProof(
          expectedGeneration: expectedGeneration,
          prepareModelRuntime: false
        )
        let status = try (await core.startDiscord(token: token, proof: proof)).validated()
        try requireCurrentOnGeneration(expectedGeneration)
        guard ["connected", "connecting", "reconnecting"].contains(status.status) else {
          if ["disconnected", "faulted"].contains(status.status) {
            throw DurableChannelRestoreError.unavailable(.discord)
          }
          throw CoreClientError.contractViolation(
            "Core returned an invalid Discord listener state."
          )
        }
        updateDiscordConnectionFeedback(status.status)
        let recoveredMissionEvent = try await requireDiscordConnected(
          expectedIMessagePairing: iMessagePairing,
          expectedDiscordPairing: pairing,
          expectedGeneration: expectedGeneration
        )
        if let recoveredMissionEvent {
          latestChannelMissionEvent = recoveredMissionEvent
        }
        channelListenerFeedback.removeValue(forKey: .discord)
        restored.insert(.discord)
      } catch {
        guard isDurableChannelAvailabilityFailure(error, channel: .discord) else {
          throw error
        }
        _ = try await core.stopChannel(.discord)
        try requireCurrentOnGeneration(expectedGeneration)
        publishListenerRestoreFailure(.discord)
      }
    } else {
      updateDiscordConnectionFeedback("disconnected")
      channelListenerFeedback.removeValue(forKey: .discord)
    }

    connectedChannels = restored
    if !restored.isEmpty {
      startChannelPolling()
    }
  }

  private func isDurableChannelAvailabilityFailure(
    _ error: Error,
    channel: ChannelKind
  ) -> Bool {
    if case DurableChannelRestoreError.unavailable(let failedChannel) = error {
      return failedChannel == channel
    }
    if case CoreClientError.remote(let code, _) = error {
      return code == -32_020
    }
    if channel == .discord, case CoreClientError.keychain = error {
      return true
    }
    return false
  }

  private func publishListenerRestoreFailure(_ channel: ChannelKind) {
    switch channel {
    case .iMessage:
      iMessageStatus = "faulted"
      channelListenerFeedback[channel] =
        "Messages paused safely. Restore its macOS permissions, then load conversations and reconnect."
    case .discord:
      updateDiscordConnectionFeedback("faulted")
      channelListenerFeedback[channel] =
        "Discord paused safely. Continue with the saved token or enter a replacement token to reconnect."
    }
  }

  private func requireDiscordConnected(
    expectedIMessagePairing: ChannelPairing?,
    expectedDiscordPairing: ChannelPairing,
    expectedGeneration: UInt64
  ) async throws -> ChannelMissionEvent? {
    // A durable Discord cursor deliberately keeps the adapter in Connecting
    // until channel.poll has ingested every recovered envelope and persisted
    // the final high-water cursor. Drain only that existing typed path inside
    // the same Core-generation fence. connectedChannels, model entry, and
    // outbound authority remain unpublished until the cursor is closed.
    let delays: [Duration] = [
      .zero, .milliseconds(250), .milliseconds(500), .seconds(1), .seconds(2),
      .seconds(4),
    ]
    var progress = DiscordRecoveryProgress()
    var priorPendingStatus: String?
    var consecutivePendingStatusReads = 0
    for delay in delays {
      try requireActiveDiscordRecovery(expectedGeneration)
      if delay > .zero {
        try await Task.sleep(for: delay)
        try requireActiveDiscordRecovery(expectedGeneration)
      }
      let status = try (await core.channelStatus(.discord)).validated()
      try requireActiveDiscordRecovery(expectedGeneration)
      if status.status == "connected" {
        guard !progress.sawRecoveredEvent || progress.cursorClosed else {
          throw CoreClientError.contractViolation(
            "Discord reported Connected before its recovered cursor was closed."
          )
        }
        try await requireDurablePairingsUnchanged(
          expectedIMessage: expectedIMessagePairing,
          expectedDiscord: expectedDiscordPairing,
          expectedGeneration: expectedGeneration
        )
        updateDiscordConnectionFeedback(status.status)
        return progress.latestMissionEvent
      }
      if status.status == "disconnected" || status.status == "faulted" {
        throw DurableChannelRestoreError.unavailable(.discord)
      }
      guard status.status == "connecting" || status.status == "reconnecting" else {
        throw CoreClientError.contractViolation(
          "The approved Discord listener did not reconnect."
        )
      }
      updateDiscordConnectionFeedback(status.status)
      if progress.cursorClosed { continue }
      if priorPendingStatus == status.status {
        consecutivePendingStatusReads += 1
      } else {
        priorPendingStatus = status.status
        consecutivePendingStatusReads = 1
      }
      // Give an ordinary no-cursor Gateway launch one bounded status interval
      // to become Connected. A cursor-bearing session remains deterministically
      // Connecting, at which point the typed recovery drain is required.
      if consecutivePendingStatusReads < 2 { continue }

      let proof = try await currentEnabledProof(
        expectedGeneration: expectedGeneration,
        prepareModelRuntime: false
      )
      try requireActiveDiscordRecovery(expectedGeneration)
      let result = try await core.pollChannel(
        .discord,
        modelWorkAllowed: false,
        proof: proof
      )
      try requireActiveDiscordRecovery(expectedGeneration)
      try await requireDurablePairingsUnchanged(
        expectedIMessage: expectedIMessagePairing,
        expectedDiscord: expectedDiscordPairing,
        expectedGeneration: expectedGeneration
      )
      try consumeDiscordRecoveryResponse(result, progress: &progress)
      try requireActiveDiscordRecovery(expectedGeneration)
      updateDiscordConnectionFeedback(result.connectionStatus)
      if result.connectionStatus == "connected" {
        guard progress.cursorClosed else {
          throw CoreClientError.contractViolation(
            "Discord attempted to publish Connected before recovery completed."
          )
        }
        return progress.latestMissionEvent
      }
    }
    throw DurableChannelRestoreError.unavailable(.discord)
  }

  private func requireDurablePairingsUnchanged(
    expectedIMessage: ChannelPairing?,
    expectedDiscord: ChannelPairing,
    expectedGeneration: UInt64
  ) async throws {
    try requireActiveDiscordRecovery(expectedGeneration)
    let currentIMessage = try await core.channelPairing(.iMessage)
    try requireActiveDiscordRecovery(expectedGeneration)
    let currentDiscord = try await core.channelPairing(.discord)
    try requireActiveDiscordRecovery(expectedGeneration)
    guard currentIMessage == expectedIMessage, currentDiscord == expectedDiscord else {
      throw CoreClientError.contractViolation(
        "A durable channel pairing changed during Discord recovery."
      )
    }
  }

  private func requireActiveDiscordRecovery(_ expectedGeneration: UInt64) throws {
    try Task.checkCancellation()
    try requireCurrentOnGeneration(expectedGeneration)
  }

  private func consumeDiscordRecoveryResponse(
    _ result: ChannelPollResponse,
    progress: inout DiscordRecoveryProgress
  ) throws {
    guard result.suggestion == nil else {
      throw CoreClientError.contractViolation(
        "Discord recovery returned model work before provider readiness."
      )
    }
    guard
      result.connectionStatus == "connecting"
        || result.connectionStatus == "reconnecting"
        || result.connectionStatus == "connected"
    else {
      throw CoreClientError.contractViolation(
        "Discord recovery returned an invalid connection state."
      )
    }

    let missionEvent = try validatedChannelMissionEvent(in: result, channel: .discord)
    switch result.eventStatus {
    case "idle":
      guard missionEvent == nil, result.invalidateSuggestionId == nil else {
        throw CoreClientError.contractViolation(
          "Discord recovery returned an invalid idle response."
        )
      }
    case "needYou":
      guard missionEvent == nil else {
        throw CoreClientError.contractViolation(
          "Discord recovery returned invalid Need you content."
        )
      }
    case "recovering", "ignored":
      guard missionEvent == nil, result.invalidateSuggestionId == nil else {
        throw CoreClientError.contractViolation(
          "Discord recovery returned an invalid recovered envelope."
        )
      }
      progress.sawRecoveredEvent = true
    case "missionUpdated", "missionUpdateRecovered":
      guard let missionEvent, result.invalidateSuggestionId == nil else {
        throw CoreClientError.contractViolation(
          "Discord recovery returned an invalid Mission participation event."
        )
      }
      progress.sawRecoveredEvent = true
      progress.latestMissionEvent = missionEvent
    case "recovered":
      guard missionEvent == nil, result.invalidateSuggestionId == nil else {
        throw CoreClientError.contractViolation(
          "Discord recovery returned an invalid cursor acknowledgement."
        )
      }
      progress.sawRecoveredEvent = true
      progress.cursorClosed = true
    default:
      throw CoreClientError.contractViolation(
        "Discord recovery returned unexpected model or event work."
      )
    }

    if result.connectionStatus == "connected" {
      guard progress.cursorClosed else {
        throw CoreClientError.contractViolation(
          "Discord recovery reached Connected without its final cursor."
        )
      }
    }
  }

  private func validateDurablePairing(
    _ pairing: ChannelPairing, expected channel: ChannelKind
  ) throws {
    _ = try pairing.validated(expectedChannel: channel)
  }

  private func refreshRecoveredAccountAndModels(expectedGeneration: UInt64) async throws -> Bool {
    let setupProof = try await currentEnabledProof(expectedGeneration: expectedGeneration)
    let setup = try await core.modelSetup(proof: setupProof)
    try requireCurrentOnGeneration(expectedGeneration)
    applyAccountCatalog(setup)
    return requiredAccountAndModelReady
  }

  private func applyAccountCatalog(_ setup: ModelSetup) {
    accountState = setup.account
    availableModels = setup.models
    persistedModelSelection = setup.selection
    catalogSnapshotId = setup.catalogSnapshotId
    catalogFingerprint = setup.catalogFingerprint
    catalogRevision = setup.catalogRevision
    let currentSelection = selectionMatchesCurrentSetup(setup)
    modelSelectionStatus =
      currentSelection ? .current : (setup.selection == nil ? .unselected : .unavailable)
    guard currentSelection, let selection = setup.selection else {
      selectedModelId = ""
      selectedModelEffort = ""
      return
    }
    selectedModelId = selection.modelId
    selectedModelEffort = selection.requestedEffort
  }

  /// Clears App-only draft/readiness state when the current Core/account is no
  /// longer available. The encrypted Store selection remains untouched for
  /// audit and provenance, but cannot look preselected or runnable.
  private func clearTransientModelSetup() {
    accountState = .notConnected
    availableModels = []
    persistedModelSelection = nil
    modelSelectionStatus = .unselected
    selectedModelId = ""
    selectedModelEffort = ""
    catalogSnapshotId = ""
    catalogFingerprint = ""
    catalogRevision = 0
  }

  private func selectionMatchesCurrentSetup(_ setup: ModelSetup) -> Bool {
    guard setup.selectionStatus == .current,
      let selection = setup.selection,
      isLowerSHA256(setup.catalogSnapshotId),
      isLowerSHA256(setup.catalogFingerprint),
      setup.catalogRevision > 0,
      selection.catalogFingerprint == setup.catalogFingerprint,
      selection.catalogRevision == setup.catalogRevision,
      selection.actualEffort == selection.requestedEffort
    else { return false }
    guard case .chatGpt(_, let planType) = setup.account,
      selection.accountDisplayClass == "chatgpt:\(planType)",
      let model = setup.models.first(where: { $0.id == selection.modelId })
    else { return false }
    if selection.requestedEffort == "not_applicable" {
      return selection.actualEffort == "not_applicable" && model.supportedReasoningEfforts.isEmpty
    }
    return model.supportedReasoningEfforts.contains(selection.requestedEffort)
  }

  private func isLowerSHA256(_ value: String) -> Bool {
    value.utf8.count == 64
      && value.utf8.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102)
      }
  }

  private var requiredAccountAndModelReady: Bool {
    // Production always supplies Core lifecycle monitoring. Test-only Core
    // stubs without that surface retain their intentionally isolated behavior;
    // the shipped App cannot take this branch.
    guard coreTerminationEvents != nil else { return true }
    guard case .chatGpt = accountState, modelSelectionStatus == .current else { return false }
    guard let selection = persistedModelSelection else { return false }
    return availableModels.contains { model in
      guard model.id == selection.modelId else { return false }
      if selection.requestedEffort == "not_applicable" {
        return model.supportedReasoningEfforts.isEmpty && selection.actualEffort == "not_applicable"
      }
      return selection.actualEffort == selection.requestedEffort
        && model.supportedReasoningEfforts.contains(selection.requestedEffort)
    }
  }

  private func enterAccountSetupRequired() {
    onRequiresReplacementCoreRestoration = false
    onRequiresAccountSetup = true
    runtimeRecoveryState = .awaitingAccount
    runtimeDisplayState = .turningOn
    errorMessage = nil
  }

  private func finishAccountSetupIfReady(expectedGeneration: UInt64) throws {
    guard requiredAccountAndModelReady else {
      enterAccountSetupRequired()
      return
    }
    onRequiresAccountSetup = false
    try finishRecoveredOn(expectedGeneration: expectedGeneration)
    if runtimeIsConverged(with: true) {
      pendingRuntimeIntent = nil
    }
  }

  /// Persona provenance is diagnostic state. A transient read failure must not
  /// erase the last verified revision or block Off, recovery, or local Choice
  /// controls; Core remains the authority for every model request.
  private func refreshPersonaStatus(expectedGeneration: UInt64) async {
    guard expectedGeneration == runtimeGeneration, !Task.isCancelled else { return }
    do {
      let status = try await core.personaStatus()
      guard expectedGeneration == runtimeGeneration, !Task.isCancelled else { return }
      personaStatus = status
    } catch {
      // Preserve the last verified projection. The Host always validates
      // Persona provenance before model work, so this read-only diagnostic
      // cannot create a permissive fallback.
    }
  }

  private func applyDashboard(_ dashboard: DashboardState) throws {
    _ = try dashboard.validated()
    let projectedIncidents = try projectedChannelFailureIncidents(
      merging: dashboard.channelFailureIncidents
    )
    let projectedRouteId = reconciledSelectedRouteId(for: dashboard.channelRouteSet)
    suggestion = dashboard.suggestion
    activeCards = dashboard.activeCards
    publishConfirmedMission(dashboard.confirmedMission)
    reminderLinks = dashboard.confirmedMission?.reminderLinks ?? []
    receipt = dashboard.receipt
    needsYou = dashboard.needsYou
    publishChannelFailureIncidents(projectedIncidents)
    channelRouteSet = dashboard.channelRouteSet
    selectedChannelRouteId = projectedRouteId
    microphone = dashboard.microphone
  }

  private func publishConfirmedMission(_ mission: ConfirmedMission?) {
    if latestChannelMissionEvent?.missionId != mission?.missionId {
      latestChannelMissionEvent = nil
    }
    confirmedMission = mission
  }

  private func clearLiveConnectionState(status: String) {
    connectedChannels.removeAll()
    iMessageStatus = status
    discordStatus = status
    discordSetupFeedback =
      status == "paused"
      ? "Discord is paused because OpenOpen Core is unavailable. No live connection is claimed."
      : "Discord is disconnected. No connection is active."
  }

  private func pauseAfterRecoveryFailure() {
    channelTask?.cancel()
    channelTask = nil
    choiceResultTask?.cancel()
    choiceResultTask = nil
    clearLiveConnectionState(status: "paused")
    clearTransientModelSetup()
    onRequiresAccountSetup = false
    authoritativeStateCertain = false
    runtimeDisplayState = .unknown
    runtimeRecoveryState = .paused
  }

  private static func shutdownHandler(
    for core: any CoreServing
  ) -> @Sendable () -> Bool {
    guard let processClient = core as? CoreProcessClient else { return { true } }
    return { processClient.shutdown() }
  }

  public func submitChoiceQuestion() async {
    let value = choiceQuestion.trimmingCharacters(in: .whitespacesAndNewlines)
    guard modelEntryEnabled, choiceContinuityAllowsBegin, !isBusy, !hasNonterminalMission,
      !hasActiveForegroundChoiceSession, !value.isEmpty, value.utf8.count <= 4_096,
      let selection = persistedModelSelection, modelSelectionStatus == .current
    else { return }
    let continuitySequence = choiceLoopRefreshSequence
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let requestId: String
      if let pendingChoiceBeginRequest, pendingChoiceBeginRequest.question == value {
        requestId = pendingChoiceBeginRequest.requestId
      } else {
        requestId = UUID().uuidString.lowercased()
        pendingChoiceBeginRequest = (value, requestId)
      }
      try requireChoiceContinuity(expectedSequence: continuitySequence, permitsEmpty: true)
      let accepted = try await core.beginChoice(
        ChoiceBeginParameters(
          requestId: requestId,
          boundedLocalQuestion: value,
          selection: selection,
          proof: proof
        )
      ).validated()
      try requireCurrentOnGeneration(generation)
      guard let snapshot = try await core.choiceLoop()?.validated(),
        snapshot.session.id == accepted.choiceSessionId,
        snapshot.session.revision == accepted.acceptedSessionRevision,
        snapshot.session.state == "interpreting"
      else {
        throw CoreClientError.contractViolation("Core did not retain the first Choice session.")
      }
      try requireCurrentOnGeneration(generation)
      adoptCommittedChoiceLoopSnapshot(snapshot)
      pendingChoiceBeginRequest = nil
      suggestion = nil
      receipt = nil
      choiceQuestion = ""
      errorMessage = nil
      awaitInitialChoiceResult(expectedGeneration: generation, sessionID: accepted.choiceSessionId)
    } catch {
      guard generation == runtimeGeneration else { return }
      // A private render can have durably advanced to AwaitingConfirmation
      // while its background filesystem step reports typed reconciliation.
      // Re-read once so the non-blocking resume control is reachable instead
      // of leaving a stale Active card that can only repeat confirmation.
      reconcileChoiceContinuityAfterAmbiguousTransport(generation: generation)
      choiceConfirmationPreview = nil
      errorMessage = userMessage(for: error)
    }
  }

  /// Refines intent through the current Host-owned A/B/C ChoiceSet. It does
  /// not confirm a Mission, create a Reminder, or authorize any effect.
  public func selectChoiceOption(_ option: ChoiceOption) async {
    guard choiceSessionActionEnabled, let snapshot = choiceLoopSnapshot,
      let choiceSet = snapshot.activeChoiceSet,
      snapshot.session.state == "active",
      choiceSet.id == snapshot.session.activeChoiceSetId,
      choiceSet.choiceSessionId == snapshot.session.id,
      choiceSet.sessionRevision == snapshot.session.revision,
      choiceSet.options.contains(where: { $0.id == option.id })
    else { return }
    let continuitySequence = choiceLoopRefreshSequence
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let selection: ChoiceSelection
      if let pendingChoiceOptionSelection,
        pendingChoiceOptionSelection.choiceSessionId == snapshot.session.id,
        pendingChoiceOptionSelection.choiceSetId == choiceSet.id,
        pendingChoiceOptionSelection.selectedOptionId == option.id,
        pendingChoiceOptionSelection.expectedSessionRevision == snapshot.session.revision
      {
        selection = pendingChoiceOptionSelection
      } else {
        let selectedAtMs = Int64(Date().timeIntervalSince1970 * 1_000)
        selection = ChoiceSelection(
          type: "optionSelection",
          id: UUID().uuidString.lowercased(),
          choiceSessionId: snapshot.session.id,
          choiceSetId: choiceSet.id,
          selectedOptionId: option.id,
          dInputBatchId: nil,
          expectedSessionRevision: snapshot.session.revision,
          selectedAtMs: selectedAtMs
        )
        pendingChoiceOptionSelection = selection
      }
      try requireChoiceContinuity(expectedSequence: continuitySequence)
      let next = try await core.selectChoice(selection, proof: proof)
        .validated()
      try requireCurrentOnGeneration(generation)
      guard next.session.id == snapshot.session.id,
        next.session.revision == snapshot.session.revision + 1,
        next.lastSelection?.id == selection.id
      else {
        throw CoreClientError.contractViolation("Core did not retain the selected Choice.")
      }
      adoptCommittedChoiceLoopSnapshot(next)
      invalidateChoiceDComposerTarget()
      pendingChoiceOptionSelection = nil
      errorMessage = nil
      awaitChoiceRefinementResult(expectedGeneration: generation, sessionID: snapshot.session.id)
    } catch {
      guard generation == runtimeGeneration else { return }
      choiceConfirmationPreview = nil
      // A selection can be the first Host wake that turns Active into a
      // revisioned recap. Re-read that durable successor so the user can act
      // on the new ChoiceSet instead of being left behind a transient error.
      reconcileChoiceContinuityAfterAmbiguousTransport(generation: generation)
      errorMessage = userMessage(for: error)
    }
  }

  /// Sends only the bounded D text through the typed Host intake. The Mac
  /// never chooses a batch, binding, source envelope, or refinement result.
  @discardableResult
  public func selectChoiceD(_ text: String) async -> Bool {
    let value = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard choiceSessionActionEnabled, let snapshot = choiceLoopSnapshot,
      let choiceSet = snapshot.activeChoiceSet,
      snapshot.session.state == "active",
      choiceSet.dAvailable, value.utf8.count <= 4_096, !value.isEmpty
    else { return false }
    let continuitySequence = choiceLoopRefreshSequence
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    // A decodable response can still fail App-side binding validation. Once
    // one arrives, only a later exact durable snapshot may decide whether the
    // Host accepted the private D body; treat it as transport-ambiguous rather
    // than mistaking a local contract error for a definitive rejection.
    var receivedChoiceDResponse = false
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let input: ChoiceDInput
      if let pendingChoiceDRequest,
        pendingChoiceDRequest.boundedText == value,
        pendingChoiceDRequest.choiceSessionId == snapshot.session.id,
        pendingChoiceDRequest.choiceSetId == choiceSet.id,
        pendingChoiceDRequest.expectedSessionRevision == snapshot.session.revision
      {
        input = pendingChoiceDRequest
      } else {
        input = ChoiceDInput(
          requestId: UUID().uuidString.lowercased(), boundedText: value,
          choiceSessionId: snapshot.session.id, choiceSetId: choiceSet.id,
          expectedSessionRevision: snapshot.session.revision,
          submittedAtMs: Int64(Date().timeIntervalSince1970 * 1_000))
        pendingChoiceDRequest = input
        pendingChoiceDResponseMayBeAmbiguous = false
      }
      try requireChoiceContinuity(expectedSequence: continuitySequence)
      let response = try await core.selectChoiceD(input, proof: proof)
      receivedChoiceDResponse = true
      let next = try response.validated()
      try requireCurrentOnGeneration(generation)
      guard next.session.id == snapshot.session.id,
        next.session.state == "refining",
        next.session.revision == snapshot.session.revision + 1
      else { throw CoreClientError.contractViolation("Core did not retain the D selection.") }
      adoptCommittedChoiceLoopSnapshot(next)
      invalidateChoiceDComposerTarget()
      pendingChoiceDRequest = nil
      pendingChoiceDResponseMayBeAmbiguous = false
      errorMessage = nil
      awaitChoiceRefinementResult(expectedGeneration: generation, sessionID: snapshot.session.id)
      return true
    } catch {
      let responseMayBeAmbiguous =
        receivedChoiceDResponse
        || Self.transportOutcomeMayBeAmbiguous(error)
      pendingChoiceDResponseMayBeAmbiguous = responseMayBeAmbiguous
      if !responseMayBeAmbiguous {
        // The Host gave a definitive rejection. Keep the draft but revoke the
        // stale D routing tuple so a second submit cannot replay it as D or
        // silently fall back to `choice.begin`.
        invalidateChoiceDComposerTarget()
      }
      // Reads are non-authorizing. Reconcile every error so an explicit stale
      // rejection cannot leave the Home UI on the old active ChoiceSet; the
      // ambiguity flag above is the only thing that permits private-body
      // retirement from the returned durable snapshot.
      reconcileDChoiceContinuityAfterAmbiguousTransport()
      errorMessage = userMessage(for: error)
      return false
    }
  }

  /// Routes the frozen D card to the existing Home composer. This is local
  /// focus state only: it starts neither a Choice intake nor model/effect
  /// work, and it cannot outlive the exact active ChoiceSet it names.
  public func focusChoiceDComposer() {
    guard choiceSessionActionEnabled, let snapshot = choiceLoopSnapshot,
      let choiceSet = snapshot.activeChoiceSet,
      snapshot.session.state == "active", choiceSet.dAvailable,
      choiceSet.choiceSessionId == snapshot.session.id,
      choiceSet.id == snapshot.session.activeChoiceSetId,
      choiceSet.sessionRevision == snapshot.session.revision
    else { return }
    choiceDComposerTarget = (snapshot.session.id, choiceSet.id, snapshot.session.revision)
    choiceDComposerTextIsBound = true
    choiceDComposerFocusRequested = true
  }

  public func consumeChoiceDComposerFocusRequest() {
    choiceDComposerFocusRequested = false
  }

  private func invalidateChoiceDComposerTarget() {
    choiceDComposerTarget = nil
    choiceDComposerFocusRequested = false
  }

  /// Private raw input belongs only to an in-flight local request. Once a
  /// Store snapshot proves acceptance or cancellation, retain neither that
  /// body nor a second App-side replay cache. The Host owns durable encrypted
  /// request recovery; the Mac keeps only typed visible state.
  private func retirePrivateChoiceIntakeBodies(after snapshot: ChoiceLoopSnapshot) {
    if snapshot.session.state == "cancelled" {
      pendingChoiceBeginRequest = nil
      pendingChoiceDRequest = nil
      pendingChoiceDResponseMayBeAmbiguous = false
      choiceDComposerTextIsBound = false
      choiceQuestion = ""
      invalidateChoiceDComposerTarget()
      return
    }

    // A begin request is only installed after the Host RPC is entered while
    // no foreground session exists. An authenticated interpreting snapshot
    // with its sealed batch is therefore the durable acceptance witness.
    if pendingChoiceBeginRequest != nil,
      snapshot.session.state == "interpreting", snapshot.activeBatch != nil
    {
      pendingChoiceBeginRequest = nil
      choiceQuestion = ""
    }

    guard let request = pendingChoiceDRequest else { return }
    let pendingOperationMatches =
      snapshot.pendingRefinementOperation.map { operation in
        operation.choiceSessionId == request.choiceSessionId
          && operation.expectedSessionRevision == snapshot.session.revision
          && operation.dRequestId == request.requestId
          && operation.dInputDigest != nil
      } ?? false
    let completedSelectionMatches =
      pendingChoiceDResponseMayBeAmbiguous
      && (snapshot.lastSelection.map { selection in
        selection.type == "naturalConversationSelection"
          && selection.choiceSessionId == request.choiceSessionId
          && selection.choiceSetId == request.choiceSetId
          && selection.expectedSessionRevision == request.expectedSessionRevision
          && snapshot.session.revision > request.expectedSessionRevision
      } ?? false)
    // A known Host rejection may be observed beside an independently advanced
    // snapshot. Do not let that snapshot erase the owner's unaccepted draft;
    // only a transport whose response may have been lost can prove acceptance.
    guard pendingChoiceDResponseMayBeAmbiguous,
      pendingOperationMatches || completedSelectionMatches
    else { return }
    pendingChoiceDRequest = nil
    pendingChoiceDResponseMayBeAmbiguous = false
    choiceDComposerTextIsBound = false
    choiceQuestion = ""
    invalidateChoiceDComposerTarget()
  }

  /// Transport cancellation is not durable rejection. Reconcile in a fresh
  /// unstructured MainActor task so a cancelled RPC task cannot prevent the
  /// Mac from observing Host acceptance and retiring its raw local body.
  private func reconcileChoiceContinuityAfterAmbiguousTransport(generation: UInt64) {
    Task { [weak self] in
      await self?.refreshChoiceLoopContinuity(expectedGeneration: generation)
    }
  }

  /// D input contains a short-lived local private body. If its RPC races a
  /// Core replacement, use the *current* Core generation for a read-only
  /// continuity reconciliation so a later durable acceptance can retire that
  /// body. The snapshot still has to match the exact request before removal.
  private func reconcileDChoiceContinuityAfterAmbiguousTransport() {
    Task { [weak self] in
      guard let self else { return }
      await self.refreshChoiceLoopContinuity(expectedGeneration: self.runtimeGeneration)
    }
  }

  /// Only an ambiguous local transport result can make a later terminal Store
  /// snapshot evidence of accepted D input. A known validation rejection must
  /// preserve the owner's composer draft for correction.
  private static func transportOutcomeMayBeAmbiguous(_ error: Error) -> Bool {
    if error is CancellationError { return true }
    switch error as? CoreClientError {
    case .requestTimedOut, .requestCancelled, .processTerminated, .processUnavailable,
      .malformedResponse, .oversizedFrame, .unknownResponseIdentifier:
      return true
    default:
      return false
    }
  }

  /// The sole Home-composer dispatcher. A stale D target never falls back to
  /// `choice.begin`; retained text remains user-owned until accepted or
  /// explicitly cancelled.
  public func submitHomeComposer() async {
    if choiceDComposerTextIsBound,
      choiceQuestion.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    {
      // Clearing an unaccepted D draft is the user's explicit cancellation of
      // that local routing intent. It still does not create a Choice intake.
      choiceDComposerTextIsBound = false
      pendingChoiceDRequest = nil
      pendingChoiceDResponseMayBeAmbiguous = false
      invalidateChoiceDComposerTarget()
      return
    }
    if choiceDComposerTextIsBound, choiceDComposerTarget == nil {
      // The prior D target has become stale. Keep the user's draft visible;
      // do not reinterpret it as a new first question after a state drift.
      await refreshChoiceLoopContinuity(expectedGeneration: runtimeGeneration)
      return
    }
    if let target = choiceDComposerTarget {
      guard let snapshot = choiceLoopSnapshot,
        let choiceSet = snapshot.activeChoiceSet,
        choiceSessionActionEnabled,
        snapshot.session.state == "active", choiceSet.dAvailable,
        snapshot.session.id == target.sessionId,
        choiceSet.id == target.choiceSetId,
        snapshot.session.revision == target.revision,
        choiceSet.sessionRevision == target.revision
      else {
        invalidateChoiceDComposerTarget()
        await refreshChoiceLoopContinuity(expectedGeneration: runtimeGeneration)
        return
      }
      if await selectChoiceD(choiceQuestion) {
        choiceQuestion = ""
        choiceDComposerTextIsBound = false
      }
      return
    }
    await submitChoiceQuestion()
  }

  /// Requests a Host-derived, effect-free summary for the active ChoiceSet.
  /// This does not create a Mission, Reminder, delivery, or permission grant.
  public func prepareChoiceConfirmation() async {
    guard choiceSessionActionEnabled, let snapshot = choiceLoopSnapshot,
      snapshot.session.state == "active"
    else {
      return
    }
    let continuitySequence = choiceLoopRefreshSequence
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    let draftRevision = choiceReminderScheduleDraftRevision
    do {
      if let schedule = try choiceReminderScheduleInput(for: snapshot) {
        // Runtime challenges are single-use. Recording a local schedule and
        // preparing its confirmation are two separate Host transactions, so
        // each must carry a fresh broker-issued proof.
        let scheduleProof = try await currentEnabledProof(expectedGeneration: generation)
        try requireChoiceContinuity(expectedSequence: continuitySequence)
        let stored = try await core.recordChoiceReminderSchedule(schedule, proof: scheduleProof)
        guard stored.validated(), stored.input == schedule,
          stored.input.choiceSessionId == snapshot.session.id,
          stored.input.expectedSessionRevision == snapshot.session.revision
        else {
          throw CoreClientError.contractViolation(
            "Core returned an invalid Choice Reminder schedule.")
        }
        guard draftRevision == choiceReminderScheduleDraftRevision else { return }
        choiceReminderScheduleDraftIsDirty = false
      } else if choiceReminderScheduleDraftIsDirty {
        throw CoreClientError.contractViolation("Choose a complete future local Reminder schedule.")
      }
      try requireCurrentOnGeneration(generation)
      guard draftRevision == choiceReminderScheduleDraftRevision else { return }
      let confirmationProof = try await currentEnabledProof(expectedGeneration: generation)
      try requireChoiceContinuity(expectedSequence: continuitySequence)
      let confirmation = try await core.prepareChoiceConfirmation(proof: confirmationProof)
      guard
        confirmation.validated(),
        confirmation.choiceSessionId == choiceLoopSnapshot?.session.id,
        confirmation.expectedSessionRevision == choiceLoopSnapshot?.session.revision
      else {
        throw CoreClientError.contractViolation("Core returned an invalid Choice confirmation.")
      }
      try requireCurrentOnGeneration(generation)
      guard draftRevision == choiceReminderScheduleDraftRevision else { return }
      choiceConfirmationPreview = confirmation
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      reconcileChoiceContinuityAfterAmbiguousTransport(generation: generation)
      errorMessage = userMessage(for: error)
    }
  }

  private func prepareCurrentChoiceIMessageReply(expectedGeneration: UInt64) async {
    guard expectedGeneration == runtimeGeneration,
      let snapshot = choiceLoopSnapshot, snapshot.session.state == "active",
      iMessageIsConnected
    else { return }
    do {
      let proof = try await currentEnabledProof(expectedGeneration: expectedGeneration)
      let prepared = try await core.prepareChoiceIMessageReply(proof: proof).validated()
      let preview = prepared.preview
      try requireCurrentOnGeneration(expectedGeneration)
      guard choiceLoopSnapshot?.session.id == snapshot.session.id,
        choiceLoopSnapshot?.session.revision == preview.previewRevision,
        choiceLoopSnapshot?.session.state == "active"
      else { return }
      choiceIMessageReplyPreview = preview
      choiceIMessageReplyStatus = prepared.status == "prepared" ? nil : prepared.status
    } catch {
      guard expectedGeneration == runtimeGeneration else { return }
      choiceIMessageReplyPreview = nil
      choiceIMessageReplyStatus = nil
      errorMessage = userMessage(for: error)
    }
  }

  public var choiceIMessageReplySendEnabled: Bool {
    guard let preview = choiceIMessageReplyPreview,
      let snapshot = choiceLoopSnapshot
    else { return false }
    return !isBusy && storeControlEnabled && iMessageIsConnected
      && snapshot.session.state == "active"
      && snapshot.session.revision == preview.previewRevision
      && choiceIMessageReplyStatus == nil
  }

  public var choiceIMessageReplyRecoveryEnabled: Bool {
    guard let preview = choiceIMessageReplyPreview,
      let snapshot = choiceLoopSnapshot
    else { return false }
    return !isBusy && storeControlEnabled && iMessageIsConnected
      && snapshot.session.state == "active"
      && snapshot.session.revision == preview.previewRevision
      && choiceIMessageReplyStatus == "authorized"
  }

  public func authorizeCurrentChoiceIMessageReply() async {
    guard choiceIMessageReplySendEnabled || choiceIMessageReplyRecoveryEnabled,
      let preview = choiceIMessageReplyPreview
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let response = try await core.authorizeChoiceIMessageReply(preview, proof: proof).validated()
      try requireCurrentOnGeneration(generation)
      guard choiceIMessageReplyPreview == preview else { return }
      choiceIMessageReplyStatus = response.status == "sent" ? "delivered" : "authorized"
      errorMessage =
        response.status == "needYou"
        ? "Need you: verify the existing iMessage reply before trying anything else."
        : nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  /// Returns nil only when the user has not supplied any schedule fields.
  /// Partial data fails locally before an RPC; complete data is still treated
  /// as untrusted and revalidated by the Host/Store transaction.
  private func choiceReminderScheduleInput(
    for snapshot: ChoiceLoopSnapshot
  ) throws -> ChoiceReminderScheduleInput? {
    let dateTime = choiceReminderDateTime.trimmingCharacters(in: .whitespacesAndNewlines)
    let timeZone = choiceReminderTimeZone.trimmingCharacters(in: .whitespacesAndNewlines)
    let listId = choiceReminderListId.trimmingCharacters(in: .whitespacesAndNewlines)
    let count = choiceReminderCount.trimmingCharacters(in: .whitespacesAndNewlines)
    if !choiceReminderScheduleDraftIsDirty,
      let pending = pendingChoiceReminderSchedule,
      pending.sessionId == snapshot.session.id,
      pending.sessionRevision == snapshot.session.revision
    {
      // A recovered exact proposal is already durable. Re-reading it for a
      // preview must not round a displayed minute or mint another revision.
      return nil
    }
    if dateTime.isEmpty && timeZone.isEmpty && listId.isEmpty && count.isEmpty {
      return nil
    }
    guard !dateTime.isEmpty, !timeZone.isEmpty, !listId.isEmpty, !count.isEmpty,
      let reminderCount = UInt32(count), (1...16).contains(reminderCount),
      validChoiceReminderListId(listId), let zone = TimeZone(identifier: timeZone)
    else {
      throw CoreClientError.contractViolation("Choose a complete local Reminder schedule.")
    }

    guard let date = unambiguousLocalReminderDate(dateTime, timeZone: zone) else {
      throw CoreClientError.contractViolation("Choose a valid local Reminder date and time.")
    }
    let dueAtMs = Int64((date.timeIntervalSince1970 * 1_000).rounded())
    guard dueAtMs > Int64((Date().timeIntervalSince1970 * 1_000).rounded()) else {
      throw CoreClientError.contractViolation("Choose a future local Reminder time.")
    }

    let requestId: String
    if let pending = pendingChoiceReminderSchedule,
      pending.dateTime == dateTime, pending.timeZone == timeZone, pending.listId == listId,
      pending.count == count,
      pending.sessionId == snapshot.session.id,
      pending.sessionRevision == snapshot.session.revision
    {
      requestId = pending.requestId
    } else {
      requestId = "reminder-schedule-" + UUID().uuidString.lowercased()
      pendingChoiceReminderSchedule = (
        dateTime, timeZone, listId, count, snapshot.session.id, snapshot.session.revision, requestId
      )
    }
    let input = ChoiceReminderScheduleInput(
      requestId: requestId, choiceSessionId: snapshot.session.id,
      expectedSessionRevision: snapshot.session.revision, reminderListId: listId,
      reminderCount: reminderCount,
      dueAtMs: dueAtMs, timeZone: timeZone)
    guard input.validated() else {
      throw CoreClientError.contractViolation("Choose a bounded Reminder schedule.")
    }
    return input
  }

  private func validChoiceReminderListId(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128
      && value.utf8.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 90) || ($0 >= 97 && $0 <= 122)
          || $0 == 45 || $0 == 95 || $0 == 46
      }
  }

  /// Rejects both non-existent and repeated local wall times. The wire record
  /// binds one exact instant plus an IANA zone, so silently choosing one side
  /// of a daylight-saving fallback would not be an explicit user selection.
  private func unambiguousLocalReminderDate(_ value: String, timeZone: TimeZone) -> Date? {
    let local = DateFormatter()
    local.locale = Locale(identifier: "en_US_POSIX")
    local.calendar = Calendar(identifier: .gregorian)
    local.timeZone = timeZone
    local.dateFormat = "yyyy-MM-dd'T'HH:mm"
    guard let parsed = local.date(from: value), local.string(from: parsed) == value else {
      return nil
    }
    let utc = DateFormatter()
    utc.locale = Locale(identifier: "en_US_POSIX")
    utc.calendar = Calendar(identifier: .gregorian)
    utc.timeZone = TimeZone(secondsFromGMT: 0)
    utc.dateFormat = "yyyy-MM-dd'T'HH:mm"
    guard let wallAsUTC = utc.date(from: value) else { return nil }
    let offsets = Set([
      timeZone.secondsFromGMT(for: parsed),
      timeZone.secondsFromGMT(for: parsed.addingTimeInterval(-86_400)),
      timeZone.secondsFromGMT(for: parsed.addingTimeInterval(86_400)),
    ])
    let candidates = offsets.compactMap { offset -> Date? in
      let candidate = wallAsUTC.addingTimeInterval(TimeInterval(-offset))
      guard timeZone.secondsFromGMT(for: candidate) == offset,
        local.string(from: candidate) == value
      else { return nil }
      return candidate
    }
    return candidates.count == 1 ? candidates[0] : nil
  }

  /// A schedule edit is never allowed to confirm a previous preview. The
  /// Host creates the new revision only when the user chooses review again.
  public func invalidateChoiceReminderScheduleDraft() {
    guard !applyingChoiceReminderScheduleHydration else { return }
    choiceReminderScheduleDraftRevision &+= 1
    choiceReminderScheduleDraftIsDirty = true
    choiceConfirmationPreview = nil
    pendingChoiceReminderSchedule = nil
  }

  /// Records an explicit native date-picker interaction. Merely presenting
  /// the picker never creates a schedule or guesses a due time.
  public func selectChoiceReminderDate(_ date: Date) {
    choiceReminderPickerDate = date
    choiceReminderPickerIsPresented = true
    choiceReminderPickerDateIsExplicit = true
    updateChoiceReminderDateTimeFromNativeSelection()
  }

  /// Reveals the native picker without accepting its initial display value as
  /// owner-provided schedule authority. Review remains disabled until the
  /// owner changes the picker and every other exact schedule field is valid.
  public func presentChoiceReminderDatePicker() {
    choiceReminderPickerIsPresented = true
  }

  /// Frozen-UI local Back action. It changes no Store state or effect
  /// authority; the current verified ChoiceSet remains available above.
  public func backFromChoiceReminderSchedule() {
    clearChoiceReminderScheduleDraft()
    choiceReminderScheduleIsVisible = false
  }

  public var choiceReminderScheduleReadyForReview: Bool {
    guard choiceReminderPickerDateIsExplicit,
      let zone = TimeZone(identifier: choiceReminderTimeZone),
      validChoiceReminderListId(choiceReminderListId), choiceReminderCount == "1",
      let date = unambiguousLocalReminderDate(choiceReminderDateTime, timeZone: zone)
    else { return false }
    return date > Date()
  }

  /// Records an explicit IANA time-zone selection and re-renders the already
  /// selected instant in that zone. No selection means no schedule proposal.
  public func selectChoiceReminderTimeZone(_ identifier: String) {
    choiceReminderTimeZone = identifier
    if choiceReminderPickerDateIsExplicit {
      updateChoiceReminderDateTimeFromNativeSelection()
    } else {
      invalidateChoiceReminderScheduleDraft()
    }
  }

  public func selectChoiceReminderList(_ identifier: String) {
    choiceReminderListId = identifier
    choiceReminderCount = identifier.isEmpty ? "" : "1"
    invalidateChoiceReminderScheduleDraft()
  }

  private func updateChoiceReminderDateTimeFromNativeSelection() {
    guard choiceReminderPickerDateIsExplicit,
      let zone = TimeZone(identifier: choiceReminderTimeZone)
    else {
      choiceReminderDateTime = ""
      invalidateChoiceReminderScheduleDraft()
      return
    }
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.timeZone = zone
    formatter.dateFormat = "yyyy-MM-dd'T'HH:mm"
    choiceReminderDateTime = formatter.string(from: choiceReminderPickerDate)
    invalidateChoiceReminderScheduleDraft()
  }

  private func clearChoiceReminderScheduleDraft() {
    choiceReminderDateTime = ""
    choiceReminderTimeZone = ""
    choiceReminderListId = ""
    choiceReminderCount = ""
    choiceReminderPickerDate = Date()
    choiceReminderPickerIsPresented = false
    choiceReminderPickerDateIsExplicit = false
    pendingChoiceReminderSchedule = nil
    choiceReminderScheduleDraftRevision &+= 1
    choiceReminderScheduleDraftIsDirty = false
    choiceConfirmationPreview = nil
  }

  private func hydrateChoiceReminderSchedule(_ schedule: ChoiceReminderSchedule) {
    guard let zone = TimeZone(identifier: schedule.input.timeZone) else { return }
    applyingChoiceReminderScheduleHydration = true
    defer {
      applyingChoiceReminderScheduleHydration = false
      choiceReminderScheduleDraftIsDirty = false
    }
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.timeZone = zone
    formatter.dateFormat = "yyyy-MM-dd'T'HH:mm"
    choiceReminderDateTime = formatter.string(
      from: Date(timeIntervalSince1970: TimeInterval(schedule.input.dueAtMs) / 1_000))
    choiceReminderPickerDate = Date(
      timeIntervalSince1970: TimeInterval(schedule.input.dueAtMs) / 1_000)
    choiceReminderPickerIsPresented = true
    choiceReminderPickerDateIsExplicit = true
    choiceReminderTimeZone = schedule.input.timeZone
    choiceReminderListId = schedule.input.reminderListId
    choiceReminderCount = String(schedule.input.reminderCount)
    pendingChoiceReminderSchedule = (
      choiceReminderDateTime, choiceReminderTimeZone, choiceReminderListId, choiceReminderCount,
      schedule.input.choiceSessionId, schedule.input.expectedSessionRevision,
      schedule.input.requestId
    )
  }

  public func formattedChoiceReminderDateTime(_ item: ChoiceReminderItem) -> String {
    guard let zone = TimeZone(identifier: item.timeZone) else { return "Unavailable" }
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.timeZone = zone
    formatter.dateFormat = "yyyy-MM-dd'T'HH:mm"
    return formatter.string(from: Date(timeIntervalSince1970: TimeInterval(item.dueAtMs) / 1_000))
  }

  public func confirmPreparedChoice() async {
    guard choiceSessionActionEnabled, let snapshot = choiceLoopSnapshot,
      let confirmation = choiceConfirmationPreview,
      confirmation.choiceSessionId == snapshot.session.id,
      confirmation.expectedSessionRevision == snapshot.session.revision
    else {
      return
    }
    let continuitySequence = choiceLoopRefreshSequence
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      try requireChoiceContinuity(expectedSequence: continuitySequence)
      let next = try await core.confirmChoice(confirmation, proof: proof).validated()
      try requireCurrentOnGeneration(generation)
      guard
        next.session.id == confirmation.choiceSessionId,
        next.session.state == "awaitingConfirmation",
        next.confirmation == confirmation,
        next.session.pendingConfirmationId == confirmation.id,
        next.session.revision == confirmation.expectedSessionRevision + 1
      else {
        throw CoreClientError.contractViolation(
          "Core did not complete the exact local Choice journal."
        )
      }
      adoptCommittedChoiceLoopSnapshot(next)
      invalidateChoiceDComposerTarget()
      choiceConfirmationPreview = nil
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      // The Store may already be AwaitingConfirmation if its private render
      // hit a durable reconciliation boundary. Refresh once so the recovery
      // control remains reachable instead of retaining a stale Active card.
      await refreshChoiceLoopContinuity(expectedGeneration: generation)
      errorMessage = userMessage(for: error)
    }
  }

  /// The separate, explicit action-time boundary for the exact confirmed
  /// Reminder write. Choice confirmation itself never enters this route.
  public func requestChoiceReminderWrite() {
    guard choiceReminderWriteTask == nil else { return }
    let taskID = UUID()
    choiceReminderWriteTaskID = taskID
    choiceReminderWriteTask = Task { [weak self] in
      guard let self else { return }
      await self.authorizeChoiceReminderWrite()
      guard self.choiceReminderWriteTaskID == taskID else { return }
      self.choiceReminderWriteTask = nil
      self.choiceReminderWriteTaskID = nil
    }
  }

  private func authorizeChoiceReminderWrite() async {
    guard storeControlEnabled, !isBusy, let confirmation = choiceLoopSnapshot?.confirmation,
      choiceLoopSnapshot?.session.state == "awaitingConfirmation",
      let ownedTaskID = choiceReminderWriteTaskID
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    var abortContext: (confirmationId: String, missionId: String)?
    do {
      try Task.checkCancellation()
      let target = try await reminders.prepareTarget()
      try Task.checkCancellation()
      let authorizeProof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      let mission = try
        (await core.authorizeChoiceReminders(
          confirmationId: confirmation.id, reminderTarget: target, proof: authorizeProof
        )).validated()
      try Task.checkCancellation()
      try requireCurrentOnGeneration(generation)
      guard Self.matchesChoiceMission(mission, confirmation: confirmation) else {
        throw CoreClientError.contractViolation(
          "Core did not bind the exact confirmed Choice to Reminders.")
      }
      let dispatchProof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      let start = try await core.beginChoiceReminderDispatch(
        confirmationId: confirmation.id, proof: dispatchProof)
      _ = try start.mission.validated()
      guard Self.matchesChoiceMission(start.mission, confirmation: confirmation) else {
        throw CoreClientError.contractViolation("Core returned a different Reminder dispatch.")
      }
      if start.executeNow {
        abortContext = (confirmation.id, start.mission.missionId)
      }
      if Task.isCancelled || generation != runtimeGeneration {
        throw RemindersClientError.cancelledBeforeCommit
      }
      let links =
        start.executeNow
        ? try await reminders.executeInitialMirror(start)
        : try await recoverOrRetireAbsentChoiceReminder(
          start: start, confirmationId: confirmation.id, generation: generation,
          ownedTaskID: ownedTaskID)
      // A proven-absent read-only recovery records the stopped attempt but
      // deliberately returns no links. The owner must make a second explicit
      // request before EventKit can be entered again.
      guard !links.isEmpty else {
        errorMessage = userMessage(for: RemindersClientError.mirrorAbsent(start.mission.title))
        return
      }
      abortContext = nil
      try Task.checkCancellation()
      let recordProof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      let persisted = try
        (await core.recordChoiceReminderMirror(
          confirmationId: confirmation.id, links: links, proof: recordProof
        )).validated()
      try requireCurrentOnGeneration(generation)
      guard Self.matchesChoiceMission(persisted, confirmation: confirmation),
        persisted.reminderLinks == links
      else {
        throw CoreClientError.contractViolation(
          "Core did not retain exact Reminder Evidence links.")
      }
      publishConfirmedMission(persisted)
      reminderLinks = links
      activeCards = [
        ActiveOutcomeCard(
          id: persisted.missionId, title: persisted.title,
          state: "Waiting for the confirmed Reminder to be completed")
      ]
      errorMessage = nil
    } catch RemindersClientError.cancelledBeforeCommit {
      // This is the only safe retry transition: EventKit reset its uncommitted
      // batch and the signed local client reports that bounded fact while the
      // exact protected Core is still alive. Ambiguous post-commit failures
      // never enter this route and remain read-only recovery.
      if let abortContext {
        // The local EventKit transaction is definitively reset. Release its
        // in-process claim before the Store acknowledgement so a lost abort
        // response cannot consume the next durable attempt and then reject it
        // locally. The Store command itself is bounded and idempotent; it uses
        // a fresh one-time runtime challenge on every reconciliation attempt.
        reminders.releaseInitialMirrorClaim(for: abortContext.missionId)
        let abortError = await Task { @MainActor [weak self] () -> String? in
          guard let self else { return "OpenOpen could not reconcile the stopped Reminder write." }
          return await self.reconcileChoiceReminderPrecommitAbort(
            confirmationId: abortContext.confirmationId,
            expectedGeneration: generation,
            ownedTaskID: ownedTaskID)
        }.value
        if let abortError {
          if generation == runtimeGeneration { errorMessage = abortError }
          return
        }
      }
      if generation == runtimeGeneration {
        errorMessage = userMessage(for: RemindersClientError.cancelledBeforeCommit)
      }
    } catch {
      if generation == runtimeGeneration {
        errorMessage = userMessage(for: error)
      }
    }
  }

  private func recoverOrRetireAbsentChoiceReminder(
    start: ReminderDispatchStart,
    confirmationId: String,
    generation: UInt64,
    ownedTaskID: UUID
  ) async throws -> [ReminderLink] {
    do {
      return try await reminders.recoverMirror(for: start.mission)
    } catch RemindersClientError.mirrorAbsent {
      // This is an authenticated read of the exact durable dispatch after a
      // process/task boundary. Zero matching marker rows means there is no
      // recoverable committed effect. Retire only that attempt, then require a
      // later explicit owner action; never write EventKit in this recovery.
      guard generation == runtimeGeneration, desiredEnabled else {
        throw CoreClientError.requestCancelled
      }
      if let message = await reconcileChoiceReminderPrecommitAbort(
        confirmationId: confirmationId,
        expectedGeneration: generation,
        ownedTaskID: ownedTaskID)
      {
        throw CoreClientError.contractViolation(message)
      }
      return []
    }
  }

  private func reconcileChoiceReminderPrecommitAbort(
    confirmationId: String,
    expectedGeneration: UInt64,
    ownedTaskID: UUID
  ) async -> String? {
    var lastMessage = "OpenOpen could not reconcile the stopped Reminder write."
    for _ in 0..<3 {
      do {
        let proof =
          if expectedGeneration == runtimeGeneration, desiredEnabled {
            try await currentEnabledProof(
              expectedGeneration: expectedGeneration,
              prepareModelRuntime: false)
          } else {
            try await currentReminderOffQuiescenceProof(
              previousGeneration: expectedGeneration,
              ownedTaskID: ownedTaskID)
          }
        _ = try await core.abortChoiceReminderDispatchBeforeCommit(
          confirmationId: confirmationId, proof: proof)
        return nil
      } catch {
        lastMessage = userMessage(for: error)
        // A protected Off intentionally advances the runtime generation before
        // it cancels and awaits this exact task. Keep the bounded reconciliation
        // loop alive in that window so a lost Store response is confirmed with
        // a fresh one-time challenge. Each pass revalidates either current On
        // or the exact task-owned Off-quiescence proof; drift still fails closed.
      }
    }
    return lastMessage
  }

  /// Produces one fresh proof only for the exact Reminder task that protected
  /// Off has cancelled and is currently awaiting. It cannot authorize model
  /// work or a new effect: desired state is already Off, the visible state is
  /// Turning Off, and the proof is passed only to the idempotent abort command
  /// before Core shutdown and the Store Off commit.
  private func currentReminderOffQuiescenceProof(
    previousGeneration: UInt64,
    ownedTaskID: UUID
  ) async throws -> BrokerRuntimeState {
    let (expectedGeneration, overflow) = previousGeneration.addingReportingOverflow(1)
    guard !overflow,
      runtimeGeneration == expectedGeneration,
      !desiredEnabled,
      pendingRuntimeIntent == false,
      runtimeDisplayState == .turningOff,
      choiceReminderWriteTaskID == ownedTaskID,
      choiceReminderWriteTask != nil,
      enabled,
      confirmedEnabled,
      let expectedProtected = protectedRuntime,
      expectedProtected.authorization.enabled
    else {
      throw CoreClientError.contractViolation(
        "The Reminder abort is no longer inside the protected Off quiescence window.")
    }
    _ = try await provisionBrokerTrust()
    guard runtimeGeneration == expectedGeneration,
      !desiredEnabled,
      pendingRuntimeIntent == false,
      choiceReminderWriteTaskID == ownedTaskID,
      let protected = try await readProtectedRuntime(),
      protected.authorization == expectedProtected.authorization
    else {
      throw CoreClientError.contractViolation(
        "The protected runtime changed while reconciling the stopped Reminder write.")
    }
    let runtime = try await core.recoverRuntime(
      protected.authorization,
      brokerReceipt: protected.receipt)
    guard runtimeGeneration == expectedGeneration,
      !desiredEnabled,
      pendingRuntimeIntent == false,
      choiceReminderWriteTaskID == ownedTaskID,
      runtime.enabled,
      runtime.revision == protected.authorization.revision,
      runtime.updatedAtMs == protected.authorization.updatedAtMs
    else {
      throw CoreClientError.contractViolation(
        "The protected runtime changed while reconciling the stopped Reminder write.")
    }
    return protected
  }

  public func checkChoiceReminderProgress() async {
    guard storeControlEnabled, !isBusy, let confirmation = choiceLoopSnapshot?.confirmation,
      let mission = confirmedMission, !reminderLinks.isEmpty,
      Self.matchesChoiceMission(mission, confirmation: confirmation)
    else { return }
    isBusy = true
    let generation = runtimeGeneration
    var shouldRefreshForNextChoice = false
    do {
      let completed = try await reminders.completedReminders(
        for: reminderLinks, confirmation: confirmation)
      guard completed.count == confirmation.reminderItems.count else {
        throw CoreClientError.contractViolation(
          "Complete every confirmed Reminder before continuing.")
      }
      let proof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      let completion = try await core.completeChoiceReminders(
        confirmationId: confirmation.id, completions: completed, proof: proof)
      let receipt = try completion.receipt.validated()
      let next = try completion.choiceLoop.validated()
      try requireCurrentOnGeneration(generation)
      guard receipt.missionId == mission.missionId,
        receipt.outputHashes.contains(confirmation.payloadDigest),
        next.confirmation == confirmation,
        next.session.state == "softIdle"
      else {
        throw CoreClientError.contractViolation(
          "Core did not bind Receipt and Markdown to the exact confirmed Choice.")
      }
      self.receipt = receipt
      adoptCommittedChoiceLoopSnapshot(next)
      activeCards = []
      errorMessage = nil
      shouldRefreshForNextChoice = true
    } catch {
      if generation == runtimeGeneration {
        errorMessage = userMessage(for: error)
      }
    }
    isBusy = false
    guard shouldRefreshForNextChoice, generation == runtimeGeneration else { return }
    // An authenticated foreground return is the only authorizing wake for
    // the private post-Receipt next-choice operation. It must occur after the
    // effect task releases its busy fence, or the one-shot resume is lost.
    await refreshDashboard(authenticatedHomeForeground: true)
  }

  private static func matchesChoiceMission(
    _ mission: ConfirmedMission, confirmation: ChoiceConsolidatedConfirmation
  ) -> Bool {
    mission.choiceConfirmationId == confirmation.id
      && mission.choicePayloadDigest == confirmation.payloadDigest
      && mission.choiceReminderPayloadDigest == confirmation.reminderPayloadDigest
      && mission.choiceReminderItems == confirmation.reminderItems
      && mission.workItems.map(\.id) == confirmation.reminderItems.map(\.id)
      && mission.workItems.map(\.title) == confirmation.reminderItems.map(\.text)
  }

  private func awaitChoiceRefinementResult(expectedGeneration: UInt64, sessionID: String) {
    choiceResultTask?.cancel()
    choiceResultTask = Task { [weak self] in
      guard let self else { return }
      for _ in 0..<240 {
        guard !Task.isCancelled, expectedGeneration == runtimeGeneration else { return }
        try? await Task.sleep(for: .seconds(1))
        await refreshChoiceLoopContinuity(expectedGeneration: expectedGeneration)
        guard !Task.isCancelled, expectedGeneration == runtimeGeneration,
          let snapshot = choiceLoopSnapshot, snapshot.session.id == sessionID
        else { return }
        if snapshot.session.state != "refining" {
          choiceResultTask = nil
          return
        }
      }
      guard expectedGeneration == runtimeGeneration,
        choiceLoopSnapshot?.session.id == sessionID,
        choiceLoopSnapshot?.session.state == "refining"
      else { return }
      choiceLoopContinuityState = .needsYou(.readFailed)
      choiceResultTask = nil
    }
  }

  /// Retires only the current foreground Choice path. It is independent from
  /// Mission cancellation and never creates, changes, or authorizes an effect.
  public func cancelChoiceSession() async {
    guard !isBusy, let snapshot = choiceLoopSnapshot,
      snapshot.session.state != "completed", snapshot.session.state != "cancelled"
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      let next = try await core.cancelChoice(proof: proof).validated()
      try requireCurrentOnGeneration(generation)
      guard next.session.id == snapshot.session.id,
        next.session.state == "cancelled",
        next.session.revision == snapshot.session.revision + 1
      else {
        throw CoreClientError.contractViolation("Core did not retain the cancelled Choice.")
      }
      adoptCommittedChoiceLoopSnapshot(next)
      invalidateChoiceDComposerTarget()
      pendingChoiceBeginRequest = nil
      pendingChoiceDRequest = nil
      choiceDComposerTextIsBound = false
      choiceQuestion = ""
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      reconcileChoiceContinuityAfterAmbiguousTransport(generation: generation)
      errorMessage = userMessage(for: error)
    }
  }

  /// Retries only a Host-created local Markdown journal after a crash or
  /// interrupted cleanup. It has no body/path input and never authorizes a
  /// Reminder, Mission, channel delivery, or provider work.
  public func reconcileChoiceMarkdown() async {
    guard !isBusy,
      matchesChoiceMarkdownRecoveryState(
        choiceLoopSnapshot?.session.state,
        receiptCleanupAvailable: choiceMarkdownReceiptCleanupAvailable)
    else { return }
    isBusy = true
    defer { isBusy = false }
    do {
      let snapshot: ChoiceLoopSnapshot
      if enabled {
        let generation = runtimeGeneration
        let proof = try await currentEnabledProof(expectedGeneration: generation)
        snapshot = try await core.reconcileChoiceMarkdown(proof: proof).validated()
        try requireCurrentOnGeneration(generation)
      } else {
        // This is deletion-only recovery of a Host-created, receipt-verified
        // journal. It neither publishes Markdown nor starts model/effect work,
        // so Global Off must not strand it behind an On proof.
        snapshot = try await core.cleanupChoiceMarkdownReceipt().validated()
      }
      adoptCommittedChoiceLoopSnapshot(snapshot)
      errorMessage = nil
    } catch {
      // A journal/receipt failure is continuity state, not dismissible local
      // feedback. Keep the last verified snapshot and leave Off/cancel usable
      // while making the required recovery visible across refreshes.
      choiceLoopContinuityState = .needsYou(.readFailed)
      errorMessage = userMessage(for: error)
    }
  }

  private func matchesChoiceMarkdownRecoveryState(
    _ state: String?, receiptCleanupAvailable: Bool
  ) -> Bool {
    state == "awaitingConfirmation" || state == "executing"
      || (state == "cancelled" && receiptCleanupAvailable)
  }

  /// Compatibility for the existing Dashboard action wiring. The route is no
  /// longer an Outcome proposal: it delegates only to Host-owned choice.begin.
  public func submitPrompt() async {
    choiceQuestion = prompt
    await submitHomeComposer()
    prompt = choiceQuestion
  }

  public func chooseModel(_ identifier: String) {
    guard let model = availableModels.first(where: { $0.id == identifier }) else {
      selectedModelId = ""
      selectedModelEffort = ""
      return
    }
    selectedModelId = model.id
    selectedModelEffort = model.supportedReasoningEfforts.isEmpty ? "not_applicable" : ""
  }

  public func chooseModelEffort(_ effort: String) {
    guard selectedCatalogModelEfforts.contains(effort) else {
      selectedModelEffort = ""
      return
    }
    selectedModelEffort = effort
  }

  public func persistSelectedModel() async {
    guard modelSelectionCanBeSaved, let model = selectedCatalogModel else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let selection = try await core.selectModel(
        modelId: model.id,
        requestedEffort: selectedModelEffort,
        catalogSnapshotId: catalogSnapshotId,
        catalogFingerprint: catalogFingerprint,
        catalogRevision: catalogRevision,
        proof: proof)
      try requireCurrentOnGeneration(generation)
      guard selection.modelId == model.id,
        selection.requestedEffort == selectedModelEffort,
        selection.actualEffort == selectedModelEffort
      else {
        throw CoreClientError.contractViolation("Core did not preserve the selected model.")
      }
      let setupProof = try await currentEnabledProof(expectedGeneration: generation)
      let setup = try await core.modelSetup(proof: setupProof)
      try requireCurrentOnGeneration(generation)
      applyAccountCatalog(setup)
      guard modelSelectionStatus == .current, persistedModelSelection == selection else {
        throw CoreClientError.contractViolation(
          "Core could not bind the saved model selection to the current account catalog.")
      }
      try finishAccountSetupIfReady(expectedGeneration: generation)
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func connectIMessage() async {
    guard !isBusy, !iMessageIsConnected else { return }
    if let pairing = durablePairings[.iMessage] {
      guard channelEffectEntryEnabled else { return }
      await connectChannel(pairing, requireExistingPairing: true)
      return
    }
    guard modelEntryEnabled, let chatId = Int64(iMessageChatId), chatId > 0 else {
      errorMessage = "Load and choose a Messages conversation first."
      return
    }
    let owner = iMessageOwnerSender.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !owner.isEmpty, owner == iMessageOwnerSender else {
      errorMessage = "Choose the approved owner for this Messages conversation."
      return
    }
    await connectChannel(
      ChannelPairing(
        channel: .iMessage,
        ownerSenderId: owner,
        conversationId: String(chatId),
        imessage: iMessageChats.first(where: { $0.chatId == iMessageChatId }).map {
          IMessagePairingMetadata(
            chatGuid: $0.chatGuid,
            chatIdentifier: $0.chatIdentifier,
            service: $0.service,
            participantIds: $0.participants)
        },
        pairedAtMs: Self.currentMilliseconds()
      )
    )
  }

  public func refreshIMessageChats() async {
    guard modelEntryEnabled, !isBusy, !iMessageIsConnected else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      _ = try await core.stopChannel(.iMessage)
      try requireCurrentOnGeneration(generation)
      let prepareProof = try await currentEnabledProof(expectedGeneration: generation)
      try await core.prepareIMessageChatDiscovery(proof: prepareProof)
      try requireCurrentOnGeneration(generation)
      let listProof = try await currentEnabledProof(expectedGeneration: generation)
      let listedChats = try await core.listPreparedIMessageChats(proof: listProof)
      let chats = try IMessageChatsResponse(chats: listedChats).validated().chats
      try requireCurrentOnGeneration(generation)
      iMessageChats = chats
      if !chats.contains(where: { $0.chatId == iMessageChatId }) {
        iMessageChatId = ""
        iMessageOwnerSender = ""
      } else if !iMessageOwnerOptions.contains(iMessageOwnerSender) {
        iMessageOwnerSender = ""
      }
      errorMessage = chats.isEmpty ? "No Messages conversations found." : nil
    } catch {
      guard generation == runtimeGeneration else { return }
      _ = try? await core.stopChannelIfRunning(.iMessage)
      guard generation == runtimeGeneration else { return }
      iMessageStatus = "faulted"
      errorMessage = userMessage(for: error)
    }
  }

  public func selectIMessageChat(_ chatId: String) {
    guard !iMessageIsConnected else { return }
    iMessageChatId = chatId
    if !iMessageOwnerOptions.contains(iMessageOwnerSender) {
      iMessageOwnerSender = ""
    }
  }

  public func selectIMessageOwner(_ ownerSender: String) {
    guard !iMessageIsConnected else { return }
    iMessageOwnerSender = ownerSender
  }

  public func connectDiscord() async {
    guard !isBusy, discordStatus != "connected" else { return }
    let reconnectingDurablePairing = durablePairings[.discord] != nil
    guard reconnectingDurablePairing ? channelEffectEntryEnabled : modelEntryEnabled else {
      return
    }
    let token = discordTokenDraft
    discardDiscordTokenDraft()
    let generation = runtimeGeneration
    do {
      if !token.isEmpty { try discordTokenStore.save(token) }
      guard let storedToken = try discordTokenStore.load() else {
        throw CoreClientError.contractViolation("Paste the official Discord bot token once.")
      }
      isBusy = true
      defer { isBusy = false }
      if let pairing = try await core.channelPairing(.discord) {
        try validateDurablePairing(pairing, expected: .discord)
        durablePairings[.discord] = pairing
        try await startDurableDiscordPairing(token: storedToken, generation: generation)
      } else {
        _ = try await core.stopChannel(.discord)
        try requireCurrentOnGeneration(generation)
        let proof = try await currentEnabledProof(expectedGeneration: generation)
        let setup = try (await core.startDiscordSetup(token: storedToken, proof: proof)).validated()
        try requireCurrentOnGeneration(generation)
        discordSetup = setup
        discordPairingCandidate = nil
        discordStatus = setup.status
        discordSetupFeedback =
          "Discord security key saved in Keychain. Continue the official setup below."
      }
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      discordStatus = "faulted"
      discordSetupFeedback = "Discord setup failed safely. Review the status and retry."
      errorMessage = userMessage(for: error)
    }
  }

  public func discardDiscordTokenDraft() {
    discordTokenDraft = ""
  }

  public func checkDiscordPairingMessage() async {
    guard discordSetupCheckEnabled, let setup = discordSetup else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let result = try (await core.pollDiscordSetup(proof: proof))
        .validated(expectedIdentity: setup.identity)
      try requireCurrentOnGeneration(generation)
      discordStatus = result.status
      discordPairingCandidate = result.candidate
      discordSetupFeedback =
        result.candidate == nil
        ? "No approved pairing message found yet."
        : "Discord pairing message and permissions verified. Confirm the exact owner and channel."
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      discordStatus = "faulted"
      discordSetupFeedback = "Discord setup check failed safely. Review the status and retry."
      errorMessage = userMessage(for: error)
    }
  }

  public func confirmDiscordPairing() async {
    guard discordSetupConfirmationEnabled, let candidate = discordPairingCandidate,
      let setup = discordSetup
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      _ = try candidate.validated(expectedIdentity: setup.identity)
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      try await core.confirmDiscordSetup(
        candidateId: candidate.candidateId,
        confirmedAtMs: Self.currentMilliseconds(),
        proof: proof
      )
      guard let pairing = try await core.channelPairing(.discord) else {
        throw CoreClientError.contractViolation(
          "Core did not persist the confirmed Discord pairing."
        )
      }
      try validateDurablePairing(pairing, expected: .discord)
      durablePairings[.discord] = pairing
      guard let token = try discordTokenStore.load() else {
        throw CoreClientError.contractViolation("Discord Keychain setup is incomplete.")
      }
      let startProof = try await currentEnabledProof(expectedGeneration: generation)
      let status = try (await core.startDiscord(token: token, proof: startProof)).validated()
      try requireCurrentOnGeneration(generation)
      discordSetup = nil
      discordPairingCandidate = nil
      updateDiscordConnectionFeedback(status.status)
      connectedChannels.insert(.discord)
      startChannelPolling()
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      if await recoverCommittedDiscordConfirmation(candidate, generation: generation) {
        errorMessage = nil
        return
      }
      guard generation == runtimeGeneration else { return }
      discordStatus = "faulted"
      discordSetupFeedback = "Discord confirmation failed safely. Review the status and retry."
      errorMessage = userMessage(for: error)
    }
  }

  private func startDurableDiscordPairing(token: String, generation: UInt64) async throws {
    let proof = try await currentEnabledProof(
      expectedGeneration: generation,
      prepareModelRuntime: false
    )
    let status = try (await core.startDiscord(token: token, proof: proof)).validated()
    try requireCurrentOnGeneration(generation)
    discordSetup = nil
    discordPairingCandidate = nil
    updateDiscordConnectionFeedback(status.status)
    connectedChannels.insert(.discord)
    startChannelPolling()
  }

  private func recoverCommittedDiscordConfirmation(
    _ candidate: DiscordPairingCandidate, generation: UInt64
  ) async -> Bool {
    do {
      guard let pairing = try await core.channelPairing(.discord),
        (try? pairing.validated(expectedChannel: .discord)) != nil,
        pairing.channel == .discord,
        pairing.requireExplicitAddress,
        pairing.ownerSenderId == candidate.ownerUserId,
        pairing.conversationId == candidate.channelId,
        pairing.discord?.guildId == candidate.guildId,
        pairing.discord?.botUserId == candidate.botUserId,
        pairing.discord?.applicationId == candidate.applicationId,
        pairing.discord?.setupSourceMessageId == candidate.sourceMessageId,
        pairing.discord?.setupCandidateId == candidate.candidateId,
        let token = try discordTokenStore.load()
      else { return false }
      try requireCurrentOnGeneration(generation)
      try await startDurableDiscordPairing(token: token, generation: generation)
      return true
    } catch {
      return false
    }
  }

  public func prepareAdditionalRoute(_ channel: ChannelKind) async {
    guard channelEffectEntryEnabled, !isBusy, let mission = confirmedMission,
      let routeSet = channelRouteSet, routeSet.missionId == mission.missionId
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      guard let pairing = try await core.channelPairing(channel) else {
        throw CoreClientError.contractViolation(
          "Pair and verify this channel before adding it to the Mission."
        )
      }
      _ = try pairing.validated(expectedChannel: channel)
      try requireCurrentOnGeneration(generation)
      guard
        !routeSet.routes.contains(where: {
          $0.channel == pairing.channel
            && $0.conversationId == pairing.conversationId
            && $0.ownerSenderId == pairing.ownerSenderId
        })
      else {
        throw CoreClientError.contractViolation(
          "This exact channel route is already bound to the Mission."
        )
      }
      pendingAdditionalRoute = ChannelRouteDraft(
        approvalId: "route-approval-\(UUID().uuidString.lowercased())",
        missionId: mission.missionId,
        expectedRouteSetRevision: routeSet.revision,
        pairing: pairing
      )
      routeAllowsProgress = false
      routeAllowsNeedYou = false
      routeAllowsReceipt = false
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func cancelAdditionalRoute() {
    pendingAdditionalRoute = nil
    routeAllowsProgress = false
    routeAllowsNeedYou = false
    routeAllowsReceipt = false
  }

  public func confirmAdditionalRoute() async {
    guard modelEntryEnabled, !isBusy, let draft = pendingAdditionalRoute,
      channelRouteSet?.missionId == draft.missionId,
      channelRouteSet?.revision == draft.expectedRouteSetRevision
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      var outbound: [ChannelMessageKind] = []
      if routeAllowsNeedYou { outbound.append(.needYou) }
      if routeAllowsProgress { outbound.append(.progress) }
      if routeAllowsReceipt { outbound.append(.receipt) }
      let approval = ChannelRouteApproval(
        approvalId: draft.approvalId,
        missionId: draft.missionId,
        expectedRouteSetRevision: draft.expectedRouteSetRevision,
        channel: draft.pairing.channel,
        conversationId: draft.pairing.conversationId,
        ownerSenderId: draft.pairing.ownerSenderId,
        providerIdentity: draft.providerIdentity,
        allowedInboundClasses: [.missionParticipation, .needYouResponse],
        allowedOutboundClasses: outbound,
        actorId: "openopen-local-owner",
        decision: .approve,
        decidedAtMs: Self.currentMilliseconds()
      )
      let proof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false
      )
      let routeSet = try (await core.bindChannelRoute(approval, proof: proof))
        .validated(expectedMissionId: draft.missionId)
      try requireCurrentOnGeneration(generation)
      guard routeSet.missionId == draft.missionId,
        routeSet.revision == draft.expectedRouteSetRevision + 1,
        routeSet.routes.contains(where: {
          $0.approvalId == draft.approvalId
            && $0.channel == draft.pairing.channel
            && $0.conversationId == draft.pairing.conversationId
            && $0.ownerSenderId == draft.pairing.ownerSenderId
            && $0.providerIdentity == draft.providerIdentity
            && $0.allowedInboundClasses == [.missionParticipation, .needYouResponse]
            && $0.allowedOutboundClasses == outbound
        })
      else {
        throw CoreClientError.contractViolation(
          "Core did not bind the exact approved Mission route."
        )
      }
      channelRouteSet = routeSet
      pendingAdditionalRoute = nil
      reconcileSelectedRoute()
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  private func connectChannel(
    _ requested: ChannelPairing,
    requireExistingPairing: Bool = false
  ) async {
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      guard requested.channel == .iMessage else {
        throw CoreClientError.contractViolation("Discord pairing requires the setup wizard.")
      }
      _ = try await core.stopChannel(.iMessage)
      try requireCurrentOnGeneration(generation)
      let pairing: ChannelPairing
      let usesExistingPairing: Bool
      if let existing = try await core.channelPairing(requested.channel) {
        _ = try existing.validated(expectedChannel: requested.channel)
        if existing.channel == .iMessage, existing.imessage == nil,
          requested.imessage != nil, !requireExistingPairing,
          existing.ownerSenderId == requested.ownerSenderId,
          existing.conversationId == requested.conversationId
        {
          let proof = try await currentEnabledProof(expectedGeneration: generation)
          try await core.pairChannel(requested, proof: proof)
          pairing = requested
          durablePairings[requested.channel] = requested
          usesExistingPairing = false
        } else {
          guard existing.channel == requested.channel,
            existing.ownerSenderId == requested.ownerSenderId,
            existing.conversationId == requested.conversationId,
            existing.imessage == requested.imessage,
            !existing.requireExplicitAddress
          else {
            throw CoreClientError.contractViolation(
              "This channel is already paired to a different owner or conversation."
            )
          }
          pairing = existing
          usesExistingPairing = true
        }
      } else {
        guard !requireExistingPairing, modelEntryEnabled else {
          throw CoreClientError.contractViolation(
            "The approved Messages pairing is no longer available."
          )
        }
        let proof = try await currentEnabledProof(expectedGeneration: generation)
        try await core.pairChannel(requested, proof: proof)
        pairing = requested
        durablePairings[requested.channel] = requested
        usesExistingPairing = false
      }
      let proof = try await currentEnabledProof(
        expectedGeneration: generation,
        prepareModelRuntime: !usesExistingPairing
      )
      try await core.prepareIMessage(proof: proof)
      let activationProof = try await currentEnabledProof(
        expectedGeneration: generation,
        prepareModelRuntime: !usesExistingPairing
      )
      let status = try (await core.activateIMessage(proof: activationProof)).validated()
      try requireCurrentOnGeneration(generation)
      iMessageStatus = status.status
      guard status.status == "connected" else {
        throw CoreClientError.contractViolation(
          "The approved iMessage listener did not connect."
        )
      }
      channelListenerFeedback.removeValue(forKey: .iMessage)
      durablePairings[.iMessage] = pairing
      connectedChannels.insert(pairing.channel)
      startChannelPolling()
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      _ = try? await core.stopChannelIfRunning(.iMessage)
      guard generation == runtimeGeneration else { return }
      iMessageStatus = "faulted"
      errorMessage = userMessage(for: error)
    }
  }

  private func startChannelPolling() {
    guard choiceCoreConnectionsAvailable else { return }
    guard channelTask == nil else { return }
    channelTask = Task { [weak self] in
      guard let self else { return }
      while !Task.isCancelled {
        if channelPollingEnabled, !isBusy {
          let permitsModelEntry = modelEntryEnabled
          let channels = connectedChannels
          for channel in channels where !Task.isCancelled {
            let generation = runtimeGeneration
            do {
              let proof = try await currentEnabledProof(
                expectedGeneration: generation,
                prepareModelRuntime: permitsModelEntry
              )
              try requireCurrentOnGeneration(generation)
              let result = try
                (await core.pollChannel(
                  channel,
                  modelWorkAllowed: permitsModelEntry,
                  proof: proof
                ))
                .validated(for: channel)
              try requireCurrentOnGeneration(generation)
              guard !Task.isCancelled else { return }
              if !permitsModelEntry, result.suggestion != nil {
                throw CoreClientError.contractViolation(
                  "Core returned unbound model work while account or model readiness was unavailable."
                )
              }
              if channel == .iMessage {
                if iMessageStatus != result.connectionStatus {
                  iMessageStatus = result.connectionStatus
                }
                if result.connectionStatus == "connected" {
                  if channelListenerFeedback[.iMessage] != nil {
                    channelListenerFeedback.removeValue(forKey: .iMessage)
                  }
                }
                if result.eventStatus == "deferred" {
                  await refreshChoiceLoopContinuity(expectedGeneration: generation)
                  if let snapshot = choiceLoopSnapshot,
                    snapshot.session.state == "interpreting"
                  {
                    awaitInitialChoiceResult(
                      expectedGeneration: generation,
                      sessionID: snapshot.session.id,
                      prepareIMessageReply: true
                    )
                  }
                }
              }
              if channel == .discord {
                updateDiscordConnectionFeedback(result.connectionStatus)
              }
              if let event = try validatedChannelMissionEvent(in: result, channel: channel) {
                latestChannelMissionEvent = event
              }
              if !hasNonterminalMission,
                result.eventStatus == "recovering" || result.eventStatus == "superseded",
                suggestion.map(Self.isChannelSuggestion) == true
              {
                suggestion = nil
              }
              if result.eventStatus == "needYou" {
                guard let incidents = result.failureIncidents, !incidents.isEmpty else {
                  throw CoreClientError.contractViolation(
                    "Core omitted the durable terminal incident behind Need you."
                  )
                }
                try mergeChannelFailureIncidents(incidents)
                if !hasNonterminalMission,
                  let invalidated = result.invalidateSuggestionId,
                  suggestion?.id == invalidated
                {
                  suggestion = nil
                }
                continue
              }
              if !hasNonterminalMission, let proposed = result.suggestion {
                // Host releases only the newest recovery-arbitrated channel
                // Outcome. Replace a previously displayed channel result so
                // an offline correction cannot be silently ignored.
                suggestion = proposed
                receipt = nil
              }
            } catch CoreClientError.remote(let code, _) where code == -32_011 {
              guard generation == runtimeGeneration, !Task.isCancelled else { return }
              // Another explicitly initiated operation owns the single model slot.
            } catch CoreClientError.remote(let code, _)
              where [-32_014, -32_015, -32_016, -32_017].contains(code)
            {
              guard generation == runtimeGeneration, !Task.isCancelled else { return }
              errorMessage = nil
              channelFailureFeedback =
                "A background result failed safely and was not run again. OpenOpen is restoring its verified local runtime."
              beginCoreRecovery(
                shuttingDownCurrentCore: true,
                dueToTerminalChannelFailure: true
              )
              return
            } catch {
              guard generation == runtimeGeneration, !Task.isCancelled else { return }
              if channel == .iMessage { iMessageStatus = "faulted" }
              if channel == .discord {
                updateDiscordConnectionFeedback("faulted")
              }
              errorMessage = nil
              channelListenerFeedback[channel] =
                "The \(channel == .iMessage ? "Messages" : "Discord") listener paused safely. Reconnect it in Settings; other local controls remain available."
              connectedChannels.remove(channel)
            }
          }
        }
        try? await Task.sleep(for: channelPollInterval)
      }
    }
  }

  private func validatedChannelMissionEvent(
    in result: ChannelPollResponse,
    channel: ChannelKind
  ) throws -> ChannelMissionEvent? {
    let carriesMissionEvent =
      result.eventStatus == "missionUpdated"
      || result.eventStatus == "missionUpdateRecovered"
    guard let event = result.missionEvent else {
      if carriesMissionEvent {
        throw CoreClientError.contractViolation(
          "Core omitted a required Mission participation event."
        )
      }
      return nil
    }
    _ = try event.validated()
    guard carriesMissionEvent,
      let mission = confirmedMission,
      let routeSet = channelRouteSet,
      event.channel == channel,
      event.missionId == mission.missionId,
      routeSet.missionId == event.missionId,
      event.routeSetRevision <= routeSet.revision,
      routeSet.routes.contains(where: {
        $0.routeId == event.routeId && $0.channel == event.channel
          && $0.revision <= event.routeSetRevision
          && $0.allowedInboundClasses.contains(event.messageClass)
      })
    else {
      throw CoreClientError.contractViolation(
        "Core returned a Mission participation event outside the exact route set."
      )
    }
    return event
  }

  private func updateDiscordConnectionFeedback(_ status: String) {
    let normalizedStatus: String
    let feedback: String
    switch status {
    case "connected":
      normalizedStatus = status
      feedback = "Discord connected to the approved channel."
    case "connecting":
      normalizedStatus = status
      feedback = "Discord is connecting to the official Gateway."
    case "reconnecting":
      normalizedStatus = status
      feedback = "Discord is reconnecting to the official Gateway."
    case "disconnected":
      normalizedStatus = status
      feedback = "Discord is disconnected. No connection is active."
    case "faulted":
      normalizedStatus = status
      feedback = "Discord connection failed safely. Review the status and retry."
    default:
      normalizedStatus = "faulted"
      feedback = "Discord returned an unknown state. No connection is claimed."
    }
    if discordStatus != normalizedStatus {
      discordStatus = normalizedStatus
    }
    if discordSetupFeedback != feedback {
      discordSetupFeedback = feedback
    }
    if normalizedStatus == "connected", channelListenerFeedback[.discord] != nil {
      channelListenerFeedback.removeValue(forKey: .discord)
    }
  }

  public func sendChannelProgress() async {
    guard historicalMissionEffectsAvailable else {
      errorMessage = "Channel delivery is unavailable during local Choice Core."
      return
    }
    let value = channelMessageDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    guard channelEffectEntryEnabled, !isBusy, let mission = confirmedMission,
      channelRouteSet?.missionId == mission.missionId,
      selectedChannelRoute(for: .progress) != nil,
      !value.isEmpty, value == channelMessageDraft
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      try await deliverChannelMessage(
        missionId: mission.missionId,
        kind: .progress,
        content: value,
        approvedAtMs: Self.currentMilliseconds(),
        generation: generation
      )
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func sendChannelNeedYou() async {
    guard historicalMissionEffectsAvailable else {
      errorMessage = "Channel delivery is unavailable during local Choice Core."
      return
    }
    guard channelEffectEntryEnabled, !isBusy, let needsYou,
      channelRouteSet?.missionId == needsYou.missionId,
      selectedChannelRoute(for: .needYou) != nil
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      try await deliverChannelMessage(
        missionId: needsYou.missionId,
        kind: .needYou,
        content: "Need you: \(needsYou.prompt)",
        approvedAtMs: max(Self.currentMilliseconds(), needsYou.createdAtMs),
        generation: generation
      )
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func sendChannelReceipt() async {
    guard historicalMissionEffectsAvailable else {
      errorMessage = "Channel delivery is unavailable during local Choice Core."
      return
    }
    guard channelEffectEntryEnabled, !isBusy, let receipt,
      channelRouteSet?.missionId == receipt.missionId,
      selectedChannelRoute(for: .receipt) != nil
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      try await deliverChannelMessage(
        missionId: receipt.missionId,
        kind: .receipt,
        content: Self.channelReceiptContent(receipt),
        approvedAtMs: max(Self.currentMilliseconds(), receipt.completedAtMs),
        generation: generation
      )
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  private func deliverChannelMessage(
    missionId: String,
    kind: ChannelMessageKind,
    content: String,
    approvedAtMs: Int64,
    generation: UInt64
  ) async throws {
    guard let route = selectedChannelRoute(for: kind) else {
      throw CoreClientError.contractViolation(
        "The selected Mission route is not approved for this message class."
      )
    }
    let proof = try await currentEnabledProof(
      expectedGeneration: generation,
      prepareModelRuntime: false
    )
    let result = try
      (await core.sendChannelMessage(
        missionId: missionId,
        routeId: route.routeId,
        kind: kind,
        content: content,
        approvedAtMs: approvedAtMs,
        proof: proof
      )).validated()
    try requireCurrentOnGeneration(generation)
    if result.status == "needYou" {
      throw CoreClientError.contractViolation(
        "Delivery is uncertain. OpenOpen will not resend it automatically."
      )
    }
    guard result.status == "sent", result.providerMessageId != nil else {
      throw CoreClientError.contractViolation("The channel did not confirm this send.")
    }
  }

  private func reconcileSelectedRoute() {
    selectedChannelRouteId = reconciledSelectedRouteId(for: channelRouteSet)
  }

  private func reconciledSelectedRouteId(for routeSet: ChannelRouteSet?) -> String {
    guard let routeSet else { return "" }
    if routeSet.routes.contains(where: { $0.routeId == selectedChannelRouteId }) {
      return selectedChannelRouteId
    }
    return routeSet.primaryRouteId
  }

  private func selectedChannelRoute(for kind: ChannelMessageKind) -> ChannelRoute? {
    channelRouteSet?.routes.first {
      $0.routeId == selectedChannelRouteId && $0.allowedOutboundClasses.contains(kind)
        && connectedChannels.contains($0.channel)
        && channelIsEffectReady($0.channel)
    }
  }

  private func channelIsEffectReady(_ channel: ChannelKind) -> Bool {
    switch channel {
    case .iMessage: iMessageStatus == "connected"
    case .discord: discordStatus == "connected"
    }
  }

  private static func channelReceiptContent(_ receipt: MissionReceipt) -> String {
    let count = receipt.evidenceIds.count
    return
      "Done: \(receipt.summary)\nEvidence: \(count) verified completion\(count == 1 ? "" : "s")\nModel: \(receipt.actualModel)"
  }

  private static func currentMilliseconds() -> Int64 {
    Int64((Date().timeIntervalSince1970 * 1_000).rounded(.down))
  }

  public func confirmSuggestion() async {
    guard historicalMissionEffectsAvailable else {
      errorMessage = "Historical Mission actions are unavailable during local Choice Core."
      return
    }
    guard modelEntryEnabled, !isBusy, suggestion != nil || confirmedMission != nil else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      try requireCurrentOnGeneration(generation)
      let mission: ConfirmedMission
      if let confirmedMission {
        guard suggestion.map({ Self.matches(confirmedMission, suggestion: $0) }) ?? true else {
          throw CoreClientError.contractViolation(
            "The pending Mission does not match the current suggestion."
          )
        }
        mission = confirmedMission
      } else {
        guard let suggestion else {
          throw CoreClientError.contractViolation("There is no suggestion to confirm.")
        }
        let target = try await reminders.prepareTarget()
        try Task.checkCancellation()
        try requireCurrentOnGeneration(generation)
        mission = try await core.confirmSuggestion(
          identifier: suggestion.id, reminderTarget: target
        )
        try requireCurrentOnGeneration(generation)
        _ = try mission.validated()
        guard Self.matches(mission, suggestion: suggestion) else {
          throw CoreClientError.contractViolation(
            "Core confirmed a Mission that does not match the exact suggestion."
          )
        }
        publishConfirmedMission(mission)
        let dashboard = try await core.dashboard()
        _ = try dashboard.validated()
        channelRouteSet = dashboard.channelRouteSet
        reconcileSelectedRoute()
      }
      try Task.checkCancellation()
      try requireCurrentOnGeneration(generation)
      guard
        mission.reminderAuthorization.validates(
          missionId: mission.missionId, workItems: mission.workItems
        )
      else {
        throw CoreClientError.contractViolation(
          "Core did not authorize the exact Reminder write."
        )
      }
      if !mission.reminderLinks.isEmpty {
        reminderLinks = mission.reminderLinks
        let count = mission.reminderLinks.count
        activeCards = [
          ActiveOutcomeCard(
            id: mission.missionId,
            title: mission.title,
            state: "Waiting for \(count) Reminder completion\(count == 1 ? "" : "s")"
          )
        ]
        self.suggestion = nil
        errorMessage = nil
        return
      }
      let dispatchStart = try await core.beginReminderDispatch(identifier: mission.missionId)
      try Task.checkCancellation()
      try requireCurrentOnGeneration(generation)
      _ = try dispatchStart.mission.validated()
      guard Self.matchesDispatchStart(dispatchStart, mission: mission) else {
        throw CoreClientError.contractViolation(
          "Core did not durably bind the exact Reminder dispatch."
        )
      }
      publishConfirmedMission(dispatchStart.mission)
      if !dispatchStart.mission.reminderLinks.isEmpty {
        reminderLinks = dispatchStart.mission.reminderLinks
        let count = dispatchStart.mission.reminderLinks.count
        activeCards = [
          ActiveOutcomeCard(
            id: dispatchStart.mission.missionId,
            title: dispatchStart.mission.title,
            state: "Waiting for \(count) Reminder completion\(count == 1 ? "" : "s")"
          )
        ]
        self.suggestion = nil
        errorMessage = nil
        return
      }
      let links =
        if dispatchStart.executeNow {
          try await reminders.executeInitialMirror(dispatchStart)
        } else {
          try await reminders.recoverMirror(for: dispatchStart.mission)
        }
      try Task.checkCancellation()
      try requireCurrentOnGeneration(generation)
      guard links.count == mission.workItems.count,
        Set(links.map(\.workItemId)) == Set(mission.workItems.map(\.id))
      else {
        throw CoreClientError.contractViolation(
          "Reminders did not return an exact link for every Mission step."
        )
      }
      let persisted = try await core.recordReminderMirror(
        identifier: mission.missionId, links: links
      )
      try Task.checkCancellation()
      try requireCurrentOnGeneration(generation)
      _ = try persisted.validated()
      guard persisted.missionId == mission.missionId,
        persisted.reminderAuthorization == dispatchStart.mission.reminderAuthorization,
        persisted.reminderDispatch == dispatchStart.mission.reminderDispatch,
        persisted.reminderLinks == links
      else {
        throw CoreClientError.contractViolation(
          "Core did not persist the exact Reminder mirror."
        )
      }
      publishConfirmedMission(persisted)
      reminderLinks = persisted.reminderLinks
      activeCards = [
        ActiveOutcomeCard(
          id: mission.missionId,
          title: mission.title,
          state: "Waiting for \(links.count) Reminder completion\(links.count == 1 ? "" : "s")"
        )
      ]
      self.suggestion = nil
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      if let mission = confirmedMission {
        activeCards = [
          ActiveOutcomeCard(
            id: mission.missionId,
            title: mission.title,
            state: "Need you: inspect the OpenOpen Reminders list before retrying"
          )
        ]
      }
      errorMessage = userMessage(for: error)
    }
  }

  public func cancelMission(identifier: String) async {
    guard historicalMissionEffectsAvailable else {
      errorMessage = "Historical Mission actions are unavailable during local Choice Core."
      return
    }
    guard !isBusy, dashboardControls.missionCancellationEnabled,
      activeCards.contains(where: { $0.id == identifier })
    else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      let cancellation = try await core.cancelMission(identifier: identifier, proof: proof)
      _ = try cancellation.validated(expectedMissionId: identifier)
      try requireCurrentOnGeneration(generation)
      let dashboard = try await core.dashboard()
      try requireCurrentOnGeneration(generation)
      _ = try dashboard.validated()
      guard !dashboard.activeCards.contains(where: { $0.id == identifier }),
        dashboard.confirmedMission?.missionId != identifier,
        dashboard.needsYou?.missionId != identifier,
        dashboard.receipt?.missionId != identifier
      else {
        throw CoreClientError.contractViolation(
          "Core did not publish the exact terminal Mission cancellation."
        )
      }
      try applyDashboard(dashboard)
      errorMessage =
        "Mission stopped. OpenOpen did not retry or remove any Reminders."
    } catch {
      guard generation == runtimeGeneration else { return }
      let cancellationError = error
      do {
        let dashboard = try await core.dashboard()
        try requireCurrentOnGeneration(generation)
        _ = try dashboard.validated()
        let remainsActive =
          dashboard.activeCards.contains(where: { $0.id == identifier })
          || dashboard.confirmedMission?.missionId == identifier
          || dashboard.needsYou?.missionId == identifier
        try applyDashboard(dashboard)
        if remainsActive {
          errorMessage = userMessage(for: cancellationError)
        } else if dashboard.receipt?.missionId == identifier {
          errorMessage = "This Mission already finished with verified Evidence."
        } else {
          errorMessage =
            "Mission is no longer active. OpenOpen did not retry or remove any Reminders."
        }
      } catch {
        errorMessage = userMessage(for: cancellationError)
      }
    }
  }

  private static func matchesDispatchStart(
    _ start: ReminderDispatchStart, mission: ConfirmedMission
  ) -> Bool {
    let dispatched = start.mission
    return dispatched.missionId == mission.missionId
      && dispatched.title == mission.title
      && dispatched.workItems == mission.workItems
      && dispatched.reminderAuthorization == mission.recoveryOnly().reminderAuthorization
      && dispatched.reminderDispatch.count == mission.workItems.count
      && Set(dispatched.reminderDispatch.map(\.workItemId)) == Set(mission.workItems.map(\.id))
      && Set(dispatched.reminderDispatch.map(\.token)).count
        == dispatched.reminderDispatch.count
      && dispatched.reminderDispatch.allSatisfy {
        !$0.token.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      }
      && (!start.executeNow || dispatched.reminderLinks.isEmpty)
  }

  public func checkMissionProgress() async {
    guard historicalMissionEffectsAvailable else {
      errorMessage = "Historical Mission actions are unavailable during local Choice Core."
      return
    }
    guard storeControlEnabled, !isBusy, let mission = confirmedMission,
      !reminderLinks.isEmpty
    else {
      return
    }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      _ = try await currentEnabledProof(
        expectedGeneration: generation,
        prepareModelRuntime: false
      )
      let completed = try await reminders.completedReminders(for: reminderLinks)
      try Task.checkCancellation()
      try requireCurrentOnGeneration(generation)
      guard completed.count == mission.workItems.count,
        Set(completed.map(\.workItemId)) == Set(mission.workItems.map(\.id))
      else {
        throw CoreClientError.contractViolation(
          "Finish every OpenOpen reminder, then check progress again."
        )
      }
      // Completing from durable Reminder Evidence is local Store work. A
      // return route is authorized only when the separate model/outbound gate
      // is also ready; awaiting account/catalog completion must stay local.
      let receiptRoute = channelEffectEntryEnabled ? selectedChannelRoute(for: .receipt) : nil
      let receipt = try
        (await core.completeReminderMission(
          identifier: mission.missionId,
          completions: completed,
          receiptReturnApprovedAtMs: receiptRoute == nil ? nil : Self.currentMilliseconds(),
          receiptReturnRouteId: receiptRoute?.routeId
        )).validated()
      try requireCurrentOnGeneration(generation)
      guard receipt.missionId == mission.missionId else {
        throw CoreClientError.contractViolation("Core returned a Receipt for another Mission.")
      }
      var returnError: Error?
      if receiptRoute != nil {
        do {
          try await deliverChannelMessage(
            missionId: receipt.missionId,
            kind: .receipt,
            content: Self.channelReceiptContent(receipt),
            approvedAtMs: max(Self.currentMilliseconds(), receipt.completedAtMs),
            generation: generation
          )
        } catch {
          returnError = error
        }
      }
      // Completion can reveal a different queued Mission or Need-you item.
      // Rebuild the visible state from the authoritative Store only after the
      // bounded return attempt, so a completed focus never creates a transient
      // false Done screen or hides the next safe action.
      let dashboard = try await core.dashboard()
      try requireCurrentOnGeneration(generation)
      _ = try dashboard.validated()
      guard dashboard.receipt == receipt || !dashboard.activeCards.isEmpty else {
        throw CoreClientError.contractViolation(
          "Core did not publish the completed Receipt or the next Mission."
        )
      }
      try applyDashboard(dashboard)
      if let returnError { throw returnError }
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func requestSuggestionConfirmation() {
    guard historicalMissionEffectsAvailable else {
      errorMessage = "Historical Mission actions are unavailable during local Choice Core."
      return
    }
    // This is retained only for recovery of an already-durable historical
    // Mission. The PR1 Choice surface never invokes it to create a new
    // proposal or Reminder effect; its own confirmation route is choice.confirm.
    guard heroTask == nil else { return }
    heroTask = Task { [weak self] in
      await self?.confirmSuggestion()
      self?.heroTask = nil
    }
  }

  public func requestMissionProgressCheck() {
    guard historicalMissionEffectsAvailable else {
      errorMessage = "Historical Mission actions are unavailable during local Choice Core."
      return
    }
    guard heroTask == nil else { return }
    heroTask = Task { [weak self] in
      await self?.checkMissionProgress()
      self?.heroTask = nil
    }
  }

  public func refreshAccountAndModels() async {
    guard accountSetupEnabled else {
      clearTransientModelSetup()
      return
    }
    let generation = runtimeGeneration
    do {
      let setupProof = try await currentEnabledProof(expectedGeneration: generation)
      let setup = try await core.modelSetup(proof: setupProof)
      try requireCurrentOnGeneration(generation)
      applyAccountCatalog(setup)
      try finishAccountSetupIfReady(expectedGeneration: generation)
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func connectChatGpt() async {
    guard accountSetupEnabled, !isBusy else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    var loginRuntimeActive = false
    do {
      let identity = try await provisionBrokerTrust()
      if managedLoginMayHaveCompleted {
        try await ensureCodexReady(coreIdentity: identity)
        let setupProof = try await currentEnabledProof(expectedGeneration: generation)
        let setup = try await core.modelSetup(proof: setupProof)
        if case .chatGpt = setup.account {
          try requireCurrentOnGeneration(generation)
          applyAccountCatalog(setup)
          managedLoginMayHaveCompleted = false
          try finishAccountSetupIfReady(expectedGeneration: generation)
          errorMessage = nil
          return
        }
        managedLoginMayHaveCompleted = false
      }
      try await ensureLoginCodexReady(coreIdentity: identity)
      loginRuntimeActive = true
      let proof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      let login = try await core.beginLogin(proof: proof)
      try requireCurrentOnGeneration(generation)
      guard let url = URL(string: login.authUrl), url.scheme == "https" else {
        throw CoreClientError.contractViolation("OpenOpen received an invalid sign-in URL.")
      }
      guard openOfficialURL(url) else {
        throw CoreClientError.contractViolation(
          "OpenOpen could not open the official sign-in page.")
      }
      let currentProof = try await currentEnabledProof(
        expectedGeneration: generation, prepareModelRuntime: false)
      try await core.awaitLogin(identifier: login.loginId, proof: currentProof)
      managedLoginMayHaveCompleted = true
      loginRuntimeActive = false
      try requireCurrentOnGeneration(generation)
      let currentIdentity = try await provisionBrokerTrust()
      try await ensureCodexReady(coreIdentity: currentIdentity)
      let setupProof = try await currentEnabledProof(expectedGeneration: generation)
      let setup = try await core.modelSetup(proof: setupProof)
      try requireCurrentOnGeneration(generation)
      applyAccountCatalog(setup)
      managedLoginMayHaveCompleted = false
      try finishAccountSetupIfReady(expectedGeneration: generation)
      errorMessage = nil
    } catch {
      codexReadyCoreInstanceNonce = nil
      if loginRuntimeActive {
        try? await core.abortCodexCandidate()
      }
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func dismissError() {
    errorMessage = nil
  }

  public func canAcknowledgeChannelFailure(_ incident: ChannelFailureIncident) -> Bool {
    incident.acknowledgement == nil
      && channelFailureAcknowledgementTasks[incident.incidentId] == nil
      && accountSetupEnabled
  }

  public func acknowledgeChannelFailure(_ incidentId: String) {
    guard channelFailureAcknowledgementTasks[incidentId] == nil,
      let incident = channelFailureIncidents.first(where: { $0.incidentId == incidentId }),
      canAcknowledgeChannelFailure(incident)
    else { return }
    let token = UUID()
    channelFailureAcknowledgementTokens[incidentId] = token
    channelFailureAcknowledgementTasks[incidentId] = Task { [weak self] in
      await self?.performChannelFailureAcknowledgement(incident)
      guard self?.channelFailureAcknowledgementTokens[incidentId] == token else { return }
      self?.channelFailureAcknowledgementTasks[incidentId] = nil
      self?.channelFailureAcknowledgementTokens[incidentId] = nil
    }
  }

  private func performChannelFailureAcknowledgement(
    _ incident: ChannelFailureIncident
  ) async {
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(
        expectedGeneration: generation,
        prepareModelRuntime: false
      )
      let acknowledged = try await core.acknowledgeChannelFailure(
        incident,
        acknowledgedAtMs: Self.currentMilliseconds(),
        proof: proof
      )
      try requireCurrentOnGeneration(generation)
      let validated = try acknowledged.validatedAcknowledgementResponse(for: incident)
      try mergeChannelFailureIncidents([validated])
      channelFailureAcknowledgementFeedback.removeValue(forKey: incident.incidentId)
    } catch {
      guard generation == runtimeGeneration else { return }
      channelFailureAcknowledgementFeedback[incident.incidentId] =
        "OpenOpen could not verify and save that acknowledgement. The incident remains visible and no work was retried."
      return
    }
    do {
      let dashboard = try await core.dashboard()
      try requireCurrentOnGeneration(generation)
      _ = try dashboard.validated()
      try applyDashboard(dashboard)
    } catch {
      guard generation == runtimeGeneration else { return }
      channelFailureAcknowledgementFeedback[incident.incidentId] =
        "Acknowledgement saved. OpenOpen could not refresh the incident history; no acknowledgement or work was retried."
    }
  }

  private func mergeChannelFailureIncidents(_ incidents: [ChannelFailureIncident]) throws {
    publishChannelFailureIncidents(
      try projectedChannelFailureIncidents(merging: incidents)
    )
  }

  private func projectedChannelFailureIncidents(
    merging incidents: [ChannelFailureIncident]
  ) throws -> [ChannelFailureIncident] {
    _ = try ChannelFailureIncident.validateCollection(incidents)
    var byIdentifier = Dictionary(
      uniqueKeysWithValues: channelFailureIncidents.map { ($0.incidentId, $0) })
    for incident in incidents {
      if let existing = byIdentifier[incident.incidentId] {
        byIdentifier[incident.incidentId] = try existing.mergedMonotonically(with: incident)
      } else {
        byIdentifier[incident.incidentId] = incident
      }
    }
    let merged = byIdentifier.values.sorted {
      ($0.occurredAtMs, $0.incidentId) < ($1.occurredAtMs, $1.incidentId)
    }
    let unacknowledged = merged.filter { $0.acknowledgement == nil }.prefix(128)
    let remaining = 128 - unacknowledged.count
    let acknowledged = merged.reversed().filter { $0.acknowledgement != nil }.prefix(remaining)
    let projected = (Array(unacknowledged) + Array(acknowledged)).sorted {
      ($0.occurredAtMs, $0.incidentId) < ($1.occurredAtMs, $1.incidentId)
    }
    _ = try ChannelFailureIncident.validateCollection(projected)
    return projected
  }

  private func publishChannelFailureIncidents(_ incidents: [ChannelFailureIncident]) {
    for incident in incidents where incident.acknowledgement != nil {
      channelFailureAcknowledgementFeedback.removeValue(forKey: incident.incidentId)
    }
    if channelFailureIncidents != incidents { channelFailureIncidents = incidents }
  }

  private func cancelChannelFailureAcknowledgements() {
    for task in channelFailureAcknowledgementTasks.values {
      task.cancel()
    }
    channelFailureAcknowledgementTasks.removeAll()
    channelFailureAcknowledgementTokens.removeAll()
  }

  private func userMessage(for error: Error) -> String {
    (error as? LocalizedError)?.errorDescription ?? "OpenOpen failed closed. Please try again."
  }

  private static func matches(
    _ mission: ConfirmedMission, suggestion: OutcomeSuggestion
  ) -> Bool {
    mission.title == suggestion.title
      && mission.workItems.map(\.title) == suggestion.proposedSteps
  }

  private static func isChannelSuggestion(_ suggestion: OutcomeSuggestion) -> Bool {
    !suggestion.sourceRefs.isEmpty
      && suggestion.sourceRefs.allSatisfy { $0.hasPrefix("channel:") }
  }
}
