import Foundation
import Security

public struct CodeSigningIdentity: Equatable, Sendable {
  public let signingIdentifier: String
  public let teamIdentifier: String

  public init(signingIdentifier: String, teamIdentifier: String) throws {
    guard Self.isValidSigningIdentifier(signingIdentifier),
      Self.isValidTeamIdentifier(teamIdentifier)
    else {
      throw CodeSigningIdentityError.invalidIdentity
    }
    self.signingIdentifier = signingIdentifier
    self.teamIdentifier = teamIdentifier
  }

  private static func isValidSigningIdentifier(_ value: String) -> Bool {
    guard !value.isEmpty, value.utf8.count <= 255 else {
      return false
    }
    let components = value.split(separator: ".", omittingEmptySubsequences: false)
    guard components.count >= 2 else {
      return false
    }
    return components.allSatisfy { component in
      guard let first = component.utf8.first,
        let last = component.utf8.last,
        first.isASCIIAlphaNumeric,
        last.isASCIIAlphaNumeric
      else {
        return false
      }
      return component.utf8.allSatisfy { $0.isASCIIAlphaNumeric || $0 == 0x2D }
    }
  }

  private static func isValidTeamIdentifier(_ value: String) -> Bool {
    value.utf8.count == 10
      && value.utf8.allSatisfy { byte in
        byte.isOpenOpenASCIIDigit || (0x41...0x5A).contains(byte)
      }
  }
}

extension UInt8 {
  fileprivate var isOpenOpenASCIIDigit: Bool {
    (0x30...0x39).contains(self)
  }

  fileprivate var isASCIIAlphaNumeric: Bool {
    isOpenOpenASCIIDigit
      || (0x41...0x5A).contains(self)
      || (0x61...0x7A).contains(self)
  }
}

public enum CodeSigningIdentityError: Error, Equatable {
  case invalidIdentity
  case missingSigningIdentifier
  case missingTeamIdentifier
  case securityFailure(operation: String, status: OSStatus)
  case unexpectedSigningIdentifier(expected: String, actual: String)
}

public protocol CodeSigningIdentityProviding {
  func currentIdentity() throws -> CodeSigningIdentity
}

public struct SecurityCodeSigningIdentityProvider: CodeSigningIdentityProviding {
  public init() {}

  public func currentIdentity() throws -> CodeSigningIdentity {
    var code: SecCode?
    let selfStatus = SecCodeCopySelf([], &code)
    guard selfStatus == errSecSuccess, let code else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecCodeCopySelf",
        status: selfStatus
      )
    }

    var staticCode: SecStaticCode?
    let staticStatus = SecCodeCopyStaticCode(code, [], &staticCode)
    guard staticStatus == errSecSuccess, let staticCode else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecCodeCopyStaticCode",
        status: staticStatus
      )
    }
    let validityFlags = SecCSFlags(
      rawValue: kSecCSStrictValidate | kSecCSCheckAllArchitectures
    )
    let validityStatus = SecStaticCodeCheckValidity(staticCode, validityFlags, nil)
    guard validityStatus == errSecSuccess else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecStaticCodeCheckValidity",
        status: validityStatus
      )
    }

    var signingInformation: CFDictionary?
    let informationStatus = SecCodeCopySigningInformation(
      staticCode,
      SecCSFlags(rawValue: kSecCSSigningInformation),
      &signingInformation
    )
    guard informationStatus == errSecSuccess,
      let information = signingInformation as? [CFString: Any]
    else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecCodeCopySigningInformation",
        status: informationStatus
      )
    }
    guard let signingIdentifier = information[kSecCodeInfoIdentifier] as? String else {
      throw CodeSigningIdentityError.missingSigningIdentifier
    }
    guard let teamIdentifier = information[kSecCodeInfoTeamIdentifier] as? String else {
      throw CodeSigningIdentityError.missingTeamIdentifier
    }
    return try CodeSigningIdentity(
      signingIdentifier: signingIdentifier,
      teamIdentifier: teamIdentifier
    )
  }
}

public enum StaticCodeSigningValidator {
  public static func validate(
    executableURL: URL,
    expectedSigningIdentifier: String,
    teamIdentifier: String
  ) throws {
    var staticCode: SecStaticCode?
    let createStatus = SecStaticCodeCreateWithPath(
      executableURL as CFURL,
      [],
      &staticCode
    )
    guard createStatus == errSecSuccess, let staticCode else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecStaticCodeCreateWithPath",
        status: createStatus
      )
    }
    let requirementText = try ExactCodeSigningRequirement.make(
      peerSigningIdentifier: expectedSigningIdentifier,
      teamIdentifier: teamIdentifier
    )
    var requirement: SecRequirement?
    let requirementStatus = SecRequirementCreateWithString(
      requirementText as CFString,
      [],
      &requirement
    )
    guard requirementStatus == errSecSuccess, let requirement else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecRequirementCreateWithString",
        status: requirementStatus
      )
    }
    let flags = SecCSFlags(
      rawValue: kSecCSStrictValidate | kSecCSCheckAllArchitectures
    )
    let validityStatus = SecStaticCodeCheckValidity(staticCode, flags, requirement)
    guard validityStatus == errSecSuccess else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecStaticCodeCheckValidity",
        status: validityStatus
      )
    }
  }
}

public enum ExactCodeSigningRequirement {
  public static func make(
    peerSigningIdentifier: String,
    teamIdentifier: String
  ) throws -> String {
    let identity = try CodeSigningIdentity(
      signingIdentifier: peerSigningIdentifier,
      teamIdentifier: teamIdentifier
    )
    let requirement =
      "anchor apple generic and identifier \"\(identity.signingIdentifier)\" "
      + "and certificate leaf[subject.OU] = \"\(identity.teamIdentifier)\""
    var compiledRequirement: SecRequirement?
    let status = SecRequirementCreateWithString(
      requirement as CFString,
      [],
      &compiledRequirement
    )
    guard status == errSecSuccess, compiledRequirement != nil else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecRequirementCreateWithString",
        status: status
      )
    }
    return requirement
  }
}
