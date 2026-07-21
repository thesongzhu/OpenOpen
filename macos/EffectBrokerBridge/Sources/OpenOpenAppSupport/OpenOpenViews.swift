import AppKit
import SwiftUI

// Production shell contract: the immutable Owner-selected Editorial Native V3
// artifact has SHA-256 7b251d81e228d7cec11abc473c28ff23ce47db396233eb6c4eb3bd5eed050cd3.
// This file adapts only Host-owned typed state; it does not substitute a mock
// state machine or grant any action authority.

private final class DashboardInteractionAnchorView: NSView {
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

private struct DashboardInteractionAnchor: NSViewRepresentable {
  let identifier: String

  func makeNSView(context: Context) -> DashboardInteractionAnchorView {
    let view = DashboardInteractionAnchorView()
    view.identifier = NSUserInterfaceItemIdentifier(identifier)
    view.setAccessibilityElement(false)
    return view
  }

  func updateNSView(_ view: DashboardInteractionAnchorView, context: Context) {
    view.identifier = NSUserInterfaceItemIdentifier(identifier)
  }
}

extension View {
  fileprivate func dashboardInteractionAnchor(_ identifier: String) -> some View {
    background {
      DashboardInteractionAnchor(identifier: identifier)
        .allowsHitTesting(false)
    }
  }
}

public struct OpenOpenRootView: View {
  @ObservedObject private var model: AppModel
  @State private var section: EditorialSection

  public init(model: AppModel) {
    self.model = model
    _section = State(initialValue: model.showsSettings ? .settings : .home)
  }

  public var body: some View {
    GeometryReader { geometry in
      if geometry.size.width < 560 {
        VStack(spacing: 0) {
          EditorialToolbar(model: model, section: $section)
          Divider()
          EditorialCompactNavigation(section: $section)
          Divider()
          editorialContent
        }
        .background(EditorialPalette.background)
      } else {
        NavigationSplitView {
          EditorialSidebar(section: $section)
        } detail: {
          VStack(spacing: 0) {
            EditorialToolbar(model: model, section: $section)
            Divider()
            editorialContent
          }
          .background(EditorialPalette.background)
        }
        .navigationSplitViewStyle(.balanced)
      }
    }
    .frame(minWidth: 390, minHeight: 520)
    // Root presentation can be opened directly into Settings.  It refreshes
    // read-only state only; the Home card below is the sole UI owner-return
    // signal that may resume an already-idle Choice session.
    .task { await model.refreshDashboard() }
    .onAppear { section = model.showsSettings ? .settings : .home }
    .onChange(of: model.showsSettings) { _, showsSettings in
      if showsSettings { section = .settings }
    }
    .onChange(of: section) { _, section in
      model.showsSettings = section == .settings
    }
  }

  @ViewBuilder private var editorialContent: some View {
    switch section {
    case .home:
      DashboardView(model: model, section: $section)
    case .activity:
      EditorialActivityView(model: model)
    case .settings:
      SettingsView(model: model)
    case .messages:
      EditorialMessagesView(model: model)
    case .memory:
      EditorialMemoryView(model: model)
    case .skills:
      EditorialSkillsView(model: model)
    }
  }
}

/// The selected editorial navigation is a presentation shell only. It never
/// creates a model, channel, Memory, Skill, or effect authority. Those remain
/// represented by the Host-owned typed state that each production phase wires.
private enum EditorialSection: String, CaseIterable, Hashable, Identifiable {
  case home
  case activity
  case messages
  case memory
  case skills
  case settings

  var id: String { rawValue }

  var title: String {
    switch self {
    case .home: "Home"
    case .activity: "Activity"
    case .messages: "Messages"
    case .memory: "Memory"
    case .skills: "Skills"
    case .settings: "Settings"
    }
  }

  var symbol: String {
    switch self {
    // Match the frozen Editorial Native V3 navigation vocabulary.  These are
    // semantic SF Symbol equivalents of its locked native-icon contract, not
    // a second visual system.
    case .home: "message.square.text"
    case .activity: "list.bullet"
    case .messages: "messages"
    case .memory: "notebook"
    case .skills: "shippingbox"
    case .settings: "gearshape"
    }
  }
}

private enum EditorialPalette {
  static let background = Color(nsColor: .windowBackgroundColor)
  static let sidebar = Color(nsColor: .underPageBackgroundColor)
  static let card = Color(nsColor: .controlBackgroundColor).opacity(0.72)
  static let border = Color(nsColor: .separatorColor).opacity(0.65)
  static let accent = Color(red: 51 / 255, green: 156 / 255, blue: 1)
  static let destructive = Color(red: 226 / 255, green: 85 / 255, blue: 7 / 255)
  static let cornerRadius: CGFloat = 20
}

private struct EditorialSidebar: View {
  @Binding var section: EditorialSection

  var body: some View {
    List(selection: $section) {
      Section {
        ForEach(
          [
            EditorialSection.home,
            .activity,
            .messages,
            .memory,
            .skills,
          ]
        ) { destination in
          Label(destination.title, systemImage: destination.symbol)
            .tag(destination)
            .accessibilityIdentifier("openopen-nav-\(destination.rawValue)")
        }
      }
    }
    .listStyle(.sidebar)
    .scrollContentBackground(.hidden)
    .background(EditorialPalette.sidebar)
    .navigationSplitViewColumnWidth(min: 180, ideal: 180, max: 180)
    .accessibilityIdentifier("openopen-editorial-sidebar")
  }
}

/// The selected narrow shell turns the five primary destinations into a
/// horizontal text navigation row rather than squeezing the desktop sidebar
/// beside the 390-point content column.  Settings remains behind the frozen
/// overflow action in both widths.
private struct EditorialCompactNavigation: View {
  @Binding var section: EditorialSection

  private let destinations: [EditorialSection] = [.home, .activity, .messages, .memory, .skills]

  var body: some View {
    ScrollView(.horizontal, showsIndicators: false) {
      HStack(spacing: 2) {
        ForEach(destinations) { destination in
          if destination == section {
            Button(destination.title) { section = destination }
              .buttonStyle(.borderedProminent)
              .accessibilityIdentifier("openopen-nav-compact-\(destination.rawValue)")
          } else {
            Button(destination.title) { section = destination }
              .buttonStyle(.borderless)
              .accessibilityIdentifier("openopen-nav-compact-\(destination.rawValue)")
          }
        }
      }
      .padding(.horizontal, 12)
      .padding(.vertical, 10)
    }
    .accessibilityIdentifier("openopen-editorial-compact-navigation")
  }
}

private struct EditorialToolbar: View {
  @ObservedObject var model: AppModel
  @Binding var section: EditorialSection

  private var statusLabel: String {
    if model.runtimeDisplayState == .on, model.runtimeRecoveryState == .ready { return "On" }
    if model.runtimeDisplayState == .off { return "Off" }
    return "Needs you"
  }

  var body: some View {
    ZStack {
      HStack(spacing: 9) {
        EditorialOpenMark()
        Text("OpenOpen")
          .font(.headline.weight(.medium))
          .accessibilityIdentifier("openopen-editorial-product-title")
        Spacer()
        Label(statusLabel, systemImage: model.runtimeDisplayState.menuBarSymbol)
          .font(.caption.weight(.medium))
          .foregroundStyle(statusLabel == "Needs you" ? EditorialPalette.destructive : .secondary)
          .accessibilityIdentifier("openopen-editorial-runtime-status")
        Button {
          section = .settings
        } label: {
          Image(systemName: "ellipsis")
        }
        .buttonStyle(.borderless)
        .disabled(!model.dashboardControls.settingsEnabled)
        .accessibilityLabel("More options")
        .accessibilityIdentifier("openopen-editorial-more-options")
      }
      Text(section.title)
        .font(.headline.weight(.medium))
        .accessibilityIdentifier("openopen-editorial-toolbar-title")
    }
    .padding(.horizontal, 14)
    .padding(.vertical, 10)
    .background(EditorialPalette.background)
  }
}

/// The frozen shell uses a restrained double-ring OpenOpen mark.  Keeping it
/// local to the toolbar preserves the selected hierarchy without adding an
/// asset pipeline or changing any typed product state.
private struct EditorialOpenMark: View {
  var body: some View {
    ZStack {
      Circle().stroke(.primary, lineWidth: 1)
      Circle().stroke(.primary.opacity(0.55), lineWidth: 1).padding(4)
    }
    .frame(width: 17, height: 17)
    .accessibilityHidden(true)
  }
}

private struct EditorialActivityView: View {
  @ObservedObject var model: AppModel

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        EditorialPageHeader(
          eyebrow: "Recent",
          title: "Activity",
          detail:
            "Local progress, evidence, and recovery stay visible without implying a new effect."
        )
        if let receipt = model.receipt {
          EditorialCard(title: "Verified receipt", symbol: "checkmark.seal") {
            Text(receipt.summary).font(.headline)
            Text("Evidence: \(receipt.evidenceIds.count) Reminder completion(s)")
              .foregroundStyle(.secondary)
          }
          .accessibilityIdentifier("openopen-activity-receipt")
        }
        if model.activeCards.isEmpty, model.receipt == nil, model.needsYou == nil {
          EditorialEmptyState(
            title: "No activity yet",
            detail: "Your local Choice progress and verified receipts will appear here.",
            symbol: "clock"
          )
        }
        ForEach(model.activeCards) { card in
          EditorialCard(title: card.title, symbol: "circle.dotted") {
            Text(card.state).foregroundStyle(.secondary)
          }
        }
        if let needsYou = model.needsYou {
          EditorialCard(title: needsYou.title, symbol: "exclamationmark.circle") {
            Text(needsYou.prompt)
          }
          .accessibilityIdentifier("openopen-activity-needs-you")
        }
      }
      .padding(30)
      .frame(maxWidth: 760, alignment: .leading)
    }
    .background(EditorialPalette.background)
  }
}

