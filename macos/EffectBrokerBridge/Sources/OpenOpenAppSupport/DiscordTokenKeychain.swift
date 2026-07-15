import Foundation
import Security

public protocol DiscordTokenStoring: Sendable {
  func save(_ token: String) throws
  func load() throws -> String?
  func delete() throws
}

public final class DiscordTokenKeychain: DiscordTokenStoring, @unchecked Sendable {
  private let service = "com.thesongzhu.OpenOpen.Discord"
  private let account = "official-bot-token"

  public init() {}

  public func save(_ token: String) throws {
    guard !token.isEmpty, token == token.trimmingCharacters(in: .whitespacesAndNewlines),
      token.utf8.count <= 4_096, !token.utf8.contains(0)
    else {
      throw CoreClientError.contractViolation("Discord rejected an invalid bot token.")
    }
    let data = Data(token.utf8)
    let query = baseQuery()
    let update: [String: Any] = [kSecValueData as String: data]
    let status = SecItemUpdate(query as CFDictionary, update as CFDictionary)
    if status == errSecSuccess { return }
    guard status == errSecItemNotFound else { throw CoreClientError.keychain(status) }
    var insert = query
    insert[kSecValueData as String] = data
    insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    let inserted = SecItemAdd(insert as CFDictionary, nil)
    guard inserted == errSecSuccess else { throw CoreClientError.keychain(inserted) }
  }

  public func load() throws -> String? {
    var query = baseQuery()
    query[kSecReturnData as String] = true
    query[kSecMatchLimit as String] = kSecMatchLimitOne
    var item: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &item)
    if status == errSecItemNotFound { return nil }
    guard status == errSecSuccess, let data = item as? Data,
      let token = String(data: data, encoding: .utf8), !token.isEmpty
    else {
      throw CoreClientError.keychain(status)
    }
    return token
  }

  public func delete() throws {
    let status = SecItemDelete(baseQuery() as CFDictionary)
    guard status == errSecSuccess || status == errSecItemNotFound else {
      throw CoreClientError.keychain(status)
    }
  }

  private func baseQuery() -> [String: Any] {
    [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
      kSecUseDataProtectionKeychain as String: true,
    ]
  }
}
