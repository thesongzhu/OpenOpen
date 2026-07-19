import Foundation

@objc(OOEffectBrokerXPCProtocol)
public protocol EffectBrokerXPCProtocol {
  func brokerStatus(
    _ requestJSON: Data,
    withReply reply: @escaping (Data) -> Void
  )

  func session(
    _ requestJSON: Data,
    withReply reply: @escaping (Data) -> Void
  )

  func enrollCore(
    _ requestJSON: Data,
    withReply reply: @escaping (Data) -> Void
  )

  func prepareCodexRuntimeHome(
    _ requestJSON: Data,
    withReply reply: @escaping (Data) -> Void
  )

  func acquireCoreLease(
    _ requestJSON: Data,
    withReply reply: @escaping (Data) -> Void
  )

  func applyRuntimeControl(
    _ requestJSON: Data,
    withReply reply: @escaping (Data) -> Void
  )

  func putMissionFile(
    _ permitJSON: Data,
    payload: FileHandle,
    withReply reply: @escaping (Data) -> Void
  )

  func reconcileMissionFile(
    _ permitJSON: Data,
    withReply reply: @escaping (Data) -> Void
  )
}

public struct AuthenticatedBrokerPeer: Equatable, Sendable {
  public let effectiveUserIdentifier: uid_t
  public let processIdentifier: pid_t
  public let auditSessionIdentifier: au_asid_t

  public init(
    effectiveUserIdentifier: uid_t,
    processIdentifier: pid_t,
    auditSessionIdentifier: au_asid_t
  ) {
    self.effectiveUserIdentifier = effectiveUserIdentifier
    self.processIdentifier = processIdentifier
    self.auditSessionIdentifier = auditSessionIdentifier
  }
}

public protocol EffectBrokerBackend: AnyObject {
  func brokerStatus(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  )

  func session(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  )

  func enrollCore(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  )

  func prepareCodexRuntimeHome(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  )

  func acquireCoreLease(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  )

  func applyRuntimeControl(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  )

  func putMissionFile(
    peer: AuthenticatedBrokerPeer,
    permitJSON: Data,
    payload: FileHandle,
    reply: @escaping (Data) -> Void
  )

  func reconcileMissionFile(
    peer: AuthenticatedBrokerPeer,
    permitJSON: Data,
    reply: @escaping (Data) -> Void
  )
}

enum BrokerRequestKind: String {
  case brokerStatus
  case session
  case enrollCore
  case prepareCodexRuntimeHome
  case coreLeaseAcquire
  case applyRuntimeControl
  case putMissionFile
  case reconcileMissionFile
}

enum TypedJSONEnvelope {
  static let maximumBytes = 256 * 1024

  static func accepts(_ data: Data, kind: BrokerRequestKind) -> Bool {
    canonicalized(data, kind: kind) != nil
  }

  static func canonicalized(_ data: Data, kind: BrokerRequestKind) -> Data? {
    guard !data.isEmpty, data.count <= maximumBytes,
      let dictionary = StrictJSONDocument.object(from: data),
      dictionary["version"] as? Int == 1,
      dictionary["type"] as? String == kind.rawValue
    else {
      return nil
    }
    guard acceptsExactSchema(dictionary, kind: kind) else {
      return nil
    }
    return StrictJSONDocument.canonicalData(from: dictionary)
  }

