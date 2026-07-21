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

public enum C2SkillDemoStage: String, Codable, CaseIterable, Equatable, Sendable {
  case candidate
  case staged
  case runnable
  case used
}

public enum C2SkillDemoCommandKind: String, Codable, Equatable, Sendable {
  case registerCandidate
  case stageReviewed
  case enableRunnable
  case recordFirstNoEffectUse
}

public struct C2SkillDemoSeal: Codable, Equatable, Sendable {
  public static let instructionOnlyPermissionDigest =
    "3cb2dbae054a787c18b5ba9a60ab0e4541fbe6f9c4c165e9de77f84a7363c298"

  public let packageId: String
  public let sourceUrl: String
  public let commit: String
  public let packageDigest: String
  public let auditAnchor: String
  public let permissionDigest: String
  public let license: String

  public var isValid: Bool {
    Self.validIdentifier(packageId)
      && sourceUrl.hasPrefix("https://github.com/")
      && sourceUrl.utf8.count <= 512
      && !sourceUrl.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
      && Self.lowerHex(commit, count: 40)
      && Self.lowerHex(packageDigest, count: 64)
      && Self.lowerHex(auditAnchor, count: 64)
      && permissionDigest == Self.instructionOnlyPermissionDigest
      && (license == "MIT" || license == "Apache-2.0")
  }

  private static func validIdentifier(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 256
      && value.utf8.allSatisfy { byte in
        (48...57).contains(byte) || (65...90).contains(byte) || (97...122).contains(byte)
          || byte == 45 || byte == 46 || byte == 95
      }
  }

  public static func lowerHex(_ value: String, count: Int) -> Bool {
    value.utf8.count == count
      && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
  }
}

public struct C2SkillDemoCommand: Codable, Equatable, Sendable {
  public let requestId: String
  public let expectedRevision: UInt64
  public let kind: C2SkillDemoCommandKind
  public let seal: C2SkillDemoSeal
  public let actorId: String
  public let decisionId: String
  public let approvalNonce: String
  public let resultDigest: String?
  public let explicitlyConfirmed: Bool
  public let decidedAtMs: Int64

  public var isValid: Bool {
    seal.isValid && C2SkillDemoSeal.lowerHex(approvalNonce, count: 64)
      && explicitlyConfirmed && decidedAtMs >= 0
      && (kind == .recordFirstNoEffectUse
        ? resultDigest.map { C2SkillDemoSeal.lowerHex($0, count: 64) } == true
        : resultDigest == nil)
  }
}

public struct C2SkillDemoReceipt: Codable, Equatable, Identifiable, Sendable {
  public var id: String { requestId }
  public let requestId: String
  public let commandDigest: String
  public let revision: UInt64
  public let stage: C2SkillDemoStage
  public let receiptDigest: String
}

public struct C2SkillDemoState: Codable, Equatable, Sendable {
  public let revision: UInt64
  public let stage: C2SkillDemoStage
  public let seal: C2SkillDemoSeal
  public let consumedNonces: [String]
  public let receipts: [C2SkillDemoReceipt]
  public let firstUseResultDigest: String?
}

public struct C2SkillDemoView: Codable, Equatable, Sendable {
  public let state: C2SkillDemoState?
}

public struct ApplyC2SkillDemoParameters: Codable, Sendable {
  public let command: C2SkillDemoCommand
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(command: C2SkillDemoCommand, proof: BrokerRuntimeState) {
    self.command = command
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ApplyC2SkillDemoResponse: Codable, Equatable, Sendable {
  public let state: C2SkillDemoState
  public let receipt: C2SkillDemoReceipt
}

public enum B2MemoryDemoStage: String, Codable, Equatable, Sendable {
  case prepared, processing, candidates, selected, diffReview, confirmed, readBack
}

public struct B2MemoryPreparedSource: Codable, Equatable, Sendable {
  public let requestId: String
  public let sourceIdentityDigest: String
  public let byteLength: UInt64

  public var isValid: Bool {
    !requestId.isEmpty && requestId.utf8.count <= 256 && byteLength > 0
      && C2SkillDemoSeal.lowerHex(sourceIdentityDigest, count: 64)
  }
}

public struct B2MemoryPrepareSourceRequest: Codable, Equatable, Sendable {
  public let requestId: String
  public let selectedPath: String

  public var isValid: Bool {
    !requestId.isEmpty && requestId.utf8.count <= 256 && selectedPath.hasPrefix("/")
      && !selectedPath.isEmpty && selectedPath.utf8.count <= 4_096
      && !selectedPath.unicodeScalars.contains(where: { $0.value < 0x20 || $0.value == 0x7f })
  }
}

public struct B2MemoryProcessingConsent: Codable, Equatable, Sendable {
  public let requestId: String
  public let expectedRevision: UInt64
  public let sourceIdentityDigest: String
  public let explicitlyConfirmed: Bool

  public var isValid: Bool {
    !requestId.isEmpty && requestId.utf8.count <= 256 && expectedRevision > 0
      && C2SkillDemoSeal.lowerHex(sourceIdentityDigest, count: 64) && explicitlyConfirmed
  }
}

public struct B2MemoryProcessingOperation: Codable, Equatable, Sendable {
  public let operationId: String
  public let requestId: String
  public let expectedRevision: UInt64
  public let runtimeRevision: UInt64
  public let sourceIdentityDigest: String
  public let sourceDigest: String
  public let modelProvenance: ChoiceModelProvenance
  public let catalogDigest: String
  public let protocolVersion: UInt32
  public let personaRevision: PersonaRevisionRef
  public let documentManifestDigest: String
  public let sourceManifestDigest: String
  public let startedAtMs: Int64

  public var isValid: Bool {
    ChoiceLoopContract.identifier(operationId) && ChoiceLoopContract.identifier(requestId)
      && expectedRevision > 0 && runtimeRevision > 0
      && ChoiceLoopContract.sha256(sourceIdentityDigest)
      && ChoiceLoopContract.sha256(sourceDigest) && modelProvenance.validated()
      && ChoiceLoopContract.sha256(catalogDigest) && protocolVersion == 1
      && personaRevision.validated() && ChoiceLoopContract.sha256(documentManifestDigest)
      && ChoiceLoopContract.sha256(sourceManifestDigest) && startedAtMs >= 0
  }
}

public struct B2MemoryCandidateCard: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let title: String
  public let rationale: String
  public let proposedLine: String
  public let sourceBindingDigest: String

  public var isValid: Bool {
    !id.isEmpty && id.utf8.count <= 256 && !title.isEmpty && title.utf8.count <= 160
      && !rationale.isEmpty && rationale.utf8.count <= 512 && !proposedLine.isEmpty
      && proposedLine.utf8.count <= 4_096
      && C2SkillDemoSeal.lowerHex(sourceBindingDigest, count: 64)
  }
}

public struct B2MemoryImportSeal: Codable, Equatable, Sendable {
  public let sourceDigest: String
  public let catalogDigest: String
  public let sourceManifestDigest: String
  public let modelProvenance: ChoiceModelProvenance

  public var isValid: Bool {
    C2SkillDemoSeal.lowerHex(sourceDigest, count: 64)
      && C2SkillDemoSeal.lowerHex(catalogDigest, count: 64)
      && C2SkillDemoSeal.lowerHex(sourceManifestDigest, count: 64)
      && modelProvenance.validated()
  }
}

public struct B2MemoryMarkdownDiff: Codable, Equatable, Sendable {
  public let revision: UInt64
  public let selectedCandidateId: String
  public let proposedLine: String
  public let editedLine: String
  public let expectedBase: MarkdownBaseIdentity?
  public let finalEntry: DocumentManifestEntry
  public let diffDigest: String

  public var isValid: Bool {
    revision > 0 && !selectedCandidateId.isEmpty && !proposedLine.isEmpty && !editedLine.isEmpty
      && editedLine.utf8.count <= 4_096 && finalEntry.relativePath == "sources/chatgpt.md"
      && C2SkillDemoSeal.lowerHex(diffDigest, count: 64)
  }
}

public enum B2MemoryCommandKind: String, Codable, Equatable, Sendable {
  case prepare, selectCandidate, editMarkdown, confirmDiff
}

public struct B2MemoryCommand: Codable, Equatable, Sendable {
  public let requestId: String
  public let expectedRevision: UInt64
  public let kind: B2MemoryCommandKind
  public let selectedCandidateId: String?
  public let editedLine: String?
  public let expectedDiffDigest: String?
  public let explicitlyConfirmed: Bool
  public let decidedAtMs: Int64

