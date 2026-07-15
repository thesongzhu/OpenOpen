import SwiftUI

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
    .alert(
      "OpenOpen",
      isPresented: Binding(
        get: { model.errorMessage != nil },
        set: { if !$0 { model.dismissError() } }
      )
    ) {
      Button("OK") { model.dismissError() }
    } message: {
      Text(model.errorMessage ?? "")
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
        get: { model.enabled },
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
            get: { model.enabled },
            set: { model.requestEnabled($0) }
          )
        )
        .toggleStyle(.switch)
        Text(model.runtimeDisplayState.label)
          .font(.caption)
          .foregroundStyle(.secondary)
        Button("Settings") { model.showsSettings = true }
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
          .disabled(
            !model.modelEntryEnabled || model.isBusy || model.confirmedMission != nil
          )
        Button(model.isBusy ? "Thinking…" : "Ask") {
          Task { await model.submitPrompt() }
        }
        .disabled(
          !model.modelEntryEnabled || model.isBusy || model.confirmedMission != nil
            || model.prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || model.prompt.utf8.count > 16 * 1024
        )
      }
      Text(model.microphone.reason)
        .font(.caption)
        .foregroundStyle(.secondary)

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
            .disabled(model.isBusy || !model.modelEntryEnabled)
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
              if model.reminderLinks.isEmpty, model.confirmedMission != nil {
                Button("Set up Reminders") {
                  model.requestSuggestionConfirmation()
                }
                .disabled(model.isBusy || !model.modelEntryEnabled)
              } else if model.confirmedMission != nil {
                Button(model.channelOrigin == nil ? "Check progress" : "Finish & Return Receipt") {
                  model.requestMissionProgressCheck()
                }
                .disabled(model.isBusy || !model.modelEntryEnabled)
              }
              if model.channelOrigin != nil {
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
                  model.isBusy || !model.modelEntryEnabled
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
            if model.channelOrigin != nil {
              Button("Approve & Send Need you") {
                Task { await model.sendChannelNeedYou() }
              }
              .disabled(model.isBusy || !model.modelEntryEnabled)
            }
          }
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(4)
        }
      }
      if let receipt = model.receipt {
        GroupBox("Done") {
          VStack(alignment: .leading, spacing: 8) {
            Text(receipt.summary).font(.headline)
            Text("Evidence: \(receipt.evidenceIds.count) Reminder completion(s)")
              .foregroundStyle(.secondary)
            Text("Model: \(receipt.actualModel)")
              .font(.caption)
              .foregroundStyle(.secondary)
            if model.channelOrigin != nil {
              Button("Return Receipt") {
                Task { await model.sendChannelReceipt() }
              }
              .disabled(model.isBusy || !model.modelEntryEnabled)
            }
          }
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(4)
        }
      }
      Spacer()
    }
    .padding(24)
  }
}

private struct SettingsView: View {
  @ObservedObject var model: AppModel

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
          .disabled(!model.modelEntryEnabled || model.isBusy)
      case .chatGpt(let email, let planType):
        Text(email.isEmpty ? "Connected to ChatGPT" : email)
        Text(planType).foregroundStyle(.secondary)
      }
      if !model.modelEntryEnabled {
        Text("OpenOpen must be confirmed On before connecting an account.")
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
            .disabled(!model.modelEntryEnabled || model.isBusy)
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
            Picker("Approved owner", selection: $model.iMessageOwnerSender) {
              Text("Choose the owner").tag("")
              ForEach(model.iMessageOwnerOptions, id: \.self) { participant in
                Text(participant).tag(participant)
              }
            }
            .disabled(model.iMessageOwnerOptions.isEmpty)
            Text(
              "macOS must grant Full Disk Access to read chat history and Messages Automation to send. OpenOpen does not change SIP or use private IMCore helpers."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            Button("Pair & Connect iMessage") { Task { await model.connectIMessage() } }
              .disabled(!model.modelEntryEnabled || model.isBusy)
          }
          .padding(4)
        }
        GroupBox("Discord — \(model.discordStatus)") {
          VStack(alignment: .leading, spacing: 10) {
            Text(
              "Use an official Bot Gateway token. V1 accepts one paired owner in one approved channel and requires @OpenOpen."
            )
            .foregroundStyle(.secondary)
            SecureField("Bot token (stored only in Keychain)", text: $model.discordTokenDraft)
              .textFieldStyle(.roundedBorder)
            Text(
              "1. Create a bot in Discord's Developer Portal and enable Message Content. OpenOpen derives its IDs from the token; IDs cannot be typed manually."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            Button(model.discordSetup == nil ? "Start official setup" : "Restart setup") {
              Task { await model.connectDiscord() }
            }
            .disabled(!model.modelEntryEnabled || model.isBusy)
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
              Text("<@\(setup.identity.botUserId)> pair \(setup.pairingCode)")
                .font(.system(.caption, design: .monospaced))
                .textSelection(.enabled)
              Button("Check pairing message & permissions") {
                Task { await model.checkDiscordPairingMessage() }
              }
              .disabled(model.isBusy)
            }
            if let candidate = model.discordPairingCandidate {
              Text(
                "Confirm owner \(candidate.ownerName) in #\(candidate.channelName), \(candidate.guildName). Live intents, permissions, and history readback passed."
              )
              .font(.caption)
              Button("Confirm this owner & channel") {
                Task { await model.confirmDiscordPairing() }
              }
              .disabled(model.isBusy)
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
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding()
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