private struct EditorialMessagesView: View {
  @ObservedObject var model: AppModel

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        EditorialPageHeader(
          eyebrow: "Private inbox",
          title: model.iMessageIsConnected
            ? "Self-chat connected" : "Connect one private self-chat",
          detail: model.iMessageIsConnected
            ? "Messages from this one conversation can now be processed when they address OpenOpen."
            : "Only the conversation you explicitly choose can become OpenOpen’s inbox."
        )
        EditorialCard(title: "Messages", symbol: "messages") {
          if model.iMessageIsConnected {
            Label("Connected", systemImage: "checkmark.circle.fill")
              .foregroundStyle(.green)
              .accessibilityIdentifier("openopen-messages-connected")
          } else {
            Button("Load Messages conversations") {
              Task { await model.refreshIMessageChats() }
            }
            .disabled(!model.modelEntryEnabled || model.isBusy)
            .accessibilityIdentifier("openopen-messages-load-conversations")
            Picker(
              "Approved conversation",
              selection: Binding(
                get: { model.iMessageChatId },
                set: { model.selectIMessageChat($0) }
              )
            ) {
              Text("Choose a conversation").tag("")
              ForEach(model.iMessageChats) { chat in
                Text("\(chat.displayName) · \(chat.service)").tag(chat.chatId)
              }
            }
            .disabled(model.iMessageChats.isEmpty)
            .accessibilityIdentifier("openopen-messages-self-chat-picker")
            Picker(
              "Approved owner",
              selection: Binding(
                get: { model.iMessageOwnerSender },
                set: { model.selectIMessageOwner($0) }
              )
            ) {
              Text("Choose the owner").tag("")
              ForEach(model.iMessageOwnerOptions, id: \.self) { participant in
                Text(participant).tag(participant)
              }
            }
            .disabled(model.iMessageOwnerOptions.isEmpty)
            .accessibilityIdentifier("openopen-messages-owner-picker")
            Button(
              model.iMessageHasDurablePairing
                ? "Reconnect approved iMessage" : "Use selected chat"
            ) {
              Task { await model.connectIMessage() }
            }
            .disabled(!model.iMessageConnectionActionEnabled)
            .accessibilityIdentifier("openopen-messages-connect")
          }
          if let feedback = model.channelListenerFeedback[.iMessage] {
            Text(feedback)
              .font(.caption)
              .foregroundStyle(.orange)
              .accessibilityIdentifier("openopen-messages-recovery")
          }
        }

        if let preview = model.choiceIMessageReplyPreview {
          EditorialCard(
            title: model.choiceIMessageReplyStatus == "delivered" ? "Reply sent" : "Reply ready",
            symbol: model.choiceIMessageReplyStatus == "delivered"
              ? "checkmark.circle" : "arrowshape.turn.up.left"
          ) {
            Text(preview.destination).font(.caption).foregroundStyle(.secondary)
            Text(preview.visibleBody)
              .textSelection(.enabled)
              .accessibilityIdentifier("openopen-messages-reply-body")
            if model.choiceIMessageReplyStatus == "delivered" {
              Text("Sent and verified")
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("openopen-messages-reply-delivered")
            } else {
              Button(model.choiceIMessageReplyStatus == "authorized" ? "Verify delivery" : "Send") {
                Task { await model.authorizeCurrentChoiceIMessageReply() }
              }
              .buttonStyle(.borderedProminent)
              .disabled(
                !model.choiceIMessageReplySendEnabled
                  && !model.choiceIMessageReplyRecoveryEnabled
              )
              .accessibilityIdentifier("openopen-messages-reply-authorize")
            }
          }
        }
      }
      .padding(30)
      .frame(maxWidth: 760, alignment: .leading)
    }
    .background(EditorialPalette.background)
    .accessibilityIdentifier("openopen-editorial-messages")
  }
}

private struct EditorialMemoryView: View {
  @ObservedObject var model: AppModel
  private let boundaries = [
    (
      "Processing consent",
      "This source will be sent only to the model and effort shown below to produce up to three candidates."
    ),
    (
      "Choose one memory",
      "Three candidates were extracted. Reject all if none should persist."
    ),
    (
      "Review the exact Memory change",
      "Review the exact line added to the local Memory file."
    ),
    (
      "Memory receipt",
      "The confirmed line was written and read back successfully."
    ),
  ]

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        EditorialPageHeader(
          eyebrow: "One careful import",
          title: pageTitle,
          detail: "Import one source and keep only one card you explicitly approve.",
          state: pageState)
        memoryContent
        if let feedback = model.b2MemoryFeedback {
          Text(feedback).font(.caption).foregroundStyle(.orange)
            .accessibilityIdentifier("openopen-memory-feedback")
        }
      }
      .padding(30)
      .frame(maxWidth: 760, alignment: .leading)
    }
    .background(EditorialPalette.background)
    .accessibilityIdentifier("openopen-editorial-memory")
    .alert(
      pendingTitle,
      isPresented: Binding(
        get: { model.b2MemoryPendingAction != nil },
        set: { if !$0 { model.cancelB2MemoryAction() } })
    ) {
      Button("Cancel", role: .cancel) { model.cancelB2MemoryAction() }
      Button(pendingActionTitle) { Task { await model.confirmB2MemoryAction() } }
    } message: {
      Text(pendingDetail)
    }
  }

  @ViewBuilder private var memoryContent: some View {
    switch model.b2MemoryDemoState?.stage {
    case nil:
      EditorialEmptyState(
        title: "Add one useful memory",
        detail: "Import one source and keep only one card you explicitly approve.",
        symbol: "notebook")
      EditorialBoundaryCard(
        title: "Choose one file to review",
        detail: "OpenOpen will not scan other files or folders.",
        actionTitle: "Choose import",
        accessibilityIdentifier: "openopen-memory-choose-import",
        boundaries: boundaries,
        enabled: false)
    case .prepared:
      EditorialBoundaryCard(
        title: "Review the processing scope",
        detail: boundaries[0].1,
        actionTitle: "Process source",
        accessibilityIdentifier: "openopen-memory-process-source",
        boundaries: boundaries,
        enabled: false)
    case .candidates:
      ForEach(model.b2MemoryDemoState?.candidates ?? []) { card in
        EditorialCard(title: card.title, symbol: "circle") {
          Text(card.rationale).font(.caption).foregroundStyle(.secondary)
          Button("Review selected") { model.requestB2CandidateSelection(card.id) }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("openopen-memory-candidate-\(card.id)")
        }
      }
    case .selected, .diffReview:
      EditorialCard(title: "Make the memory exact", symbol: "pencil") {
        TextField("Memory wording", text: $model.b2MemoryEditedLine, axis: .vertical)
          .accessibilityIdentifier("openopen-memory-edit-line")
        Button("Review Markdown") { Task { await model.saveB2MemoryEdit() } }
          .buttonStyle(.borderedProminent)
          .accessibilityIdentifier("openopen-memory-review-markdown")
        if let diff = model.b2MemoryDemoState?.markdownDiff {
          Text(diff.editedLine).font(.system(.body, design: .monospaced))
          Button("Confirm diff") { model.requestB2DiffConfirmation() }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("openopen-memory-confirm-diff")
        }
      }
    case .confirmed:
      EditorialCard(title: "Only this change will be written", symbol: "checkmark.circle") {
        Text("Review the exact line added to the local Memory file.")
          .font(.caption).foregroundStyle(.secondary)
        Text(model.b2MemoryDemoState?.markdownDiff?.editedLine ?? "")
          .font(.system(.body, design: .monospaced))
          .accessibilityIdentifier("openopen-memory-confirmed-readback")
      }
    case .readBack:
      EditorialCard(title: "Memory receipt", symbol: "checkmark.seal") {
        Text(model.b2MemoryDemoState?.markdownDiff?.editedLine ?? "")
          .font(.system(.body, design: .monospaced))
      }
    }
  }

  private var pageTitle: String {
    switch model.b2MemoryDemoState?.stage {
    case nil: "No memories"
    case .prepared: "Processing consent"
    case .candidates: "Choose one memory"
    case .selected: "Edit wording"
    case .diffReview, .confirmed: "Review the exact Memory change"
    case .readBack: "Memory receipt"
    }
  }

  private var pageState: String { model.b2MemoryDemoState == nil ? "Ready" : "Needs you" }
  private var pendingTitle: String {
    switch model.b2MemoryPendingAction {
    case .selectCandidate: "Choose one memory"
    case .confirmDiff: "Add this one memory?"
    default: "Confirm"
    }
  }
  private var pendingActionTitle: String {
    switch model.b2MemoryPendingAction {
    case .selectCandidate: "Review selected"
    case .confirmDiff: "Add memory"
    default: "Confirm"
    }
  }
  private var pendingDetail: String {
    switch model.b2MemoryPendingAction {
    case .selectCandidate: boundaries[1].1
    case .confirmDiff: "This confirmation authorizes only the displayed Markdown change."
    default: "This confirmation authorizes only the displayed Markdown change."
    }
  }
}