  public var isValid: Bool {
    guard !requestId.isEmpty, requestId.utf8.count <= 256, decidedAtMs >= 0 else { return false }
    switch kind {
    case .prepare:
      return expectedRevision == 0 && selectedCandidateId == nil && editedLine == nil
        && expectedDiffDigest == nil && !explicitlyConfirmed
    case .selectCandidate:
      return expectedRevision > 0 && selectedCandidateId?.isEmpty == false && editedLine == nil
        && expectedDiffDigest == nil && explicitlyConfirmed
    case .editMarkdown:
      return expectedRevision > 0 && selectedCandidateId == nil
        && editedLine?.isEmpty == false && editedLine?.utf8.count ?? 4_097 <= 4_096
        && expectedDiffDigest.map { C2SkillDemoSeal.lowerHex($0, count: 64) } == true
        && !explicitlyConfirmed
    case .confirmDiff:
      return expectedRevision > 0 && selectedCandidateId == nil && editedLine == nil
        && expectedDiffDigest.map { C2SkillDemoSeal.lowerHex($0, count: 64) } == true
        && explicitlyConfirmed
    }
  }
}

public struct B2MemoryCommandReceipt: Codable, Equatable, Identifiable, Sendable {
  public var id: String { requestId }
  public let requestId: String
  public let commandDigest: String
  public let revision: UInt64
  public let stage: B2MemoryDemoStage
  public let receiptDigest: String
}

public struct B2MemoryReadbackReceipt: Codable, Equatable, Sendable {
  public let confirmationDigest: String
  public let renderReceiptDigest: String
  public let receiptDigest: String

  public var isValid: Bool {
    ChoiceLoopContract.sha256(confirmationDigest)
      && ChoiceLoopContract.sha256(renderReceiptDigest)
      && ChoiceLoopContract.sha256(receiptDigest)
  }
}

public struct B2MemoryRenderIntent: Codable, Equatable, Sendable {
  public let intentId: String
  public let expectedRevision: UInt64
  public let confirmationDigest: String
  public let entry: DocumentManifestEntry
  public let contentDigest: String
  public let createdAtMs: Int64

  public var isValid: Bool {
    ChoiceLoopContract.identifier(intentId) && expectedRevision > 0
      && ChoiceLoopContract.sha256(confirmationDigest)
      && entry.relativePath == "sources/chatgpt.md" && entry.validated()
      && contentDigest == entry.sha256 && createdAtMs >= 0
  }
}

public struct B2MemoryDemoState: Codable, Equatable, Sendable {
  public let revision: UInt64
  public let stage: B2MemoryDemoStage
  public let preparedSource: B2MemoryPreparedSource?
  public let processingOperation: B2MemoryProcessingOperation?
  public let processingResultDigest: String?
  public let seal: B2MemoryImportSeal?
  public let candidates: [B2MemoryCandidateCard]
  public let selectedCandidate: B2MemoryCandidateCard?
  public let markdownDiff: B2MemoryMarkdownDiff?
  public let confirmationDigest: String?
  public let renderIntent: B2MemoryRenderIntent?
  public let readbackReceipt: B2MemoryReadbackReceipt?
  public let receipts: [B2MemoryCommandReceipt]

