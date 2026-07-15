import AppKit
import Combine
import EffectBrokerBridge
import Foundation

public protocol CoreServing: Sendable {
  func runtime() async throws -> RuntimeControl
  func effectIdentity() async throws -> CoreEffectIdentity
  func signBrokerEnrollment(_ anchor: EnrolledBrokerTrustAnchor) async throws -> Data
  func prepareCodexRuntime() async throws -> Int32
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
  func account(proof: BrokerRuntimeState) async throws -> AccountState
  func beginLogin(proof: BrokerRuntimeState) async throws -> ChatGptLogin
  func awaitLogin(identifier: String, proof: BrokerRuntimeState) async throws -> AccountState
  func models(proof: BrokerRuntimeState) async throws -> [GptModel]
  func propose(prompt: String, proof: BrokerRuntimeState) async throws -> OutcomeSuggestion
  func confirmSuggestion(
    identifier: String, reminderTarget: ReminderTarget
  ) async throws -> ConfirmedMission
  func beginReminderDispatch(identifier: String) async throws -> ReminderDispatchStart
  func recordReminderMirror(
    identifier: String, links: [ReminderLink]
  ) async throws -> ConfirmedMission
  func completeReminderMission(
    identifier: String, completions: [ReminderCompletionInput]
  ) async throws -> MissionReceipt
}

public protocol BrokerRuntimeServing: Sendable {
  func provision(coreIdentity: CoreEffectIdentity) async throws -> EnrolledBrokerTrustAnchor
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
  @Published public private(set) var microphone = MicrophoneState(
    available: false,
    reason: "Microphone unavailable until Voice setup"
  )
  @Published public private(set) var accountState: AccountState = .notConnected
  @Published public private(set) var availableModels: [GptModel] = []
  @Published public private(set) var isBusy = false
  @Published public private(set) var errorMessage: String?
  @Published public var showsSettings = false

  private let core: any CoreServing
  private let broker: any BrokerRuntimeServing
  private let reminders: any RemindersServing
  private let registerLoginItem: @Sendable () throws -> Void
  private var confirmedEnabled = false
  private var desiredEnabled = false
  private var pendingRuntimeIntent: Bool?
  private var switchTask: Task<Void, Never>?
  private var heroTask: Task<Void, Never>?
  private var loginItemRegistered = false
  private var runtimeGeneration: UInt64 = 0
  private var protectedRuntime: BrokerRuntimeState?
  private var authoritativeStateCertain = false
  private var brokerTrustCoreInstanceNonce: String?
  private var codexReadyCoreInstanceNonce: String?

  public var modelEntryEnabled: Bool {
    enabled && desiredEnabled && runtimeDisplayState == .on
  }

  public init(core: any CoreServing = CoreProcessClient()) {
    self.core = core
    broker = PrivilegedBrokerRuntimeClient()
    reminders = RemindersClient()
    registerLoginItem = { try LoginItemController.registerAfterOnboarding() }
  }

  init(
    core: any CoreServing,
    broker: any BrokerRuntimeServing = PrivilegedBrokerRuntimeClient(),
    reminders: any RemindersServing = RemindersClient(),
    registerLoginItem: @escaping @Sendable () throws -> Void
  ) {
    self.core = core
    self.broker = broker
    self.reminders = reminders
    self.registerLoginItem = registerLoginItem
  }

