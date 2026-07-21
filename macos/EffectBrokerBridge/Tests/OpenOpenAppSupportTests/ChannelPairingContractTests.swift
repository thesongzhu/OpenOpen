import Foundation
import Testing

@testable import OpenOpenAppSupport

private func decodePairing(_ json: String) throws -> ChannelPairing {
  try JSONDecoder().decode(ChannelPairing.self, from: Data(json.utf8))
}

@Test
func historicalIMessagePairingRemainsReadableButUnprepared() throws {
  let pairing = try decodePairing(
    """
    {"channel":"iMessage","ownerSenderId":"owner@example.invalid",\
    "conversationId":"3","requireExplicitAddress":true,"imessage":null,\
    "discord":null,"pairedAtMs":1784258588299}
    """)
  #expect(throws: Never.self) {
    _ = try pairing.validated(expectedChannel: .iMessage)
  }
  #expect(pairing.imessage == nil)
}

@Test
func currentIMessagePairingRequiresExactSelfChatIdentity() throws {
  let valid = try decodePairing(
    """
    {"channel":"iMessage","ownerSenderId":"owner@example.invalid",\
    "conversationId":"3","requireExplicitAddress":false,\
    "imessage":{"chatGuid":"iMessage;-;owner@example.invalid",\
    "chatIdentifier":"owner@example.invalid","service":"iMessage",\
    "participantIds":["owner@example.invalid"]},"discord":null,"pairedAtMs":10}
    """)
  #expect(throws: Never.self) {
    _ = try valid.validated(expectedChannel: .iMessage)
  }

  let mismatchedParticipant = try decodePairing(
    """
    {"channel":"iMessage","ownerSenderId":"owner@example.invalid",\
    "conversationId":"3","requireExplicitAddress":false,\
    "imessage":{"chatGuid":"iMessage;-;owner@example.invalid",\
    "chatIdentifier":"owner@example.invalid","service":"iMessage",\
    "participantIds":["other@example.invalid"]},"discord":null,"pairedAtMs":10}
    """)
  #expect(throws: (any Error).self) {
    _ = try mismatchedParticipant.validated(expectedChannel: .iMessage)
  }
}
