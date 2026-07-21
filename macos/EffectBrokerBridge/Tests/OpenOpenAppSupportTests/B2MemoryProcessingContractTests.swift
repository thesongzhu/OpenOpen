import Foundation
import Testing

@testable import OpenOpenAppSupport

private let b2Digest = String(repeating: "a", count: 64)

private func b2PreparedSource(_ requestId: String = "memory-source-1") -> B2MemoryPreparedSource {
  B2MemoryPreparedSource(
    requestId: requestId,
    sourceIdentityDigest: b2Digest,
    byteLength: 1_024)
}

private func b2ModelProvenance() -> ChoiceModelProvenance {
  ChoiceModelProvenance(
    id: "memory-model-provenance",
    modelId: "gpt-memory",
    requestedEffort: "high",
    actualEffort: "high",
    catalogFingerprint: b2Digest,
    catalogRevision: 1,
    accountDisplayClass: "ChatGPT",
    protocolSchemaRevision: 1,
    turnId: "memory-turn-1")
}

private func b2PersonaRevision() -> PersonaRevisionRef {
  PersonaRevisionRef(
    personaId: "openopen.nondev.default",
    revision: "persona-revision-1",
    aggregateDigest: b2Digest,
    instructionsDigest: b2Digest)
}

private func b2ProcessingOperation() -> B2MemoryProcessingOperation {
  B2MemoryProcessingOperation(
    operationId: "b2-processing-1",
    requestId: "memory-process-1",
    expectedRevision: 1,
    runtimeRevision: 7,
    sourceIdentityDigest: b2Digest,
    sourceDigest: b2Digest,
    modelProvenance: b2ModelProvenance(),
    catalogDigest: b2Digest,
    protocolVersion: 1,
    personaRevision: b2PersonaRevision(),
    documentManifestDigest: b2Digest,
    sourceManifestDigest: b2Digest,
    startedAtMs: 10)
}

private func b2Seal() -> B2MemoryImportSeal {
  B2MemoryImportSeal(
    sourceDigest: b2Digest,
    catalogDigest: b2Digest,
    sourceManifestDigest: b2Digest,
    modelProvenance: b2ModelProvenance())
}

private func b2State(
  stage: B2MemoryDemoStage,
  source: B2MemoryPreparedSource = b2PreparedSource()
) -> B2MemoryDemoState {
  let processing = stage == .prepared ? nil : b2ProcessingOperation()
  let hasResult = ![.prepared, .processing].contains(stage)
  let candidates =
    stage == .candidates
    ? [
      B2MemoryCandidateCard(
        id: "memory-candidate-1",
        title: "One candidate",
        rationale: "Bound to the selected import.",
        proposedLine: "- One confirmed memory line.",
        sourceBindingDigest: b2Digest)
    ] : []
  return B2MemoryDemoState(
    revision: stage == .prepared ? 1 : 2,
    stage: stage,
    preparedSource: source,
    processingOperation: processing,
    processingResultDigest: hasResult ? b2Digest : nil,
    seal: stage == .prepared ? nil : b2Seal(),
    candidates: candidates,
    selectedCandidate: nil,
    markdownDiff: nil,
    confirmationDigest: nil,
    renderIntent: nil,
    readbackReceipt: nil,
    receipts: [])
}

@Test
func b2ProcessingStateDecodesTheExactRustWireFields() throws {
  let state = b2State(stage: .processing)
  #expect(state.isValid)
  #expect(state.processingOperation?.isValid == true)

  let encoded = try JSONEncoder().encode(state)
  let object = try #require(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
  #expect(object["stage"] as? String == "processing")
  #expect(object["processingOperation"] != nil)
  #expect(object["processingResultDigest"] == nil)
  #expect(object["renderIntent"] == nil)
  #expect(try JSONDecoder().decode(B2MemoryDemoState.self, from: encoded) == state)
}

@Test
@MainActor
func b2ProcessingRequiresTheExactPreparedSourceAndCurrentModelRuntime() {
  let source = b2PreparedSource()
  let prepared = b2State(stage: .prepared, source: source)
  #expect(prepared.isValid)
  #expect(
    AppModel.b2MemoryProcessSourceIsEnabled(
      modelEntryEnabled: true,
      isBusy: false,
      state: prepared,
      currentSource: source))

  for (modelEntryEnabled, isBusy, state, currentSource) in [
    (false, false, prepared, source),
    (true, true, prepared, source),
    (true, false, b2State(stage: .processing, source: source), source),
    (true, false, prepared, b2PreparedSource("different-source")),
  ] {
    #expect(
      !AppModel.b2MemoryProcessSourceIsEnabled(
        modelEntryEnabled: modelEntryEnabled,
        isBusy: isBusy,
        state: state,
        currentSource: currentSource))
  }
}

