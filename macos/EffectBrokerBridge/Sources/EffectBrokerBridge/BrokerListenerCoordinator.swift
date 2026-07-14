import Foundation

public final class BrokerListenerCoordinator: NSObject, NSXPCListenerDelegate {
  private let listener: NSXPCListener
  private let backend: any EffectBrokerBackend

  public init(
    backend: any EffectBrokerBackend,
    identityProvider: any CodeSigningIdentityProviding = SecurityCodeSigningIdentityProvider(),
    expectedBrokerSigningIdentifier: String = EffectBrokerConstants.brokerSigningIdentifier,
    hostSigningIdentifier: String = EffectBrokerConstants.hostSigningIdentifier,
    machServiceName: String = EffectBrokerConstants.machServiceName
  ) throws {
    let identity = try identityProvider.currentIdentity()
    guard identity.signingIdentifier == expectedBrokerSigningIdentifier else {
      throw CodeSigningIdentityError.unexpectedSigningIdentifier(
        expected: expectedBrokerSigningIdentifier,
        actual: identity.signingIdentifier
      )
    }
    let hostRequirement = try ExactCodeSigningRequirement.make(
      peerSigningIdentifier: hostSigningIdentifier,
      teamIdentifier: identity.teamIdentifier
    )
    let listener = NSXPCListener(machServiceName: machServiceName)
    listener.setConnectionCodeSigningRequirement(hostRequirement)
    self.listener = listener
    self.backend = backend
    super.init()
    listener.delegate = self
  }

  public func activate() {
    listener.activate()
  }

  public func invalidate() {
    listener.invalidate()
  }

  public func listener(
    _: NSXPCListener,
    shouldAcceptNewConnection newConnection: NSXPCConnection
  ) -> Bool {
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: newConnection.effectiveUserIdentifier,
      processIdentifier: newConnection.processIdentifier,
      auditSessionIdentifier: newConnection.auditSessionIdentifier
    )
    let service = BrokerConnectionService(peer: peer, backend: backend)
    newConnection.exportedInterface = NSXPCInterface(with: EffectBrokerXPCProtocol.self)
    newConnection.exportedObject = service
    newConnection.activate()
    return true
  }
}
