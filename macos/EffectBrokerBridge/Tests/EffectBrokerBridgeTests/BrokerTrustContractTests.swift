import CryptoKit
import Foundation
import XCTest

@testable import EffectBrokerBridge

final class BrokerTrustContractTests: XCTestCase {
  func testValidatesSessionOnlyAgainstPersistedEnrollment() throws {
    let key = Data(repeating: 0x11, count: 32)
    let keyHex = hex(key)
    let keyID = hex(Data(SHA256.hash(data: key)))
    let anchor = try anchor(keyID: keyID, keyHex: keyHex)
    let validator = BrokerSessionTrustValidator(
      trustAnchorProvider: TestAnchorProvider(anchor: anchor)
    )

    let session = try validator.validate(
      sessionJSON: sessionJSON(keyID: keyID, keyHex: keyHex),
      nowMilliseconds: 1_000
    )
    XCTAssertEqual(session.brokerKeyID, keyID)
    XCTAssertEqual(session.brokerVerifyingKeyHex, keyHex)
  }

  func testRejectsSelfConsistentCallerSelectedSessionKey() throws {
    let enrolledKey = Data(repeating: 0x11, count: 32)
    let enrolledAnchor = try anchor(
      keyID: hex(Data(SHA256.hash(data: enrolledKey))),
      keyHex: hex(enrolledKey)
    )
    let attackerKey = Data(repeating: 0x22, count: 32)
    let validator = BrokerSessionTrustValidator(
      trustAnchorProvider: TestAnchorProvider(anchor: enrolledAnchor)
    )

    XCTAssertThrowsError(
      try validator.validate(
        sessionJSON: sessionJSON(
          keyID: hex(Data(SHA256.hash(data: attackerKey))),
          keyHex: hex(attackerKey)
        ),
        nowMilliseconds: 1_000
      )
    ) { error in
      XCTAssertEqual(error as? BrokerSessionTrustError, .brokerIdentityMismatch)
    }
  }

  func testMissingEnrollmentFailsWithoutTrustOnFirstUse() throws {
    let key = Data(repeating: 0x11, count: 32)
    let validator = BrokerSessionTrustValidator(
      trustAnchorProvider: MissingAnchorProvider()
    )

    XCTAssertThrowsError(
      try validator.validate(
        sessionJSON: sessionJSON(
          keyID: hex(Data(SHA256.hash(data: key))),
          keyHex: hex(key)
        ),
        nowMilliseconds: 1_000
      )
    ) { error in
      XCTAssertEqual(error as? TestAnchorError, .missing)
    }
  }

  func testRejectsMalformedEnrollmentAndExpiredSession() throws {
    XCTAssertThrowsError(
      try EnrolledBrokerTrustAnchor(
        persistedBrokerKeyID: String(repeating: "0", count: 64),
        persistedBrokerVerifyingKeyHex: String(repeating: "1", count: 64),
        helperDesignatedRequirementDigest: String(repeating: "2", count: 64),
        installedAtMilliseconds: 1
      )
    ) { error in
      XCTAssertEqual(error as? BrokerSessionTrustError, .invalidEnrollment)
    }

    let key = Data(repeating: 0x11, count: 32)
    let keyHex = hex(key)
    let keyID = hex(Data(SHA256.hash(data: key)))
    let anchor = try anchor(keyID: keyID, keyHex: keyHex)
    let validator = BrokerSessionTrustValidator(
      trustAnchorProvider: TestAnchorProvider(anchor: anchor)
    )
    XCTAssertThrowsError(
      try validator.validate(
        sessionJSON: sessionJSON(
          keyID: keyID,
          keyHex: keyHex,
          expiresAtMilliseconds: 1_000
        ),
        nowMilliseconds: 1_000
      )
    ) { error in
      XCTAssertEqual(error as? BrokerSessionTrustError, .expiredSession)
    }
  }

  func testMalformedUnicodeHexAndDuplicateSessionKeysRejectWithoutCrash() throws {
    let malformedHex = String(repeating: "1", count: 62) + "é"
    XCTAssertThrowsError(
      try EnrolledBrokerTrustAnchor(
        persistedBrokerKeyID: malformedHex,
        persistedBrokerVerifyingKeyHex: String(repeating: "1", count: 64),
        helperDesignatedRequirementDigest: String(repeating: "2", count: 64),
        installedAtMilliseconds: 1
      )
    )

    let key = Data(repeating: 0x11, count: 32)
    let keyHex = hex(key)
    let keyID = hex(Data(SHA256.hash(data: key)))
    let anchor = try anchor(keyID: keyID, keyHex: keyHex)
    let validator = BrokerSessionTrustValidator(
      trustAnchorProvider: TestAnchorProvider(anchor: anchor)
    )
    let duplicate = Data(
      #"{"brokerKeyId":"\#(keyID)","brokerKeyId":"\#(keyID)","brokerVerifyingKeyHex":"\#(keyHex)","expiresAtMs":2000,"protocolVersion":1,"sessionNonce":"\#(String(repeating: "a", count: 64))"}"#
        .utf8
    )
    XCTAssertThrowsError(
      try validator.validate(sessionJSON: duplicate, nowMilliseconds: 1_000)
    ) { error in
      XCTAssertEqual(error as? BrokerSessionTrustError, .malformedSession)
    }
  }

  private func sessionJSON(
    keyID: String,
    keyHex: String,
    expiresAtMilliseconds: Int64 = 2_000
  ) -> Data {
    try! JSONSerialization.data(
      withJSONObject: [
        "brokerKeyId": keyID,
        "brokerVerifyingKeyHex": keyHex,
        "expiresAtMs": expiresAtMilliseconds,
        "protocolVersion": 1,
        "sessionNonce": String(repeating: "a", count: 64),
      ],
      options: [.sortedKeys]
    )
  }

  private func anchor(
    keyID: String,
    keyHex: String
  ) throws -> EnrolledBrokerTrustAnchor {
    try EnrolledBrokerTrustAnchor(
      persistedBrokerKeyID: keyID,
      persistedBrokerVerifyingKeyHex: keyHex,
      helperDesignatedRequirementDigest: String(repeating: "2", count: 64),
      installedAtMilliseconds: 1
    )
  }

  private func hex(_ data: Data) -> String {
    data.map { String(format: "%02x", $0) }.joined()
  }
}

private struct TestAnchorProvider: EnrolledBrokerTrustAnchorProviding {
  let anchor: EnrolledBrokerTrustAnchor

  func loadEnrolledBrokerTrustAnchor() throws -> EnrolledBrokerTrustAnchor {
    anchor
  }
}

private enum TestAnchorError: Error {
  case missing
}

private struct MissingAnchorProvider: EnrolledBrokerTrustAnchorProviding {
  func loadEnrolledBrokerTrustAnchor() throws -> EnrolledBrokerTrustAnchor {
    throw TestAnchorError.missing
  }
}
