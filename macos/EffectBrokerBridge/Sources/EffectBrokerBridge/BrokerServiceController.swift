import ServiceManagement

public enum BrokerServiceState: Equatable, Sendable {
  case notRegistered
  case enabled
  case requiresApproval
  case notFound
}

public final class BrokerServiceController {
  private let statusProvider: () -> SMAppService.Status
  private let registerAction: () throws -> Void
  private let openSettingsAction: () -> Void

  public convenience init(
    plistName: String = EffectBrokerConstants.launchDaemonPlistName
  ) {
    let service = SMAppService.daemon(plistName: plistName)
    self.init(
      statusProvider: { service.status },
      registerAction: { try service.register() },
      openSettingsAction: { SMAppService.openSystemSettingsLoginItems() }
    )
  }

  init(
    statusProvider: @escaping () -> SMAppService.Status,
    registerAction: @escaping () throws -> Void,
    openSettingsAction: @escaping () -> Void
  ) {
    self.statusProvider = statusProvider
    self.registerAction = registerAction
    self.openSettingsAction = openSettingsAction
  }

  public var state: BrokerServiceState {
    Self.map(statusProvider())
  }

  public func register() throws -> BrokerServiceState {
    do {
      try registerAction()
    } catch {
      if state == .requiresApproval {
        return .requiresApproval
      }
      throw error
    }
    return state
  }

  public func registerIfNeeded() throws -> BrokerServiceState {
    switch state {
    case .enabled, .requiresApproval:
      return state
    case .notRegistered, .notFound:
      return try register()
    }
  }

  public func openLoginItemsSettings() {
    openSettingsAction()
  }

  static func map(_ status: SMAppService.Status) -> BrokerServiceState {
    switch status {
    case .notRegistered:
      .notRegistered
    case .enabled:
      .enabled
    case .requiresApproval:
      .requiresApproval
    case .notFound:
      .notFound
    @unknown default:
      .notFound
    }
  }
}
