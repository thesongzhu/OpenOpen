import XCTest

@testable import EffectBrokerBridge

final class CodeSigningIdentityTests: XCTestCase {
  func testExactRequirementBindsIdentifierAndTeam() throws {
    let requirement = try ExactCodeSigningRequirement.make(
      peerSigningIdentifier: "com.thesongzhu.OpenOpen.EffectBroker",
      teamIdentifier: "A1B2C3D4E5"
    )
    XCTAssertEqual(
      requirement,
      "anchor apple generic and identifier \"com.thesongzhu.OpenOpen.EffectBroker\" "
        + "and certificate leaf[subject.OU] = \"A1B2C3D4E5\""
    )
  }

  func testRequirementRejectsInvalidIdentityInput() {
    XCTAssertThrowsError(
      try ExactCodeSigningRequirement.make(
        peerSigningIdentifier: "com.thesongzhu.OpenOpen\" or true",
        teamIdentifier: "A1B2C3D4E5"
      )
    )
    XCTAssertThrowsError(
      try ExactCodeSigningRequirement.make(
        peerSigningIdentifier: "com.thesongzhu.OpenOpen",
        teamIdentifier: "a1b2c3d4e5"
      )
    )
    XCTAssertThrowsError(
      try ExactCodeSigningRequirement.make(
        peerSigningIdentifier: "singlecomponent",
        teamIdentifier: "A1B2C3D4E5"
      )
    )
  }

  func testClientBuilderRejectsUnexpectedCurrentIdentifierBeforeConnecting() throws {
    let provider = FixedIdentityProvider(
      identity: try CodeSigningIdentity(
        signingIdentifier: "com.thesongzhu.OtherApp",
        teamIdentifier: "A1B2C3D4E5"
      )
    )
    let builder = PrivilegedBrokerClientBuilder(identityProvider: provider)
    XCTAssertThrowsError(try builder.makeActivatedConnection()) { error in
      XCTAssertEqual(
        error as? CodeSigningIdentityError,
        .unexpectedSigningIdentifier(
          expected: EffectBrokerConstants.hostSigningIdentifier,
          actual: "com.thesongzhu.OtherApp"
        )
      )
    }
  }
}

private struct FixedIdentityProvider: CodeSigningIdentityProviding {
  let identity: CodeSigningIdentity

  func currentIdentity() throws -> CodeSigningIdentity {
    identity
  }
}
