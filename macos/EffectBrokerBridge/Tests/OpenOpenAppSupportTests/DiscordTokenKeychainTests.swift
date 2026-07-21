import Foundation
import Security
import Testing

@testable import OpenOpenAppSupport

@Test
func discordProductionQueryUsesOnlyTheNativeLoginKeychainBackend() {
  let store = DiscordTokenKeychain()
  let base = store.baseQuery()
  let insert = store.insertQuery(valueData: Data("bounded-test-value".utf8))

  #expect(base[kSecClass as String] as? String == kSecClassGenericPassword as String)
  #expect(base[kSecAttrService as String] as? String == "com.thesongzhu.OpenOpen.Discord")
  #expect(base[kSecAttrAccount as String] as? String == "official-bot-token")
  #expect(base[kSecUseDataProtectionKeychain as String] == nil)
  #expect(
    insert[kSecAttrAccessible as String] as? String
      == kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as String
  )
}

// SwiftPM signs this XCTest bundle ad hoc with no TeamIdentifier, so it cannot
// honestly certify the Developer-ID login-Keychain lifecycle. The exact
// save/readback/update/delete proof is required in the signed candidate
// package gate; this suite retains only deterministic selector/failure tests.
@Test(.disabled("Requires the Developer-ID signed candidate package gate."))
func discordNativeLoginKeychainSaveReadbackUpdateDeleteLifecycle() throws {
  let suffix = UUID().uuidString
  let store = DiscordTokenKeychain(
    service: "com.thesongzhu.OpenOpen.Tests.Discord.\(suffix)",
    account: "disposable-\(suffix)"
  )
  defer { try? store.delete() }

  try store.save("bounded-test-value-one")
  #expect(try store.load() == "bounded-test-value-one")
  try store.save("bounded-test-value-two")
  #expect(try store.load() == "bounded-test-value-two")
  try store.delete()
  #expect(try store.load() == nil)
}

@Test
func legacyDiscordDataProtectionSelectorReproducesMissingEntitlement() {
  let suffix = UUID().uuidString
  let service = "com.thesongzhu.OpenOpen.Tests.Discord.Legacy.\(suffix)"
  let account = "disposable-\(suffix)"
  var query: [String: Any] = [
    kSecClass as String: kSecClassGenericPassword,
    kSecAttrService as String: service,
    kSecAttrAccount as String: account,
    kSecValueData as String: Data("x".utf8),
    kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
    kSecUseDataProtectionKeychain as String: true,
  ]
  defer {
    query.removeValue(forKey: kSecValueData as String)
    query.removeValue(forKey: kSecAttrAccessible as String)
    _ = SecItemDelete(query as CFDictionary)
  }

  #expect(SecItemAdd(query as CFDictionary, nil) == errSecMissingEntitlement)
}
