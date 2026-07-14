import CryptoKit
import Foundation
import Security

public enum BrokerEnrollmentError: Error, Equatable {
  case serviceNotEnabled
  case missingTrustAnchor
  case malformedTrustRecord
  case trustRotationDenied
  case malformedCoreAuthority
  case malformedBrokerResponse
  case xpcUnavailable
  case invalidSystemTime
  case securityFailure(operation: String, status: OSStatus)
}

public protocol DurableBrokerTrustAnchorStore: EnrolledBrokerTrustAnchorProviding {
  func persistProvisionedBrokerTrustAnchor(_ anchor: EnrolledBrokerTrustAnchor) throws
}

public final class KeychainBrokerTrustAnchorStore: DurableBrokerTrustAnchorStore {
  private let keychain: KeychainDataStore

  public convenience init() {
    self.init(keychain: KeychainDataStore())
  }

  init(keychain: KeychainDataStore) {
    self.keychain = keychain
  }

  public func loadEnrolledBrokerTrustAnchor() throws -> EnrolledBrokerTrustAnchor {
    guard let data = try keychain.read(account: .brokerTrustAnchor) else {
      throw BrokerEnrollmentError.missingTrustAnchor
    }
    guard let record = try? JSONDecoder().decode(BrokerTrustRecord.self, from: data),
      record.version == 1
    else {
      throw BrokerEnrollmentError.malformedTrustRecord
    }
    return try EnrolledBrokerTrustAnchor(
      persistedBrokerKeyID: record.brokerKeyID,
      persistedBrokerVerifyingKeyHex: record.brokerVerifyingKeyHex,
      helperDesignatedRequirementDigest: record.helperDesignatedRequirementDigest,
      installedAtMilliseconds: record.installedAtMilliseconds
    )
  }

  public func persistProvisionedBrokerTrustAnchor(
    _ anchor: EnrolledBrokerTrustAnchor
  ) throws {
    if let existingData = try keychain.read(account: .brokerTrustAnchor) {
      guard let existing = try? JSONDecoder().decode(BrokerTrustRecord.self, from: existingData),
        existing == BrokerTrustRecord(anchor: anchor)
      else {
        throw BrokerEnrollmentError.trustRotationDenied
      }
      return
    }
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let encoded = try encoder.encode(BrokerTrustRecord(anchor: anchor))
    do {
      try keychain.insert(encoded, account: .brokerTrustAnchor)
    } catch BrokerEnrollmentError.securityFailure(_, let status)
      where status == errSecDuplicateItem
    {
      let existing = try loadEnrolledBrokerTrustAnchor()
      guard existing == anchor else {
        throw BrokerEnrollmentError.trustRotationDenied
      }
    }
  }
}

public struct CoreEffectIdentity: Equatable, Sendable {
  public let coreKeyID: String
  public let coreVerifyingKeyHex: String
}

public struct ProvisionedBrokerEnrollment: Equatable, Sendable {
  public let trustAnchor: EnrolledBrokerTrustAnchor
  /// Exact signed JSON consumed by Rust Core to construct its opaque trusted
  /// enrollment. It contains public keys and a signature, never the master.
  public let coreEnrollmentRecordJSON: Data
}

public final class KeychainCoreAuthorityStore {
  private let keychain: KeychainDataStore

  public convenience init() {
    self.init(keychain: KeychainDataStore())
  }

  init(keychain: KeychainDataStore) {
    self.keychain = keychain
  }

