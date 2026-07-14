import Foundation

public struct PrivilegedBrokerClientBuilder {
  private let identityProvider: any CodeSigningIdentityProviding
  private let expectedHostSigningIdentifier: String
  private let brokerSigningIdentifier: String
  private let machServiceName: String

  public init(
    identityProvider: any CodeSigningIdentityProviding = SecurityCodeSigningIdentityProvider(),
    expectedHostSigningIdentifier: String = EffectBrokerConstants.hostSigningIdentifier,
    brokerSigningIdentifier: String = EffectBrokerConstants.brokerSigningIdentifier,
    machServiceName: String = EffectBrokerConstants.machServiceName
  ) {
    self.identityProvider = identityProvider
    self.expectedHostSigningIdentifier = expectedHostSigningIdentifier
    self.brokerSigningIdentifier = brokerSigningIdentifier
    self.machServiceName = machServiceName
  }

  public func makeActivatedConnection() throws -> NSXPCConnection {
    let identity = try identityProvider.currentIdentity()
    guard identity.signingIdentifier == expectedHostSigningIdentifier else {
      throw CodeSigningIdentityError.unexpectedSigningIdentifier(
        expected: expectedHostSigningIdentifier,
        actual: identity.signingIdentifier
      )
    }
    let brokerRequirement = try ExactCodeSigningRequirement.make(
      peerSigningIdentifier: brokerSigningIdentifier,
      teamIdentifier: identity.teamIdentifier
    )
    let connection = NSXPCConnection(
      machServiceName: machServiceName,
      options: .privileged
    )
    connection.remoteObjectInterface = NSXPCInterface(with: EffectBrokerXPCProtocol.self)
    connection.setCodeSigningRequirement(brokerRequirement)
    connection.activate()
    return connection
  }
}
