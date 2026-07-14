import Foundation
import XCTest

@testable import EffectBrokerBridge

final class LaunchDaemonPlistTests: XCTestCase {
  func testLaunchDaemonPlistHasOnlyTheApprovedPrivilegeContract() throws {
    let url = try XCTUnwrap(
      Bundle.module.url(
        forResource: "com.thesongzhu.OpenOpen.EffectBroker",
        withExtension: "plist",
        subdirectory: "LaunchDaemons"
      )
    )
    let data = try Data(contentsOf: url)
    let plist = try XCTUnwrap(
      PropertyListSerialization.propertyList(from: data, format: nil)
        as? [String: Any]
    )

    XCTAssertEqual(plist["Label"] as? String, EffectBrokerConstants.machServiceName)
    XCTAssertEqual(
      plist["BundleProgram"] as? String,
      "Contents/MacOS/OpenOpenEffectBroker"
    )
    XCTAssertEqual(
      plist["AssociatedBundleIdentifiers"] as? String,
      EffectBrokerConstants.hostSigningIdentifier
    )
    XCTAssertEqual(plist["UserName"] as? String, "root")
    XCTAssertEqual(plist["Umask"] as? Int, 63)
    XCTAssertEqual(
      (plist["MachServices"] as? [String: Bool])?[EffectBrokerConstants.machServiceName],
      true
    )
    XCTAssertNil(plist["RunAtLoad"])
    XCTAssertNil(plist["KeepAlive"])
    XCTAssertNil(plist["Program"])
    XCTAssertNil(plist["ProgramArguments"])
  }
}