  /// Loads or creates the 32-byte user-session Core master. The caller passes
  /// it only to the bundled Core child over its private inherited transport.
  public func loadOrCreateMasterKey() throws -> Data {
    if let existing = try keychain.read(account: .coreAuthorityMaster) {
      guard existing.count == 32 else {
        throw BrokerEnrollmentError.malformedCoreAuthority
      }
      return existing
    }
    var bytes = Data(count: 32)
    let status = bytes.withUnsafeMutableBytes { buffer in
      SecRandomCopyBytes(kSecRandomDefault, buffer.count, buffer.baseAddress!)
    }
    guard status == errSecSuccess else {
      throw BrokerEnrollmentError.securityFailure(
        operation: "SecRandomCopyBytes",
        status: status
      )
    }
    do {
      try keychain.insert(bytes, account: .coreAuthorityMaster)
      return bytes
    } catch BrokerEnrollmentError.securityFailure(_, let insertStatus)
      where insertStatus == errSecDuplicateItem
    {
      guard let raced = try keychain.read(account: .coreAuthorityMaster), raced.count == 32 else {
        throw BrokerEnrollmentError.malformedCoreAuthority
      }
      return raced
    }
  }

  public func loadOrCreateEffectIdentity() throws -> CoreEffectIdentity {
    let master = try loadOrCreateMasterKey()
    var derivation = Data("openopen-effect-authorizer-v1".utf8)
    derivation.append(master)
    let seed = Data(SHA256.hash(data: derivation))
    let signingKey = try Curve25519.Signing.PrivateKey(rawRepresentation: seed)
    let verifyingKey = signingKey.publicKey.rawRepresentation
    return CoreEffectIdentity(
      coreKeyID: verifyingKey.sha256LowerHex,
      coreVerifyingKeyHex: verifyingKey.lowerHex
    )
  }

  func signedBrokerEnrollmentRecord(
    anchor: EnrolledBrokerTrustAnchor
  ) throws -> Data {
    let master = try loadOrCreateMasterKey()
    var derivation = Data("openopen-effect-authorizer-v1".utf8)
    derivation.append(master)
    let seed = Data(SHA256.hash(data: derivation))
    let signingKey = try Curve25519.Signing.PrivateKey(rawRepresentation: seed)
    let identity = try loadOrCreateEffectIdentity()
    let unsigned: [String: Any] = [
      "brokerKeyId": anchor.brokerKeyID,
      "brokerVerifyingKeyHex": anchor.brokerVerifyingKeyHex,
      "coreKeyId": identity.coreKeyID,
      "helperDesignatedRequirementDigest": anchor.helperDesignatedRequirementDigest,
      "installedAtMs": anchor.installedAtMilliseconds,
      "version": 1,
    ]
    guard let signingBytes = StrictJSONDocument.canonicalData(from: unsigned) else {
      throw BrokerEnrollmentError.malformedCoreAuthority
    }
    var signed = unsigned
    signed["coreAuthorizationSignatureHex"] = try signingKey.signature(
      for: signingBytes
    ).lowerHex
    guard let record = StrictJSONDocument.canonicalData(from: signed) else {
      throw BrokerEnrollmentError.malformedCoreAuthority
    }
    return record
  }
}

public final class BrokerEnrollmentCoordinator {
  private let serviceController: BrokerServiceController
  private let clientBuilder: PrivilegedBrokerClientBuilder
  private let trustStore: any DurableBrokerTrustAnchorStore
  private let coreAuthorityStore: KeychainCoreAuthorityStore
  private let identityProvider: any CodeSigningIdentityProviding
  private let nowMilliseconds: () throws -> Int64

  public convenience init() {
    self.init(
      serviceController: BrokerServiceController(),
      clientBuilder: PrivilegedBrokerClientBuilder(),
      trustStore: KeychainBrokerTrustAnchorStore(),
      coreAuthorityStore: KeychainCoreAuthorityStore(),
      identityProvider: SecurityCodeSigningIdentityProvider(),
      nowMilliseconds: Self.systemTimeMilliseconds
    )
  }

