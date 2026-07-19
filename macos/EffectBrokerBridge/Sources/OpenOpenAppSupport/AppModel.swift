import AppKit
import Combine
import EffectBrokerBridge
import Foundation

public struct CoreGenerationFence: Equatable, Sendable {
  public let identifier: UUID
  public let generation: UInt64
}

public protocol CoreServing: Sendable {
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
  func propose(prompt: String, proof: BrokerRuntimeState) async throws -> OutcomeSuggestion
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
      "Need you: connect ChatGPT and verify GPT-5.6 Sol with high reasoning before OpenOpen finishes turning on."
    case .paused:
      "Need you: OpenOpen paused after Core stopped. No listener, model, or outbound work is running."
    }
  }
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
    prompt: String
  ) -> Self {
    let inputEnabled = modelEntryEnabled && !isBusy && !hasConfirmedMission && !hasNeedsYou
    let normalizedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
    return Self(
      // Runtime-control and navigation must remain reachable independently of
      // model, listener, incident, Mission, or recovery state.
      globalToggleEnabled: true,
      settingsEnabled: true,
      outcomeInputEnabled: inputEnabled,
      outcomeSubmitEnabled: inputEnabled && !normalizedPrompt.isEmpty
        && prompt.utf8.count <= 16 * 1024,
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
  @Published public private(set) var isBusy = false
  @Published public private(set) var errorMessage: String?
  @Published public var showsSettings = false
  @Published public private(set) var runtimeRecoveryState: RuntimeRecoveryState = .ready

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
  private var heroTask: Task<Void, Never>?
  private var channelTask: Task<Void, Never>?
  private var channelFailureAcknowledgementTasks: [String: Task<Void, Never>] = [:]
  private var channelFailureAcknowledgementTokens: [String: UUID] = [:]
  private var recoveringTerminalChannelFailure = false
  private var coreLifecycleTask: Task<Void, Never>?
  private var coreRecoveryTask: Task<Void, Never>?
  private var connectedChannels: Set<ChannelKind> = []
  private var durablePairings: [ChannelKind: ChannelPairing] = [:]
  private var loginItemRegistered = false
  private var runtimeGeneration: UInt64 = 0
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
      modelEntryEnabled: modelEntryEnabled,
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
      prompt: prompt
    )
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

  public func refreshDashboard() async {
    if let dashboardRefreshTask {
      await dashboardRefreshTask.value
      return
    }
    let identifier = UUID()
    let task = Task { [weak self] in
      guard let self else { return }
      await performDashboardRefresh()
      finishDashboardRefresh(identifier)
    }
    dashboardRefreshIdentifier = identifier
    dashboardRefreshTask = task
    await task.value
  }

  private func finishDashboardRefresh(_ identifier: UUID) {
    guard dashboardRefreshIdentifier == identifier else { return }
    dashboardRefreshIdentifier = nil
    dashboardRefreshTask = nil
  }

  private func performDashboardRefresh() async {
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
          if runtime.enabled, coreTerminationEvents != nil {
            let fencedDashboard = try await core.dashboard()
            try requireCurrentOnGeneration(generation)
            try applyDashboard(fencedDashboard)
            try await restoreDurableConnections(expectedGeneration: generation)
            accountReady = try await refreshRecoveredAccountAndModels(
              expectedGeneration: generation)
            try requireCurrentOnGeneration(generation)
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
        accountState = .notConnected
        availableModels = []
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
    let mustInterruptActiveCore =
      !requested
      && (dashboardRefreshTask != nil || heroTask != nil || channelTask != nil
        || coreRecoveryTask != nil
        || runtimeRecoveryState == .recovering || runtimeRecoveryState == .paused || isBusy
        || offCoreInterruptionFailed
        || onRequiresReplacementCoreRestoration)
    runtimeGeneration &+= 1
    desiredEnabled = requested
    pendingRuntimeIntent = requested
    if requested {
      offCoreInterruptionFailed = false
    }
    if !requested {
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
      if mustInterruptActiveCore {
        if shutdownCore() {
          // The next Core is a new process generation and therefore has no
          // in-memory broker enrollment. The exact old Core is already proven
          // stopped, so installing trust on the quiescent replacement before
          // it validates checkpointed runtime history preserves cancellation-
          // before-provisioning without weakening Store verification.
          brokerTrustCoreInstanceNonce = nil
          codexReadyCoreInstanceNonce = nil
          offCoreInterruptionFailed = false
          offRequiresReplacementCoreProvisioning = true
          onRequiresReplacementCoreRestoration = false
          onRequiresAccountSetup = false
        } else {
          authoritativeStateCertain = false
          runtimeDisplayState = .unknown
          errorMessage = "OpenOpen could not verify that the previous Core stopped."
          offCoreInterruptionFailed = true
          onRequiresReplacementCoreRestoration = true
          coreInterruptionFailed = true
        }
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
      await self?.reconcileEnabledState()
    }
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
      accountState = .notConnected
      availableModels = []
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
    let recoveryGeneration = runtimeGeneration
    switchTask?.cancel()
    switchTask = nil
    heroTask?.cancel()
    heroTask = nil
    channelTask?.cancel()
    channelTask = nil
    coreRecoveryTask?.cancel()
    brokerTrustCoreInstanceNonce = nil
    codexReadyCoreInstanceNonce = nil
    connectedChannels.removeAll()
    clearLiveConnectionState(status: "paused")
    accountState = .notConnected
    availableModels = []
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
    try requireCurrentOnGeneration(expectedGeneration)
    let iMessagePairing = try await core.channelPairing(.iMessage)
    try requireCurrentOnGeneration(expectedGeneration)
    let discordPairing = try await core.channelPairing(.discord)
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
    let accountProof = try await currentEnabledProof(expectedGeneration: expectedGeneration)
    let account = try await core.account(proof: accountProof)
    let modelsProof = try await currentEnabledProof(expectedGeneration: expectedGeneration)
    let models = try await core.models(proof: modelsProof)
    try requireCurrentOnGeneration(expectedGeneration)
    accountState = account
    availableModels = models
    return requiredAccountAndModelReady
  }

  private var requiredAccountAndModelReady: Bool {
    // Production always supplies Core lifecycle monitoring. Test-only Core
    // stubs without that surface retain their intentionally isolated behavior;
    // the shipped App cannot take this branch.
    guard coreTerminationEvents != nil else { return true }
    guard case .chatGpt = accountState else { return false }
    return availableModels.contains { model in
      model.id == "gpt-5.6-sol" && model.supportedReasoningEfforts.contains("high")
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
    clearLiveConnectionState(status: "paused")
    accountState = .notConnected
    availableModels = []
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

  public func submitPrompt() async {
    let value = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
    guard modelEntryEnabled, !isBusy, !hasNonterminalMission, !value.isEmpty else { return }
    guard value.utf8.count <= 16 * 1024 else {
      errorMessage = "Outcome requests are limited to 16 KiB."
      return
    }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let proposed = try (await core.propose(prompt: value, proof: proof)).validated()
      try requireCurrentOnGeneration(generation)
      suggestion = proposed
      receipt = nil
      prompt = ""
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
        guard existing.channel == requested.channel,
          existing.ownerSenderId == requested.ownerSenderId,
          existing.conversationId == requested.conversationId,
          existing.requireExplicitAddress
        else {
          throw CoreClientError.contractViolation(
            "This channel is already paired to a different owner or conversation."
          )
        }
        pairing = existing
        usesExistingPairing = true
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
    guard heroTask == nil else { return }
    heroTask = Task { [weak self] in
      await self?.confirmSuggestion()
      self?.heroTask = nil
    }
  }

  public func requestMissionProgressCheck() {
    guard heroTask == nil else { return }
    heroTask = Task { [weak self] in
      await self?.checkMissionProgress()
      self?.heroTask = nil
    }
  }

  public func refreshAccountAndModels() async {
    guard accountSetupEnabled else {
      accountState = .notConnected
      availableModels = []
      return
    }
    let generation = runtimeGeneration
    do {
      let accountProof = try await currentEnabledProof(expectedGeneration: generation)
      let account = try await core.account(proof: accountProof)
      try requireCurrentOnGeneration(generation)
      let modelsProof = try await currentEnabledProof(expectedGeneration: generation)
      let models = try await core.models(proof: modelsProof)
      try requireCurrentOnGeneration(generation)
      accountState = account
      availableModels = models
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
        let accountProof = try await currentEnabledProof(expectedGeneration: generation)
        let account = try await core.account(proof: accountProof)
        if case .chatGpt = account {
          let modelsProof = try await currentEnabledProof(expectedGeneration: generation)
          let models = try await core.models(proof: modelsProof)
          try requireCurrentOnGeneration(generation)
          accountState = account
          availableModels = models
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
      let accountProof = try await currentEnabledProof(expectedGeneration: generation)
      let account = try await core.account(proof: accountProof)
      let modelsProof = try await currentEnabledProof(expectedGeneration: generation)
      let models = try await core.models(proof: modelsProof)
      try requireCurrentOnGeneration(generation)
      accountState = account
      availableModels = models
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
