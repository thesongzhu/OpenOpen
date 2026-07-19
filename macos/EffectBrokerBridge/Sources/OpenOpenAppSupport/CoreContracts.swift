import CryptoKit
import EffectBrokerBridge
import Foundation

public struct EmptyParameters: Codable, Sendable {}

public enum CoreTerminationReason: String, Equatable, Sendable {
  case exited
  case uncaughtSignal
  case transportFailure
  case requestTimedOut
  case protocolViolation
  case explicitShutdown
}

public struct CoreTerminationEvent: Equatable, Sendable {
  public let generation: UInt64
  public let reason: CoreTerminationReason
  public let exitStatus: Int32?

  public init(
    generation: UInt64,
    reason: CoreTerminationReason,
    exitStatus: Int32? = nil
  ) {
    self.generation = generation
    self.reason = reason
    self.exitStatus = exitStatus
  }
}

public protocol CoreLifecycleMonitoring: Sendable {
  func terminationEvents() -> AsyncStream<CoreTerminationEvent>
}

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

public enum ChannelKind: String, Codable, CaseIterable, Equatable, Hashable, Sendable {
  case iMessage
  case discord
}

public struct ChannelPairing: Codable, Equatable, Sendable {
  public let channel: ChannelKind
  public let ownerSenderId: String
  public let conversationId: String
  public let requireExplicitAddress: Bool
  public let discord: DiscordPairingMetadata?
  public let pairedAtMs: Int64

  public init(
    channel: ChannelKind,
    ownerSenderId: String,
    conversationId: String,
    discord: DiscordPairingMetadata? = nil,
    pairedAtMs: Int64
  ) {
    self.channel = channel
    self.ownerSenderId = ownerSenderId
    self.conversationId = conversationId
    requireExplicitAddress = true
    self.discord = discord
    self.pairedAtMs = pairedAtMs
  }

  public func validated(expectedChannel: ChannelKind? = nil) throws -> Self {
    guard expectedChannel == nil || channel == expectedChannel,
      ChannelContractValidation.providerId(ownerSenderId),
      ChannelContractValidation.providerId(conversationId),
      requireExplicitAddress,
      pairedAtMs >= 0
    else {
      throw CoreClientError.contractViolation("Core returned an invalid durable channel pairing.")
    }
    switch (channel, discord) {
    case (.iMessage, nil):
      break
    case (.discord, .some(let discord)):
      _ = try discord.validated()
    default:
      throw CoreClientError.contractViolation("Core returned mismatched channel pairing metadata.")
    }
    return self
  }
}

public struct DiscordPairingMetadata: Codable, Equatable, Sendable {
  public let guildId: String
  public let botUserId: String
  public let applicationId: String
  public let setupSourceMessageId: String
  public let setupCandidateId: String

  public func validated() throws -> Self {
    guard ChannelContractValidation.positiveSnowflake(guildId),
      ChannelContractValidation.positiveSnowflake(botUserId),
      ChannelContractValidation.positiveSnowflake(applicationId),
      ChannelContractValidation.positiveSnowflake(setupSourceMessageId),
      setupCandidateId.hasPrefix("discord-pair-"),
      ChannelContractValidation.lowerHex(String(setupCandidateId.dropFirst(13)), count: 64)
    else {
      throw CoreClientError.contractViolation("Core returned invalid durable Discord identity.")
    }
    return self
  }
}

public enum ChannelInboundMessageClass: String, Codable, CaseIterable, Equatable, Sendable {
  case missionParticipation
  case needYouResponse
}

public enum ChannelRouteRole: String, Codable, Equatable, Sendable {
  case primary
  case additional
}

public struct ChannelRoute: Codable, Equatable, Identifiable, Sendable {
  public var id: String { routeId }
  public let routeId: String
  public let role: ChannelRouteRole
  public let channel: ChannelKind
  public let conversationId: String
  public let ownerSenderId: String
  public let providerIdentity: String?
  public let sourceMessageId: String?
  public let allowedInboundClasses: [ChannelInboundMessageClass]
  public let allowedOutboundClasses: [ChannelMessageKind]
  public let revision: UInt64
  public let approvalId: String
  public let auditId: String
  public let boundAtMs: Int64
  public let updatedAtMs: Int64

  public func validated() throws -> Self {
    guard ChannelContractValidation.canonicalEffectId(routeId),
      ChannelContractValidation.providerId(conversationId),
      ChannelContractValidation.providerId(ownerSenderId),
      providerIdentity.map(ChannelContractValidation.providerId) ?? true,
      sourceMessageId.map(ChannelContractValidation.providerId) ?? true,
      !allowedInboundClasses.isEmpty,
      ChannelContractValidation.sortedInbound(allowedInboundClasses),
      ChannelContractValidation.sortedOutbound(allowedOutboundClasses),
      revision > 0,
      ChannelContractValidation.canonicalEffectId(approvalId),
      ChannelContractValidation.canonicalEffectId(auditId),
      boundAtMs >= 0,
      updatedAtMs >= boundAtMs,
      role != .primary || sourceMessageId != nil
    else {
      throw CoreClientError.contractViolation("Core returned an invalid Mission channel route.")
    }
    return self
  }
}

public struct ChannelRouteSet: Codable, Equatable, Sendable {
  public let missionId: String
  public let revision: UInt64
  public let primaryRouteId: String
  public let routes: [ChannelRoute]

  public var primaryRoute: ChannelRoute? {
    routes.first { $0.routeId == primaryRouteId }
  }