private struct EditorialSkillsView: View {
  @ObservedObject var model: AppModel
  private let boundaries = [
    (
      "Acquire for review",
      "Acquisition does not enable it. The staged copy remains inactive."
    ),
    (
      "Audit",
      "Checking instructions, files, network use, credentials, and external effects."
    ),
    (
      "Enable confirmation",
      "Only the reviewed instruction text will be promoted. No script or external effect is allowed."
    ),
    (
      "Try without external effects",
      "Ask a question that only needs reasoning and a written recommendation."
    ),
  ]

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        EditorialPageHeader(
          eyebrow: skillEyebrow,
          title: skillTitle,
          detail: skillDetail
        )
        if model.c2SkillDemoState == nil {
          EditorialEmptyState(
            title: "Add one reviewed instruction-only Skill",
            detail: "Skills can shape how OpenOpen works without gaining hidden permission.",
            symbol: "shippingbox"
          )
        }
        EditorialBoundaryCard(
          title: skillBoundaryTitle,
          detail: skillBoundaryDetail,
          actionTitle: skillActionTitle,
          accessibilityIdentifier: "openopen-skills-find",
          boundaries: boundaries,
          action: { model.requestNextC2SkillDemoAction() },
          enabled: model.c2SkillDemoActionAvailable
        )
        if let feedback = model.c2SkillDemoFeedback {
          Text(feedback)
            .font(.caption)
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("openopen-skills-feedback")
        }
      }
      .padding(30)
      .frame(maxWidth: 760, alignment: .leading)
    }
    .background(EditorialPalette.background)
    .accessibilityIdentifier("openopen-editorial-skills")
    .alert(
      skillConfirmationTitle,
      isPresented: Binding(
        get: { model.c2SkillDemoPendingAction != nil },
        set: { if !$0 { model.cancelC2SkillDemoAction() } })
    ) {
      Button("Cancel", role: .cancel) { model.cancelC2SkillDemoAction() }
      Button(skillConfirmationAction) {
        Task { await model.confirmC2SkillDemoAction() }
      }
    } message: {
      Text(skillConfirmationDetail)
    }
  }

  private var skillEyebrow: String {
    switch model.c2SkillDemoState?.stage {
    case nil: "No skills"
    case .candidate: "Audit"
    case .staged: "Enable confirmation"
    case .runnable: "Decision brief is enabled"
    case .used: "Result"
    }
  }

  private var skillTitle: String {
    switch model.c2SkillDemoState?.stage {
    case nil: "No skills"
    case .candidate: "Reviewing the staged Skill"
    case .staged: "Enable Decision brief?"
    case .runnable: "Enabled"
    case .used: "Finish the Core loop first"
    }
  }

  private var skillDetail: String {
    switch model.c2SkillDemoState?.stage {
    case nil: "Skills can shape how OpenOpen works without gaining hidden permission."
    case .candidate:
      "Checking instructions, files, network use, credentials, and external effects."
    case .staged:
      "Only the reviewed instruction text will be promoted. No script or external effect is allowed."
    case .runnable: "It can now shape eligible answers. Disable it at any time."
    case .used: "The Skill produced a recommendation with alternatives and explicit tradeoffs."
    }
  }

  private var skillBoundaryTitle: String {
    switch model.c2SkillDemoState?.stage {
    case nil: "Find one public instruction-only Skill"
    case .candidate: "Reviewing the staged Skill"
    case .staged: "Enable Decision brief?"
    case .runnable: "Try without external effects"
    case .used: "Finish the Core loop first"
    }
  }

  private var skillBoundaryDetail: String {
    switch model.c2SkillDemoState?.stage {
    case nil:
      "Executable files and external-effect Skills are not eligible for this instruction-only setup."
    case .candidate:
      "Checking instructions, files, network use, credentials, and external effects."
    case .staged:
      "Only the reviewed instruction text will be promoted. No script or external effect is allowed."
    case .runnable: "Ask a question that only needs reasoning and a written recommendation."
    case .used: "The Skill produced a recommendation with alternatives and explicit tradeoffs."
    }
  }

  private var skillActionTitle: String {
    switch model.c2SkillDemoState?.stage {
    case nil: "Find a Skill"
    case .candidate: "Audit"
    case .staged: "Enable"
    case .runnable: "Use Skill"
    case .used: "Done"
    }
  }

  private var skillConfirmationTitle: String {
    switch model.c2SkillDemoPendingAction {
    case .registerCandidate: "Download this public Skill for review?"
    case .stageReviewed: "Reviewing the staged Skill"
    case .enableRunnable: "Enable Decision brief?"
    case .recordFirstNoEffectUse: "Try without external effects"
    case nil: "No skills"
    }
  }

  private var skillConfirmationAction: String {
    switch model.c2SkillDemoPendingAction {
    case .registerCandidate: "Acquire"
    case .stageReviewed: "Audit"
    case .enableRunnable: "Enable"
    case .recordFirstNoEffectUse: "Use Skill"
    case nil: "Cancel"
    }
  }

  private var skillConfirmationDetail: String {
    switch model.c2SkillDemoPendingAction {
    case .registerCandidate: "Acquisition does not enable it. The staged copy remains inactive."
    case .stageReviewed:
      "Checking instructions, files, network use, credentials, and external effects."
    case .enableRunnable:
      "Only the reviewed instruction text will be promoted. No script or external effect is allowed."
    case .recordFirstNoEffectUse:
      "Ask a question that only needs reasoning and a written recommendation."
    case nil: "Skills can shape how OpenOpen works without gaining hidden permission."
    }
  }
}

private struct EditorialBoundaryCard: View {
  let title: String
  let detail: String
  let actionTitle: String
  let accessibilityIdentifier: String
  let boundaries: [(String, String)]
  var action: () -> Void = {}
  var enabled = false

  var body: some View {
    EditorialCard(title: title, symbol: "lock.shield") {
      Text(detail)
        .foregroundStyle(.secondary)
      VStack(alignment: .leading, spacing: 10) {
        ForEach(Array(boundaries.enumerated()), id: \.offset) { index, boundary in
          HStack(alignment: .top, spacing: 10) {
            Text("\(index + 1)")
              .font(.caption.monospacedDigit())
              .foregroundStyle(.secondary)
              .frame(width: 18, alignment: .trailing)
            VStack(alignment: .leading, spacing: 2) {
              Text(boundary.0).font(.subheadline.weight(.medium))
              Text(boundary.1).font(.caption).foregroundStyle(.secondary)
            }
          }
        }
      }
      Button(actionTitle, action: action)
        .buttonStyle(.borderedProminent)
        .disabled(!enabled)
        .accessibilityIdentifier(accessibilityIdentifier)
    }
  }
}

