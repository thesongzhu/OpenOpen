import Darwin
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
  public static func validateRunningProcessIdentifier(
    _ processIdentifier: Int32,
    expectedSigningIdentifier: String,
    teamIdentifier: String
  ) throws {
    guard processIdentifier > 0 else {
      throw CodeSigningIdentityError.invalidIdentity
    }
    let auditTokenHex = try auditTokenHex(for: processIdentifier)
    try validateRunningProcess(
      auditTokenHex: auditTokenHex,
      expectedSigningIdentifier: expectedSigningIdentifier,
      teamIdentifier: teamIdentifier
    )
  }

  public static func validateRunningProcess(
    auditTokenHex: String,
    expectedSigningIdentifier: String,
    teamIdentifier: String
  ) throws {
    _ = try CodeSigningIdentity(
      signingIdentifier: expectedSigningIdentifier,
      teamIdentifier: teamIdentifier
    )
    let auditToken = try auditTokenData(auditTokenHex)
    var code: SecCode?
    let guestStatus = SecCodeCopyGuestWithAttributes(
      nil,
      [kSecGuestAttributeAudit: auditToken] as CFDictionary,
      [],
      &code
    )
    guard guestStatus == errSecSuccess, let code else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecCodeCopyGuestWithAttributes",
        status: guestStatus
      )
    }
    let text = try ExactCodeSigningRequirement.make(
      peerSigningIdentifier: expectedSigningIdentifier,
      teamIdentifier: teamIdentifier
    )
    try validateRunningCode(code, requirementText: text)
  }

  public static func validatePinnedCodex(auditTokenHex: String) throws {
    let auditToken = try auditTokenData(auditTokenHex)
    var code: SecCode?
    let guestStatus = SecCodeCopyGuestWithAttributes(
      nil,
      [kSecGuestAttributeAudit: auditToken] as CFDictionary,
      [],
      &code
    )
    guard guestStatus == errSecSuccess, let code else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecCodeCopyGuestWithAttributes",
        status: guestStatus
      )
    }
    let text = try pinnedCodexRequirementText()
    try validateRunningCode(code, requirementText: text)
  }

  static func pinnedCodexRequirementText() throws -> String {
    let cdHash = EffectBrokerConstants.codexCDHash
    guard cdHash.count == 40,
      cdHash.utf8.allSatisfy({ byte in
        byte.isOpenOpenASCIIDigit || (0x61...0x66).contains(byte)
      })
    else {
      throw CodeSigningIdentityError.invalidIdentity
    }
    return
      "identifier \"\(EffectBrokerConstants.codexSigningIdentifier)\" and anchor apple generic "
      + "and certificate leaf[subject.OU] = \"\(EffectBrokerConstants.codexTeamIdentifier)\" "
      + "and cdhash H\"\(cdHash)\""
  }

  private static func validateRunningCode(
    _ code: SecCode, requirementText text: String
  ) throws {
    var requirement: SecRequirement?
    let requirementStatus = SecRequirementCreateWithString(
      text as CFString,
      [],
      &requirement
    )
    guard requirementStatus == errSecSuccess, let requirement else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecRequirementCreateWithString",
        status: requirementStatus
      )
    }
    let validityStatus = SecCodeCheckValidity(
      code,
      SecCSFlags(rawValue: kSecCSStrictValidate),
      requirement
    )
    guard validityStatus == errSecSuccess else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "SecCodeCheckValidity",
        status: validityStatus
      )
    }
  }

  private static func auditTokenData(_ hex: String) throws -> Data {
    guard hex.count == 64 else { throw CodeSigningIdentityError.invalidIdentity }
    var bytes = [UInt8]()
    bytes.reserveCapacity(32)
    var index = hex.startIndex
    while index < hex.endIndex {
      let next = hex.index(index, offsetBy: 2)
      guard let byte = UInt8(hex[index..<next], radix: 16) else {
        throw CodeSigningIdentityError.invalidIdentity
      }
      bytes.append(byte)
      index = next
    }
    return Data(bytes)
  }

  private static func auditTokenHex(for processIdentifier: Int32) throws -> String {
    var task = mach_port_name_t(MACH_PORT_NULL)
    let nameStatus = task_name_for_pid(mach_task_self_, processIdentifier, &task)
    guard nameStatus == KERN_SUCCESS else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "task_name_for_pid",
        status: OSStatus(nameStatus)
      )
    }
    defer { mach_port_deallocate(mach_task_self_, task) }
    var token = audit_token_t()
    let expectedCount = mach_msg_type_number_t(
      MemoryLayout<audit_token_t>.size / MemoryLayout<natural_t>.size
    )
    var count = expectedCount
    let tokenStatus = withUnsafeMutablePointer(to: &token) { pointer in
      pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { words in
        task_info(task, task_flavor_t(TASK_AUDIT_TOKEN), words, &count)
      }
    }
    guard tokenStatus == KERN_SUCCESS, count == expectedCount,
      audit_token_to_pid(token) == processIdentifier
    else {
      throw CodeSigningIdentityError.securityFailure(
        operation: "task_info(TASK_AUDIT_TOKEN)",
        status: OSStatus(tokenStatus)
      )
    }
    return withUnsafeBytes(of: token) { bytes in
      bytes.map { String(format: "%02x", $0) }.joined()
    }
  }

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