  private static func acceptsExactSchema(
    _ dictionary: [String: Any],
    kind: BrokerRequestKind
  ) -> Bool {
    switch kind {
    case .session:
      return hasExactlyKeys(dictionary, ["type", "version"])
    case .brokerStatus:
      return hasExactlyKeys(dictionary, ["challenge", "type", "version"])
        && isLowerHex(dictionary["challenge"], count: 64)
    case .enrollCore:
      guard
        hasExactlyKeys(
          dictionary,
          ["coreKeyId", "coreVerifyingKeyHex", "type", "version"]
        ),
        let coreKeyID = dictionary["coreKeyId"] as? String,
        let coreVerifyingKeyHex = dictionary["coreVerifyingKeyHex"] as? String
      else {
        return false
      }
      return EnrolledBrokerTrustAnchor.isSelfConsistent(
        keyID: coreKeyID,
        verifyingKeyHex: coreVerifyingKeyHex
      )
    case .prepareCodexRuntimeHome:
      return hasExactlyKeys(dictionary, ["type", "version"])
    case .coreLeaseAcquire:
      return hasExactlyKeys(
        dictionary,
        ["codexPid", "coreInstanceNonce", "corePid", "type", "version"]
      )
        && isPositiveInt64(dictionary["corePid"])
        && isPositiveInt64(dictionary["codexPid"])
        && isLowerHex(dictionary["coreInstanceNonce"], count: 64)
    case .applyRuntimeControl:
      guard hasExactlyKeys(dictionary, ["control", "type", "version"]),
        let control = dictionary["control"] as? [String: Any]
      else {
        return false
      }
      return acceptsRuntimeControl(control)
    case .putMissionFile, .reconcileMissionFile:
      guard hasExactlyKeys(dictionary, ["permit", "type", "version"]),
        let permit = dictionary["permit"] as? [String: Any]
      else {
        return false
      }
      let allowedPurposes: Set<String> =
        kind == .putMissionFile
        ? ["execute", "reattestOnly"]
        : ["reconcile"]
      return acceptsPermit(permit, allowedPurposes: allowedPurposes)
    }
  }

  private static func acceptsPermit(
    _ permit: [String: Any],
    allowedPurposes: Set<String>
  ) -> Bool {
    guard
      hasExactlyKeys(
        permit,
        [
          "authorizationSignatureHex",
          "authorizationAnchor",
          "brokerSessionNonce",
          "command",
          "coreKeyId",
          "expiresAtMs",
          "issuedAtMs",
          "purpose",
          "runtimeRevision",
          "stableEffectHash",
        ]
      ),
      let authorizationAnchor = permit["authorizationAnchor"] as? [String: Any],
      acceptsSourceAnchor(authorizationAnchor),
      let command = permit["command"] as? [String: Any],
      isLowerHex(permit["authorizationSignatureHex"], count: 128),
      isLowerHex(permit["brokerSessionNonce"], count: 64),
      isLowerHex(permit["coreKeyId"], count: 64),
      isNonnegativeInt64(permit["expiresAtMs"]),
      isNonnegativeInt64(permit["issuedAtMs"]),
      (permit["purpose"] as? String).map(allowedPurposes.contains) == true,
      isPositiveUInt64(permit["runtimeRevision"]),
      isLowerHex(permit["stableEffectHash"], count: 64)
    else {
      return false
    }
    return acceptsCommand(command)
  }

  private static func acceptsRuntimeControl(_ control: [String: Any]) -> Bool {
    hasExactlyKeys(
      control,
      [
        "authorizationSignatureHex", "coreKeyId", "enabled", "protocolVersion",
        "revision", "updatedAtMs",
      ]
    )
      && control["protocolVersion"] as? Int == 1
      && control["enabled"] is Bool
      && isPositiveUInt64(control["revision"])
      && isNonnegativeInt64(control["updatedAtMs"])
      && isLowerHex(control["coreKeyId"], count: 64)
      && isLowerHex(control["authorizationSignatureHex"], count: 128)
  }

  private static func acceptsCommand(_ command: [String: Any]) -> Bool {
    guard
      hasExactlyKeys(
        command,
        [
          "approvalIds",
          "effect",
          "effectId",
          "missionId",
          "missionScopeDigest",
          "missionUpdatedAtMs",
          "protocolVersion",
          "sourceAnchor",
        ]
      ),
      command["protocolVersion"] as? Int == 1,
      isBoundedIdentifier(command["effectId"]),
      isBoundedIdentifier(command["missionId"]),
      isNonemptyBoundedString(command["missionScopeDigest"], maximumBytes: 256),
      isNonnegativeInt64(command["missionUpdatedAtMs"]),
      let approvalIDs = command["approvalIds"] as? [String],
      !approvalIDs.isEmpty,
      approvalIDs.count <= 64,
      Set(approvalIDs).count == approvalIDs.count,
      approvalIDs.allSatisfy({ isBoundedIdentifier($0) }),
      let sourceAnchor = command["sourceAnchor"] as? [String: Any],
      acceptsSourceAnchor(sourceAnchor),
      let effect = command["effect"] as? [String: Any],
      acceptsPutFileEffect(effect)
    else {
      return false
    }
    return true
  }

