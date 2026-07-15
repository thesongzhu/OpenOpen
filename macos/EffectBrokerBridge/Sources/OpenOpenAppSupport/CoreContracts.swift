import EffectBrokerBridge
import Foundation

public struct EmptyParameters: Codable, Sendable {}

public struct RuntimeControl: Codable, Equatable, Sendable {
  public let enabled: Bool
  public let revision: UInt64
  public let updatedAtMs: Int64
}

public struct RuntimeChallenge: Codable, Equatable, Sendable {
  public let challenge: String
}

public struct SetEnabledParameters: Codable, Sendable {
  public let enabled: Bool

  public init(enabled: Bool) {
    self.enabled = enabled
  }
}

public struct RuntimeControlAuthorization: Codable, Equatable, Sendable {
  public let protocolVersion: UInt32
  public let enabled: Bool
  public let revision: UInt64
  public let updatedAtMs: Int64
  public let coreKeyId: String
  public let authorizationSignatureHex: String
}

public struct RuntimeControlReceipt: Codable, Equatable, Sendable {
  public let protocolVersion: UInt32
  public let authorizationHash: String
  public let checkpointNonce: String
  public let requestNonce: String?
  public let brokerKeyId: String
  public let brokerSignatureHex: String
}

public struct CommitRuntimeParameters: Codable, Sendable {
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    authorization: RuntimeControlAuthorization,
    brokerReceipt: RuntimeControlReceipt
  ) {
    self.authorization = authorization
    self.brokerReceipt = brokerReceipt
  }
}

public struct RuntimeProofParameters: Codable, Sendable {
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(_ state: BrokerRuntimeState) {
    authorization = state.authorization
    brokerReceipt = state.receipt
  }
}

public struct BrokerEnrollmentRecord: Codable, Equatable, Sendable {
  public let version: UInt32
  public let brokerKeyId: String
  public let brokerVerifyingKeyHex: String
  public let helperDesignatedRequirementDigest: String
  public let installedAtMs: Int64
  public let coreKeyId: String
  public let coreAuthorizationSignatureHex: String
}

struct CoreEffectIdentityResponse: Codable, Sendable {
  let coreKeyId: String
  let coreVerifyingKeyHex: String
  let corePid: Int32
  let coreInstanceNonce: String
}

struct CodexRuntimeIdentityResponse: Codable, Sendable {
  let codexPid: Int32
}

public struct CoreInstanceLease: Codable, Equatable, Sendable {
  public let protocolVersion: UInt32
  public let auditEuid: UInt32
  public let appPid: Int32
  public let appStartTimeUs: UInt64
  public let corePid: Int32
  public let coreStartTimeUs: UInt64
  public let coreAuditTokenHex: String
  public let codexPid: Int32
  public let codexStartTimeUs: UInt64
  public let codexAuditTokenHex: String
  public let coreInstanceNonce: String
  public let issuedAtMs: Int64
  public let brokerKeyId: String
  public let brokerSignatureHex: String
}

public struct InstallCoreLeaseParameters: Codable, Sendable {
  public let lease: CoreInstanceLease
}

struct SignBrokerEnrollmentParameters: Codable, Sendable {
  let brokerKeyId: String
  let brokerVerifyingKeyHex: String
  let helperDesignatedRequirementDigest: String
  let installedAtMs: Int64

  init(_ anchor: EnrolledBrokerTrustAnchor) {
    brokerKeyId = anchor.brokerKeyID
    brokerVerifyingKeyHex = anchor.brokerVerifyingKeyHex
    helperDesignatedRequirementDigest = anchor.helperDesignatedRequirementDigest
    installedAtMs = anchor.installedAtMilliseconds
  }
}

public struct InstallBrokerParameters: Codable, Sendable {
  public let record: BrokerEnrollmentRecord
}

public struct InstallBrokerResult: Codable, Sendable {
  public let status: String
}