  init(
    serviceController: BrokerServiceController,
    clientBuilder: PrivilegedBrokerClientBuilder,
    trustStore: any DurableBrokerTrustAnchorStore,
    coreAuthorityStore: KeychainCoreAuthorityStore,
    identityProvider: any CodeSigningIdentityProviding,
    nowMilliseconds: @escaping () throws -> Int64
  ) {
    self.serviceController = serviceController
    self.clientBuilder = clientBuilder
    self.trustStore = trustStore
    self.coreAuthorityStore = coreAuthorityStore
    self.identityProvider = identityProvider
    self.nowMilliseconds = nowMilliseconds
  }

  /// Performs first enrollment only after macOS reports the signed daemon as
  /// enabled. The XPC connection pins the daemon identifier and Team ID before
  /// the live key is accepted, so this is explicit signed provisioning rather
  /// than request-path TOFU. Exact retries are idempotent; key rotation fails.
  public func provisionAfterAdminApproval(
    completion: @escaping (Result<ProvisionedBrokerEnrollment, Error>) -> Void
  ) {
    do {
      guard serviceController.state == .enabled else {
        throw BrokerEnrollmentError.serviceNotEnabled
      }
      let identity = try identityProvider.currentIdentity()
      guard identity.signingIdentifier == EffectBrokerConstants.hostSigningIdentifier else {
        throw CodeSigningIdentityError.unexpectedSigningIdentifier(
          expected: EffectBrokerConstants.hostSigningIdentifier,
          actual: identity.signingIdentifier
        )
      }
      let requirement = try ExactCodeSigningRequirement.make(
        peerSigningIdentifier: EffectBrokerConstants.brokerSigningIdentifier,
        teamIdentifier: identity.teamIdentifier
      )
      let requirementDigest = Data(requirement.utf8).sha256LowerHex
      let coreIdentity = try coreAuthorityStore.loadOrCreateEffectIdentity()
      let now = try nowMilliseconds()
      let connection = try clientBuilder.makeActivatedConnection()
      let callback = OneShotResult(completion)
      guard
        let proxy = connection.remoteObjectProxyWithErrorHandler({ _ in
          connection.invalidate()
          callback.finish(.failure(BrokerEnrollmentError.xpcUnavailable))
        }) as? EffectBrokerXPCProtocol
      else {
        connection.invalidate()
        throw BrokerEnrollmentError.xpcUnavailable
      }
      let sessionRequest = Data(#"{"type":"session","version":1}"#.utf8)
      proxy.session(sessionRequest) { [trustStore] sessionJSON in
        do {
          let session = try BrokerSessionTrustValidator.parseProvisioningSession(
            sessionJSON: sessionJSON,
            nowMilliseconds: now
          )
          let anchor: EnrolledBrokerTrustAnchor
          do {
            let existing = try trustStore.loadEnrolledBrokerTrustAnchor()
            guard existing.brokerKeyID == session.brokerKeyID,
              existing.brokerVerifyingKeyHex == session.brokerVerifyingKeyHex,
              existing.helperDesignatedRequirementDigest == requirementDigest
            else {
              throw BrokerEnrollmentError.trustRotationDenied
            }
            anchor = existing
          } catch BrokerEnrollmentError.missingTrustAnchor {
            anchor = try EnrolledBrokerTrustAnchor(
              persistedBrokerKeyID: session.brokerKeyID,
              persistedBrokerVerifyingKeyHex: session.brokerVerifyingKeyHex,
              helperDesignatedRequirementDigest: requirementDigest,
              installedAtMilliseconds: now
            )
          }
          let enrollRequest = try StrictJSONDocument.canonicalData(
            from: [
              "coreKeyId": coreIdentity.coreKeyID,
              "coreVerifyingKeyHex": coreIdentity.coreVerifyingKeyHex,
              "type": "enrollCore",
              "version": 1,
            ]
          ).unwrapped(or: BrokerEnrollmentError.malformedCoreAuthority)
          proxy.enrollCore(enrollRequest) { response in
            defer { connection.invalidate() }
            do {
              guard let object = StrictJSONDocument.object(from: response),
                Set(object.keys) == Set(["status", "version"]),
                object["status"] as? String == "enrolled",
                object["version"] as? Int == 1
              else {
                throw BrokerEnrollmentError.malformedBrokerResponse
              }
              try trustStore.persistProvisionedBrokerTrustAnchor(anchor)
              let coreRecord = try self.coreAuthorityStore.signedBrokerEnrollmentRecord(
                anchor: anchor
              )
              callback.finish(
                .success(
                  ProvisionedBrokerEnrollment(
                    trustAnchor: anchor,
                    coreEnrollmentRecordJSON: coreRecord
                  )
                )
              )
            } catch {
              callback.finish(.failure(error))
            }
          }
        } catch {
          connection.invalidate()
          callback.finish(.failure(error))
        }
      }
    } catch {
      completion(.failure(error))
    }
  }

  private static func systemTimeMilliseconds() throws -> Int64 {
    let milliseconds = Date().timeIntervalSince1970 * 1_000
    guard milliseconds.isFinite, milliseconds > 0, milliseconds <= Double(Int64.max) else {
      throw BrokerEnrollmentError.invalidSystemTime
    }
    return Int64(milliseconds)
  }
}

private struct BrokerTrustRecord: Codable, Equatable {
  let version: Int
  let brokerKeyID: String
  let brokerVerifyingKeyHex: String
  let helperDesignatedRequirementDigest: String
  let installedAtMilliseconds: Int64