  private static func acceptsSourceAnchor(_ anchor: [String: Any]) -> Bool {
    hasExactlyKeys(anchor, ["entryHash", "sequence", "signatureHex"])
      && isPositiveInt64(anchor["sequence"])
      && isLowerHex(anchor["entryHash"], count: 64)
      && isLowerHex(anchor["signatureHex"], count: 128)
  }

  private static func acceptsPutFileEffect(_ effect: [String: Any]) -> Bool {
    guard
      hasExactlyKeys(
        effect,
        ["actionDigest", "pathComponents", "payload", "type"]
      ),
      effect["type"] as? String == "putFile",
      isLowerHex(effect["actionDigest"], count: 64),
      let pathComponents = effect["pathComponents"] as? [String],
      !pathComponents.isEmpty,
      pathComponents.count <= 16,
      pathComponents.allSatisfy(isSafePathComponent),
      let payload = effect["payload"] as? [String: Any]
    else {
      return false
    }
    return hasExactlyKeys(payload, ["byteLen", "sha256"])
      && isBoundedUInt64(payload["byteLen"], maximum: 512 * 1024 * 1024)
      && isLowerHex(payload["sha256"], count: 64)
  }

  private static func hasExactlyKeys(
    _ dictionary: [String: Any],
    _ keys: Set<String>
  ) -> Bool {
    Set(dictionary.keys) == keys
  }

  private static func isBoundedIdentifier(_ value: Any?) -> Bool {
    guard let value = value as? String else {
      return false
    }
    return isBoundedIdentifier(value)
  }

  private static func isBoundedIdentifier(_ value: String) -> Bool {
    let bytes = Array(value.utf8)
    return !bytes.isEmpty && bytes.count <= 64
      && bytes.first.map(isLowerASCIIAlphaNumeric) == true
      && bytes.last.map(isLowerASCIIAlphaNumeric) == true
      && bytes.allSatisfy { isLowerASCIIAlphaNumeric($0) || $0 == 0x2D }
  }

  private static func isNonemptyBoundedString(
    _ value: Any?,
    maximumBytes: Int
  ) -> Bool {
    guard let value = value as? String else {
      return false
    }
    return !value.isEmpty && value.utf8.count <= maximumBytes
  }

  private static func isSafePathComponent(_ value: String) -> Bool {
    !value.isEmpty && value != "." && value != ".." && value.utf8.count <= 128
      && value.utf8.allSatisfy { byte in
        isLowerASCIIAlphaNumeric(byte)
          || byte == 0x2E || byte == 0x5F || byte == 0x2D
      }
  }

  private static func isLowerASCIIAlphaNumeric(_ byte: UInt8) -> Bool {
    (0x30...0x39).contains(byte) || (0x61...0x7A).contains(byte)
  }

  private static func isLowerHex(_ value: Any?, count: Int) -> Bool {
    guard let value = value as? String, value.utf8.count == count else {
      return false
    }
    return value.utf8.allSatisfy { byte in
      (0x30...0x39).contains(byte) || (0x61...0x66).contains(byte)
    }
  }

  private static func isNonnegativeInt64(_ value: Any?) -> Bool {
    guard let number = value as? NSNumber,
      CFGetTypeID(number) != CFBooleanGetTypeID(), !CFNumberIsFloatType(number)
    else {
      return false
    }
    return Int64(number.stringValue).map { $0 >= 0 } == true
  }

  private static func isPositiveInt64(_ value: Any?) -> Bool {
    guard let number = value as? NSNumber,
      CFGetTypeID(number) != CFBooleanGetTypeID(), !CFNumberIsFloatType(number)
    else {
      return false
    }
    return Int64(number.stringValue).map { $0 > 0 } == true
  }

  private static func isBoundedUInt64(_ value: Any?, maximum: UInt64) -> Bool {
    guard let number = value as? NSNumber,
      CFGetTypeID(number) != CFBooleanGetTypeID(),
      !CFNumberIsFloatType(number),
      let integer = UInt64(number.stringValue)
    else {
      return false
    }
    return integer <= maximum
  }

  private static func isPositiveUInt64(_ value: Any?) -> Bool {
    isBoundedUInt64(value, maximum: UInt64.max) && (value as? NSNumber)?.uint64Value != 0
  }

