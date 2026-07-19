import ServiceManagement
import XCTest

@testable import EffectBrokerBridge

final class BrokerServiceControllerTests: XCTestCase {
  func testServiceStatusMappingIsExplicit() {
    XCTAssertEqual(BrokerServiceController.map(.notRegistered), .notRegistered)
    XCTAssertEqual(BrokerServiceController.map(.enabled), .enabled)
    XCTAssertEqual(BrokerServiceController.map(.requiresApproval), .requiresApproval)
    XCTAssertEqual(BrokerServiceController.map(.notFound), .notFound)
  }

  func testRegistrationReturnsRequiresApprovalWithoutFallback() throws {
    let controller = BrokerServiceController(
      statusProvider: { .requiresApproval },
      registerAction: { throw RegistrationDenied() },
      openSettingsAction: {}
    )
    XCTAssertEqual(try controller.register(), .requiresApproval)
  }

  func testRegistrationPropagatesUnrelatedErrors() {
    let controller = BrokerServiceController(
      statusProvider: { .notFound },
      registerAction: { throw RegistrationDenied() },
      openSettingsAction: {}
    )
    XCTAssertThrowsError(try controller.register()) { error in
      XCTAssertTrue(error is RegistrationDenied)
    }
  }

  func testMissingInitialRecordUsesTheOfficialRegistrationRoute() throws {
    var status = SMAppService.Status.notFound
    var registrations = 0
    let controller = BrokerServiceController(
      statusProvider: { status },
      registerAction: {
        registrations += 1
        status = .requiresApproval
      },
      openSettingsAction: {}
    )

    XCTAssertEqual(try controller.registerIfNeeded(), .requiresApproval)
    XCTAssertEqual(registrations, 1)
  }

  func testEnabledServiceDoesNotReregister() throws {
    var registrations = 0
    let controller = BrokerServiceController(
      statusProvider: { .enabled },
      registerAction: { registrations += 1 },
      openSettingsAction: {}
    )

    XCTAssertEqual(try controller.registerIfNeeded(), .enabled)
    XCTAssertEqual(registrations, 0)
  }
}

private struct RegistrationDenied: Error {}