private struct EditorialPageHeader: View {
  let eyebrow: String
  let title: String
  let detail: String
  var state: String? = nil

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      VStack(alignment: .leading, spacing: 6) {
        Text(eyebrow.uppercased())
          .font(.system(size: 12, weight: .medium))
          .foregroundStyle(.secondary)
        Text(title)
          .font(.custom("New York", size: 24).weight(.medium))
        Text(detail)
          .font(.system(size: 14, weight: .regular))
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      Spacer(minLength: 8)
      if let state {
        Text(state)
          .font(.caption.weight(.medium))
          .padding(.horizontal, 8)
          .padding(.vertical, 4)
          .background(EditorialPalette.card, in: Capsule())
          .overlay { Capsule().stroke(EditorialPalette.border, lineWidth: 1) }
          .accessibilityIdentifier("openopen-editorial-page-state")
      }
    }
  }
}

private struct EditorialCard<Content: View>: View {
  let title: String
  let symbol: String
  @ViewBuilder let content: Content

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      Label(title, systemImage: symbol)
        .font(.headline)
      content
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(12)
    .background(
      EditorialPalette.card,
      in: RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous)
    )
    .overlay {
      RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous)
        .stroke(EditorialPalette.border, lineWidth: 1)
    }
  }
}

private struct EditorialEmptyState: View {
  let title: String
  let detail: String
  let symbol: String

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      Image(systemName: symbol)
        .font(.title2)
        .foregroundStyle(.secondary)
      Text(title).font(.headline)
      Text(detail).foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(12)
    .background(
      EditorialPalette.card,
      in: RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous)
    )
  }
}

private struct EditorialGroupBoxStyle: GroupBoxStyle {
  func makeBody(configuration: Configuration) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      configuration.label
        .font(.headline)
      configuration.content
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(12)
    .background(
      EditorialPalette.card,
      in: RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous)
    )
    .overlay {
      RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous)
        .stroke(EditorialPalette.border, lineWidth: 1)
    }
  }
}

public struct OpenOpenMenuView: View {
  @ObservedObject private var model: AppModel
  @Environment(\.openWindow) private var openWindow

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    Toggle(
      model.runtimeDisplayState.label,
      isOn: Binding(
        get: { model.runtimeToggleValue },
        set: { model.requestEnabled($0) }
      )
    )
    Divider()
    Button("Show OpenOpen") {
      model.showsSettings = false
      openWindow(id: "main")
      NSApp.activate(ignoringOtherApps: true)
    }
    Button("Settings") {
      model.showsSettings = true
      openWindow(id: "main")
      NSApp.activate(ignoringOtherApps: true)
    }
    Divider()
    Button("Quit OpenOpen") { NSApp.terminate(nil) }
  }
}

private struct DashboardView: View {
  @ObservedObject var model: AppModel
  @Binding var section: EditorialSection
  @FocusState private var homeComposerFocused: Bool

