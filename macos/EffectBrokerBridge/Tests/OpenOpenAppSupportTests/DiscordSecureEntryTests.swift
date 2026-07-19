import Testing

@testable import OpenOpenAppSupport

@Test
func discordSecureEntryAccessibilityContractIsStableAndUnambiguous() {
  let identifiers = [
    DiscordSecureEntryAccessibility.openButton,
    DiscordSecureEntryAccessibility.sheet,
    DiscordSecureEntryAccessibility.secureField,
    DiscordSecureEntryAccessibility.cancelButton,
    DiscordSecureEntryAccessibility.submitButton,
  ]

  #expect(Set(identifiers).count == identifiers.count)
  #expect(identifiers.allSatisfy { !$0.isEmpty })
  #expect(DiscordSecureEntryAccessibility.secureField == "discord-token-secure-field")
}

@MainActor
@Test
func discardingDiscordSecureEntryErasesTheEphemeralDraft() {
  let model = AppModel()
  model.discordTokenDraft = "test-only-sensitive-discord-token"

  model.discardDiscordTokenDraft()

  #expect(model.discordTokenDraft.isEmpty)
}
