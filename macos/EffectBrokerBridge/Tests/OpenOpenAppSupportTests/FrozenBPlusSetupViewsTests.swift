import Foundation
import Testing

@testable import OpenOpenAppSupport

private func openOpenViewsSource() throws -> String {
  let sourceURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/OpenOpenAppSupport/OpenOpenViews.swift")
  return try String(contentsOf: sourceURL, encoding: .utf8)
}

private func appModelSource() throws -> String {
  let sourceURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/OpenOpenAppSupport/AppModel.swift")
  return try String(contentsOf: sourceURL, encoding: .utf8)
}

private func sourceSlice(_ source: String, from start: String, to end: String) throws -> String {
  let startIndex = try #require(source.range(of: start)?.lowerBound)
  let endIndex = try #require(
    source.range(of: end, range: startIndex..<source.endIndex)?.lowerBound)
  return String(source[startIndex..<endIndex])
}

@Test
func frozenMemorySetupUsesOnlyTypedImportAndProcessingAuthority() throws {
  let source = try openOpenViewsSource()
  let memory = try sourceSlice(
    source,
    from: "private struct EditorialMemoryView: View",
    to: "private struct EditorialSkillsView: View"
  )

  #expect(memory.contains("One careful import"))
  #expect(memory.contains("Import one source and keep only one card you explicitly approve."))
  #expect(memory.contains("produce up to three candidates."))
  #expect(memory.contains("Reject all if none should persist."))
  #expect(memory.contains("Review the exact line added to the local Memory file."))
  #expect(memory.contains("written and read back successfully."))
  #expect(memory.contains("openopen-memory-choose-import"))
  #expect(!memory.contains("NSOpenPanel"))
  #expect(memory.contains("model.b2MemoryChooseImportEnabled"))
  #expect(memory.contains("model.chooseB2MemoryImport"))
  #expect(memory.contains("model.b2MemoryProcessSourceEnabled"))
  #expect(memory.contains("model.requestB2MemoryProcessingConsent"))
  #expect(memory.contains("model.confirmB2MemoryProcessingConsent"))
  #expect(!memory.contains("beginB2MemoryDemo"))
  #expect(!memory.contains("requestB2CandidateGeneration"))
  #expect(memory.contains("model.requestB2CandidateSelection"))
  #expect(memory.contains("model.requestB2DiffConfirmation"))
  #expect(memory.contains("openopen-memory-confirmed-readback"))
  #expect(memory.contains("Only this change will be written"))
  #expect(memory.contains("Review the exact line added to the local Memory file."))
  #expect(!memory.contains("Nothing changes until you confirm."))

  let appModel = try appModelSource()
  #expect(appModel.contains("core.prepareB2MemorySource"))
  #expect(appModel.contains("core.processB2MemorySource"))
  #expect(appModel.contains("B2MemoryProcessingConsent"))
  #expect(appModel.contains("explicitlyConfirmed: true"))
  #expect(appModel.contains("panel.title = \"Choose one file to review\""))
  #expect(appModel.contains("panel.prompt = \"Choose file\""))
  #expect(appModel.contains("b2MemoryPendingPrepareRequest"))
  #expect(!appModel.contains("c2StableDigest(selectedURL.path)"))
  #expect(!appModel.contains("The Memory Demo state could not be verified."))
  #expect(!appModel.contains("The Memory Demo command is invalid."))
  #expect(!appModel.contains("The Memory Demo step was not verified."))
}

@Test
func frozenSkillSetupUsesOnlyTheTypedLifecycleAuthority() throws {
  let source = try openOpenViewsSource()
  let skills = try sourceSlice(
    source,
    from: "private struct EditorialSkillsView: View",
    to: "private struct EditorialBoundaryCard: View"
  )

  #expect(skills.contains("Add one reviewed instruction-only Skill"))
  #expect(skills.contains("Executable files and external-effect Skills are not eligible"))
  #expect(skills.contains("Acquisition does not enable it. The staged copy remains inactive."))
  #expect(
    skills.contains("Checking instructions, files, network use, credentials, and external effects.")
  )
  #expect(skills.contains("No script or external effect is allowed."))
  #expect(skills.contains("Try without external effects"))
  #expect(skills.contains("openopen-skills-find"))
  #expect(!skills.contains("URLSession"))
  #expect(skills.contains("model.requestNextC2SkillDemoAction()"))
  #expect(skills.contains("model.confirmC2SkillDemoAction()"))
  #expect(!skills.contains("c2SkillDemoRequestIds"))

  let appModel = try appModelSource()
  #expect(appModel.contains("This Skill cannot be enabled"))
  #expect(!appModel.contains("The reviewed Skill state could not be verified."))
  #expect(!appModel.contains("A valid reviewed instruction-only Skill seal is required."))
  #expect(!appModel.contains("The supplied Skill does not match the durable reviewed package."))
  #expect(!appModel.contains("A verified no-external-effect result is required."))
  #expect(!appModel.contains("A reviewed sealed instruction-only Skill is required"))
  #expect(!appModel.contains("The reviewed Skill command is invalid."))
  #expect(!appModel.contains("The confirmed instruction-only Skill step was recorded."))
  #expect(!appModel.contains("The Skill step was not verified."))
}

@Test
func frozenSetupActionsRequireTypedHostState() throws {
  let source = try openOpenViewsSource()
  let boundary = try sourceSlice(
    source,
    from: "private struct EditorialBoundaryCard: View",
    to: "private struct EditorialPageHeader: View"
  )

  #expect(boundary.contains("Button(actionTitle, action: action)"))
  #expect(boundary.contains(".disabled(!enabled)"))
  #expect(!source.contains("EditorialUnavailableView"))
}

@Test
func c2SkillContractsRejectExecutableOrUnsealedAuthority() {
  let valid = C2SkillDemoSeal(
    packageId: "decision-brief",
    sourceUrl:
      "https://github.com/example/skills/tree/0123456789abcdef0123456789abcdef01234567/decision-brief",
    commit: "0123456789abcdef0123456789abcdef01234567",
    packageDigest: String(repeating: "a", count: 64),
    auditAnchor: String(repeating: "b", count: 64),
    permissionDigest: C2SkillDemoSeal.instructionOnlyPermissionDigest,
    license: "MIT")
  #expect(valid.isValid)
  #expect(
    !C2SkillDemoSeal(
      packageId: valid.packageId, sourceUrl: valid.sourceUrl, commit: valid.commit,
      packageDigest: valid.packageDigest, auditAnchor: valid.auditAnchor,
      permissionDigest: String(repeating: "c", count: 64), license: valid.license
    ).isValid)

  let command = C2SkillDemoCommand(
    requestId: "skill-request", expectedRevision: 0, kind: .registerCandidate, seal: valid,
    actorId: "owner", decisionId: "decision-skill-request",
    approvalNonce: String(repeating: "d", count: 64), resultDigest: nil,
    explicitlyConfirmed: true, decidedAtMs: 0)
  #expect(command.isValid)
  #expect(
    !C2SkillDemoCommand(
      requestId: command.requestId, expectedRevision: 0, kind: .recordFirstNoEffectUse,
      seal: valid, actorId: command.actorId, decisionId: command.decisionId,
      approvalNonce: command.approvalNonce, resultDigest: nil, explicitlyConfirmed: true,
      decidedAtMs: 0
    ).isValid)
}