  public func validated(expectedMissionId: String? = nil) throws -> Self {
    guard ChannelContractValidation.canonicalMissionId(missionId),
      expectedMissionId == nil || expectedMissionId == missionId,
      revision > 0,
      (1...8).contains(routes.count),
      routes.first?.role == .primary,
      routes.first?.routeId == primaryRouteId,
      routes.filter({ $0.role == .primary }).count == 1
    else {
      throw CoreClientError.contractViolation("Core returned an invalid Mission route set.")
    }
    var routeIds = Set<String>()
    var boundaries = Set<String>()
    for route in routes {
      _ = try route.validated()
      let boundary =
        "\(route.channel.rawValue)\u{1f}\(route.conversationId)\u{1f}\(route.ownerSenderId)"
      guard route.revision <= revision,
        routeIds.insert(route.routeId).inserted,
        boundaries.insert(boundary).inserted
      else {
        throw CoreClientError.contractViolation("Core returned conflicting Mission routes.")
      }
    }
    return self
  }
}

private enum ChannelContractValidation {
  static func providerId(_ value: String) -> Bool {
    !value.isEmpty
      && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
      && value.utf8.count <= 512
      && !value.unicodeScalars.contains { scalar in
        scalar.value == 0 || CharacterSet.controlCharacters.contains(scalar)
      }
  }

  static func canonicalEffectId(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128
      && value.utf8.allSatisfy { byte in
        (97...122).contains(byte) || (48...57).contains(byte) || byte == 45 || byte == 95
      }
  }

  static func canonicalMissionId(_ value: String) -> Bool {
    guard !value.isEmpty, value.utf8.count <= 64,
      let first = value.utf8.first, let last = value.utf8.last,
      asciiLowerAlphanumeric(first), asciiLowerAlphanumeric(last)
    else { return false }
    return value.utf8.allSatisfy { asciiLowerAlphanumeric($0) || $0 == 45 }
  }

  static func lowerHex(_ value: String, count: Int) -> Bool {
    value.utf8.count == count
      && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
  }

  static func positiveSnowflake(_ value: String) -> Bool {
    value.utf8.allSatisfy { (48...57).contains($0) }
      && UInt64(value).map { $0 > 0 } == true
  }

  static func boundedDisplayName(_ value: String) -> Bool {
    !value.isEmpty && value.count <= 100
      && !value.unicodeScalars.contains { CharacterSet.controlCharacters.contains($0) }
  }

  static func connectionStatus(_ value: String) -> Bool {
    ["disconnected", "connecting", "connected", "reconnecting", "faulted"].contains(value)
  }

  static func sortedInbound(_ values: [ChannelInboundMessageClass]) -> Bool {
    let ranks = values.map { $0 == .missionParticipation ? 0 : 1 }
    return Set(ranks).count == ranks.count && ranks == ranks.sorted()
  }

  static func sortedOutbound(_ values: [ChannelMessageKind]) -> Bool {
    let ranks = values.map { value in
      switch value {
      case .needYou: 0
      case .progress: 1
      case .receipt: 2
      }
    }
    return Set(ranks).count == ranks.count && ranks == ranks.sorted()
  }

  private static func asciiLowerAlphanumeric(_ byte: UInt8) -> Bool {
    (97...122).contains(byte) || (48...57).contains(byte)
  }
}

public enum ChannelRouteApprovalDecision: String, Codable, Equatable, Sendable {
  case approve
  case reject
}

public struct ChannelRouteApproval: Codable, Equatable, Sendable {
  public let approvalId: String
  public let missionId: String
  public let expectedRouteSetRevision: UInt64
  public let channel: ChannelKind
  public let conversationId: String
  public let ownerSenderId: String
  public let providerIdentity: String?
  public let allowedInboundClasses: [ChannelInboundMessageClass]
  public let allowedOutboundClasses: [ChannelMessageKind]
  public let actorId: String
  public let decision: ChannelRouteApprovalDecision
  public let decidedAtMs: Int64
}

public struct ChannelRouteDraft: Equatable, Identifiable, Sendable {
  public var id: String { approvalId }
  public let approvalId: String
  public let missionId: String
  public let expectedRouteSetRevision: UInt64
  public let pairing: ChannelPairing

  public var providerIdentity: String? { pairing.discord?.applicationId }
}