  public var isValid: Bool {
    guard revision > 0, preparedSource?.isValid == true,
      UInt64(receipts.count) <= revision,
      Set(receipts.map(\.requestId)).count == receipts.count,
      receipts.allSatisfy({ receipt in
        !receipt.requestId.isEmpty && receipt.requestId.utf8.count <= 128
          && ChoiceLoopContract.sha256(receipt.commandDigest)
          && receipt.revision > 0 && receipt.revision <= revision
          && ChoiceLoopContract.sha256(receipt.receiptDigest)
      })
    else { return false }

    let hasProcessingOperation = processingOperation?.isValid == true
    let hasProcessingResult = processingResultDigest.map(ChoiceLoopContract.sha256) == true
    switch stage {
    case .prepared:
      guard processingOperation == nil, processingResultDigest == nil, seal == nil,
        candidates.isEmpty, selectedCandidate == nil, markdownDiff == nil, renderIntent == nil
      else { return false }
    case .processing:
      guard hasProcessingOperation, processingResultDigest == nil, seal?.isValid == true,
        candidates.isEmpty, selectedCandidate == nil, markdownDiff == nil, renderIntent == nil
      else { return false }
    case .candidates:
      guard hasProcessingOperation, hasProcessingResult, seal?.isValid == true,
        (1...3).contains(candidates.count), candidates.allSatisfy(\.isValid),
        selectedCandidate == nil, markdownDiff == nil, renderIntent == nil
      else { return false }
    case .selected, .diffReview, .confirmed, .readBack:
      guard hasProcessingOperation, hasProcessingResult, seal?.isValid == true,
        candidates.isEmpty, selectedCandidate?.isValid == true, markdownDiff?.isValid == true
      else { return false }
    }

    let isConfirmed = stage == .confirmed || stage == .readBack
    guard isConfirmed == (confirmationDigest.map(ChoiceLoopContract.sha256) == true),
      (stage == .readBack) == (readbackReceipt?.isValid == true),
      (stage == .confirmed) == (renderIntent?.isValid == true)
    else { return false }
    return true
  }
}

public struct PrepareB2MemorySourceParameters: Codable, Sendable {
  public let request: B2MemoryPrepareSourceRequest
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(request: B2MemoryPrepareSourceRequest, proof: BrokerRuntimeState) {
    self.request = request
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ProcessB2MemorySourceParameters: Codable, Sendable {
  public let consent: B2MemoryProcessingConsent
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(consent: B2MemoryProcessingConsent, proof: BrokerRuntimeState) {
    self.consent = consent
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct B2MemoryDemoView: Codable, Equatable, Sendable {
  public let state: B2MemoryDemoState?
}

public struct ApplyB2MemoryDemoParameters: Codable, Sendable {
  public let command: B2MemoryCommand
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(command: B2MemoryCommand, proof: BrokerRuntimeState) {
    self.command = command
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ApplyB2MemoryDemoResponse: Codable, Equatable, Sendable {
  public let state: B2MemoryDemoState
  public let receipt: B2MemoryCommandReceipt
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
  public let imessage: IMessagePairingMetadata?
  public let discord: DiscordPairingMetadata?
  public let pairedAtMs: Int64

  public init(
    channel: ChannelKind,
    ownerSenderId: String,
    conversationId: String,
    imessage: IMessagePairingMetadata? = nil,
    discord: DiscordPairingMetadata? = nil,
    pairedAtMs: Int64
  ) {
    self.channel = channel
    self.ownerSenderId = ownerSenderId
    self.conversationId = conversationId
    requireExplicitAddress = channel != .iMessage
    self.imessage = imessage
    self.discord = discord
    self.pairedAtMs = pairedAtMs
  }

  public func validated(expectedChannel: ChannelKind? = nil) throws -> Self {
    guard expectedChannel == nil || channel == expectedChannel,
      ChannelContractValidation.providerId(ownerSenderId),
      ChannelContractValidation.providerId(conversationId),
      pairedAtMs >= 0
    else {
      throw CoreClientError.contractViolation("Core returned an invalid durable channel pairing.")
    }
    switch (channel, imessage, discord) {
    case (.iMessage, nil, nil):
      // Historical rows remain readable so AppModel can present the typed
      // re-selection path. They are never eligible to start the PR2 listener.
      break
    case (.iMessage, .some(let imessage), nil):
      guard !requireExplicitAddress,
        ChannelContractValidation.providerId(imessage.chatGuid),
        ChannelContractValidation.providerId(imessage.chatIdentifier),
        imessage.service == "iMessage",
        imessage.participantIds == [ownerSenderId]
      else {
        throw CoreClientError.contractViolation(
          "Core returned invalid self-chat pairing metadata.")
      }
    case (.discord, nil, .some(let discord)):
      guard requireExplicitAddress else {
        throw CoreClientError.contractViolation(
          "Core returned invalid Discord pairing metadata.")
      }
      _ = try discord.validated()
    default:
      throw CoreClientError.contractViolation("Core returned mismatched channel pairing metadata.")
    }
    return self
  }
}

public struct IMessagePairingMetadata: Codable, Equatable, Sendable {
  public let chatGuid: String
  public let chatIdentifier: String
  public let service: String
  public let participantIds: [String]
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
  public let chatGuid: String
  public let chatIdentifier: String
  public let name: String
  public let service: String
  public let participants: [String]

  public init(
    chatId: String,
    chatGuid: String? = nil,
    chatIdentifier: String? = nil,
    name: String,
    service: String,
    participants: [String]
  ) {
    self.chatId = chatId
    self.chatGuid = chatGuid ?? "iMessage;+;test-\(chatId)"
    self.chatIdentifier = chatIdentifier ?? "test-\(chatId)"
    self.name = name
    self.service = service
    self.participants = participants
  }

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
        Self.validField(chat.chatGuid, allowEmpty: false),
        Self.validField(chat.chatIdentifier, allowEmpty: false),
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

  public func validates(
    missionId: String, workItems: [MissionWorkItem],
    choiceConfirmationId: String? = nil,
    choicePayloadDigest: String? = nil,
    choiceReminderPayloadDigest: String? = nil,
    choiceReminderItems: [ChoiceReminderItem]? = nil
  ) -> Bool {
    let expectedPayload: String
    switch (
      choiceConfirmationId, choicePayloadDigest, choiceReminderPayloadDigest, choiceReminderItems
    ) {
    case (nil, nil, nil, nil):
      expectedPayload = Self.payloadSha256(
        missionId: missionId, target: target, workItems: workItems)
    case (
      .some(let confirmationId), .some(let choicePayloadDigest),
      .some(let reminderPayloadDigest), .some(let reminderItems)
    ):
      var payload = Self.domain
      Self.appendLengthPrefixed(missionId, to: &payload)
      Self.appendLengthPrefixed(confirmationId, to: &payload)
      Self.appendLengthPrefixed(choicePayloadDigest, to: &payload)
      Self.appendLengthPrefixed(reminderPayloadDigest, to: &payload)
      Self.appendLengthPrefixed(listId, to: &payload)
      Self.appendLengthPrefixed(target.sourceIdentifier, to: &payload)
      Self.appendLengthPrefixed(target.calendarIdentifier, to: &payload)
      for item in reminderItems {
        Self.appendLengthPrefixed(item.id, to: &payload)
        Self.appendLengthPrefixed(item.text, to: &payload)
        var dueAtMs = UInt64(bitPattern: item.dueAtMs).bigEndian
        withUnsafeBytes(of: &dueAtMs) { payload.append(contentsOf: $0) }
        Self.appendLengthPrefixed(item.timeZone, to: &payload)
        Self.appendLengthPrefixed(item.evidenceIntent, to: &payload)
      }
      expectedPayload = SHA256.hash(data: payload).map { String(format: "%02x", $0) }.joined()
    default:
      return false
    }
    return self.missionId == missionId
      && listId == Self.logicalListId
      && Self.isLowercaseHex(payloadSha256, count: 64)
      && !approvalId.isEmpty
      && Self.isLowercaseHex(approvalDigest, count: 64)
      && target.isValid
      && payloadSha256 == expectedPayload
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
  public let choiceConfirmationId: String?
  public let choicePayloadDigest: String?
  public let choiceReminderPayloadDigest: String?
  public let choiceReminderItems: [ChoiceReminderItem]?

  public init(
    missionId: String, title: String, workItems: [MissionWorkItem],
    reminderAuthorization: ReminderWriteAuthorization,
    reminderDispatch: [ConfirmedReminderDispatch], reminderLinks: [ReminderLink],
    choiceConfirmationId: String? = nil, choicePayloadDigest: String? = nil,
    choiceReminderPayloadDigest: String? = nil,
    choiceReminderItems: [ChoiceReminderItem]? = nil
  ) {
    self.missionId = missionId
    self.title = title
    self.workItems = workItems
    self.reminderAuthorization = reminderAuthorization
    self.reminderDispatch = reminderDispatch
    self.reminderLinks = reminderLinks
    self.choiceConfirmationId = choiceConfirmationId
    self.choicePayloadDigest = choicePayloadDigest
    self.choiceReminderPayloadDigest = choiceReminderPayloadDigest
    self.choiceReminderItems = choiceReminderItems
  }

  public func validated() throws -> Self {
    guard Self.validField(missionId, maximumBytes: 256),
      Self.validField(title, maximumBytes: 16 * 1024),
      (1...3).contains(workItems.count),
      Set(workItems.map(\.id)).count == workItems.count,
      workItems.allSatisfy({
        Self.validField($0.id, maximumBytes: 256)
          && Self.validField($0.title, maximumBytes: 16 * 1024)
      }),
      reminderAuthorization.validates(
        missionId: missionId, workItems: workItems,
        choiceConfirmationId: choiceConfirmationId,
        choicePayloadDigest: choicePayloadDigest,
        choiceReminderPayloadDigest: choiceReminderPayloadDigest,
        choiceReminderItems: choiceReminderItems),
      Self.validChoiceBinding(
        confirmationId: choiceConfirmationId, payloadDigest: choicePayloadDigest,
        reminderPayloadDigest: choiceReminderPayloadDigest,
        reminderItems: choiceReminderItems, workItems: workItems),
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
      reminderLinks: reminderLinks, choiceConfirmationId: choiceConfirmationId,
      choicePayloadDigest: choicePayloadDigest,
      choiceReminderPayloadDigest: choiceReminderPayloadDigest,
      choiceReminderItems: choiceReminderItems
    )
  }

  private static func validChoiceBinding(
    confirmationId: String?, payloadDigest: String?, reminderPayloadDigest: String?,
    reminderItems: [ChoiceReminderItem]?,
    workItems: [MissionWorkItem]
  ) -> Bool {
    switch (confirmationId, payloadDigest, reminderPayloadDigest, reminderItems) {
    case (nil, nil, nil, nil): return true
    case (
      .some(let confirmationId), .some(let payloadDigest), .some(let reminderPayloadDigest),
      .some(let reminderItems)
    ):
      return validField(confirmationId, maximumBytes: 256)
        && payloadDigest.utf8.count == 64
        && payloadDigest.utf8.allSatisfy {
          (48...57).contains($0) || (97...102).contains($0)
        }
        && reminderPayloadDigest.utf8.count == 64
        && reminderPayloadDigest.utf8.allSatisfy {
          (48...57).contains($0) || (97...102).contains($0)
        }
        && reminderItems.count == workItems.count
        && zip(reminderItems, workItems).allSatisfy { item, work in
          item.validated() && item.id == work.id && item.text == work.title
        }
    default: return false
    }
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

public struct ChoiceReminderCompletion: Codable, Equatable, Sendable {
  public let receipt: MissionReceipt
  public let choiceLoop: ChoiceLoopSnapshot
}

public struct AuthorizeChoiceRemindersParameters: Codable, Sendable {
  public let confirmationId: String
  public let reminderTarget: ReminderTarget
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    confirmationId: String, reminderTarget: ReminderTarget, proof: BrokerRuntimeState
  ) {
    self.confirmationId = confirmationId
    self.reminderTarget = reminderTarget
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ChoiceReminderRequestParameters: Codable, Sendable {
  public let confirmationId: String
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(confirmationId: String, proof: BrokerRuntimeState) {
    self.confirmationId = confirmationId
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct RecordChoiceReminderMirrorParameters: Codable, Sendable {
  public let confirmationId: String
  public let links: [ReminderLink]
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(confirmationId: String, links: [ReminderLink], proof: BrokerRuntimeState) {
    self.confirmationId = confirmationId
    self.links = links
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct CompleteChoiceRemindersParameters: Codable, Sendable {
  public let confirmationId: String
  public let completions: [ReminderCompletionInput]
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    confirmationId: String, completions: [ReminderCompletionInput], proof: BrokerRuntimeState
  ) {
    self.confirmationId = confirmationId
    self.completions = completions
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
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
      Self.validModelIdentifier(actualModel),
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

  private static func validModelIdentifier(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128
      && value.utf8.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 90) || ($0 >= 97 && $0 <= 122)
          || $0 == 45 || $0 == 95 || $0 == 46
      }
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

public struct ModelSelection: Codable, Equatable, Sendable {
  public let id: String
  public let modelId: String
  public let requestedEffort: String
  public let actualEffort: String
  public let catalogFingerprint: String
  public let catalogRevision: UInt64
  public let accountDisplayClass: String
  public let protocolSchemaRevision: UInt64
}

/// A Host-verified status for a persisted selection against one account and
/// one complete compatible-model catalog snapshot. `unavailable` retains the
/// underlying selection for audit, but is never model-entry authority.
public enum ModelSelectionStatus: String, Codable, Equatable, Sendable {
  case current
  case unselected
  case unavailable
}

/// One RPC response binds the account, catalog, catalog identity, and durable
/// selection. The App must not compose readiness from independent reads.
public struct ModelSetup: Codable, Equatable, Sendable {
  public let account: AccountState
  public let models: [GptModel]
  public let selection: ModelSelection?
  public let selectionStatus: ModelSelectionStatus
  /// Opaque, short-lived identity for the Host-owned catalog snapshot. It is
  /// required to persist a choice; catalog data supplied by the UI has no
  /// authority on its own.
  public let catalogSnapshotId: String
  public let catalogFingerprint: String
  public let catalogRevision: UInt64
}

public struct SelectModelParameters: Codable, Sendable {
  public let modelId: String
  public let requestedEffort: String
  public let catalogSnapshotId: String
  public let catalogFingerprint: String
  public let catalogRevision: UInt64
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    modelId: String,
    requestedEffort: String,
    catalogSnapshotId: String,
    catalogFingerprint: String,
    catalogRevision: UInt64,
    proof: BrokerRuntimeState
  ) {
    self.modelId = modelId
    self.requestedEffort = requestedEffort
    self.catalogSnapshotId = catalogSnapshotId
    self.catalogFingerprint = catalogFingerprint
    self.catalogRevision = catalogRevision
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct SelectChoiceParameters: Codable, Sendable {
  public let selection: ChoiceSelection?
  public let dInput: ChoiceDInput?
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(selection: ChoiceSelection, proof: BrokerRuntimeState) {
    self.selection = selection
    dInput = nil
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }

  public init(dInput: ChoiceDInput, proof: BrokerRuntimeState) {
    selection = nil
    self.dInput = dInput
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct CancelChoiceParameters: Codable, Sendable {
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(proof: BrokerRuntimeState) {
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

/// The only public first-local-question request. Trusted envelope, delivery
/// binding, batch, session, and audit fields are deliberately absent: Host
/// derives and persists them before any model work can begin.
public struct ChoiceBeginParameters: Codable, Sendable {
  public let requestId: String
  public let boundedLocalQuestion: String
  public let expectedModelProvenanceRef: String
  public let expectedCatalogFingerprint: String
  public let expectedCatalogRevision: UInt64
  public let expectedProtocolRevision: UInt64
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    requestId: String,
    boundedLocalQuestion: String,
    selection: ModelSelection,
    proof: BrokerRuntimeState
  ) {
    self.requestId = requestId
    self.boundedLocalQuestion = boundedLocalQuestion
    expectedModelProvenanceRef = selection.id
    expectedCatalogFingerprint = selection.catalogFingerprint
    expectedCatalogRevision = selection.catalogRevision
    expectedProtocolRevision = selection.protocolSchemaRevision
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ChoiceBeginAccepted: Codable, Equatable, Sendable {
  public let requestId: String
  public let operationId: String
  public let choiceSessionId: String
  public let acceptedSessionRevision: UInt64
  public let sourceEnvelopeId: String
  public let conversationTurnBatchId: String
  public let state: String

  public func validated() throws -> Self {
    guard ChoiceLoopContract.identifier(requestId), ChoiceLoopContract.identifier(operationId),
      ChoiceLoopContract.identifier(choiceSessionId), acceptedSessionRevision > 0,
      ChoiceLoopContract.identifier(sourceEnvelopeId),
      ChoiceLoopContract.identifier(conversationTurnBatchId), state == "interpreting"
    else {
      throw CoreClientError.contractViolation("Core returned an invalid first Choice acceptance.")
    }
    return self
  }
}

public struct ConfirmChoiceParameters: Codable, Sendable {
  public let confirmation: ChoiceConsolidatedConfirmation
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(
    confirmation: ChoiceConsolidatedConfirmation, proof: BrokerRuntimeState
  ) {
    self.confirmation = confirmation
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct PrepareChoiceConfirmationParameters: Codable, Sendable {
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(proof: BrokerRuntimeState) {
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ChoiceIMessageReplyPreview: Codable, Equatable, Sendable {
  public let replyId: String
  public let previewRevision: UInt64
  public let destination: String
  public let visibleBody: String
  public let confirmationDigest: String

  public func validated() throws -> Self {
    guard ChoiceLoopContract.identifier(replyId), previewRevision > 0,
      destination == "Your selected iMessage self-chat",
      !visibleBody.isEmpty, visibleBody.utf8.count <= 8_000,
      !visibleBody.utf8.contains(0), ChoiceLoopContract.sha256(confirmationDigest)
    else {
      throw CoreClientError.contractViolation("Core returned an invalid iMessage reply preview.")
    }
    return self
  }
}

public struct ChoiceIMessageReplyPrepareResponse: Codable, Equatable, Sendable {
  public let preview: ChoiceIMessageReplyPreview
  public let status: String

  public func validated() throws -> Self {
    guard ["prepared", "authorized", "delivered"].contains(status) else {
      throw CoreClientError.contractViolation("Core returned an invalid iMessage reply state.")
    }
    _ = try preview.validated()
    return self
  }
}

public struct AuthorizeChoiceIMessageReplyParameters: Codable, Sendable {
  public let replyId: String
  public let previewRevision: UInt64
  public let confirmationDigest: String
  public let explicitlyApproved: Bool
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(preview: ChoiceIMessageReplyPreview, proof: BrokerRuntimeState) {
    replyId = preview.replyId
    previewRevision = preview.previewRevision
    confirmationDigest = preview.confirmationDigest
    explicitlyApproved = true
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

public struct ChoiceIMessageReplyResponse: Codable, Equatable, Sendable {
  public let status: String
  public let recoveryOnly: Bool?

  public func validated() throws -> Self {
    guard status == "sent" || status == "needYou",
      status == "needYou" ? recoveryOnly == true : recoveryOnly == nil
    else {
      throw CoreClientError.contractViolation("Core returned an invalid iMessage reply result.")
    }
    return self
  }
}

/// An effect-free, revisioned local schedule proposal.  The Host validates
/// the selected IANA zone and future instant again before persistence; this
/// contract only keeps the Mac from presenting an unbounded wire value.
public struct ChoiceReminderScheduleInput: Codable, Equatable, Sendable {
  public let requestId: String
  public let choiceSessionId: String
  public let expectedSessionRevision: UInt64
  public let reminderListId: String
  public let reminderCount: UInt32
  public let dueAtMs: Int64
  public let timeZone: String

  public func validated() -> Bool {
    ChoiceLoopContract.identifier(requestId)
      && ChoiceLoopContract.identifier(choiceSessionId)
      && expectedSessionRevision > 0
      && ChoiceLoopContract.identifier(reminderListId)
      && (1...16).contains(reminderCount)
      && dueAtMs >= 0
      && ChoiceLoopContract.text(timeZone, maximum: 128)
      && timeZone.unicodeScalars.allSatisfy { $0.value < 128 }
      && TimeZone(identifier: timeZone) != nil
  }
}

public struct ChoiceReminderSchedule: Codable, Equatable, Sendable, Identifiable {
  public let id: String
  public let input: ChoiceReminderScheduleInput
  public let revision: UInt64
  public let acceptedAtMs: Int64

  public func validated() -> Bool {
    ChoiceLoopContract.identifier(id) && input.validated() && revision > 0
      && acceptedAtMs >= 0 && input.dueAtMs > acceptedAtMs
  }
}

public struct RecordChoiceReminderScheduleParameters: Codable, Sendable {
  public let input: ChoiceReminderScheduleInput
  public let authorization: RuntimeControlAuthorization
  public let brokerReceipt: RuntimeControlReceipt

  public init(input: ChoiceReminderScheduleInput, proof: BrokerRuntimeState) {
    self.input = input
    authorization = proof.authorization
    brokerReceipt = proof.receipt
  }
}

/// Read-only continuity state returned by the typed Choice Loop RPC. This
/// state never grants a Mission, Reminder, channel, or model authority.
public struct ChoiceLoopSnapshot: Codable, Equatable, Sendable {
  public let session: ChoiceSession
  public let activeBatch: ConversationTurnBatch?
  public let interpretation: InterpretationFrame?
  public let activeChoiceSet: ChoiceSet?
  public let lastSelection: ChoiceSelection?
  public var pendingRefinementOperation: ChoiceRefinementOperation? = nil
  public let confirmation: ChoiceConsolidatedConfirmation?
  public let documentManifest: DocumentManifest

  public func validated() throws -> Self {
    guard session.validated(), documentManifest.validated() else {
      throw CoreClientError.contractViolation("Core returned invalid Choice Loop state.")
    }
    if (session.state == "completed" || session.state == "cancelled")
      && (activeBatch != nil || interpretation != nil || activeChoiceSet != nil
        || lastSelection != nil || pendingRefinementOperation != nil || confirmation != nil
        || session.pendingConfirmationId != nil)
    {
      throw CoreClientError.contractViolation("Core returned replayable terminal Choice state.")
    }
    if let activeBatch {
      guard activeBatch.validated(), activeBatch.choiceSessionId == session.id,
        activeBatch.revision == session.revision
      else {
        throw CoreClientError.contractViolation("Core returned an invalid Choice batch.")
      }
    }
    if let interpretation {
      guard interpretation.validated(), interpretation.choiceSessionId == session.id,
        interpretation.sourceManifestDigest == documentManifest.aggregateDigest,
        session.activeInterpretationRevision == interpretation.revision
      else {
        throw CoreClientError.contractViolation("Core returned an invalid interpretation.")
      }
    } else if session.activeInterpretationRevision != nil {
      throw CoreClientError.contractViolation("Core returned an incomplete interpretation state.")
    }
    if let activeChoiceSet {
      guard activeChoiceSet.validated(), activeChoiceSet.choiceSessionId == session.id,
        activeChoiceSet.sessionRevision == session.revision,
        activeChoiceSet.sourceManifestDigest == documentManifest.aggregateDigest,
        session.activeChoiceSetId == activeChoiceSet.id,
        session.activeInterpretationRevision == activeChoiceSet.interpretationRevision
      else {
        throw CoreClientError.contractViolation("Core returned an invalid ChoiceSet.")
      }
    } else if session.activeChoiceSetId != nil {
      throw CoreClientError.contractViolation("Core returned an incomplete ChoiceSet state.")
    }
    if let lastSelection {
      guard lastSelection.validated(), lastSelection.choiceSessionId == session.id,
        lastSelection.expectedSessionRevision < session.revision,
        lastSelection.selectedAtMs <= session.lastInputAtMs
      else {
        throw CoreClientError.contractViolation("Core returned an invalid Choice selection.")
      }
    }
    if let pendingRefinementOperation {
      guard pendingRefinementOperation.validated(),
        pendingRefinementOperation.choiceSessionId == session.id,
        pendingRefinementOperation.expectedSessionRevision == session.revision,
        pendingRefinementOperation.sourceManifestDigest == documentManifest.aggregateDigest,
        session.state == "refining",
        pendingRefinementOperation.isOwnerResume
          || lastSelection?.id == pendingRefinementOperation.selectionId
      else {
        throw CoreClientError.contractViolation("Core returned an unbound Choice refinement.")
      }
    } else if session.state == "refining" {
      throw CoreClientError.contractViolation("Core omitted the pending Choice refinement.")
    }
    if let confirmation {
      guard confirmation.validated(), confirmation.choiceSessionId == session.id,
        session.pendingConfirmationId == confirmation.id,
        ["awaitingConfirmation", "executing", "softIdle"].contains(session.state),
        confirmation.expectedSessionRevision
          + (session.state == "awaitingConfirmation" ? 1 : 2) == session.revision,
        confirmation.deliveryBindingId == session.primaryDeliveryBindingId
      else {
        throw CoreClientError.contractViolation("Core returned an invalid Choice confirmation.")
      }
    } else if session.pendingConfirmationId != nil {
      throw CoreClientError.contractViolation("Core returned an incomplete Choice confirmation.")
    }
    return self
  }
}

public struct ChoiceSession: Codable, Equatable, Sendable {
  public let id: String
  public let state: String
  public let revision: UInt64
  public let modelSelectionState: ChoiceModelSelectionState
  public let communicationProfileRevision: UInt64
  public let activeChoiceSetId: String?
  public let activeInterpretationRevision: UInt64?
  public let openedAtMs: Int64
  public let lastInputAtMs: Int64
  public let softIdleAtMs: Int64
  public let staleReviewAtMs: Int64
  public let primaryDeliveryBindingId: String?
  public let pendingConfirmationId: String?
  public let backgroundMissionIds: [String]

  func validated() -> Bool {
    let validStates: Set<String> = [
      "interpreting", "active", "refining", "softIdle", "staleReview", "awaitingConfirmation",
      "executing",
      "completed", "cancelled", "blocked",
    ]
    return ChoiceLoopContract.identifier(id)
      && validStates.contains(state)
      && revision > 0
      && modelSelectionState.validated()
      && openedAtMs >= 0
      && lastInputAtMs >= openedAtMs
      && softIdleAtMs == lastInputAtMs + 1_800_000
      && staleReviewAtMs == lastInputAtMs + 86_400_000
      && (activeChoiceSetId.map(ChoiceLoopContract.identifier) ?? true)
      && (activeInterpretationRevision.map { $0 > 0 } ?? true)
      && (primaryDeliveryBindingId.map(ChoiceLoopContract.identifier) ?? true)
      && (pendingConfirmationId.map(ChoiceLoopContract.identifier) ?? true)
      && backgroundMissionIds.count <= 32
      && Set(backgroundMissionIds).count == backgroundMissionIds.count
      && backgroundMissionIds.allSatisfy(ChoiceLoopContract.identifier)
  }
}

public struct ChoiceModelSelectionState: Codable, Equatable, Sendable {
  public let state: String
  public let modelProvenanceRef: String?
  public let catalogRevision: UInt64?
  public let reason: String?

  func validated() -> Bool {
    switch state {
    case "unselected":
      return modelProvenanceRef == nil && catalogRevision == nil && reason == nil
    case "selected":
      return modelProvenanceRef.map(ChoiceLoopContract.identifier) == true
        && catalogRevision == nil && reason == nil
    case "unavailable":
      return modelProvenanceRef == nil && (catalogRevision ?? 0) > 0
        && reason.map { ChoiceLoopContract.text($0, maximum: 512) } == true
    default:
      return false
    }
  }
}

public struct ConversationTurnBatch: Codable, Equatable, Sendable {
  public let id: String
  public let choiceSessionId: String
  public let deliveryBindingId: String
  public let sourceEnvelopeIds: [String]
  public let openedAtMs: Int64
  public let quietDeadlineMs: Int64
  public let hardDeadlineMs: Int64
  public let sealedAtMs: Int64?
  public let sealReason: String?
  public let revision: UInt64

  func validated() -> Bool {
    let validReasons: Set<String> = [
      "initialIntake", "quietDeadline", "hardDeadline", "attachmentContinuation", "immediateOff",
      "immediateCancel",
      "immediateConfirm",
      "immediateRefinement",
    ]
    let sealsTogether = (sealedAtMs == nil) == (sealReason == nil)
    return ChoiceLoopContract.identifier(id)
      && ChoiceLoopContract.identifier(choiceSessionId)
      && ChoiceLoopContract.identifier(deliveryBindingId)
      && !sourceEnvelopeIds.isEmpty && sourceEnvelopeIds.count <= 64
      && Set(sourceEnvelopeIds).count == sourceEnvelopeIds.count
      && sourceEnvelopeIds.allSatisfy(ChoiceLoopContract.identifier)
      && openedAtMs >= 0
      && quietDeadlineMs == openedAtMs + 2_500
      && hardDeadlineMs == openedAtMs + 8_000
      && quietDeadlineMs <= hardDeadlineMs
      && (sealedAtMs.map { $0 >= openedAtMs && $0 <= hardDeadlineMs } ?? true)
      && sealsTogether && (sealReason.map(validReasons.contains) ?? true)
      && revision > 0
  }
}

/// A strict view of the tagged Rust Selection enum. It is data-only intent
/// refinement and never effect authority.
public struct ChoiceSelection: Codable, Equatable, Sendable {
  public let type: String
  public let id: String
  public let choiceSessionId: String
  public let choiceSetId: String
  public let selectedOptionId: String?
  public let dInputBatchId: String?
  public let expectedSessionRevision: UInt64
  public let selectedAtMs: Int64

  fileprivate func validated() -> Bool {
    let common =
      ChoiceLoopContract.identifier(id)
      && ChoiceLoopContract.identifier(choiceSessionId)
      && ChoiceLoopContract.identifier(choiceSetId)
      && expectedSessionRevision > 0 && selectedAtMs >= 0
    switch type {
    case "optionSelection":
      return common && selectedOptionId.map(ChoiceLoopContract.identifier) == true
        && dInputBatchId == nil
    case "naturalConversationSelection":
      return common && dInputBatchId.map(ChoiceLoopContract.identifier) == true
        && selectedOptionId == nil
    default:
      return false
    }
  }
}

/// The only untrusted input shape for the product-owned D direction. The Mac
/// supplies bounded text and an idempotent request identity only; the Host
/// derives every envelope, batch, selection, binding, and acceptance time.
public struct ChoiceDInput: Codable, Equatable, Sendable {
  public let requestId: String
  public let boundedText: String
  public let choiceSessionId: String
  public let choiceSetId: String
  public let expectedSessionRevision: UInt64
  public let submittedAtMs: Int64

  fileprivate func validated() -> Bool {
    ChoiceLoopContract.identifier(requestId)
      && ChoiceLoopContract.text(boundedText, maximum: 4 * 1_024)
      && ChoiceLoopContract.identifier(choiceSessionId)
      && ChoiceLoopContract.identifier(choiceSetId)
      && expectedSessionRevision > 0 && submittedAtMs >= 0
  }
}

/// Metadata for a Host-owned private refinement operation. D text remains
/// Store-private and is represented only by its digest here.
public struct ChoiceRefinementOperation: Codable, Equatable, Sendable {
  public let id: String
  public let selectionId: String
  public let choiceSessionId: String
  public let sourceEnvelopeId: String
  public let conversationTurnBatchId: String
  public let expectedSessionRevision: UInt64
  public let expectedGeneration: UInt64
  public let modelProvenance: ChoiceModelProvenance
  public let sourceManifestDigest: String
  public let personaRevision: PersonaRevisionRef
  public let dRequestId: String?
  public let dInputDigest: String?
  public let createdAtMs: Int64

  fileprivate func validated() -> Bool {
    let dShapeIsComplete =
      (dRequestId == nil && dInputDigest == nil)
      || (dRequestId != nil && dInputDigest != nil)
    return ChoiceLoopContract.identifier(id)
      && ChoiceLoopContract.identifier(selectionId)
      && ChoiceLoopContract.identifier(choiceSessionId) && expectedSessionRevision > 0
      && ChoiceLoopContract.identifier(sourceEnvelopeId)
      && ChoiceLoopContract.identifier(conversationTurnBatchId)
      && expectedGeneration > 0 && modelProvenance.validated()
      && ChoiceLoopContract.sha256(sourceManifestDigest)
      && personaRevision.validated()
      && (dRequestId.map(ChoiceLoopContract.identifier) ?? true)
      && (dInputDigest.map(ChoiceLoopContract.sha256) ?? true)
      && dShapeIsComplete && createdAtMs >= 0
  }

  /// Store-minted resume metadata is intentionally not an RPC field. The Mac
  /// uses it only to ask the Host to recover an exact persisted operation
  /// after a Host restart; it never creates, edits, or otherwise trusts it as
  /// user authority.
  var isOwnerResume: Bool {
    id.hasPrefix("resume-")
      && (selectionId.hasPrefix("resume-soft-idle-")
        || selectionId.hasPrefix("resume-stale-review-"))
  }
}

public struct ChoiceReminderItem: Codable, Equatable, Sendable, Identifiable {
  public let id: String
  public let text: String
  public let dueAtMs: Int64
  public let timeZone: String
  public let evidenceIntent: String

  fileprivate func validated() -> Bool {
    ChoiceLoopContract.identifier(id) && ChoiceLoopContract.text(text, maximum: 1_024)
      && dueAtMs >= 0 && ChoiceLoopContract.text(timeZone, maximum: 128)
      && timeZone.unicodeScalars.allSatisfy { $0.value < 128 }
      && ChoiceLoopContract.text(evidenceIntent, maximum: 1_024)
  }
}

public struct ChoiceConsolidatedConfirmation: Codable, Equatable, Sendable {
  public let id: String
  public let choiceSessionId: String
  public let choiceSetId: String
  public let selectionId: String
  public let expectedSessionRevision: UInt64
  public let interpretationRevision: UInt64
  public let payloadRevision: UInt64
  public let payloadDigest: String
  public let goal: String
  public let steps: [String]
  public let markdownEntry: DocumentManifestEntry
  public let markdownExpectedBase: MarkdownBaseIdentity?
  public let markdownManifestDigests: [String]
  public let documentDiffDigest: String
  public let modelProvenance: ChoiceModelProvenance
  public let personaRevision: PersonaRevisionRef
  public let reminderListId: String
  public let reminderItems: [ChoiceReminderItem]
  public let reminderCount: UInt32
  public let reminderPayloadDigest: String
  public let evidenceRequirements: [String]
  public let deliveryBindingId: String?
  public let recipient: String?
  public let deliveryScope: String?
  public let dataCategories: [String]
  public let retention: String
  public let permissions: [String]
  public let effectClasses: [String]
  public let confirmedAtMs: Int64

  /// Matches the Rust protocol's domain-separated typed byte preimage. This
  /// deliberately avoids JSON key, null, number, Unicode, and escaping
  /// differences at the Host/Mac security boundary.
  func canonicalPayloadDigest() -> String? {
    guard let bytes = canonicalPayloadPreimage() else { return nil }
    return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
  }

  func canonicalPayloadPreimage() -> Data? {
    var bytes = Data("openopen:choice-consolidated-confirmation:v1\0".utf8)

    func appendLength(_ count: Int, to bytes: inout Data) -> Bool {
      guard var value = UInt64(exactly: count)?.bigEndian else { return false }
      withUnsafeBytes(of: &value) { bytes.append(contentsOf: $0) }
      return true
    }
    func appendField(_ name: String, to bytes: inout Data) -> Bool {
      guard appendLength(name.utf8.count, to: &bytes) else { return false }
      bytes.append(contentsOf: name.utf8)
      return true
    }
    func appendObject(_ count: UInt32, to bytes: inout Data) {
      bytes.append(0x05)
      var value = count.bigEndian
      withUnsafeBytes(of: &value) { bytes.append(contentsOf: $0) }
    }
    func appendArray(_ count: Int, to bytes: inout Data) -> Bool {
      guard var value = UInt32(exactly: count)?.bigEndian else { return false }
      bytes.append(0x06)
      withUnsafeBytes(of: &value) { bytes.append(contentsOf: $0) }
      return true
    }
    func appendString(_ value: String, to bytes: inout Data) -> Bool {
      bytes.append(0x01)
      guard appendLength(value.utf8.count, to: &bytes) else { return false }
      bytes.append(contentsOf: value.utf8)
      return true
    }
    func appendOptionalString(_ value: String?, to bytes: inout Data) -> Bool {
      guard let value else {
        bytes.append(0x00)
        return true
      }
      return appendString(value, to: &bytes)
    }
    func appendUInt64(_ value: UInt64, to bytes: inout Data) {
      bytes.append(0x02)
      var value = value.bigEndian
      withUnsafeBytes(of: &value) { bytes.append(contentsOf: $0) }
    }
    func appendInt64(_ value: Int64, to bytes: inout Data) {
      bytes.append(0x03)
      var value = value.bigEndian
      withUnsafeBytes(of: &value) { bytes.append(contentsOf: $0) }
    }
    func appendUInt32(_ value: UInt32, to bytes: inout Data) {
      bytes.append(0x04)
      var value = value.bigEndian
      withUnsafeBytes(of: &value) { bytes.append(contentsOf: $0) }
    }
    func appendStrings(_ values: [String], to bytes: inout Data) -> Bool {
      guard appendArray(values.count, to: &bytes) else { return false }
      for value in values where !appendString(value, to: &bytes) { return false }
      return true
    }
    func appendEntry(_ entry: DocumentManifestEntry, to bytes: inout Data) -> Bool {
      appendObject(4, to: &bytes)
      guard appendField("relativePath", to: &bytes),
        appendString(entry.relativePath, to: &bytes), appendField("sha256", to: &bytes),
        appendString(entry.sha256, to: &bytes), appendField("byteLength", to: &bytes)
      else { return false }
      appendUInt64(entry.byteLength, to: &bytes)
      guard appendField("mode", to: &bytes) else { return false }
      appendUInt32(entry.mode, to: &bytes)
      return true
    }
    func appendBase(_ base: MarkdownBaseIdentity?, to bytes: inout Data) -> Bool {
      guard let base else {
        bytes.append(0x00)
        return true
      }
      appendObject(3, to: &bytes)
      guard appendField("entry", to: &bytes), appendEntry(base.entry, to: &bytes),
        appendField("device", to: &bytes)
      else { return false }
      appendUInt64(base.device, to: &bytes)
      guard appendField("inode", to: &bytes) else { return false }
      appendUInt64(base.inode, to: &bytes)
      return true
    }
    func appendModel(_ model: ChoiceModelProvenance, to bytes: inout Data) -> Bool {
      appendObject(9, to: &bytes)
      guard appendField("id", to: &bytes), appendString(model.id, to: &bytes),
        appendField("modelId", to: &bytes), appendString(model.modelId, to: &bytes),
        appendField("requestedEffort", to: &bytes),
        appendString(model.requestedEffort, to: &bytes),
        appendField("actualEffort", to: &bytes), appendString(model.actualEffort, to: &bytes),
        appendField("catalogFingerprint", to: &bytes),
        appendString(model.catalogFingerprint, to: &bytes),
        appendField("catalogRevision", to: &bytes)
      else { return false }
      appendUInt64(model.catalogRevision, to: &bytes)
      guard appendField("accountDisplayClass", to: &bytes),
        appendString(model.accountDisplayClass, to: &bytes),
        appendField("protocolSchemaRevision", to: &bytes)
      else { return false }
      appendUInt64(model.protocolSchemaRevision, to: &bytes)
      return appendField("turnId", to: &bytes) && appendString(model.turnId, to: &bytes)
    }
    func appendPersona(_ persona: PersonaRevisionRef, to bytes: inout Data) -> Bool {
      appendObject(4, to: &bytes)
      return appendField("personaId", to: &bytes) && appendString(persona.personaId, to: &bytes)
        && appendField("revision", to: &bytes) && appendString(persona.revision, to: &bytes)
        && appendField("aggregateDigest", to: &bytes)
        && appendString(persona.aggregateDigest, to: &bytes)
        && appendField("instructionsDigest", to: &bytes)
        && appendString(persona.instructionsDigest, to: &bytes)
    }
    func appendItems(_ items: [ChoiceReminderItem], to bytes: inout Data) -> Bool {
      guard appendArray(items.count, to: &bytes) else { return false }
      for item in items {
        appendObject(5, to: &bytes)
        guard appendField("id", to: &bytes), appendString(item.id, to: &bytes),
          appendField("text", to: &bytes), appendString(item.text, to: &bytes),
          appendField("dueAtMs", to: &bytes)
        else { return false }
        appendInt64(item.dueAtMs, to: &bytes)
        guard appendField("timeZone", to: &bytes), appendString(item.timeZone, to: &bytes),
          appendField("evidenceIntent", to: &bytes),
          appendString(item.evidenceIntent, to: &bytes)
        else { return false }
      }
      return true
    }

    appendObject(29, to: &bytes)
    guard appendField("id", to: &bytes), appendString(id, to: &bytes),
      appendField("choiceSessionId", to: &bytes), appendString(choiceSessionId, to: &bytes),
      appendField("choiceSetId", to: &bytes), appendString(choiceSetId, to: &bytes),
      appendField("selectionId", to: &bytes), appendString(selectionId, to: &bytes),
      appendField("expectedSessionRevision", to: &bytes)
    else { return nil }
    appendUInt64(expectedSessionRevision, to: &bytes)
    guard appendField("interpretationRevision", to: &bytes) else { return nil }
    appendUInt64(interpretationRevision, to: &bytes)
    guard appendField("payloadRevision", to: &bytes) else { return nil }
    appendUInt64(payloadRevision, to: &bytes)
    guard appendField("payloadDigest", to: &bytes), appendString("", to: &bytes),
      appendField("goal", to: &bytes), appendString(goal, to: &bytes),
      appendField("steps", to: &bytes), appendStrings(steps, to: &bytes),
      appendField("markdownEntry", to: &bytes), appendEntry(markdownEntry, to: &bytes),
      appendField("markdownExpectedBase", to: &bytes),
      appendBase(markdownExpectedBase, to: &bytes),
      appendField("markdownManifestDigests", to: &bytes),
      appendStrings(markdownManifestDigests, to: &bytes),
      appendField("documentDiffDigest", to: &bytes),
      appendString(documentDiffDigest, to: &bytes),
      appendField("modelProvenance", to: &bytes), appendModel(modelProvenance, to: &bytes),
      appendField("personaRevision", to: &bytes), appendPersona(personaRevision, to: &bytes),
      appendField("reminderListId", to: &bytes), appendString(reminderListId, to: &bytes),
      appendField("reminderItems", to: &bytes), appendItems(reminderItems, to: &bytes),
      appendField("reminderCount", to: &bytes)
    else { return nil }
    appendUInt32(reminderCount, to: &bytes)
    guard appendField("reminderPayloadDigest", to: &bytes),
      appendString(reminderPayloadDigest, to: &bytes),
      appendField("evidenceRequirements", to: &bytes),
      appendStrings(evidenceRequirements, to: &bytes),
      appendField("deliveryBindingId", to: &bytes),
      appendOptionalString(deliveryBindingId, to: &bytes),
      appendField("recipient", to: &bytes), appendOptionalString(recipient, to: &bytes),
      appendField("deliveryScope", to: &bytes),
      appendOptionalString(deliveryScope, to: &bytes),
      appendField("dataCategories", to: &bytes), appendStrings(dataCategories, to: &bytes),
      appendField("retention", to: &bytes), appendString(retention, to: &bytes),
      appendField("permissions", to: &bytes), appendStrings(permissions, to: &bytes),
      appendField("effectClasses", to: &bytes), appendStrings(effectClasses, to: &bytes),
      appendField("confirmedAtMs", to: &bytes)
    else { return nil }
    appendInt64(confirmedAtMs, to: &bytes)
    return bytes
  }

  func canonicalReminderPayloadDigest() -> String? {
    guard reminderCount == UInt32(reminderItems.count) else { return nil }
    var bytes = Data()
    func appendText(_ value: String) -> Bool {
      guard let count = UInt64(exactly: value.utf8.count) else { return false }
      var bigEndian = count.bigEndian
      withUnsafeBytes(of: &bigEndian) { bytes.append(contentsOf: $0) }
      bytes.append(contentsOf: value.utf8)
      return true
    }
    guard appendText(reminderListId) else { return nil }
    var count = reminderCount.bigEndian
    withUnsafeBytes(of: &count) { bytes.append(contentsOf: $0) }
    for item in reminderItems {
      guard appendText(item.id), appendText(item.text) else { return nil }
      var dueAtMs = item.dueAtMs.bigEndian
      withUnsafeBytes(of: &dueAtMs) { bytes.append(contentsOf: $0) }
      guard appendText(item.timeZone), appendText(item.evidenceIntent) else { return nil }
    }
    return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
  }

  func validated() -> Bool {
    let deliveryShapeIsComplete: Bool
    switch (deliveryBindingId, recipient, deliveryScope) {
    case (.none, .none, .none), (.some, .some, .some):
      deliveryShapeIsComplete = true
    default:
      deliveryShapeIsComplete = false
    }
    return ChoiceLoopContract.identifier(id) && ChoiceLoopContract.identifier(choiceSessionId)
      && ChoiceLoopContract.identifier(choiceSetId) && ChoiceLoopContract.identifier(selectionId)
      && expectedSessionRevision > 0
      && interpretationRevision > 0 && payloadRevision > 0
      && ChoiceLoopContract.sha256(payloadDigest)
      && canonicalPayloadDigest() == payloadDigest
      && ChoiceLoopContract.text(goal, maximum: 4 * 1_024) && !steps.isEmpty
      && ChoiceLoopContract.list(steps, maximumItems: 64, maximumText: 1_024)
      && markdownEntry.validated()
      && (markdownExpectedBase.map {
        $0.validated() && $0.entry.relativePath == markdownEntry.relativePath
      } ?? true)
      && markdownManifestDigests.count == 2
      && markdownManifestDigests.allSatisfy(ChoiceLoopContract.sha256)
      && DocumentManifest.canonicalAggregateDigest(entries: [markdownEntry])
        == markdownManifestDigests[1]
      && ChoiceLoopContract.sha256(documentDiffDigest) && modelProvenance.validated()
      && personaRevision.validated()
      && ChoiceLoopContract.identifier(reminderListId) && !reminderItems.isEmpty
      && reminderItems.count <= 64 && reminderCount == UInt32(reminderItems.count)
      && reminderItems.allSatisfy { $0.validated() }
      && ChoiceLoopContract.sha256(reminderPayloadDigest)
      && canonicalReminderPayloadDigest() == reminderPayloadDigest
      && ChoiceLoopContract.list(evidenceRequirements, maximumItems: 32, maximumText: 1_024)
      && (deliveryBindingId.map(ChoiceLoopContract.identifier) ?? true)
      && (recipient.map(ChoiceLoopContract.identifier) ?? true)
      && (deliveryScope.map { ChoiceLoopContract.text($0, maximum: 512) } ?? true)
      && deliveryShapeIsComplete
      && ChoiceLoopContract.list(dataCategories, maximumItems: 32, maximumText: 256)
      && ChoiceLoopContract.text(retention, maximum: 512)
      && ChoiceLoopContract.list(permissions, maximumItems: 32, maximumText: 256)
      && ChoiceLoopContract.list(effectClasses, maximumItems: 32, maximumText: 256)
      && confirmedAtMs >= 0
  }
}

public struct InterpretationFrame: Codable, Equatable, Sendable {
  public let choiceSessionId: String
  public let revision: UInt64
  public let understoodGoal: String
  public let currentContext: String
  public let assumptions: [String]
  public let constraints: [String]
  public let uncertainties: [String]
  public let whatToAvoid: [String]
  public let sourceManifestDigest: String

  fileprivate func validated() -> Bool {
    ChoiceLoopContract.identifier(choiceSessionId) && revision > 0
      && ChoiceLoopContract.text(understoodGoal, maximum: 4 * 1_024)
      && ChoiceLoopContract.text(currentContext, maximum: 8 * 1_024)
      && ChoiceLoopContract.list(assumptions, maximumItems: 64, maximumText: 1_024)
      && ChoiceLoopContract.list(constraints, maximumItems: 64, maximumText: 1_024)
      && ChoiceLoopContract.list(uncertainties, maximumItems: 64, maximumText: 1_024)
      && ChoiceLoopContract.list(whatToAvoid, maximumItems: 64, maximumText: 1_024)
      && ChoiceLoopContract.sha256(sourceManifestDigest)
  }
}

public struct ChoiceSet: Codable, Equatable, Sendable {
  public let id: String
  public let choiceSessionId: String
  public let sessionRevision: UInt64
  public let interpretationRevision: UInt64
  public let generatedAtMs: Int64
  public let expiresOnRevision: UInt64
  public let options: [ChoiceOption]
  public let dAvailable: Bool
  public let sourceManifestDigest: String
  public let modelProvenance: ChoiceModelProvenance
  public let personaRevision: PersonaRevisionRef

  fileprivate func validated() -> Bool {
    let positions = Set(options.map(\.position))
    let directions = Set(options.map(\.direction))
    return ChoiceLoopContract.identifier(id)
      && ChoiceLoopContract.identifier(choiceSessionId)
      && sessionRevision > 0 && interpretationRevision > 0 && generatedAtMs >= 0
      && expiresOnRevision >= sessionRevision && dAvailable
      && options.count == 3 && positions == Set<UInt8>([1, 2, 3]) && directions.count == 3
      && options.allSatisfy { $0.validated() }
      && ChoiceLoopContract.sha256(sourceManifestDigest)
      && modelProvenance.validated()
      && personaRevision.validated()
  }
}

/// Immutable provenance for the verified local Persona bundle used to
/// generate a Choice. It is metadata only and grants no model/effect
/// authority to the Mac.
public struct PersonaRevisionRef: Codable, Equatable, Sendable {
  public let personaId: String
  public let revision: String
  public let aggregateDigest: String
  public let instructionsDigest: String

  fileprivate func validated() -> Bool {
    ChoiceLoopContract.identifier(personaId) && ChoiceLoopContract.identifier(revision)
      && ChoiceLoopContract.sha256(aggregateDigest)
      && ChoiceLoopContract.sha256(instructionsDigest)
  }
}

/// Read-only projection of the embedded Persona revision currently used by
/// Core. It carries provenance only; the Mac cannot stage, activate, or roll
/// back Persona content through this contract.
public struct PersonaStatus: Codable, Equatable, Sendable {
  public let active: PersonaRevisionRef
  public let staged: PersonaRevisionRef?
  public let warning: String?
  public let changeNotePending: Bool

  func validated() -> Bool {
    active.validated()
      && (staged == nil || staged?.validated() == true)
      && (warning == nil || ChoiceLoopContract.text(warning ?? "", maximum: 1_024))
  }
}

public struct PersonaStatusView: Codable, Equatable, Sendable {
  public let status: PersonaStatus
  public let changeNote: String?

  func validated() -> Bool {
    status.validated()
      && (changeNote == nil || ChoiceLoopContract.text(changeNote ?? "", maximum: 1_024))
  }
}

public struct ChoiceOption: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let position: UInt8
  public let direction: String
  public let rationale: String
  public let expectedResult: String
  public let informationNeeded: [String]
  public let externalEffectsPreview: [String]
  public let sourceCategories: [String]

  fileprivate func validated() -> Bool {
    (1...3).contains(position) && ChoiceLoopContract.identifier(id)
      && ChoiceLoopContract.text(direction, maximum: 512)
      && ChoiceLoopContract.text(rationale, maximum: 1_024)
      && ChoiceLoopContract.text(expectedResult, maximum: 1_024)
      && ChoiceLoopContract.list(informationNeeded, maximumItems: 16, maximumText: 512)
      && ChoiceLoopContract.list(externalEffectsPreview, maximumItems: 16, maximumText: 512)
      && ChoiceLoopContract.list(sourceCategories, maximumItems: 16, maximumText: 128)
  }
}

public struct ChoiceModelProvenance: Codable, Equatable, Sendable {
  public let id: String
  public let modelId: String
  public let requestedEffort: String
  public let actualEffort: String
  public let catalogFingerprint: String
  public let catalogRevision: UInt64
  public let accountDisplayClass: String
  public let protocolSchemaRevision: UInt64
  public let turnId: String

  fileprivate func validated() -> Bool {
    ChoiceLoopContract.text(id, maximum: 128) && ChoiceLoopContract.modelId(modelId)
      && ChoiceLoopContract.effort(requestedEffort) && ChoiceLoopContract.effort(actualEffort)
      && ChoiceLoopContract.sha256(catalogFingerprint) && catalogRevision > 0
      && ChoiceLoopContract.text(accountDisplayClass, maximum: 256)
      && protocolSchemaRevision > 0 && ChoiceLoopContract.text(turnId, maximum: 128)
  }
}

public struct DocumentManifest: Codable, Equatable, Sendable {
  public let rootVersion: UInt64
  public let entries: [DocumentManifestEntry]
  public let aggregateDigest: String
  public let generatedAtMs: Int64

  public static func canonicalAggregateDigest(entries: [DocumentManifestEntry]) -> String? {
    guard !entries.isEmpty,
      entries.count <= 256,
      entries.allSatisfy({ $0.validated() }),
      Set(entries.map { $0.relativePath.lowercased() }).count == entries.count
    else { return nil }

    var bytes = Data("openopen:document-manifest:v1\0".utf8)
    for entry in entries.sorted(by: { $0.relativePath < $1.relativePath }) {
      appendLengthDelimited(Data(entry.relativePath.utf8), to: &bytes)
      appendLengthDelimited(Data(entry.sha256.utf8), to: &bytes)
      var byteLength = entry.byteLength.bigEndian
      appendLengthDelimited(Data(bytes: &byteLength, count: MemoryLayout<UInt64>.size), to: &bytes)
      var mode = entry.mode.bigEndian
      appendLengthDelimited(Data(bytes: &mode, count: MemoryLayout<UInt32>.size), to: &bytes)
    }
    return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
  }

  fileprivate func validated() -> Bool {
    rootVersion > 0 && generatedAtMs >= 0
      && aggregateDigest == Self.canonicalAggregateDigest(entries: entries)
  }

  private static func appendLengthDelimited(_ value: Data, to bytes: inout Data) {
    var length = UInt64(value.count).bigEndian
    withUnsafeBytes(of: &length) { bytes.append(contentsOf: $0) }
    bytes.append(value)
  }
}

public struct DocumentManifestEntry: Codable, Equatable, Sendable {
  public let relativePath: String
  public let sha256: String
  public let byteLength: UInt64
  public let mode: UInt32

  fileprivate func validated() -> Bool {
    ChoiceLoopContract.documentPath(relativePath) && ChoiceLoopContract.sha256(sha256)
      && byteLength <= 512 * 1_024 && mode == 0o600
  }
}

public struct MarkdownBaseIdentity: Codable, Equatable, Sendable {
  public let entry: DocumentManifestEntry
  public let device: UInt64
  public let inode: UInt64

  fileprivate func validated() -> Bool {
    entry.validated() && device > 0 && inode > 0
  }
}

private enum ChoiceLoopContract {
  static func sha256(_ value: String) -> Bool {
    value.utf8.count == 64
      && value.utf8.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102)
      }
  }

  static func identifier(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128
      && value.utf8.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 90) || ($0 >= 97 && $0 <= 122)
          || $0 == 45 || $0 == 95 || $0 == 46
      }
  }

  static func text(_ value: String, maximum: Int) -> Bool {
    !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
      && value.utf8.count <= maximum
      && !value.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) })
  }

  static func list(_ values: [String], maximumItems: Int, maximumText: Int) -> Bool {
    values.count <= maximumItems && values.allSatisfy { text($0, maximum: maximumText) }
  }

  static func effort(_ value: String) -> Bool {
    value == "not_applicable"
      || (!value.isEmpty && value.utf8.count <= 32
        && value.utf8.allSatisfy {
          ($0 >= 97 && $0 <= 122) || $0 == 45
        })
  }

  static func modelId(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128
      && value.utf8.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 90) || ($0 >= 97 && $0 <= 122)
          || $0 == 45 || $0 == 95 || $0 == 46
      }
  }

  static func documentPath(_ value: String) -> Bool {
    let components = value.split(separator: "/").map(String.init)
    return !value.isEmpty && value.utf8.count <= 512
      && value.unicodeScalars.allSatisfy { $0.isASCII }
      && !value.hasPrefix("/") && !value.contains("//")
      && components.allSatisfy { component in
        !component.isEmpty && component != "." && component != ".."
          && component.utf8.allSatisfy {
            ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 90) || ($0 >= 97 && $0 <= 122)
              || $0 == 45 || $0 == 95 || $0 == 46
          }
      }
      && matchesDocumentManifestPath(components)
  }

  static func matchesDocumentManifestPath(_ components: [String]) -> Bool {
    if components == ["INDEX.md"] || components == ["profile", "USER.md"]
      || components == ["profile", "COMMUNICATION.md"] || components == ["sources", "INDEX.md"]
    {
      return true
    }
    if components.count == 2, components[0] == "sources" {
      return dynamicMarkdownName(components[1])
    }
    if components.count == 3, components[0] == "tasks" {
      return identifier(components[1])
        && [
          "OVERVIEW.md", "STATE.md", "DECISIONS.md", "QUESTIONS.md", "MODEL_BRIEF.md",
        ].contains(components[2])
    }
    if components.count == 4,
      components[0] == "tasks"
        && (components[2] == "paths" || components[2] == "updates")
    {
      return identifier(components[1]) && dynamicMarkdownName(components[3])
    }
    if components.count == 3, components[0] == "sessions",
      ["SESSION.md", "CHOICE.md"].contains(components[2])
    {
      return identifier(components[1])
    }
    if components.count == 4, components[0] == "sessions", components[2] == "choice-sets" {
      return identifier(components[1]) && dynamicMarkdownName(components[3])
    }
    return false
  }

  static func dynamicMarkdownName(_ value: String) -> Bool {
    value.utf8.count > 3 && value.hasSuffix(".md")
      && identifier(String(value.dropLast(3)))
  }
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

public struct ChoiceMarkdownReceiptCleanupAvailability: Codable, Equatable, Sendable {
  public let available: Bool

  public init(available: Bool) {
    self.available = available
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