@Test
@MainActor
func b2MemoryActionsAreUnavailableWhileOffBusyOrAtTheWrongStage() {
  let candidates = b2State(stage: .candidates)
  #expect(candidates.isValid)
  #expect(
    AppModel.b2MemoryActionIsEnabled(
      storeControlEnabled: true, isBusy: false, state: candidates, kind: .selectCandidate))
  #expect(
    !AppModel.b2MemoryActionIsEnabled(
      storeControlEnabled: false, isBusy: false, state: candidates, kind: .selectCandidate))
  #expect(
    !AppModel.b2MemoryActionIsEnabled(
      storeControlEnabled: true, isBusy: true, state: candidates, kind: .selectCandidate))
  #expect(
    !AppModel.b2MemoryActionIsEnabled(
      storeControlEnabled: true, isBusy: false, state: b2State(stage: .processing),
      kind: .editMarkdown))
  #expect(
    !AppModel.b2MemoryActionIsEnabled(
      storeControlEnabled: true, isBusy: false, state: b2State(stage: .prepared),
      kind: .confirmDiff))

  #expect(
    AppModel.b2MemoryPendingActionIsEnabled(
      storeControlEnabled: true,
      isBusy: false,
      state: candidates,
      action: .selectCandidate,
      candidateId: "memory-candidate-1"))
  for (storeControlEnabled, isBusy, candidateId) in [
    (false, false, "memory-candidate-1"),
    (true, true, "memory-candidate-1"),
    (true, false, "different-candidate"),
  ] {
    #expect(
      !AppModel.b2MemoryPendingActionIsEnabled(
        storeControlEnabled: storeControlEnabled,
        isBusy: isBusy,
        state: candidates,
        action: .selectCandidate,
        candidateId: candidateId))
  }
}

@Test
func b2ProcessingAndCandidateStatesFailClosedOnMissingBindings() {
  let processing = b2State(stage: .processing)
  let missingOperation = B2MemoryDemoState(
    revision: processing.revision,
    stage: processing.stage,
    preparedSource: processing.preparedSource,
    processingOperation: nil,
    processingResultDigest: processing.processingResultDigest,
    seal: processing.seal,
    candidates: processing.candidates,
    selectedCandidate: processing.selectedCandidate,
    markdownDiff: processing.markdownDiff,
    confirmationDigest: processing.confirmationDigest,
    renderIntent: processing.renderIntent,
    readbackReceipt: processing.readbackReceipt,
    receipts: processing.receipts)
  #expect(!missingOperation.isValid)

  let candidate = B2MemoryCandidateCard(
    id: "memory-candidate-1",
    title: "One candidate",
    rationale: "Bound to the selected import.",
    proposedLine: "- One confirmed memory line.",
    sourceBindingDigest: b2Digest)
  let candidates = B2MemoryDemoState(
    revision: 3,
    stage: .candidates,
    preparedSource: b2PreparedSource(),
    processingOperation: b2ProcessingOperation(),
    processingResultDigest: b2Digest,
    seal: b2Seal(),
    candidates: [candidate],
    selectedCandidate: nil,
    markdownDiff: nil,
    confirmationDigest: nil,
    renderIntent: nil,
    readbackReceipt: nil,
    receipts: [])
  #expect(candidates.isValid)

  let missingResult = B2MemoryDemoState(
    revision: candidates.revision,
    stage: candidates.stage,
    preparedSource: candidates.preparedSource,
    processingOperation: candidates.processingOperation,
    processingResultDigest: nil,
    seal: candidates.seal,
    candidates: candidates.candidates,
    selectedCandidate: candidates.selectedCandidate,
    markdownDiff: candidates.markdownDiff,
    confirmationDigest: candidates.confirmationDigest,
    renderIntent: candidates.renderIntent,
    readbackReceipt: candidates.readbackReceipt,
    receipts: candidates.receipts)
  #expect(!missingResult.isValid)
}
