import Foundation
import Testing

@testable import OpenOpenAppSupport

@Test
func discordPairingInstructionIsVerbatimUnderANumberGroupingLocale() {
  let botUserId: UInt64 = 1_472_345_678_901_234_567
  let pairingCode = "0123456789abcdef0123456789abcdef"
  let grouped = botUserId.formatted(.number.locale(Locale(identifier: "en_US")))
  let setup = DiscordSetupStart(
    identity: DiscordBotIdentity(
      botUserId: botUserId,
      applicationId: 1,
      botName: "OpenOpen"
    ),
    installUrl: "https://discord.com/api/oauth2/authorize",
    pairingCode: pairingCode,
    status: "connecting"
  )

  #expect(grouped.contains(","))
  #expect(
    setup.pairingInstruction
      == "<@1472345678901234567> pair 0123456789abcdef0123456789abcdef"
  )
  let mention = setup.pairingInstruction.split(separator: " ")[0]
  #expect(mention == "<@1472345678901234567>")
  #expect(
    !mention.unicodeScalars.contains { CharacterSet.whitespacesAndNewlines.contains($0) }
  )
  #expect(!setup.pairingInstruction.contains(","))
  #expect(!setup.pairingInstruction.contains("\n"))
  #expect(setup.pairingInstruction.split(separator: " ").count == 3)
}