  static let rejectedResponse = Data(
    #"{"error":{"code":"invalidTypedJSON","message":"Request must match the exact typed schema without caller-supplied authority"},"status":"rejected","version":1}"#
      .utf8
  )
}

final class BrokerConnectionService: NSObject, EffectBrokerXPCProtocol {
  private let peer: AuthenticatedBrokerPeer
  private let backend: any EffectBrokerBackend

  init(peer: AuthenticatedBrokerPeer, backend: any EffectBrokerBackend) {
    self.peer = peer
    self.backend = backend
  }

  func brokerStatus(_ requestJSON: Data, withReply reply: @escaping (Data) -> Void) {
    guard
      let canonicalJSON = TypedJSONEnvelope.canonicalized(
        requestJSON,
        kind: .brokerStatus
      )
    else {
      reply(TypedJSONEnvelope.rejectedResponse)
      return
    }
    backend.brokerStatus(peer: peer, requestJSON: canonicalJSON, reply: reply)
  }

  func session(_ requestJSON: Data, withReply reply: @escaping (Data) -> Void) {
    guard
      let canonicalJSON = TypedJSONEnvelope.canonicalized(
        requestJSON,
        kind: .session
      )
    else {
      reply(TypedJSONEnvelope.rejectedResponse)
      return
    }
    backend.session(peer: peer, requestJSON: canonicalJSON, reply: reply)
  }

  func enrollCore(_ requestJSON: Data, withReply reply: @escaping (Data) -> Void) {
    guard
      let canonicalJSON = TypedJSONEnvelope.canonicalized(
        requestJSON,
        kind: .enrollCore
      )
    else {
      reply(TypedJSONEnvelope.rejectedResponse)
      return
    }
    backend.enrollCore(peer: peer, requestJSON: canonicalJSON, reply: reply)
  }

  func prepareCodexRuntimeHome(
    _ requestJSON: Data, withReply reply: @escaping (Data) -> Void
  ) {
    guard
      let canonicalJSON = TypedJSONEnvelope.canonicalized(
        requestJSON,
        kind: .prepareCodexRuntimeHome
      )
    else {
      reply(TypedJSONEnvelope.rejectedResponse)
      return
    }
    backend.prepareCodexRuntimeHome(peer: peer, requestJSON: canonicalJSON, reply: reply)
  }

  func acquireCoreLease(_ requestJSON: Data, withReply reply: @escaping (Data) -> Void) {
    guard
      let canonicalJSON = TypedJSONEnvelope.canonicalized(
        requestJSON,
        kind: .coreLeaseAcquire
      )
    else {
      reply(TypedJSONEnvelope.rejectedResponse)
      return
    }
    backend.acquireCoreLease(peer: peer, requestJSON: canonicalJSON, reply: reply)
  }

  func applyRuntimeControl(_ requestJSON: Data, withReply reply: @escaping (Data) -> Void) {
    guard
      let canonicalJSON = TypedJSONEnvelope.canonicalized(
        requestJSON,
        kind: .applyRuntimeControl
      )
    else {
      reply(TypedJSONEnvelope.rejectedResponse)
      return
    }
    backend.applyRuntimeControl(
      peer: peer,
      requestJSON: canonicalJSON,
      reply: reply
    )
  }

  func putMissionFile(
    _ permitJSON: Data,
    payload: FileHandle,
    withReply reply: @escaping (Data) -> Void
  ) {
    guard
      let canonicalJSON = TypedJSONEnvelope.canonicalized(
        permitJSON,
        kind: .putMissionFile
      )
    else {
      reply(TypedJSONEnvelope.rejectedResponse)
      return
    }
    backend.putMissionFile(
      peer: peer,
      permitJSON: canonicalJSON,
      payload: payload,
      reply: reply
    )
  }

  func reconcileMissionFile(
    _ permitJSON: Data,
    withReply reply: @escaping (Data) -> Void
  ) {
    guard
      let canonicalJSON = TypedJSONEnvelope.canonicalized(
        permitJSON,
        kind: .reconcileMissionFile
      )
    else {
      reply(TypedJSONEnvelope.rejectedResponse)
      return
    }
    backend.reconcileMissionFile(
      peer: peer,
      permitJSON: canonicalJSON,
      reply: reply
    )
  }
}
