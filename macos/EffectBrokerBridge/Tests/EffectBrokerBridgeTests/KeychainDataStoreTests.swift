import Security
import XCTest

@testable import EffectBrokerBridge

final class KeychainDataStoreTests: XCTestCase {
  func testDirectDistributionUsesOneExplicitLoginKeychainBackend() {
    let query = KeychainDataStore().baseQuery(account: .coreAuthorityMaster)

    XCTAssertEqual(query[kSecClass as String] as? String, kSecClassGenericPassword as String)
    XCTAssertEqual(query[kSecAttrService as String] as? String, "com.thesongzhu.OpenOpen.Security")
    XCTAssertEqual(query[kSecAttrAccount as String] as? String, "core-authority-master-v1")
    XCTAssertNil(query[kSecUseDataProtectionKeychain as String])
  }
}
