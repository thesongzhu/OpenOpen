import AppKit
import SwiftUI

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

  public init(model: AppModel) {
    self.model = model
  }

  public var body: some View {
    Group {
      if model.showsSettings {
        SettingsView(model: model)
      } else {
        DashboardView(model: model)
      }
    }
    .frame(minWidth: 720, minHeight: 520)
    .task { await model.refreshDashboard() }
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

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 20) {
        HStack {
          VStack(alignment: .leading, spacing: 4) {
            Text("OpenOpen")
              .font(.largeTitle.bold())
            Text("One useful outcome, kept within your boundaries.")
              .foregroundStyle(.secondary)
          }
          Spacer()
          Toggle(
            model.runtimeDisplayState.label,
            isOn: Binding(
              get: { model.runtimeToggleValue },
              set: { model.requestEnabled($0) }
            )
          )
          .toggleStyle(.switch)
          .disabled(!model.dashboardControls.globalToggleEnabled)
          .accessibilityIdentifier("openopen-dashboard-global-toggle")
          Text(model.runtimeDisplayState.label)
            .font(.caption)
            .foregroundStyle(.secondary)
          Button("Settings") { model.showsSettings = true }
            .disabled(!model.dashboardControls.settingsEnabled)
            .accessibilityIdentifier("openopen-dashboard-settings")
        }

        if let errorMessage = model.errorMessage {
          LocalOperationFeedbackPanel(message: errorMessage) {
            model.dismissError()
          }
        }

        if let recoveryMessage = model.runtimeRecoveryState.message {
          RuntimeRecoveryBanner(
            message: recoveryMessage,
            isPaused: model.runtimeRecoveryState == .paused
          )
        }

        HStack(spacing: 10) {
          Button {
          } label: {
            Label("Microphone", systemImage: "mic.fill")
          }
          .disabled(true)
          .help(model.microphone.reason)
          TextField("What outcome would help right now?", text: $model.prompt)
            .textFieldStyle(.roundedBorder)
            .onSubmit { Task { await model.submitPrompt() } }
            .disabled(!model.dashboardControls.outcomeInputEnabled)
            .accessibilityIdentifier("openopen-dashboard-outcome-input")
          Button(model.isBusy ? "Thinking…" : "Ask") {
            Task { await model.submitPrompt() }
          }
          .disabled(!model.dashboardControls.outcomeSubmitEnabled)
          .accessibilityIdentifier("openopen-dashboard-outcome-submit")
          .dashboardInteractionAnchor("openopen-dashboard-outcome-submit")
        }
        Text(model.microphone.reason)
          .font(.caption)
          .foregroundStyle(.secondary)

        if !model.channelFailureIncidents.isEmpty || model.channelFailureFeedback != nil
          || !model.channelListenerFeedback.isEmpty
        {
          ChannelFailureIncidentPanel(model: model)
        }

        if let suggestion = model.suggestion {
          GroupBox("I can help") {
            VStack(alignment: .leading, spacing: 8) {
              Text(suggestion.title).font(.headline)
              Text(suggestion.whyNow).foregroundStyle(.secondary)
              ForEach(Array(suggestion.proposedSteps.enumerated()), id: \.offset) { index, step in
                Text("\(index + 1). \(step)")
              }
              Text(
                "Confirming authorizes OpenOpen to create these exact items in its OpenOpen Reminders list."
              )
              .font(.caption)
              .foregroundStyle(.secondary)
              Button(
                model.confirmedMission == nil
                  ? "Confirm & Create Reminders" : "Retry Reminders setup"
              ) {
                model.requestSuggestionConfirmation()
              }
              .disabled(!model.dashboardControls.suggestionConfirmationEnabled)
              .accessibilityIdentifier("openopen-dashboard-confirm-mission")
              .dashboardInteractionAnchor("openopen-dashboard-confirm-mission")
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(4)
          }
        } else {
          ContentUnavailableView(
            "No suggestion yet",
            systemImage: "sparkles",
            description: Text(
              model.modelEntryEnabled
                ? "Ask for an outcome above. OpenOpen shows at most one suggestion."
                : "Turn OpenOpen on when you want it to make a model call."
            )
          )
        }

        if !model.activeCards.isEmpty {
          Text("Working on it").font(.title2.bold())
          ForEach(model.activeCards) { card in
            GroupBox(card.title) {
              VStack(alignment: .leading, spacing: 8) {
                Text(card.state)
                if model.confirmedMission?.missionId == card.id,
                  model.reminderLinks.isEmpty
                {
                  Button("Set up Reminders") {
                    model.requestSuggestionConfirmation()
                  }
                  .disabled(model.isBusy || !model.modelEntryEnabled)
                } else if model.confirmedMission?.missionId == card.id {
                  Button(
                    model.channelRouteSet == nil ? "Check progress" : "Finish & Return Receipt"
                  ) {
                    model.requestMissionProgressCheck()
                  }
                  .disabled(!model.dashboardControls.missionProgressEnabled)
                  .accessibilityIdentifier("openopen-dashboard-check-progress")
                  .dashboardInteractionAnchor("openopen-dashboard-check-progress")
                }
                Button("Stop this Mission") {
                  Task { await model.cancelMission(identifier: card.id) }
                }
                .disabled(!model.dashboardControls.missionCancellationEnabled)
                .accessibilityIdentifier("openopen-dashboard-stop-mission")
                Text(
                  "Stops this Mission without retrying or removing any Reminders. Its audit history remains."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                if model.channelRouteSet?.missionId == card.id {
                  Picker("Approved return route", selection: $model.selectedChannelRouteId) {
                    ForEach(model.outboundChannelRoutes) { route in
                      Text(
                        "\(route.channel.displayName) · \(route.conversationId) · \(route.ownerSenderId)"
                      )
                      .tag(route.routeId)
                    }
                  }
                  TextField("Exact progress message", text: $model.channelMessageDraft)
                    .textFieldStyle(.roundedBorder)
                  Text(
                    "Send requires a fresh confirmation for this exact recipient and these exact bytes."
                  )
                  .font(.caption)
                  .foregroundStyle(.secondary)
                  Button("Approve & Send Progress") {
                    Task { await model.sendChannelProgress() }
                  }
                  .disabled(
                    model.isBusy || !model.channelEffectEntryEnabled
                      || !model.selectedRouteAllowsProgress
                      || model.channelMessageDraft.trimmingCharacters(
                        in: .whitespacesAndNewlines
                      ).isEmpty
                  )
                }
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
              if model.channelRouteSet?.missionId == needsYou.missionId {
                Button("Approve & Send Need you") {
                  Task { await model.sendChannelNeedYou() }
                }
                .disabled(
                  model.isBusy || !model.channelEffectEntryEnabled
                    || !model.selectedRouteAllowsNeedYou
                )
              }
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
        if model.dashboardControls.doneVisible, let receipt = model.receipt {
          GroupBox("Done") {
            VStack(alignment: .leading, spacing: 8) {
              Text(receipt.summary).font(.headline)
              Text("Evidence: \(receipt.evidenceIds.count) Reminder completion(s)")
                .foregroundStyle(.secondary)
              Text("Model: \(receipt.actualModel)")
                .font(.caption)
                .foregroundStyle(.secondary)
              if model.channelRouteSet?.missionId == receipt.missionId {
                Button("Return Receipt") {
                  Task { await model.sendChannelReceipt() }
                }
                .disabled(
                  model.isBusy || !model.channelEffectEntryEnabled
                    || !model.selectedRouteAllowsReceipt
                )
              }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(4)
          }
          .accessibilityIdentifier("openopen-dashboard-done")
          .dashboardInteractionAnchor("openopen-dashboard-done")
        }
        Spacer()
      }
      .padding(24)
    }
  }
}

private struct SettingsView: View {
  @ObservedObject var model: AppModel
  @State private var showsDiscordTokenSheet = false

  var body: some View {
    VStack(spacing: 0) {
      HStack {
        Button {
          model.showsSettings = false
        } label: {
          Label("Dashboard", systemImage: "chevron.left")
        }
        Spacer()
        Text("Settings").font(.title2.bold())
        Spacer()
        Color.clear.frame(width: 90, height: 1)
      }
      .padding()
      Divider()
      if let recoveryMessage = model.runtimeRecoveryState.message {
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
    ScrollView {
      VStack(alignment: .leading, spacing: 20) {
        Text("Connections").font(.title.bold())
        GroupBox("iMessage — \(model.iMessageStatus)") {
          VStack(alignment: .leading, spacing: 10) {
            Text(
              "Choose one Messages conversation and its approved owner. Inbound work must begin with @OpenOpen."
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
      Text(
        "OpenOpen is off by default. Off stops model calls and cancels an active Codex operation without deleting local state."
      )
      Text(
        "Codex credentials stay in the macOS Keychain. Model input uses a short-lived, isolated local workspace."
      )
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