public struct MicrophoneState: Codable, Equatable, Sendable {
  public let available: Bool
  public let reason: String
}

public struct OutcomeSuggestion: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let title: String
  public let whyNow: String
  public let proposedSteps: [String]
  public let sourceRefs: [String]
}

public struct DashboardState: Codable, Equatable, Sendable {
  public let activeCards: [ActiveOutcomeCard]
  public let microphone: MicrophoneState
  public let runtime: RuntimeControl
  public let suggestion: OutcomeSuggestion?

  public func validated() throws -> Self {
    guard activeCards.count <= 3 else {
      throw CoreClientError.contractViolation("Dashboard exceeded three active cards")
    }
    return self
  }
}

public struct ActiveOutcomeCard: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let title: String
  public let state: String
}

public enum AccountState: Codable, Equatable, Sendable {
  case notConnected
  case chatGpt(email: String, planType: String)

  private enum CodingKeys: String, CodingKey {
    case state, email, planType
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    switch try container.decode(String.self, forKey: .state) {
    case "notConnected":
      self = .notConnected
    case "chatGpt":
      self = .chatGpt(
        email: try container.decode(String.self, forKey: .email),
        planType: try container.decode(String.self, forKey: .planType)
      )
    default:
      throw DecodingError.dataCorruptedError(
        forKey: .state,
        in: container,
        debugDescription: "Unsupported account state"
      )
    }
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    switch self {
    case .notConnected:
      try container.encode("notConnected", forKey: .state)
    case .chatGpt(let email, let planType):
      try container.encode("chatGpt", forKey: .state)
      try container.encode(email, forKey: .email)
      try container.encode(planType, forKey: .planType)
    }
  }
}

public struct ChatGptLogin: Codable, Equatable, Sendable {
  public let authUrl: String
  public let loginId: String
}

public struct AwaitLoginParameters: Codable, Sendable {
  public let loginId: String
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt
}

public struct GptModel: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let displayName: String
  public let supportedReasoningEfforts: [String]
}

public struct OutcomeRequest: Codable, Sendable {
  public let prompt: String
  public let allowedSourceRefs: [String]
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    prompt: String,
    proof: BrokerRuntimeState,
    allowedSourceRefs: [String] = []
  ) {
    self.prompt = prompt
    self.allowedSourceRefs = allowedSourceRefs
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

struct RpcRequest<Parameters: Encodable & Sendable>: Encodable, Sendable {
  let jsonrpc = "2.0"
  let id: UInt64
  let method: String
  let params: Parameters
}

struct RpcFailure: Decodable, Sendable {
  let code: Int
  let message: String
}

public enum CoreClientError: Error, Equatable, LocalizedError, Sendable {
  case invalidBundleLayout
  case keychain(Int32)
  case processUnavailable
  case processTerminated
  case requestTimedOut
  case requestCancelled
  case oversizedRequest
  case oversizedFrame
  case malformedResponse
  case unknownResponseIdentifier
  case remote(code: Int, message: String)
  case contractViolation(String)

  public var errorDescription: String? {
    switch self {
    case .invalidBundleLayout: "OpenOpen installation is incomplete."
    case .keychain: "OpenOpen could not access its local security key."
    case .processUnavailable: "OpenOpen Core could not start."
    case .processTerminated: "OpenOpen Core stopped unexpectedly."
    case .requestTimedOut: "OpenOpen Core did not respond before the safety deadline."
    case .requestCancelled: "The OpenOpen request was cancelled."
    case .oversizedRequest: "The OpenOpen request exceeded the local safety limit."
    case .oversizedFrame: "OpenOpen Core returned an oversized response."
    case .malformedResponse: "OpenOpen Core returned an invalid response."
    case .unknownResponseIdentifier: "OpenOpen Core returned an unknown response."
    case .remote(_, let message): message
    case .contractViolation(let message): message
    }
  }
}
