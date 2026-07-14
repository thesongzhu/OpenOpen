import CryptoKit
import Foundation

public struct EnrolledBrokerTrustAnchor: Equatable, Sendable {
  public let brokerKeyID: String
  public let brokerVerifyingKeyHex: String
  public let helperDesignatedRequirementDigest: String
  public let installedAtMilliseconds: Int64

  /// Creates a trust anchor already loaded from Core's durable, protected
  /// enrollment store. Production callers must never construct this from the
  /// XPC session being validated or from any request/model-controlled field.
  public init(
    persistedBrokerKeyID: String,
    persistedBrokerVerifyingKeyHex: String,
    helperDesignatedRequirementDigest: String,
    installedAtMilliseconds: Int64
  ) throws {
    guard
      Self.isSelfConsistent(
        keyID: persistedBrokerKeyID,
        verifyingKeyHex: persistedBrokerVerifyingKeyHex
      ),
      Self.decodeLowerHex(helperDesignatedRequirementDigest, byteCount: 32) != nil,
      installedAtMilliseconds > 0
    else {
      throw BrokerSessionTrustError.invalidEnrollment
    }
    brokerKeyID = persistedBrokerKeyID
    brokerVerifyingKeyHex = persistedBrokerVerifyingKeyHex
    self.helperDesignatedRequirementDigest = helperDesignatedRequirementDigest
    self.installedAtMilliseconds = installedAtMilliseconds
  }

  fileprivate static func decodeLowerHex(
    _ value: String,
    byteCount: Int
  ) -> Data? {
    let encoded = Array(value.utf8)
    guard encoded.count == byteCount * 2 else {
      return nil
    }
    var bytes = [UInt8]()
    bytes.reserveCapacity(byteCount)
    for index in stride(from: 0, to: encoded.count, by: 2) {
      guard let high = lowerHexNibble(encoded[index]),
        let low = lowerHexNibble(encoded[index + 1])
      else {
        return nil
      }
      bytes.append((high << 4) | low)
    }
    return Data(bytes)
  }

  private static func lowerHexNibble(_ byte: UInt8) -> UInt8? {
    switch byte {
    case 0x30...0x39:
      byte - 0x30
    case 0x61...0x66:
      byte - 0x61 + 10
    default:
      nil
    }
  }

  static func isSelfConsistent(
    keyID: String,
    verifyingKeyHex: String
  ) -> Bool {
    guard
      let verifyingKey = decodeLowerHex(verifyingKeyHex, byteCount: 32),
      let decodedKeyID = decodeLowerHex(keyID, byteCount: 32)
    else {
      return false
    }
    return Data(SHA256.hash(data: verifyingKey)) == decodedKeyID
  }
}

public protocol EnrolledBrokerTrustAnchorProviding {
  /// Must read an existing anchor from a durable Core trust store populated by
  /// signed provisioning or an explicit owner/admin-authorized enrollment.
  /// Absence and rotation are errors; the live XPC session is never a source.
  func loadEnrolledBrokerTrustAnchor() throws -> EnrolledBrokerTrustAnchor
}

public struct ValidatedBrokerSession: Equatable, Sendable {
  public let protocolVersion: Int
  public let sessionNonce: String
  public let brokerKeyID: String
  public let brokerVerifyingKeyHex: String
  public let expiresAtMilliseconds: Int64
  public let canonicalJSON: Data
}

public enum BrokerSessionTrustError: Error, Equatable {
  case invalidEnrollment
  case malformedSession
  case expiredSession
  case brokerIdentityMismatch
}

public struct BrokerSessionTrustValidator {
  private let trustAnchorProvider: any EnrolledBrokerTrustAnchorProviding

  public init(
    trustAnchorProvider: any EnrolledBrokerTrustAnchorProviding
  ) {
    self.trustAnchorProvider = trustAnchorProvider
  }

  /// Validates a broker session against a pre-existing persistent trust anchor.
  /// There is deliberately no optional-anchor or trust-on-first-use overload.
  public func validate(
    sessionJSON: Data,
    nowMilliseconds: Int64
  ) throws -> ValidatedBrokerSession {
    let parsed = try Self.parseProvisioningSession(
      sessionJSON: sessionJSON,
      nowMilliseconds: nowMilliseconds
    )

    let enrolledAnchor = try trustAnchorProvider.loadEnrolledBrokerTrustAnchor()
    guard parsed.brokerKeyID == enrolledAnchor.brokerKeyID,
      parsed.brokerVerifyingKeyHex == enrolledAnchor.brokerVerifyingKeyHex
    else {
      throw BrokerSessionTrustError.brokerIdentityMismatch
    }
    return parsed
  }

  static func parseProvisioningSession(
    sessionJSON: Data,
    nowMilliseconds: Int64
  ) throws -> ValidatedBrokerSession {
    guard !sessionJSON.isEmpty,
      sessionJSON.count <= TypedJSONEnvelope.maximumBytes,
      let dictionary = StrictJSONDocument.object(from: sessionJSON),
      Set(dictionary.keys)
        == Set([
          "brokerKeyId",
          "brokerVerifyingKeyHex",
          "expiresAtMs",
          "protocolVersion",
          "sessionNonce",
        ]),
      dictionary["protocolVersion"] as? Int == 1,
      let sessionNonce = dictionary["sessionNonce"] as? String,
      EnrolledBrokerTrustAnchor.decodeLowerHex(sessionNonce, byteCount: 32) != nil,
      let brokerKeyID = dictionary["brokerKeyId"] as? String,
      let brokerVerifyingKeyHex = dictionary["brokerVerifyingKeyHex"] as? String,
      let verifyingKey = EnrolledBrokerTrustAnchor.decodeLowerHex(
        brokerVerifyingKeyHex,
        byteCount: 32
      ),
      let encodedKeyID = EnrolledBrokerTrustAnchor.decodeLowerHex(
        brokerKeyID,
        byteCount: 32
      ),
      Data(SHA256.hash(data: verifyingKey)) == encodedKeyID,
      let expiresAtNumber = dictionary["expiresAtMs"] as? NSNumber,
      CFGetTypeID(expiresAtNumber) != CFBooleanGetTypeID(),
      let expiresAtMilliseconds = Int64(expiresAtNumber.stringValue),
      let canonicalJSON = StrictJSONDocument.canonicalData(from: dictionary)
    else {
      throw BrokerSessionTrustError.malformedSession
    }
    guard expiresAtMilliseconds > nowMilliseconds else {
      throw BrokerSessionTrustError.expiredSession
    }

    return ValidatedBrokerSession(
      protocolVersion: 1,
      sessionNonce: sessionNonce,
      brokerKeyID: brokerKeyID,
      brokerVerifyingKeyHex: brokerVerifyingKeyHex,
      expiresAtMilliseconds: expiresAtMilliseconds,
      canonicalJSON: canonicalJSON
    )
  }
}