  init(anchor: EnrolledBrokerTrustAnchor) {
    version = 1
    brokerKeyID = anchor.brokerKeyID
    brokerVerifyingKeyHex = anchor.brokerVerifyingKeyHex
    helperDesignatedRequirementDigest = anchor.helperDesignatedRequirementDigest
    installedAtMilliseconds = anchor.installedAtMilliseconds
  }
}

enum KeychainAccount: String {
  case brokerTrustAnchor = "effect-broker-trust-anchor-v1"
  case coreAuthorityMaster = "core-authority-master-v1"
}

final class KeychainDataStore {
  private let service = "com.thesongzhu.OpenOpen.Security"

  func read(account: KeychainAccount) throws -> Data? {
    var query = baseQuery(account: account)
    query[kSecReturnData as String] = true
    query[kSecMatchLimit as String] = kSecMatchLimitOne
    var item: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &item)
    if status == errSecItemNotFound {
      return nil
    }
    guard status == errSecSuccess, let data = item as? Data else {
      throw BrokerEnrollmentError.securityFailure(
        operation: "SecItemCopyMatching",
        status: status
      )
    }
    return data
  }

  func insert(_ data: Data, account: KeychainAccount) throws {
    var query = baseQuery(account: account)
    query[kSecValueData as String] = data
    query[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    let status = SecItemAdd(query as CFDictionary, nil)
    guard status == errSecSuccess else {
      throw BrokerEnrollmentError.securityFailure(operation: "SecItemAdd", status: status)
    }
  }

  private func baseQuery(account: KeychainAccount) -> [String: Any] {
    [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account.rawValue,
      kSecUseDataProtectionKeychain as String: true,
    ]
  }
}

private final class OneShotResult<Success>: @unchecked Sendable {
  private let lock = NSLock()
  private var completion: ((Result<Success, Error>) -> Void)?

  init(_ completion: @escaping (Result<Success, Error>) -> Void) {
    self.completion = completion
  }

  func finish(_ result: Result<Success, Error>) {
    lock.lock()
    let callback = completion
    completion = nil
    lock.unlock()
    callback?(result)
  }
}

extension Data {
  fileprivate var lowerHex: String {
    map { String(format: "%02x", $0) }.joined()
  }

  fileprivate var sha256LowerHex: String {
    Data(SHA256.hash(data: self)).lowerHex
  }
}

extension Optional {
  fileprivate func unwrapped(or error: @autoclosure () -> Error) throws -> Wrapped {
    guard let self else {
      throw error()
    }
    return self
  }
}
