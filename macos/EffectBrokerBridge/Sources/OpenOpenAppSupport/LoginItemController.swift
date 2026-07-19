import ServiceManagement

enum LoginItemController {
  static func registerAfterOnboarding() throws {
    let service = SMAppService.mainApp
    if service.status != .enabled {
      try service.register()
    }
  }
}