  private var editorialPageState: String {
    if model.runtimeRecoveryMessage != nil || model.errorMessage != nil { return "Needs you" }
    if model.isBusy { return "Working" }
    if model.runtimeDisplayState == .on, model.runtimeRecoveryState == .ready { return "Listening" }
    if model.runtimeDisplayState == .off { return "Off" }
    return "Checking"
  }

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 20) {
        EditorialPageHeader(
          eyebrow: "Today",
          title: "Let’s make tomorrow easier.",
          detail: "Talk naturally. OpenOpen narrows choices only when they change what happens.",
          state: editorialPageState
        )
        .accessibilityIdentifier("openopen-editorial-home-header")

        if let errorMessage = model.errorMessage {
          LocalOperationFeedbackPanel(message: errorMessage) {
            model.dismissError()
          }
        }

        if let recoveryMessage = model.runtimeRecoveryMessage {
          RuntimeRecoveryBanner(
            message: recoveryMessage,
            isPaused: model.runtimeRecoveryState == .paused
          )
        }

        if let continuityMessage = model.choiceLoopContinuityMessage {
          ChoiceLoopContinuityBanner(message: continuityMessage)
        }

        if model.runtimeRecoveryState == .awaitingAccount {
          EditorialCard(title: "Welcome", symbol: "person.crop.circle.badge.checkmark") {
            Text(
              "One useful outcome, kept within your boundaries."
            )
            .foregroundStyle(.secondary)
            Text(
              "Start with one private conversation. OpenOpen will explain each permission before you grant it."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            Button("Connect ChatGPT") { model.showsSettings = true }
              .disabled(!model.accountSetupEnabled)
              .accessibilityIdentifier("openopen-editorial-first-run-account-setup")
          }
        }

        HStack(spacing: 10) {
          TextField("Tell OpenOpen what you want to sort out…", text: $model.choiceQuestion)
            .textFieldStyle(.roundedBorder)
            .focused($homeComposerFocused)
            .onSubmit { Task { await model.submitHomeComposer() } }
            .disabled(!model.dashboardControls.outcomeInputEnabled)
            .accessibilityIdentifier("openopen-dashboard-outcome-input")
          Button(model.isBusy ? "Working…" : "Send") {
            Task { await model.submitHomeComposer() }
          }
          .disabled(!model.dashboardControls.outcomeSubmitEnabled)
          .accessibilityIdentifier("openopen-dashboard-outcome-submit")
          .dashboardInteractionAnchor("openopen-dashboard-outcome-submit")
        }
        .onChange(of: model.choiceDComposerFocusRequested) { _, requested in
          guard requested else { return }
          homeComposerFocused = true
          model.consumeChoiceDComposerFocusRequest()
        }

        if !model.channelFailureIncidents.isEmpty || model.channelFailureFeedback != nil
          || !model.channelListenerFeedback.isEmpty
        {
          ChannelFailureIncidentPanel(model: model)
        }

        if let choiceSet = model.choiceLoopSnapshot?.activeChoiceSet,
          ["active", "softIdle", "staleReview"].contains(model.choiceLoopSnapshot?.session.state)
        {
          VStack(spacing: 0) {
            ForEach(choiceSet.options, id: \.id) { option in
              EditorialChoiceRow(
                key: ["A", "B", "C"][Int(option.position) - 1],
                title: option.direction,
                detail: option.rationale,
                enabled: model.choiceSessionActionEnabled,
                action: { Task { await model.selectChoiceOption(option) } }
              )
              .accessibilityIdentifier("openopen-choice-option-\(option.position)")
            }
            EditorialChoiceDRow(
              enabled: model.choiceSessionActionEnabled,
              action: { model.focusChoiceDComposer() }
            )
          }
          .background(
            EditorialPalette.card,
            in: RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous)
          )
          .overlay {
            RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous)
              .stroke(EditorialPalette.border, lineWidth: 1)
          }
        }

        if let preview = model.choiceIMessageReplyPreview {
          GroupBox("iMessage reply") {
            VStack(alignment: .leading, spacing: 8) {
              Text(preview.destination)
                .font(.caption)
                .foregroundStyle(.secondary)
              Text(preview.visibleBody)
                .textSelection(.enabled)
                .accessibilityIdentifier("openopen-choice-imessage-reply-body")
              if model.choiceIMessageReplyStatus == "delivered" {
                Text("Sent and verified")
                  .foregroundStyle(.secondary)
                  .accessibilityIdentifier("openopen-choice-imessage-reply-delivered")
              } else {
                Button(
                  model.choiceIMessageReplyStatus == "authorized"
                    ? "Verify delivery" : "Send reply"
                ) {
                  Task { await model.authorizeCurrentChoiceIMessageReply() }
                }
                .disabled(
                  !model.choiceIMessageReplySendEnabled
                    && !model.choiceIMessageReplyRecoveryEnabled
                )
                .accessibilityIdentifier("openopen-choice-imessage-reply-authorize")
              }
            }
          }
        }

        if model.choiceLoopSnapshot?.session.state == "active",
          model.choiceLoopSnapshot?.lastSelection != nil,
          model.choiceReminderScheduleIsVisible
        {
          if let confirmation = model.choiceConfirmationPreview {
            EditorialReminderProposal(
              confirmation: confirmation,
              formattedDateTime: { item in
                "\(model.formattedChoiceReminderDateTime(item)) · \(item.timeZone)"
              },
              onEdit: { model.invalidateChoiceReminderScheduleDraft() },
              onConfirm: { Task { await model.confirmPreparedChoice() } },
              enabled: model.choiceSessionActionEnabled
            )
          } else {
            GroupBox("A time is still needed") {
              VStack(alignment: .leading, spacing: 8) {
                Text(
                  "The reminder can be prepared once you choose a date, time, and time zone. Nothing will be guessed."
                )
                .font(.caption)
                if model.choiceReminderPickerIsPresented {
                  DatePicker(
                    "Date and time",
                    selection: Binding(
                      get: { model.choiceReminderPickerDate },
                      set: { model.selectChoiceReminderDate($0) }
                    )
                  )
                  .accessibilityIdentifier("openopen-choice-reminder-date-time")
                } else {
                  Button("Date and time") {
                    model.presentChoiceReminderDatePicker()
                  }
                  .accessibilityIdentifier("openopen-choice-reminder-date-time")
                }
                Picker(
                  "Time zone",
                  selection: Binding(
                    get: { model.choiceReminderTimeZone },
                    set: { model.selectChoiceReminderTimeZone($0) }
                  )
                ) {
                  Text("Choose time zone").tag("")
                  ForEach(TimeZone.knownTimeZoneIdentifiers, id: \.self) { identifier in
                    Text(identifier).tag(identifier)
                  }
                }
                .accessibilityIdentifier("openopen-choice-reminder-time-zone")
                Picker(
                  "List",
                  selection: Binding(
                    get: { model.choiceReminderListId },
                    set: { model.selectChoiceReminderList($0) }
                  )
                ) {
                  Text("Choose list").tag("")
                  Text("Reminders").tag("openopen.default-reminders")
                }
                .accessibilityIdentifier("openopen-choice-reminder-list")
                Text("Quantity · 1 Reminder")
                  .font(.caption)
                  .accessibilityIdentifier("openopen-choice-reminder-count")
              }
            }
            HStack {
              Button("Back") { model.backFromChoiceReminderSchedule() }
                .accessibilityIdentifier("openopen-choice-reminder-back")
              Button("Review reminder") { Task { await model.prepareChoiceConfirmation() } }
                .disabled(
                  !model.choiceSessionActionEnabled
                    || !model.choiceReminderScheduleReadyForReview
                )
                .accessibilityIdentifier("openopen-choice-confirm-prepare")
            }
          }
        }

        if let state = model.choiceLoopSnapshot?.session.state,
          state != "completed",
          state != "cancelled" || model.choiceMarkdownReceiptCleanupAvailable
        {
          if state == "awaitingConfirmation", model.confirmedMission == nil {
            GroupBox("Final confirmation") {
              VStack(alignment: .leading, spacing: 8) {
                Text("Ready to add one Reminder")
                  .font(.headline)
                Text("This next confirmation authorizes the real write to Reminders.")
                  .font(.caption)
                  .foregroundStyle(.secondary)
                Button("Add Reminder") {
                  model.requestChoiceReminderWrite()
                }
                .disabled(model.isBusy || !model.storeControlEnabled)
                .accessibilityIdentifier("openopen-choice-reminder-authorize")
              }
              .frame(maxWidth: .infinity, alignment: .leading)
            }
          }
          if state == "executing" {
            GroupBox("Local continuity saved") {
              Text(
                "The prepared Markdown update is durable. Reminders and other effects remain separately gated."
              )
              .font(.caption)
              .foregroundStyle(.secondary)
            }
            .accessibilityIdentifier("openopen-choice-markdown-saved")
          }
          if state == "awaitingConfirmation", model.confirmedMission != nil,
            !model.reminderLinks.isEmpty
          {
            GroupBox("Adding and verifying the Reminder") {
              VStack(alignment: .leading, spacing: 8) {
                Text("OpenOpen is waiting for macOS, then will read back the result.")
                  .font(.caption)
                  .foregroundStyle(.secondary)
                Button("Check Reminder") {
                  Task { await model.checkChoiceReminderProgress() }
                }
                .disabled(model.isBusy || !model.storeControlEnabled)
                .accessibilityIdentifier("openopen-choice-reminder-check")
              }
              .frame(maxWidth: .infinity, alignment: .leading)
            }
          }
          if state == "awaitingConfirmation",
            model.confirmedMission?.choiceConfirmationId != nil,
            model.reminderLinks.isEmpty
          {
            GroupBox("Adding and verifying the Reminder") {
              VStack(alignment: .leading, spacing: 8) {
                Text("OpenOpen is waiting for macOS, then will read back the result.")
                  .font(.caption)
                  .foregroundStyle(.secondary)
                Button("Check Reminder") {
                  model.requestChoiceReminderWrite()
                }
                .disabled(model.isBusy || !model.storeControlEnabled)
                .accessibilityIdentifier("openopen-choice-reminder-recover")
              }
              .frame(maxWidth: .infinity, alignment: .leading)
            }
          }
          if (state == "awaitingConfirmation" && model.receipt != nil) || state == "executing"
            || (state == "cancelled" && model.choiceMarkdownReceiptCleanupAvailable)
          {
            Button("Resume local save") {
              Task { await model.reconcileChoiceMarkdown() }
            }
            // A receipt-backed cleanup is deletion-only and remains reachable
            // after Global Off; publication still requires the normal On gate.
            .disabled(model.isBusy)
            .accessibilityIdentifier("openopen-choice-markdown-reconcile")
          }
          if state != "cancelled" {
            Button("Cancel current choice") {
              Task { await model.cancelChoiceSession() }
            }
            // Cancelling durable local continuity is Store control, not model
            // work. A removed or drifted model selection must not trap a user in
            // an active Choice session.
            .disabled(model.isBusy || !model.storeControlEnabled)
            .accessibilityIdentifier("openopen-choice-cancel")
          }
        }

        if !model.activeCards.isEmpty {
          Text("Working on it").font(.title2.bold())
          ForEach(
            model.activeCards.filter { card in
              model.confirmedMission?.choiceConfirmationId == nil
                || card.id != model.confirmedMission?.missionId
            }
          ) { card in
            GroupBox(card.title) {
              VStack(alignment: .leading, spacing: 8) {
                Text(card.state)
                // Existing Mission recovery remains reachable without
                // reinstating the retired Outcome proposal entrypoint. This
                // only reads/continues an already durable historical route;
                // new Choice confirmation still has its separate boundaries.
                if model.confirmedMission?.missionId == card.id {
                  Button("Check progress") {
                    model.requestMissionProgressCheck()
                  }
                  .disabled(!model.dashboardControls.missionProgressEnabled)
                  .accessibilityIdentifier("openopen-dashboard-check-progress")
                  .dashboardInteractionAnchor("openopen-dashboard-check-progress")
                }
                Text(
                  "Historical Mission recovery stays separate from local Choice confirmation."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("openopen-pr1-historical-mission-read-only")
              }
              .frame(maxWidth: .infinity, alignment: .leading)
            }
          }
        }
        if let needsYou = model.needsYou {
          GroupBox("Need you") {
            VStack(alignment: .leading, spacing: 8) {
              Text(needsYou.title).font(.headline)
              Text(needsYou.prompt)
              Text("This historical request is read-only during local Choice Core.")
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(4)
          }
        }
        if let event = model.latestChannelMissionEvent {
          GroupBox("Mission participation") {
            VStack(alignment: .leading, spacing: 6) {
              Text(
                "Received an approved \(event.messageClass.displayName) through \(event.channel.displayName)."
              )
              Text("The update matched this Mission's approved route.")
                .font(.caption)
                .foregroundStyle(.secondary)
              Text("This update is not completion Evidence and does not expand Mission scope.")
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(4)
          }
        }
        if model.dashboardControls.doneVisible, model.receiptIsPresentableOnHome,
          let receipt = model.receipt
        {
          Group {
            if model.receiptIsForCurrentChoice {
              GroupBox("Reminder added and verified") {
                VStack(alignment: .leading, spacing: 12) {
                  Text("Reminder added and verified").font(.headline)
                  Text("Completed · verified just now")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                  Divider()
                  VStack(alignment: .leading, spacing: 3) {
                    Text("Result").font(.headline)
                    Text(
                      "The saved Reminder matches the confirmed date, time, time zone, list, and count."
                    )
                    .foregroundStyle(.secondary)
                    Text("Verified").font(.caption).foregroundStyle(.secondary)
                  }
                  VStack(alignment: .leading, spacing: 3) {
                    Text("Evidence").font(.headline)
                    Text("Readable proof is available without exposing private content.")
                      .foregroundStyle(.secondary)
                    Text("\(receipt.evidenceIds.count) verified Reminder completion(s)")
                      .font(.caption)
                      .foregroundStyle(.secondary)
                  }
                  HStack(spacing: 8) {
                    Button("View evidence") { section = .activity }
                      .accessibilityIdentifier("openopen-choice-receipt-view-evidence")
                    Button("Continue") {
                      Task { await model.refreshDashboard(authenticatedHomeForeground: true) }
                    }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("openopen-choice-receipt-continue")
                  }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(4)
              }
              .accessibilityIdentifier("openopen-choice-reminder-receipt")
            } else {
              GroupBox("Done") {
                VStack(alignment: .leading, spacing: 8) {
                  Text(receipt.summary).font(.headline)
                  Text("Evidence: \(receipt.evidenceIds.count) Reminder completion(s)")
                    .foregroundStyle(.secondary)
                  Text("Model: \(receipt.actualModel)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                  Text("Historical Receipt delivery is unavailable during local Choice Core.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(4)
              }
            }
          }
          .accessibilityIdentifier("openopen-dashboard-done")
          .dashboardInteractionAnchor("openopen-dashboard-done")
        }
        Spacer()
      }
      .padding(30)
      .frame(maxWidth: 760, alignment: .leading)
      .groupBoxStyle(EditorialGroupBoxStyle())
    }
    .scrollContentBackground(.hidden)
    .background(EditorialPalette.background)
    .task { await model.refreshDashboard(authenticatedHomeForeground: true) }
  }
}

private struct EditorialChoiceRow: View {
  let key: String
  let title: String
  let detail: String
  let enabled: Bool
  let action: () -> Void

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Text(key)
        .font(.caption.weight(.medium))
        .frame(width: 26, height: 26)
        .background(EditorialPalette.background, in: Circle())
        .overlay { Circle().stroke(EditorialPalette.border, lineWidth: 1) }
        .accessibilityHidden(true)
      VStack(alignment: .leading, spacing: 3) {
        Text(title).font(.headline)
        Text(detail).font(.caption).foregroundStyle(.secondary)
      }
      Spacer(minLength: 8)
      Button("Choose", action: action)
        .buttonStyle(.bordered)
        .disabled(!enabled)
        .accessibilityLabel("Choose \(key): \(title)")
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

private struct EditorialChoiceDRow: View {
  let enabled: Bool
  let action: () -> Void

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Text("D")
        .font(.caption.weight(.medium))
        .frame(width: 26, height: 26)
        .background(EditorialPalette.background, in: Circle())
        .overlay { Circle().stroke(EditorialPalette.border, lineWidth: 1) }
        .accessibilityHidden(true)
      VStack(alignment: .leading, spacing: 3) {
        Text("Something else").font(.headline)
        Text("Describe what these options missed.").font(.caption).foregroundStyle(.secondary)
      }
      Spacer(minLength: 8)
      Button("Choose", action: action)
        .buttonStyle(.bordered)
        .disabled(!enabled)
        .accessibilityLabel("Choose D: Something else")
        .accessibilityIdentifier("openopen-choice-d-submit")
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

private struct EditorialReminderProposal: View {
  let confirmation: ChoiceConsolidatedConfirmation
  let formattedDateTime: (ChoiceReminderItem) -> String
  let onEdit: () -> Void
  let onConfirm: () -> Void
  let enabled: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(spacing: 10) {
        Image(systemName: "calendar.badge.checkmark")
          .font(.title3)
          .accessibilityHidden(true)
        VStack(alignment: .leading, spacing: 2) {
          Text("Reminder proposal").font(.headline)
          Text("Review every detail before anything happens")
            .font(.caption).foregroundStyle(.secondary)
        }
        Spacer()
        Text("Ready to review")
          .font(.caption.weight(.medium))
          .foregroundStyle(.secondary)
      }
      Grid(alignment: .leading, horizontalSpacing: 20, verticalSpacing: 8) {
        GridRow {
          Text("Reminder").foregroundStyle(.secondary)
          VStack(alignment: .leading, spacing: 6) {
            ForEach(confirmation.reminderItems) { item in
              Text(item.text)
                .accessibilityIdentifier("openopen-choice-confirm-reminder-item-\(item.id)")
            }
          }
        }
        GridRow {
          Text("When").foregroundStyle(.secondary)
          VStack(alignment: .leading, spacing: 6) {
            ForEach(confirmation.reminderItems) { item in
              Text(formattedDateTime(item))
                .accessibilityIdentifier("openopen-choice-confirm-reminder-date-time-\(item.id)")
            }
          }
        }
        GridRow {
          Text("List").foregroundStyle(.secondary)
          Text("Reminders")
            .accessibilityIdentifier("openopen-choice-confirm-reminder-list")
        }
        GridRow {
          Text("Quantity").foregroundStyle(.secondary)
          Text(
            "\(confirmation.reminderCount) reminder\(confirmation.reminderCount == 1 ? "" : "s")"
          )
          .accessibilityIdentifier("openopen-choice-confirm-reminder-count")
        }
      }
      HStack {
        Button("Edit", action: onEdit)
          .buttonStyle(.bordered)
          .disabled(!enabled)
          .accessibilityIdentifier("openopen-choice-confirm-reminder-revise")
        Spacer()
        Button("Review and confirm", action: onConfirm)
          .buttonStyle(.borderedProminent)
          .disabled(!enabled)
          .accessibilityIdentifier("openopen-choice-confirm")
      }
    }
    .padding(14)
    .background(
      EditorialPalette.card,
      in: RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous)
    )
    .overlay {
      RoundedRectangle(cornerRadius: EditorialPalette.cornerRadius, style: .continuous).stroke(
        EditorialPalette.border, lineWidth: 1)
    }
  }
}

private struct SettingsView: View {
  @ObservedObject var model: AppModel
  @State private var showsDiscordTokenSheet = false

  var body: some View {
    VStack(spacing: 0) {
      EditorialPageHeader(
        eyebrow: "Settings",
        title: "Settings",
        detail: "Account, model, privacy, and recovery controls remain local and explicit."
      )
      .padding(.horizontal, 24)
      .padding(.vertical, 20)
      if let recoveryMessage = model.runtimeRecoveryMessage {
        RuntimeRecoveryBanner(
          message: recoveryMessage,
          isPaused: model.runtimeRecoveryState == .paused
        )
        .padding(.horizontal)
        .padding(.top, 12)
      }
      if let errorMessage = model.errorMessage {
        LocalOperationFeedbackPanel(message: errorMessage) {
          model.dismissError()
        }
        .padding(.horizontal)
        .padding(.top, 12)
      }
      TabView {
        account
          .tabItem { Label("Account", systemImage: "person.crop.circle") }
        models
          .tabItem { Label("Models", systemImage: "cpu") }
        connections
          .tabItem { Label("Connections", systemImage: "link") }
        honestEmpty(
          title: "No Skills installed",
          detail: "Only reviewed and explicitly promoted Skills will appear here."
        )
        .tabItem { Label("Skills", systemImage: "shippingbox") }
        privacy
          .tabItem { Label("Privacy", systemImage: "hand.raised") }
      }
      .padding()
    }
    .task { await model.refreshAccountAndModels() }
  }

  private var account: some View {
    VStack(spacing: 16) {
      Text("Account").font(.title.bold())
      switch model.accountState {
      case .notConnected:
        Text("Not connected. OpenOpen supports managed ChatGPT sign-in only.")
          .foregroundStyle(.secondary)
        Button("Connect ChatGPT") { Task { await model.connectChatGpt() } }
          .disabled(!model.accountSetupEnabled || model.isBusy)
      case .chatGpt(let email, let planType):
        Text(email.isEmpty ? "Connected to ChatGPT" : email)
        Text(planType).foregroundStyle(.secondary)
      }
      if !model.accountSetupEnabled {
        Text("OpenOpen must have a verified protected On runtime before connecting an account.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }

  private var models: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Models").font(.title.bold())
      if model.availableModels.isEmpty {
        ContentUnavailableView(
          "No verified models",
          systemImage: "cpu",
          description: Text("Connect ChatGPT while OpenOpen is on to load the verified catalog.")
        )
      } else {
        Picker(
          "Model",
          selection: Binding(
            get: { model.selectedModelId },
            set: { model.chooseModel($0) }
          )
        ) {
          Text("Choose a model").tag("")
          ForEach(model.availableModels) { candidate in
            Text(candidate.displayName).tag(candidate.id)
          }
        }
        .disabled(!model.accountSetupEnabled || model.isBusy)
        .accessibilityIdentifier("openopen-model-picker-model")

        if let selected = model.selectedCatalogModel {
          if selected.supportedReasoningEfforts.isEmpty {
            Text("This model has no configurable effort.")
              .font(.caption)
              .foregroundStyle(.secondary)
              .accessibilityIdentifier("openopen-model-picker-effort-not-applicable")
          } else {
            Picker(
              "Effort",
              selection: Binding(
                get: { model.selectedModelEffort },
                set: { model.chooseModelEffort($0) }
              )
            ) {
              Text("Choose an effort").tag("")
              ForEach(selected.supportedReasoningEfforts, id: \.self) { effort in
                Text(model.modelEffortLabel(effort)).tag(effort)
              }
            }
            .disabled(!model.accountSetupEnabled || model.isBusy)
            .accessibilityIdentifier("openopen-model-picker-effort")
          }
          Button("Save model selection") {
            Task { await model.persistSelectedModel() }
          }
          .disabled(!model.modelSelectionCanBeSaved)
          .accessibilityIdentifier("openopen-model-picker-save")
        }

        if model.modelSelectionStatus == .current {
          Text("A model and effort selection is saved for this account catalog.")
            .font(.caption)
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("openopen-model-picker-saved")
        } else if model.modelSelectionStatus == .unavailable {
          Text(
            "The saved model selection does not match the current account catalog. Choose again."
          )
          .font(.caption)
          .foregroundStyle(.secondary)
          .accessibilityIdentifier("openopen-model-picker-needs-selection")
        }

        List(model.availableModels) { model in
          VStack(alignment: .leading) {
            Text(model.displayName)
            Text(model.id).font(.caption).foregroundStyle(.secondary)
          }
        }
      }
    }
  }

  private var connections: some View {
    Group {
      if model.choiceCoreConnectionsAvailable {
        legacyConnections
      } else {
        VStack(alignment: .leading, spacing: 12) {
          Text("Connections").font(.title.bold())
          Text("Connection setup is unavailable while local Choice Core is active.")
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("openopen-choice-core-connections-unavailable")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
      }
    }
  }

  private var legacyConnections: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 20) {
        Text("Connections").font(.title.bold())
        GroupBox("iMessage — \(model.iMessageStatus)") {
          VStack(alignment: .leading, spacing: 10) {
            Text(
              "Choose your dedicated Messages self-chat. Every message in that chat is addressed to OpenOpen."
            )
            .foregroundStyle(.secondary)
            Button("Load Messages conversations") {
              Task { await model.refreshIMessageChats() }
            }
            .disabled(!model.modelEntryEnabled || model.isBusy || model.iMessageIsConnected)
            Picker(
              "Approved conversation",
              selection: Binding(
                get: { model.iMessageChatId },
                set: { model.selectIMessageChat($0) }
              )
            ) {
              Text("Choose a conversation").tag("")
              ForEach(model.iMessageChats) { chat in
                Text("\(chat.displayName) · \(chat.service)").tag(chat.chatId)
              }
            }
            .disabled(model.iMessageChats.isEmpty || model.iMessageIsConnected)
            Picker(
              "Approved owner",
              selection: Binding(
                get: { model.iMessageOwnerSender },
                set: { model.selectIMessageOwner($0) }
              )
            ) {
              Text("Choose the owner").tag("")
              ForEach(model.iMessageOwnerOptions, id: \.self) { participant in
                Text(participant).tag(participant)
              }
            }
            .disabled(model.iMessageOwnerOptions.isEmpty || model.iMessageIsConnected)
            Text(
              "macOS must grant Full Disk Access to read chat history and Messages Automation to send. OpenOpen does not change SIP or use private IMCore helpers."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            if model.iMessageIsConnected {
              Label(
                "Connected to the approved Messages conversation",
                systemImage: "checkmark.circle.fill"
              )
              .foregroundStyle(.green)
            } else {
              if let feedback = model.channelListenerFeedback[.iMessage] {
                Text(feedback)
                  .font(.caption)
                  .foregroundStyle(.orange)
              }
              Button(
                model.iMessageHasDurablePairing
                  ? "Reconnect approved iMessage" : "Pair & Connect iMessage"
              ) {
                Task { await model.connectIMessage() }
              }
              .disabled(!model.iMessageConnectionActionEnabled)
            }
          }
          .padding(4)
        }
        if model.discordSetupVisible {
          GroupBox("Discord — \(model.discordStatus)") {
            VStack(alignment: .leading, spacing: 10) {
              Text(
                "Use an official Bot Gateway token. V1 accepts one paired owner in one approved channel and requires @OpenOpen."
              )
              .foregroundStyle(.secondary)
              Button("Enter or replace bot token securely") {
                model.discardDiscordTokenDraft()
                showsDiscordTokenSheet = true
              }
              .accessibilityIdentifier(DiscordSecureEntryAccessibility.openButton)
              Text(
                "1. Create a bot in Discord's Developer Portal and enable Message Content. OpenOpen derives its IDs from the token; IDs cannot be typed manually."
              )
              .font(.caption)
              .foregroundStyle(.secondary)
              Button(
                model.discordSetup == nil
                  ? "Continue with saved token" : "Restart setup with saved token"
              ) {
                Task { await model.connectDiscord() }
              }
              .disabled(!model.discordConnectionActionEnabled)
              if let feedback = model.discordSetupFeedback {
                Label(
                  feedback,
                  systemImage: discordFeedbackIcon
                )
                .foregroundStyle(discordFeedbackColor)
                .font(.caption)
              }
              if let feedback = model.channelListenerFeedback[.discord] {
                Text(feedback)
                  .font(.caption)
                  .foregroundStyle(.orange)
              }
              if let setup = model.discordSetup {
                Text(
                  "2. Install \(setup.identity.botName) with exactly View Channel, Send Messages, Read Message History, and Attach Files."
                )
                .font(.caption)
                if let installURL = URL(string: setup.installUrl) {
                  Link("Open official Discord install page", destination: installURL)
                }
                Text("3. In the intended channel, the owner sends exactly:")
                  .font(.caption)
                Text(verbatim: setup.pairingInstruction)
                  .font(.system(.caption, design: .monospaced))
                  .textSelection(.enabled)
                Button("Check pairing message & permissions") {
                  Task { await model.checkDiscordPairingMessage() }
                }
                .disabled(!model.discordSetupCheckEnabled)
              }
              if let candidate = model.discordPairingCandidate {
                Text(
                  "Confirm owner \(candidate.ownerName) in #\(candidate.channelName), \(candidate.guildName). Live intents, permissions, and history readback passed."
                )
                .font(.caption)
                Button("Confirm this owner & channel") {
                  Task { await model.confirmDiscordPairing() }
                }
                .disabled(!model.discordSetupConfirmationEnabled)
              }
              Text(
                "Outbound Discord messages suppress all mentions. The token remains Keychain-only."
              )
              .font(.caption)
              .foregroundStyle(.secondary)
            }
            .padding(4)
          }
        }
        if let routeSet = model.channelRouteSet {
          GroupBox("Current Mission routes") {
            VStack(alignment: .leading, spacing: 10) {
              Text(
                "Each route is bound to this Mission only. Incoming channel messages can update participation, but only Reminders completion is completion Evidence."
              )
              .font(.caption)
              .foregroundStyle(.secondary)
              ForEach(routeSet.routes) { route in
                VStack(alignment: .leading, spacing: 3) {
                  Text(
                    "\(route.role == .primary ? "Primary" : "Additional") · \(route.channel.displayName)"
                  )
                  .font(.headline)
                  Text("Conversation: \(route.conversationId)")
                  Text("Owner: \(route.ownerSenderId)")
                  Text(
                    "Inbound: \(route.allowedInboundClasses.map(\.displayName).joined(separator: ", "))"
                  )
                  Text(
                    "Outbound: \(route.allowedOutboundClasses.isEmpty ? "Off" : route.allowedOutboundClasses.map(\.displayName).joined(separator: ", "))"
                  )
                  .foregroundStyle(route.allowedOutboundClasses.isEmpty ? .secondary : .primary)
                }
                .font(.caption)
                .padding(.vertical, 3)
              }
              HStack {
                Button("Prepare paired iMessage route") {
                  Task { await model.prepareAdditionalRoute(.iMessage) }
                }
                Button("Prepare paired Discord route") {
                  Task { await model.prepareAdditionalRoute(.discord) }
                }
              }
              .disabled(!model.modelEntryEnabled || model.isBusy)
              if let draft = model.pendingAdditionalRoute {
                Divider()
                Text("Confirm exact additional route").font(.headline)
                Text("Channel: \(draft.pairing.channel.displayName)")
                Text("Conversation: \(draft.pairing.conversationId)")
                Text("Owner: \(draft.pairing.ownerSenderId)")
                Text("Inbound: Mission updates, Need you responses")
                Text(
                  "Additional outbound classes default Off. Enable only the exact classes this conversation may receive."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                Toggle("Allow Need you", isOn: $model.routeAllowsNeedYou)
                Toggle("Allow progress", isOn: $model.routeAllowsProgress)
                Toggle("Allow Receipt", isOn: $model.routeAllowsReceipt)
                HStack {
                  Button("Cancel") { model.cancelAdditionalRoute() }
                  Button("Confirm & Bind Exact Route") {
                    Task { await model.confirmAdditionalRoute() }
                  }
                  .buttonStyle(.borderedProminent)
                  .disabled(model.isBusy || !model.modelEntryEnabled)
                }
              }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(4)
          }
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding()
    }
    .sheet(isPresented: $showsDiscordTokenSheet) {
      DiscordSecureEntrySheet(model: model, isPresented: $showsDiscordTokenSheet)
    }
  }

  private var discordFeedbackIcon: String {
    switch model.discordStatus {
    case "connected": "checkmark.circle.fill"
    case "connecting", "reconnecting": "clock.arrow.circlepath"
    default: "exclamationmark.triangle.fill"
    }
  }

  private var discordFeedbackColor: Color {
    switch model.discordStatus {
    case "connected": .green
    case "connecting", "reconnecting": .orange
    default: .red
    }
  }

  private var privacy: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Privacy").font(.title.bold())
      Toggle(
        "OpenOpen",
        isOn: Binding(
          get: { model.runtimeToggleValue },
          set: { model.requestEnabled($0) }
        )
      )
      .disabled(!model.dashboardControls.globalToggleEnabled)
      .accessibilityIdentifier("openopen-editorial-global-toggle")
      Text(
        "OpenOpen is off by default. Off stops model calls and cancels an active Codex operation without deleting local state."
      )
      Text(
        "Codex credentials stay in the macOS Keychain. Model input uses a short-lived, isolated local workspace."
      )
      if let persona = model.personaStatus {
        Divider()
        Text("Persona").font(.headline)
          .accessibilityIdentifier("openopen-persona-provenance-title")
        LabeledContent("Revision") {
          Text("\(persona.status.active.personaId) / \(persona.status.active.revision)")
            .textSelection(.enabled)
        }
        .accessibilityIdentifier("openopen-persona-provenance-revision")
        LabeledContent("Bundle digest") {
          Text(persona.status.active.aggregateDigest)
            .font(.caption.monospaced())
            .textSelection(.enabled)
        }
        .accessibilityIdentifier("openopen-persona-provenance-digest")
        if let warning = persona.status.warning {
          Label(warning, systemImage: "exclamationmark.triangle")
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("openopen-persona-provenance-warning")
        }
      }
      Spacer()
    }
  }

  private func honestEmpty(title: String, detail: String) -> some View {
    ContentUnavailableView(title, systemImage: "tray", description: Text(detail))
  }
}

private struct RuntimeRecoveryBanner: View {
  let message: String
  let isPaused: Bool

  var body: some View {
    Label(
      message,
      systemImage: isPaused ? "exclamationmark.triangle.fill" : "arrow.triangle.2.circlepath"
    )
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(10)
    .background(
      (isPaused ? Color.orange : Color.secondary).opacity(0.12),
      in: RoundedRectangle(cornerRadius: 8)
    )
    .foregroundStyle(isPaused ? .orange : .secondary)
    .accessibilityIdentifier("core-runtime-recovery-banner")
  }
}

private struct ChoiceLoopContinuityBanner: View {
  let message: String

  var body: some View {
    Label(message, systemImage: "arrow.triangle.2.circlepath")
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(10)
      .background(Color.secondary.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
      .foregroundStyle(.secondary)
      .accessibilityIdentifier("openopen-choice-continuity-needs-you")
  }
}

private struct LocalOperationFeedbackPanel: View {
  let message: String
  let dismiss: () -> Void

  var body: some View {
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: "exclamationmark.triangle.fill")
        .foregroundStyle(.orange)
      Text(message)
        .frame(maxWidth: .infinity, alignment: .leading)
      Button("Dismiss", action: dismiss)
        .accessibilityIdentifier("openopen-local-feedback-dismiss")
    }
    .padding(10)
    .background(Color.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
    .accessibilityIdentifier("openopen-local-feedback")
  }
}

private struct ChannelFailureIncidentPanel: View {
  @ObservedObject var model: AppModel

  var body: some View {
    GroupBox("Background activity") {
      VStack(alignment: .leading, spacing: 10) {
        ForEach(
          ChannelKind.allCases.filter { model.channelListenerFeedback[$0] != nil },
          id: \.self
        ) { channel in
          if let feedback = model.channelListenerFeedback[channel] {
            Text(feedback)
              .font(.caption)
              .foregroundStyle(.orange)
              .accessibilityIdentifier("channel-listener-feedback-\(channel.rawValue)")
          }
        }
        if let feedback = model.channelFailureFeedback {
          Text(feedback)
            .font(.caption)
            .foregroundStyle(.orange)
            .accessibilityIdentifier("channel-failure-feedback")
        }
        ForEach(model.channelFailureIncidents) { incident in
          HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
              .foregroundStyle(.orange)
            VStack(alignment: .leading, spacing: 4) {
              Text(
                "A \(incident.channel == .iMessage ? "Messages" : "Discord") request failed safely."
              )
              .font(.headline)
              Text(
                "It was not retried or sent again. The incident remains in local activity history."
              )
              .font(.caption)
              .foregroundStyle(.secondary)
              if let feedback = model.channelFailureAcknowledgementFeedback[incident.incidentId] {
                Text(feedback)
                  .font(.caption)
                  .foregroundStyle(.orange)
                  .accessibilityIdentifier("channel-failure-acknowledgement-feedback")
              }
              if incident.acknowledgement == nil {
                Button("Acknowledge") {
                  model.acknowledgeChannelFailure(incident.incidentId)
                }
                .disabled(!model.canAcknowledgeChannelFailure(incident))
                .accessibilityIdentifier("channel-failure-acknowledge")
              } else {
                Label("Acknowledged", systemImage: "checkmark.circle.fill")
                  .font(.caption)
                  .foregroundStyle(.secondary)
                  .accessibilityIdentifier("channel-failure-acknowledged")
              }
            }
          }
        }
        if model.channelFailureIncidents.count == 128 {
          Text(
            "OpenOpen is showing the current incident page. Acknowledging older items reveals any remaining durable history."
          )
          .font(.caption)
          .foregroundStyle(.secondary)
          .accessibilityIdentifier("channel-failure-page-boundary")
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(4)
    }
    .accessibilityIdentifier("channel-failure-panel")
  }
}

enum DiscordSecureEntryAccessibility {
  static let openButton = "discord-token-entry-open"
  static let sheet = "discord-token-entry-sheet"
  static let secureField = "discord-token-secure-field"
  static let cancelButton = "discord-token-entry-cancel"
  static let submitButton = "discord-token-entry-submit"
}

private struct DiscordSecureEntrySheet: View {
  @ObservedObject var model: AppModel
  @Binding var isPresented: Bool
  @FocusState private var tokenFieldFocused: Bool

  private var hasTokenDraft: Bool {
    !model.discordTokenDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      Text("Connect Discord securely")
        .font(.title2.bold())
      Text(
        "Paste the official Bot Gateway token here. OpenOpen saves it only in your Mac login Keychain and never shows it again."
      )
      .foregroundStyle(.secondary)
      SecureField("Bot token", text: $model.discordTokenDraft)
        .textFieldStyle(.roundedBorder)
        .focused($tokenFieldFocused)
        .accessibilityIdentifier(DiscordSecureEntryAccessibility.secureField)
        .onSubmit { submit() }
      HStack {
        Spacer()
        Button("Cancel") {
          model.discardDiscordTokenDraft()
          isPresented = false
        }
        .accessibilityIdentifier(DiscordSecureEntryAccessibility.cancelButton)
        Button(model.isBusy ? "Connecting…" : "Save & Start Official Setup") {
          submit()
        }
        .buttonStyle(.borderedProminent)
        .disabled(model.isBusy || !model.modelEntryEnabled || !hasTokenDraft)
        .accessibilityIdentifier(DiscordSecureEntryAccessibility.submitButton)
      }
    }
    .padding(24)
    .frame(minWidth: 520)
    .accessibilityIdentifier(DiscordSecureEntryAccessibility.sheet)
    .interactiveDismissDisabled(model.isBusy)
    .onAppear { tokenFieldFocused = true }
    .onDisappear { model.discardDiscordTokenDraft() }
  }

  private func submit() {
    guard model.modelEntryEnabled, !model.isBusy, hasTokenDraft else { return }
    Task {
      await model.connectDiscord()
      isPresented = false
    }
  }
}

extension ChannelKind {
  fileprivate var displayName: String {
    switch self {
    case .iMessage: "Messages"
    case .discord: "Discord"
    }
  }
}

extension ChannelInboundMessageClass {
  fileprivate var displayName: String {
    switch self {
    case .missionParticipation: "Mission updates"
    case .needYouResponse: "Need you responses"
    }
  }
}

extension ChannelMessageKind {
  fileprivate var displayName: String {
    switch self {
    case .needYou: "Need you"
    case .progress: "Progress"
    case .receipt: "Receipt"
    }
  }
}