  public func refreshDashboard() async {
    let generation = runtimeGeneration
    do {
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
      guard generation == runtimeGeneration, switchTask == nil else { return }
      suggestion = dashboard.suggestion
      activeCards = dashboard.activeCards
      confirmedMission = dashboard.confirmedMission
      reminderLinks = dashboard.confirmedMission?.reminderLinks ?? []
      receipt = dashboard.receipt
      microphone = dashboard.microphone
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
        runtimeDisplayState = displayState(
          forAuthoritativeEnabled: runtime.enabled,
          desired: desiredEnabled
        )
        if runtime.enabled == desiredEnabled {
          pendingRuntimeIntent = nil
        }
      } else {
        authoritativeStateCertain = false
        runtimeDisplayState = .unknown
      }
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration, switchTask == nil else { return }
      authoritativeStateCertain = false
      runtimeDisplayState = .unknown
      errorMessage = userMessage(for: error)
    }
  }

  public func requestEnabled(_ requested: Bool) {
    runtimeGeneration &+= 1
    if !requested {
      heroTask?.cancel()
      heroTask = nil
    }
    desiredEnabled = requested
    pendingRuntimeIntent = requested
    if requested {
      runtimeDisplayState = authoritativeStateCertain && enabled ? .on : .turningOn
    } else {
      runtimeDisplayState = authoritativeStateCertain && !enabled ? .off : .turningOff
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
      let target = desiredEnabled
      var brokerAccepted: BrokerRuntimeState?
      var brokerApplyAttempted = false
      do {
        let preparedOff = target ? nil : try await core.prepareRuntime(false)
        let identity = try await provisionBrokerTrust()
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
          runtimeDisplayState = recovered.enabled ? .on : .turningOn
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
        runtimeDisplayState = displayState(
          forAuthoritativeEnabled: control.enabled,
          desired: desiredEnabled
        )
        errorMessage = nil
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
        } else {
          runtimeDisplayState = authoritativeStateCertain ? (enabled ? .on : .off) : .unknown
        }
        if attemptGeneration != runtimeGeneration { continue }
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

  private func currentEnabledProof(expectedGeneration: UInt64) async throws -> BrokerRuntimeState {
    try requireCurrentOnGeneration(expectedGeneration)
    let identity = try await provisionBrokerTrust()
    try await ensureCodexReady(coreIdentity: identity)
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
    runtimeDisplayState = .on
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
    let codexPID = try await core.prepareCodexRuntime()
    do {
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

  private func readProtectedRuntime() async throws -> BrokerRuntimeState? {
    let challenge = try await core.runtimeChallenge()
    return try await broker.status(challenge: challenge)
  }

  public func submitPrompt() async {
    let value = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
    guard modelEntryEnabled, !isBusy, confirmedMission == nil, !value.isEmpty else { return }
    guard value.utf8.count <= 16 * 1024 else {
      errorMessage = "Outcome requests are limited to 16 KiB."
      return
    }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let proposed = try await core.propose(prompt: value, proof: proof)
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
        guard Self.matches(mission, suggestion: suggestion) else {
          throw CoreClientError.contractViolation(
            "Core confirmed a Mission that does not match the exact suggestion."
          )
        }
        confirmedMission = mission
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
      guard Self.matchesDispatchStart(dispatchStart, mission: mission) else {
        throw CoreClientError.contractViolation(
          "Core did not durably bind the exact Reminder dispatch."
        )
      }
      confirmedMission = dispatchStart.mission
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
      guard persisted.missionId == mission.missionId,
        persisted.reminderAuthorization == dispatchStart.mission.reminderAuthorization,
        persisted.reminderDispatch == dispatchStart.mission.reminderDispatch,
        persisted.reminderLinks == links
      else {
        throw CoreClientError.contractViolation(
          "Core did not persist the exact Reminder mirror."
        )
      }
      confirmedMission = persisted
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
    guard modelEntryEnabled, !isBusy, let mission = confirmedMission, !reminderLinks.isEmpty else {
      return
    }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      try requireCurrentOnGeneration(generation)
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
      let receipt = try await core.completeReminderMission(
        identifier: mission.missionId,
        completions: completed
      )
      try requireCurrentOnGeneration(generation)
      guard receipt.missionId == mission.missionId else {
        throw CoreClientError.contractViolation("Core returned a Receipt for another Mission.")
      }
      self.receipt = receipt
      activeCards = []
      confirmedMission = nil
      reminderLinks = []
      suggestion = nil
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
    guard modelEntryEnabled else {
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
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func connectChatGpt() async {
    guard modelEntryEnabled, !isBusy else { return }
    isBusy = true
    defer { isBusy = false }
    let generation = runtimeGeneration
    do {
      let proof = try await currentEnabledProof(expectedGeneration: generation)
      let login = try await core.beginLogin(proof: proof)
      try requireCurrentOnGeneration(generation)
      guard let url = URL(string: login.authUrl), url.scheme == "https" else {
        throw CoreClientError.contractViolation("OpenOpen received an invalid sign-in URL.")
      }
      NSWorkspace.shared.open(url)
      let currentProof = try await currentEnabledProof(expectedGeneration: generation)
      let account = try await core.awaitLogin(identifier: login.loginId, proof: currentProof)
      try requireCurrentOnGeneration(generation)
      accountState = account
      errorMessage = nil
    } catch {
      guard generation == runtimeGeneration else { return }
      errorMessage = userMessage(for: error)
    }
  }

  public func dismissError() {
    errorMessage = nil
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
}