public struct BindChannelRouteParameters: Codable, Sendable {
  public let approval: ChannelRouteApproval
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(approval: ChannelRouteApproval, proof: BrokerRuntimeState) {
    self.approval = approval
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct PairChannelParameters: Codable, Sendable {
  public let pairing: ChannelPairing
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(pairing: ChannelPairing, proof: BrokerRuntimeState) {
    self.pairing = pairing
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ChannelSelectionParameters: Codable, Sendable {
  public let channel: ChannelKind
}

public struct PollChannelParameters: Codable, Sendable {
  public let channel: ChannelKind
  public let modelWorkAllowed: Bool
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    channel: ChannelKind,
    modelWorkAllowed: Bool,
    proof: BrokerRuntimeState
  ) {
    self.channel = channel
    self.modelWorkAllowed = modelWorkAllowed
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct AcknowledgeChannelFailureParameters: Codable, Sendable {
  public let incidentId: String
  public let expectedIncidentAuditAnchor: ChannelFailureAuditAnchor
  public let acknowledgedAtMs: Int64
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    incident: ChannelFailureIncident,
    acknowledgedAtMs: Int64,
    proof: BrokerRuntimeState
  ) {
    incidentId = incident.incidentId
    expectedIncidentAuditAnchor = incident.incidentAuditAnchor
    self.acknowledgedAtMs = acknowledgedAtMs
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct StartDiscordParameters: Codable, Sendable {
  public let botToken: String
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(botToken: String, proof: BrokerRuntimeState) {
    self.botToken = botToken
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ConfirmDiscordSetupParameters: Codable, Sendable {
  public let candidateId: String
  public let confirmedAtMs: Int64
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(candidateId: String, confirmedAtMs: Int64, proof: BrokerRuntimeState) {
    self.candidateId = candidateId
    self.confirmedAtMs = confirmedAtMs
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct DiscordBotIdentity: Codable, Equatable, Sendable {
  public let botUserId: UInt64
  public let applicationId: UInt64
  public let botName: String

  public func validated() throws -> Self {
    guard botUserId > 0, applicationId > 0,
      ChannelContractValidation.boundedDisplayName(botName)
    else {
      throw CoreClientError.contractViolation("Core returned an invalid Discord bot identity.")
    }
    return self
  }
}

public struct DiscordPermissionProbe: Codable, Equatable, Sendable {
  public let viewChannel: String
  public let sendMessages: String
  public let readMessageHistory: String
  public let attachFiles: String
  public let historyReadback: String
  public let effectivePermissionBits: UInt64

  public func validated() throws -> Self {
    guard viewChannel == "passed", sendMessages == "passed",
      readMessageHistory == "passed", attachFiles == "passed",
      historyReadback == "passed", effectivePermissionBits == 101_376
    else {
      throw CoreClientError.contractViolation(
        "Discord did not prove the exact required permissions and history readback.")
    }
    return self
  }
}

public struct DiscordPairingCandidate: Codable, Equatable, Sendable {
  public let candidateId: String
  public let sourceMessageId: String
  public let guildId: String
  public let guildName: String
  public let channelId: String
  public let channelName: String
  public let ownerUserId: String
  public let ownerName: String
  public let botUserId: String
  public let applicationId: String
  public let receivedAtMs: Int64
  public let messageContentIntentReady: Bool
  public let permissions: DiscordPermissionProbe

  public func validated(expectedIdentity: DiscordBotIdentity? = nil) throws -> Self {
    guard candidateId.hasPrefix("discord-pair-"),
      ChannelContractValidation.lowerHex(String(candidateId.dropFirst(13)), count: 64),
      ChannelContractValidation.positiveSnowflake(sourceMessageId),
      ChannelContractValidation.positiveSnowflake(guildId),
      ChannelContractValidation.boundedDisplayName(guildName),
      ChannelContractValidation.positiveSnowflake(channelId),
      ChannelContractValidation.boundedDisplayName(channelName),
      ChannelContractValidation.positiveSnowflake(ownerUserId),
      ChannelContractValidation.boundedDisplayName(ownerName),
      ChannelContractValidation.positiveSnowflake(botUserId),
      ChannelContractValidation.positiveSnowflake(applicationId),
      receivedAtMs >= 0,
      messageContentIntentReady
    else {
      throw CoreClientError.contractViolation("Core returned an invalid Discord pairing candidate.")
    }
    _ = try permissions.validated()
    if let expectedIdentity {
      guard botUserId == String(expectedIdentity.botUserId),
        applicationId == String(expectedIdentity.applicationId)
      else {
        throw CoreClientError.contractViolation(
          "Discord pairing candidate changed the verified bot identity.")
      }
    }
    return self
  }
}

public struct DiscordSetupStart: Codable, Equatable, Sendable {
  public let identity: DiscordBotIdentity
  public let installUrl: String
  public let pairingCode: String
  public let status: String

  public var pairingInstruction: String {
    "<@\(String(identity.botUserId))> pair \(pairingCode)"
  }

  public func validated() throws -> Self {
    _ = try identity.validated()
    let expectedURL =
      "https://discord.com/api/oauth2/authorize?client_id=\(identity.applicationId)&scope=bot&permissions=101376"
    guard installUrl == expectedURL,
      ChannelContractValidation.lowerHex(pairingCode, count: 32),
      ChannelContractValidation.connectionStatus(status)
    else {
      throw CoreClientError.contractViolation("Core returned an invalid Discord setup link.")
    }
    return self
  }
}

public struct DiscordSetupPollResponse: Codable, Equatable, Sendable {
  public let status: String
  public let candidate: DiscordPairingCandidate?

  public func validated(expectedIdentity: DiscordBotIdentity) throws -> Self {
    guard ChannelContractValidation.connectionStatus(status),
      candidate == nil || status == "connected"
    else {
      throw CoreClientError.contractViolation("Core returned an invalid Discord setup state.")
    }
    if let candidate { _ = try candidate.validated(expectedIdentity: expectedIdentity) }
    return self
  }
}

public enum ChannelMessageKind: String, Codable, Equatable, Sendable {
  case needYou
  case progress
  case receipt
}

public struct SendChannelMessageParameters: Codable, Sendable {
  public let missionId: String
  public let routeId: String
  public let kind: ChannelMessageKind
  public let content: String
  public let approvedAtMs: Int64
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    missionId: String,
    routeId: String,
    kind: ChannelMessageKind,
    content: String,
    approvedAtMs: Int64,
    proof: BrokerRuntimeState
  ) {
    self.missionId = missionId
    self.routeId = routeId
    self.kind = kind
    self.content = content
    self.approvedAtMs = approvedAtMs
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ChannelStatusResponse: Codable, Equatable, Sendable {
  public let status: String

  public func validated() throws -> Self {
    guard ChannelContractValidation.connectionStatus(status) else {
      throw CoreClientError.contractViolation("Core returned an invalid channel connection state.")
    }
    return self
  }
}

public struct IMessagePrepareResponse: Codable, Equatable, Sendable {
  public let processIdentifier: Int32

  public func validated() throws -> Self {
    guard processIdentifier > 0 else {
      throw CoreClientError.contractViolation("Core returned an invalid iMessage process identity.")
    }
    return self
  }
}

public struct IMessageChat: Codable, Equatable, Identifiable, Sendable {
  public let chatId: String
  public let name: String
  public let service: String
  public let participants: [String]

  public var id: String { chatId }

  public var displayName: String {
    if !name.isEmpty { return name }
    return participants.joined(separator: ", ")
  }
}

public struct IMessageChatsResponse: Codable, Equatable, Sendable {
  public let chats: [IMessageChat]

  public func validated() throws -> Self {
    guard chats.count <= 200 else {
      throw CoreClientError.contractViolation("Core returned too many iMessage conversations.")
    }
    var identifiers = Set<String>()
    for chat in chats {
      guard let identifier = Int64(chat.chatId), identifier > 0,
        identifiers.insert(chat.chatId).inserted,
        chat.service == "iMessage",
        Self.validField(chat.name, allowEmpty: true),
        (1...64).contains(chat.participants.count),
        Set(chat.participants).count == chat.participants.count,
        chat.participants.allSatisfy({ Self.validField($0, allowEmpty: false) })
      else {
        throw CoreClientError.contractViolation("Core returned invalid iMessage conversations.")
      }
    }
    return self
  }

  private static func validField(_ value: String, allowEmpty: Bool) -> Bool {
    value.utf8.count <= 256
      && !value.utf8.contains(0)
      && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
      && (allowEmpty || !value.isEmpty)
  }
}

public struct ChannelMissionEvent: Codable, Equatable, Identifiable, Sendable {
  public var id: String { eventId }
  public let eventId: String
  public let missionId: String
  public let missionRevision: Int64
  public let missionAnchorHash: String
  public let routeId: String
  public let routeSetRevision: UInt64
  public let messageClass: ChannelInboundMessageClass
  public let channel: ChannelKind
  public let sourceMessageId: String
  public let contentSha256: String
  public let recordedAtMs: Int64

  public func validated() throws -> Self {
    guard ChannelContractValidation.canonicalEffectId(eventId),
      ChannelContractValidation.canonicalMissionId(missionId),
      missionRevision > 0,
      ChannelContractValidation.lowerHex(missionAnchorHash, count: 64),
      ChannelContractValidation.canonicalEffectId(routeId),
      routeSetRevision > 0,
      ChannelContractValidation.providerId(sourceMessageId),
      ChannelContractValidation.lowerHex(contentSha256, count: 64),
      recordedAtMs >= 0
    else {
      throw CoreClientError.contractViolation(
        "Core returned an invalid Mission channel participation event.")
    }
    return self
  }
}

public enum ChannelFailureClass: String, Codable, Equatable, Sendable {
  case modelResultUnavailable
}

public struct ChannelFailureAuditAnchor: Codable, Equatable, Sendable {
  public let sequence: Int64
  public let entryHash: String
  public let signatureHex: String

  fileprivate var isValid: Bool {
    sequence > 0
      && Self.isLowercaseHex(entryHash, count: 64)
      && Self.isLowercaseHex(signatureHex, count: 128)
  }

  private static func isLowercaseHex(_ value: String, count: Int) -> Bool {
    value.utf8.count == count
      && value.utf8.allSatisfy { byte in
        (48...57).contains(byte) || (97...102).contains(byte)
      }
  }
}

public struct ChannelFailureAcknowledgement: Codable, Equatable, Sendable {
  public let acknowledgedAtMs: Int64
  public let runtimeRevision: UInt64
  public let auditAnchor: ChannelFailureAuditAnchor
}

public struct ChannelFailureIncident: Codable, Equatable, Identifiable, Sendable {
  public var id: String { incidentId }
  public let incidentId: String
  public let channel: ChannelKind
  public let failureClass: ChannelFailureClass
  public let occurredAtMs: Int64
  public let runtimeRevision: UInt64
  public let dispatchStateHash: String
  public let sourceAuditAnchor: ChannelFailureAuditAnchor
  public let incidentAuditAnchor: ChannelFailureAuditAnchor
  public let acknowledgement: ChannelFailureAcknowledgement?

  public func validated() throws -> Self {
    guard incidentId.utf8.count == 80,
      incidentId.hasPrefix("channel-failure-"),
      Self.isLowercaseHex(String(incidentId.dropFirst(16)), count: 64),
      occurredAtMs >= 0,
      Self.isLowercaseHex(dispatchStateHash, count: 64),
      sourceAuditAnchor.isValid,
      incidentAuditAnchor.isValid,
      incidentAuditAnchor.sequence > sourceAuditAnchor.sequence
    else {
      throw CoreClientError.contractViolation(
        "Core returned an invalid terminal incident."
      )
    }
    if let acknowledgement {
      guard acknowledgement.acknowledgedAtMs >= occurredAtMs,
        acknowledgement.runtimeRevision >= runtimeRevision,
        acknowledgement.auditAnchor.isValid,
        acknowledgement.auditAnchor.sequence > incidentAuditAnchor.sequence
      else {
        throw CoreClientError.contractViolation(
          "Core returned an invalid terminal-incident acknowledgement."
        )
      }
    }
    return self
  }

  public func validatedAcknowledgementResponse(for expected: Self) throws -> Self {
    _ = try expected.validated()
    _ = try validated()
    guard incidentId == expected.incidentId,
      channel == expected.channel,
      failureClass == expected.failureClass,
      occurredAtMs == expected.occurredAtMs,
      runtimeRevision == expected.runtimeRevision,
      dispatchStateHash == expected.dispatchStateHash,
      sourceAuditAnchor == expected.sourceAuditAnchor,
      incidentAuditAnchor == expected.incidentAuditAnchor,
      let acknowledgement,
      expected.acknowledgement == nil || expected.acknowledgement == acknowledgement
    else {
      throw CoreClientError.contractViolation(
        "Core changed immutable terminal-incident evidence while acknowledging it."
      )
    }
    return self
  }

  public func mergedMonotonically(with incoming: Self) throws -> Self {
    _ = try validated()
    _ = try incoming.validated()
    guard incidentId == incoming.incidentId,
      channel == incoming.channel,
      failureClass == incoming.failureClass,
      occurredAtMs == incoming.occurredAtMs,
      runtimeRevision == incoming.runtimeRevision,
      dispatchStateHash == incoming.dispatchStateHash,
      sourceAuditAnchor == incoming.sourceAuditAnchor,
      incidentAuditAnchor == incoming.incidentAuditAnchor
    else {
      throw CoreClientError.contractViolation(
        "Core returned conflicting terminal-incident evidence."
      )
    }
    switch (acknowledgement, incoming.acknowledgement) {
    case (.none, .none):
      return self
    case (.none, .some):
      return incoming
    case (.some, .none):
      return self
    case (.some(let current), .some(let replacement)):
      guard current == replacement else {
        throw CoreClientError.contractViolation(
          "Core returned conflicting terminal-incident acknowledgements."
        )
      }
      return self
    }
  }

  public static func validateCollection(
    _ incidents: [Self], expectedChannel: ChannelKind? = nil
  ) throws -> [Self] {
    guard incidents.count <= 128 else {
      throw CoreClientError.contractViolation("Core returned too many terminal incidents.")
    }
    var identifiers = Set<String>()
    var auditAnchors = Set<String>()
    var prior: (Int64, String)?
    for incident in incidents {
      _ = try incident.validated()
      let acknowledgementAnchorIsUnique =
        incident.acknowledgement.map {
          auditAnchors.insert(Self.anchorIdentity($0.auditAnchor)).inserted
        } ?? true
      guard expectedChannel == nil || incident.channel == expectedChannel,
        identifiers.insert(incident.incidentId).inserted,
        auditAnchors.insert(Self.anchorIdentity(incident.sourceAuditAnchor)).inserted,
        auditAnchors.insert(Self.anchorIdentity(incident.incidentAuditAnchor)).inserted,
        acknowledgementAnchorIsUnique
      else {
        throw CoreClientError.contractViolation(
          "Core returned duplicate or cross-channel terminal incidents."
        )
      }
      let key = (incident.occurredAtMs, incident.incidentId)
      if let prior, key < prior {
        throw CoreClientError.contractViolation(
          "Core returned terminal incidents in an unstable order."
        )
      }
      prior = key
    }
    return incidents
  }

  private static func isLowercaseHex(_ value: String, count: Int) -> Bool {
    value.utf8.count == count
      && value.utf8.allSatisfy { byte in
        (48...57).contains(byte) || (97...102).contains(byte)
      }
  }

  private static func anchorIdentity(_ anchor: ChannelFailureAuditAnchor) -> String {
    "\(anchor.sequence):\(anchor.entryHash):\(anchor.signatureHex)"
  }
}

public struct ChannelPollResponse: Codable, Equatable, Sendable {
  public let connectionStatus: String
  public let eventStatus: String
  public let suggestion: OutcomeSuggestion?
  public let missionEvent: ChannelMissionEvent?
  public let invalidateSuggestionId: String?
  public let failureIncidents: [ChannelFailureIncident]?

  public init(
    connectionStatus: String,
    eventStatus: String,
    suggestion: OutcomeSuggestion?,
    missionEvent: ChannelMissionEvent? = nil,
    invalidateSuggestionId: String? = nil,
    failureIncidents: [ChannelFailureIncident]? = nil
  ) {
    self.connectionStatus = connectionStatus
    self.eventStatus = eventStatus
    self.suggestion = suggestion
    self.missionEvent = missionEvent
    self.invalidateSuggestionId = invalidateSuggestionId
    self.failureIncidents = failureIncidents
  }

  public func validated(for channel: ChannelKind) throws -> Self {
    guard ChannelContractValidation.connectionStatus(connectionStatus) else {
      throw CoreClientError.contractViolation("Core returned an invalid channel connection state.")
    }
    let incidents = try ChannelFailureIncident.validateCollection(
      failureIncidents ?? [], expectedChannel: channel)
    switch eventStatus {
    case "ready":
      guard let suggestion, missionEvent == nil, invalidateSuggestionId == nil,
        incidents.isEmpty
      else {
        throw CoreClientError.contractViolation("Core returned an invalid ready channel result.")
      }
      _ = try suggestion.validated()
    case "missionUpdated", "missionUpdateRecovered":
      guard suggestion == nil, let missionEvent, invalidateSuggestionId == nil,
        incidents.isEmpty, missionEvent.channel == channel
      else {
        throw CoreClientError.contractViolation(
          "Core returned an invalid Mission participation poll state.")
      }
      _ = try missionEvent.validated()
    case "needYou":
      guard !incidents.isEmpty, suggestion == nil, missionEvent == nil else {
        throw CoreClientError.contractViolation(
          "Core returned contradictory terminal-incident poll state."
        )
      }
      if let invalidateSuggestionId {
        guard OutcomeSuggestion.validSuggestionId(invalidateSuggestionId) else {
          throw CoreClientError.contractViolation(
            "Core returned an invalid superseded suggestion identity.")
        }
      }
    case "idle", "ignored", "recovering", "recovered", "deferred", "superseded":
      guard suggestion == nil, missionEvent == nil, invalidateSuggestionId == nil,
        incidents.isEmpty
      else {
        throw CoreClientError.contractViolation("Core returned a contradictory channel poll state.")
      }
    default:
      throw CoreClientError.contractViolation("Core returned an unknown channel poll state.")
    }
    return self
  }
}

public struct ChannelSendResponse: Codable, Equatable, Sendable {
  public let status: String
  public let providerMessageId: String?

  public func validated() throws -> Self {
    let valid =
      switch status {
      case "sent": providerMessageId.map(ChannelContractValidation.providerId) == true
      case "needYou": providerMessageId == nil
      default: false
      }
    guard valid else {
      throw CoreClientError.contractViolation("Core returned an invalid channel delivery result.")
    }
    return self
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

  public func validated() throws -> Self {
    var uniqueRefs = Set<String>()
    guard Self.validSuggestionId(id),
      Self.validText(title, maximumCharacters: 120),
      Self.validText(whyNow, maximumCharacters: 300),
      (1...8).contains(proposedSteps.count),
      proposedSteps.allSatisfy({ Self.validText($0, maximumCharacters: 240) }),
      sourceRefs.allSatisfy({ sourceRef in
        !sourceRef.isEmpty && sourceRef.utf8.count <= 128
          && sourceRef.utf8.allSatisfy { byte in
            (97...122).contains(byte) || (48...57).contains(byte)
              || [45, 95, 58, 46].contains(byte)
          }
          && uniqueRefs.insert(sourceRef).inserted
      })
    else {
      throw CoreClientError.contractViolation("Core returned an invalid Outcome suggestion.")
    }
    return self
  }

  fileprivate static func validSuggestionId(_ value: String) -> Bool {
    guard value.hasPrefix("suggestion-"),
      let separator = value.dropFirst(11).firstIndex(of: "-")
    else { return false }
    let timestamp = value.dropFirst(11)[..<separator]
    let nonce = value[value.index(after: separator)...]
    return Int64(timestamp).map { $0 >= 0 } == true
      && ChannelContractValidation.lowerHex(String(nonce), count: 32)
  }

  private static func validText(_ value: String, maximumCharacters: Int) -> Bool {
    !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      && value.count <= maximumCharacters
      && !value.unicodeScalars.contains { scalar in
        scalar.value == 0 || CharacterSet.controlCharacters.contains(scalar)
      }
  }
}

public struct MissionWorkItem: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let title: String
}

public struct ReminderTarget: Codable, Equatable, Sendable {
  public let sourceIdentifier: String
  public let calendarIdentifier: String

  public init(sourceIdentifier: String, calendarIdentifier: String) {
    self.sourceIdentifier = sourceIdentifier
    self.calendarIdentifier = calendarIdentifier
  }

  public var isValid: Bool {
    let source = sourceIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !source.isEmpty, source == sourceIdentifier, source.utf8.count <= 512 else {
      return false
    }
    let calendar = calendarIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
    return !calendar.isEmpty && calendar == calendarIdentifier && calendar.utf8.count <= 512
  }
}

public enum ReminderWriteDisposition: String, Codable, Equatable, Sendable {
  case createOnce
  case recoverOnly
}

public struct ReminderWriteAuthorization: Codable, Equatable, Sendable {
  public static let logicalListId = "openopen.default-reminders"
  private static let domain = Data("OPENOPEN_REMINDER_WRITE_V2\0".utf8)

  public let missionId: String
  public let listId: String
  public let payloadSha256: String
  public let approvalId: String
  public let approvalDigest: String
  public let target: ReminderTarget
  public let writeDisposition: ReminderWriteDisposition

  public static func payloadSha256(
    missionId: String, target: ReminderTarget, workItems: [MissionWorkItem]
  ) -> String {
    var payload = domain
    appendLengthPrefixed(missionId, to: &payload)
    appendLengthPrefixed(logicalListId, to: &payload)
    appendLengthPrefixed(target.sourceIdentifier, to: &payload)
    appendLengthPrefixed(target.calendarIdentifier, to: &payload)
    for workItem in workItems {
      appendLengthPrefixed(workItem.id, to: &payload)
      appendLengthPrefixed(workItem.title, to: &payload)
    }
    return SHA256.hash(data: payload).map { String(format: "%02x", $0) }.joined()
  }

  public func validates(missionId: String, workItems: [MissionWorkItem]) -> Bool {
    self.missionId == missionId
      && listId == Self.logicalListId
      && Self.isLowercaseHex(payloadSha256, count: 64)
      && !approvalId.isEmpty
      && Self.isLowercaseHex(approvalDigest, count: 64)
      && target.isValid
      && payloadSha256
        == Self.payloadSha256(missionId: missionId, target: target, workItems: workItems)
  }

  private static func appendLengthPrefixed(_ value: String, to data: inout Data) {
    let encoded = Data(value.utf8)
    var length = UInt64(encoded.count).bigEndian
    withUnsafeBytes(of: &length) { data.append(contentsOf: $0) }
    data.append(encoded)
  }

  private static func isLowercaseHex(_ value: String, count: Int) -> Bool {
    value.utf8.count == count
      && value.utf8.allSatisfy { byte in
        (48...57).contains(byte) || (97...102).contains(byte)
      }
  }
}

public struct ConfirmedMission: Codable, Equatable, Identifiable, Sendable {
  public var id: String { missionId }
  public let missionId: String
  public let title: String
  public let workItems: [MissionWorkItem]
  public let reminderAuthorization: ReminderWriteAuthorization
  public let reminderDispatch: [ConfirmedReminderDispatch]
  public let reminderLinks: [ReminderLink]

  public func validated() throws -> Self {
    guard Self.validField(missionId, maximumBytes: 256),
      Self.validField(title, maximumBytes: 16 * 1024),
      (1...3).contains(workItems.count),
      Set(workItems.map(\.id)).count == workItems.count,
      workItems.allSatisfy({
        Self.validField($0.id, maximumBytes: 256)
          && Self.validField($0.title, maximumBytes: 16 * 1024)
      }),
      reminderAuthorization.validates(missionId: missionId, workItems: workItems),
      reminderDispatch.isEmpty || reminderDispatch.count == workItems.count,
      Set(reminderDispatch.map(\.workItemId)).count == reminderDispatch.count,
      Set(reminderDispatch.map(\.token)).count == reminderDispatch.count,
      reminderDispatch.allSatisfy({ dispatch in
        workItems.contains(where: { $0.id == dispatch.workItemId })
          && Self.validField(dispatch.token, maximumBytes: 512)
      }),
      reminderLinks.isEmpty || reminderLinks.count == workItems.count,
      Set(reminderLinks.map(\.workItemId)).count == reminderLinks.count,
      Set(reminderLinks.map(\.calendarItemIdentifier)).count == reminderLinks.count,
      reminderLinks.allSatisfy({ link in
        guard let item = workItems.first(where: { $0.id == link.workItemId }),
          let dispatch = reminderDispatch.first(where: { $0.workItemId == link.workItemId })
        else { return false }
        return link.missionId == missionId
          && link.title == item.title
          && link.dispatchToken == dispatch.token
          && link.sourceIdentifier == reminderAuthorization.target.sourceIdentifier
          && link.calendarIdentifier == reminderAuthorization.target.calendarIdentifier
          && Self.validField(link.calendarItemIdentifier, maximumBytes: 512)
      })
    else {
      throw CoreClientError.contractViolation(
        "Core returned an incomplete or contradictory confirmed Mission."
      )
    }
    return self
  }

  private static func validField(_ value: String, maximumBytes: Int) -> Bool {
    !value.isEmpty
      && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
      && value.utf8.count <= maximumBytes
      && !value.unicodeScalars.contains { scalar in
        scalar.value == 0 || CharacterSet.controlCharacters.contains(scalar)
      }
  }

  public func recoveryOnly() -> Self {
    Self(
      missionId: missionId,
      title: title,
      workItems: workItems,
      reminderAuthorization: ReminderWriteAuthorization(
        missionId: reminderAuthorization.missionId,
        listId: reminderAuthorization.listId,
        payloadSha256: reminderAuthorization.payloadSha256,
        approvalId: reminderAuthorization.approvalId,
        approvalDigest: reminderAuthorization.approvalDigest,
        target: reminderAuthorization.target,
        writeDisposition: .recoverOnly
      ),
      reminderDispatch: reminderDispatch,
      reminderLinks: reminderLinks
    )
  }
}

public struct ConfirmedReminderDispatch: Codable, Equatable, Identifiable, Sendable {
  public var id: String { workItemId }
  public let workItemId: String
  public let token: String
}

public struct ReminderDispatchStart: Codable, Equatable, Sendable {
  public let mission: ConfirmedMission
  public let executeNow: Bool
}

public struct ReminderLink: Codable, Equatable, Identifiable, Sendable {
  public var id: String { workItemId }
  public let missionId: String
  public let workItemId: String
  public let sourceIdentifier: String
  public let calendarIdentifier: String
  public let calendarItemIdentifier: String
  public let dispatchToken: String
  public let title: String
}

public struct ReminderCompletionInput: Codable, Equatable, Sendable {
  public let workItemId: String
  public let sourceId: String
  public let completedAtMs: Int64
}

public struct MissionReceipt: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let missionId: String
  public let summary: String
  public let actualModel: String
  public let evidenceIds: [String]
  public let outputHashes: [String]
  public let completedAtMs: Int64

  public func validated() throws -> Self {
    guard !id.isEmpty, id == id.trimmingCharacters(in: .whitespacesAndNewlines),
      id.utf8.count <= 256,
      !missionId.isEmpty,
      missionId == missionId.trimmingCharacters(in: .whitespacesAndNewlines),
      missionId.utf8.count <= 256,
      !summary.isEmpty,
      summary == summary.trimmingCharacters(in: .whitespacesAndNewlines),
      summary.utf8.count <= 16 * 1024,
      actualModel == "gpt-5.6-sol",
      !evidenceIds.isEmpty,
      evidenceIds.count <= 128,
      Set(evidenceIds).count == evidenceIds.count,
      evidenceIds.allSatisfy(Self.validBoundedIdentity),
      outputHashes.count <= 128,
      Set(outputHashes).count == outputHashes.count,
      outputHashes.allSatisfy(Self.validOutputHash),
      completedAtMs >= 0
    else {
      throw CoreClientError.contractViolation(
        "Core returned a Receipt without complete bounded Evidence."
      )
    }
    return self
  }

  private static func validBoundedIdentity(_ value: String) -> Bool {
    !value.isEmpty
      && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
      && value.utf8.count <= 512
      && !value.unicodeScalars.contains { scalar in
        scalar.value == 0 || CharacterSet.controlCharacters.contains(scalar)
      }
  }

  private static func validOutputHash(_ value: String) -> Bool {
    value.utf8.count == 64
      && value.utf8.allSatisfy { byte in
        (48...57).contains(byte) || (97...102).contains(byte)
      }
  }
}

public struct ConfirmSuggestionParameters: Codable, Sendable {
  public let suggestionId: String
  public let reminderTarget: ReminderTarget
}

public struct CancelMissionParameters: Codable, Sendable {
  public let missionId: String
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(missionId: String, proof: BrokerRuntimeState) {
    self.missionId = missionId
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct MissionAuditAnchor: Codable, Equatable, Sendable {
  public let sequence: Int64
  public let entryHash: String
  public let signatureHex: String

  fileprivate var isValid: Bool {
    sequence > 0
      && Self.isLowercaseHex(entryHash, count: 64)
      && Self.isLowercaseHex(signatureHex, count: 128)
  }

  private static func isLowercaseHex(_ value: String, count: Int) -> Bool {
    value.utf8.count == count
      && value.utf8.allSatisfy { byte in
        (48...57).contains(byte) || (97...102).contains(byte)
      }
  }
}

public struct MissionCancellation: Codable, Equatable, Sendable {
  public let missionId: String
  public let status: String
  public let auditAnchor: MissionAuditAnchor

  public func validated(expectedMissionId: String) throws -> Self {
    guard missionId == expectedMissionId,
      !missionId.isEmpty,
      missionId == missionId.trimmingCharacters(in: .whitespacesAndNewlines),
      missionId.utf8.count <= 256,
      !missionId.unicodeScalars.contains(where: { scalar in
        scalar.value == 0 || CharacterSet.controlCharacters.contains(scalar)
      }),
      status == "cancelled",
      auditAnchor.isValid
    else {
      throw CoreClientError.contractViolation(
        "Core returned an invalid Mission cancellation receipt."
      )
    }
    return self
  }
}

public struct CompleteReminderMissionParameters: Codable, Sendable {
  public let missionId: String
  public let completions: [ReminderCompletionInput]
  public let receiptReturnApprovedAtMs: Int64?
  public let receiptReturnRouteId: String?
}

public struct MissionNeedsYou: Codable, Equatable, Sendable {
  public let missionId: String
  public let title: String
  public let prompt: String
  public let createdAtMs: Int64

  fileprivate var isValid: Bool {
    createdAtMs >= 0
      && Self.validField(missionId, maximumBytes: 256)
      && Self.validField(title, maximumBytes: 16 * 1024)
      && Self.validField(prompt, maximumBytes: 16 * 1024)
  }

  private static func validField(_ value: String, maximumBytes: Int) -> Bool {
    !value.isEmpty
      && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
      && value.utf8.count <= maximumBytes
      && !value.unicodeScalars.contains { scalar in
        scalar.value == 0 || CharacterSet.controlCharacters.contains(scalar)
      }
  }
}

public struct RecordReminderMirrorParameters: Codable, Sendable {
  public let missionId: String
  public let links: [ReminderLink]
}

public struct BeginReminderDispatchParameters: Codable, Sendable {
  public let missionId: String
}

public struct DashboardState: Codable, Equatable, Sendable {
  public let activeCards: [ActiveOutcomeCard]
  public let channelFailureIncidents: [ChannelFailureIncident]
  public let channelRouteSet: ChannelRouteSet?
  public let microphone: MicrophoneState
  public let runtime: RuntimeControl
  public let suggestion: OutcomeSuggestion?
  public let confirmedMission: ConfirmedMission?
  public let needsYou: MissionNeedsYou?
  public let receipt: MissionReceipt?

  public init(
    activeCards: [ActiveOutcomeCard],
    channelFailureIncidents: [ChannelFailureIncident] = [],
    channelRouteSet: ChannelRouteSet? = nil,
    microphone: MicrophoneState,
    runtime: RuntimeControl,
    suggestion: OutcomeSuggestion?,
    confirmedMission: ConfirmedMission? = nil,
    needsYou: MissionNeedsYou? = nil,
    receipt: MissionReceipt? = nil
  ) {
    self.activeCards = activeCards
    self.channelFailureIncidents = channelFailureIncidents
    self.channelRouteSet = channelRouteSet
    self.microphone = microphone
    self.runtime = runtime
    self.suggestion = suggestion
    self.confirmedMission = confirmedMission
    self.needsYou = needsYou
    self.receipt = receipt
  }

  public func validated() throws -> Self {
    guard activeCards.count <= 3,
      Set(activeCards.map(\.id)).count == activeCards.count,
      activeCards.allSatisfy({ card in
        Self.validCardField(card.id, maximumBytes: 256)
          && Self.validCardField(card.title, maximumBytes: 16 * 1024)
          && Self.validCardField(card.state, maximumBytes: 512)
      })
    else {
      throw CoreClientError.contractViolation("Dashboard returned invalid active cards.")
    }
    _ = try ChannelFailureIncident.validateCollection(channelFailureIncidents)
    if let suggestion { _ = try suggestion.validated() }
    if suggestion != nil, !activeCards.isEmpty, confirmedMission == nil {
      throw CoreClientError.contractViolation(
        "Dashboard returned an unrelated suggestion while Mission work is nonterminal."
      )
    }
    if let confirmedMission {
      _ = try confirmedMission.validated()
      guard
        activeCards.contains(where: {
          $0.id == confirmedMission.missionId && $0.title == confirmedMission.title
        })
      else {
        throw CoreClientError.contractViolation(
          "Dashboard omitted the active card for its confirmed Mission."
        )
      }
      if let suggestion {
        guard suggestion.title == confirmedMission.title,
          suggestion.proposedSteps == confirmedMission.workItems.map(\.title)
        else {
          throw CoreClientError.contractViolation(
            "Dashboard returned a suggestion that conflicts with its active Mission."
          )
        }
      }
    }
    if let needsYou {
      guard needsYou.isValid,
        activeCards.contains(where: {
          $0.id == needsYou.missionId && $0.title == needsYou.title
        })
      else {
        throw CoreClientError.contractViolation(
          "Dashboard omitted the active item behind Need you."
        )
      }
    }
    if let receipt {
      _ = try receipt.validated()
      if suggestion != nil || confirmedMission != nil || needsYou != nil || !activeCards.isEmpty {
        throw CoreClientError.contractViolation(
          "Core returned Done while nonterminal Mission work is still visible."
        )
      }
    }
    if let channelRouteSet {
      let focusMissionId =
        needsYou?.missionId ?? confirmedMission?.missionId
        ?? activeCards.first?.id ?? receipt?.missionId
      guard let focusMissionId else {
        throw CoreClientError.contractViolation(
          "Dashboard returned Mission routes without a visible Mission focus.")
      }
      _ = try channelRouteSet.validated(expectedMissionId: focusMissionId)
    }
    return self
  }

  private static func validCardField(_ value: String, maximumBytes: Int) -> Bool {
    !value.isEmpty
      && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
      && value.utf8.count <= maximumBytes
      && !value.unicodeScalars.contains { scalar in
        scalar.value == 0 || CharacterSet.controlCharacters.contains(scalar)
      }
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
