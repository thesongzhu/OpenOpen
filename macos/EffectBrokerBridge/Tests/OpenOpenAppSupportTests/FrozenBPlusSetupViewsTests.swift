import Foundation
import Testing

private func openOpenViewsSource() throws -> String {
  let sourceURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/OpenOpenAppSupport/OpenOpenViews.swift")
  return try String(contentsOf: sourceURL, encoding: .utf8)
}

private func sourceSlice(_ source: String, from start: String, to end: String) throws -> String {
  let startIndex = try #require(source.range(of: start)?.lowerBound)
  let endIndex = try #require(
    source.range(of: end, range: startIndex..<source.endIndex)?.lowerBound)
  return String(source[startIndex..<endIndex])
}

@Test
func frozenMemorySetupShowsExactBoundariesWithoutImportAuthority() throws {
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
  #expect(!memory.contains("Task {"))
  #expect(!memory.contains("model."))
}

@Test
func frozenSkillSetupShowsExactBoundariesWithoutLifecycleAuthority() throws {
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
  #expect(!skills.contains("Task {"))
  #expect(!skills.contains("model."))
}

@Test
func frozenSetupActionsRemainDisabledUntilTypedHostStateExists() throws {
  let source = try openOpenViewsSource()
  let boundary = try sourceSlice(
    source,
    from: "private struct EditorialBoundaryCard: View",
    to: "private struct EditorialPageHeader: View"
  )

  #expect(boundary.contains("Button(actionTitle) {}"))
  #expect(boundary.contains(".disabled(true)"))
  #expect(!source.contains("EditorialUnavailableView"))
}
