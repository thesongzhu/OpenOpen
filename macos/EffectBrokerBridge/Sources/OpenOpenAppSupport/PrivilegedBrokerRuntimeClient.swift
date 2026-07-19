import Darwin
import EffectBrokerBridge
import Foundation

public final class PrivilegedBrokerRuntimeClient: @unchecked Sendable, BrokerRuntimeServing {
  private let connectionBuilder: @Sendable () throws -> NSXPCConnection

  public init() {
    connectionBuilder = { try PrivilegedBrokerClientBuilder().makeActivatedConnection() }
  }

  init(connectionBuilder: @escaping @Sendable () throws -> NSXPCConnection) {
    self.connectionBuilder = connectionBuilder
  }

  public func provision(
    coreIdentity: CoreEffectIdentity
  ) async throws -> EnrolledBrokerTrustAnchor {
    let service = BrokerServiceController()
    let state = try service.registerIfNeeded()
    switch state {
    case .enabled:
      break
    case .requiresApproval:
      service.openLoginItemsSettings()
      throw CoreClientError.contractViolation(
        "Approve the OpenOpen effect broker in Login Items, then try again."
      )
    case .notRegistered:
      throw CoreClientError.contractViolation(
        "The OpenOpen effect broker could not be registered."
      )
    case .notFound:
      throw CoreClientError.contractViolation("The OpenOpen effect broker is missing.")
    }
    let coordinator = BrokerEnrollmentCoordinator()
    return try await withCheckedThrowingContinuation { continuation in
      let reply = ProvisionReply(continuation: continuation)
      DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + .seconds(30)) {
        reply.timeout()
      }
      coordinator.provisionAfterAdminApproval(coreIdentity: coreIdentity) { result in
        reply.finish(result.map(\.trustAnchor))
      }
    }
  }

  public func status(challenge: String) async throws -> BrokerRuntimeState? {
    let requestData = try JSONEncoder().encode(
      BrokerStatusRequest(type: "brokerStatus", version: 1, challenge: challenge)
    )
    let response = try await request(requestData) { proxy, data, reply in
      proxy.brokerStatus(data, withReply: reply)
    }
    let decoded = try JSONDecoder().decode(BrokerStatusResponse.self, from: response)
    guard decoded.version == 1, decoded.status == "ready" else {
      throw CoreClientError.contractViolation("The protected effect broker is unavailable.")
    }
    switch (decoded.runtimeControl, decoded.runtimeReceipt) {
    case (nil, nil): return nil
    case (let authorization?, let receipt?):
      return BrokerRuntimeState(authorization: authorization, receipt: receipt)
    default:
      throw CoreClientError.contractViolation("The protected broker state is incomplete.")
    }
  }

  public func prepareCodexRuntimeHome() async throws -> String {
    let encoded = try JSONEncoder().encode(
      PrepareCodexRuntimeHomeRequest(type: "prepareCodexRuntimeHome", version: 1)
    )
    let response = try await request(encoded) { proxy, data, reply in
      proxy.prepareCodexRuntimeHome(data, withReply: reply)
    }
    let decoded = try JSONDecoder().decode(PrepareCodexRuntimeHomeResponse.self, from: response)
    let expected =
      "/Library/Application Support/com.thesongzhu.OpenOpenRuntime/users/\(geteuid())/CodexHome"
    guard decoded.version == 1, decoded.status == "ready", decoded.runtimeDevice > 0,
      decoded.runtimeHome == expected
    else {
      throw CoreClientError.contractViolation(
        "The protected Codex runtime home is unavailable."
      )
    }
    return decoded.runtimeHome
  }

  public func acquireCoreLease(
    coreIdentity: CoreEffectIdentity, codexProcessIdentifier: Int32
  ) async throws -> Data {
    let encoded = try JSONEncoder().encode(
      CoreLeaseAcquireRequest(
        type: "coreLeaseAcquire",
        version: 1,
        corePid: coreIdentity.coreProcessIdentifier,
        codexPid: codexProcessIdentifier,
        coreInstanceNonce: coreIdentity.coreInstanceNonce
      )
    )
    let response = try await request(encoded) { proxy, data, reply in
      proxy.acquireCoreLease(data, withReply: reply)
    }
    let decoded = try JSONDecoder().decode(CoreLeaseAcquireResponse.self, from: response)
    guard decoded.version == 1, decoded.status == "accepted" else {
      throw CoreClientError.contractViolation(
        "Another exact OpenOpen Core/Codex process lease is still active."
      )
    }
    return try JSONEncoder().encode(decoded.lease)
  }

  public func apply(
    _ authorization: RuntimeControlAuthorization
  ) async throws -> RuntimeControlReceipt {
    let requestValue = ApplyRuntimeControlRequest(
      type: "applyRuntimeControl",
      version: 1,
      control: authorization
    )
    let encoded = try JSONEncoder().encode(requestValue)
    let response = try await request(encoded) { proxy, data, reply in
      proxy.applyRuntimeControl(data, withReply: reply)
    }
    let decoded = try JSONDecoder().decode(ApplyRuntimeControlResponse.self, from: response)
    guard decoded.version == 1, decoded.status == "accepted",
      decoded.runtimeControl == authorization
    else {
      throw CoreClientError.contractViolation(
        "The protected effect broker rejected the global switch transition."
      )
    }
    return decoded.runtimeReceipt
  }

  private func request(
    _ request: Data,
    invoke:
      @escaping @Sendable (
        EffectBrokerXPCProtocol, Data, @escaping (Data) -> Void
      ) -> Void
  ) async throws -> Data {
    guard request.count <= 256 * 1024 else { throw CoreClientError.oversizedRequest }
    let connection = try connectionBuilder()
    return try await withCheckedThrowingContinuation { continuation in
      let reply = BrokerReply(continuation: continuation, connection: connection)
      connection.interruptionHandler = { reply.fail() }
      connection.invalidationHandler = { reply.fail() }
      guard
        let proxy = connection.remoteObjectProxyWithErrorHandler({ _ in reply.fail() })
          as? EffectBrokerXPCProtocol
      else {
        reply.fail()
        return
      }
      DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + .seconds(15)) {
        reply.timeout()
      }
      invoke(proxy, request) { data in reply.succeed(data) }
    }
  }
}

private final class ProvisionReply: @unchecked Sendable {
  private let lock = NSLock()
  private var continuation: CheckedContinuation<EnrolledBrokerTrustAnchor, Error>?

  init(continuation: CheckedContinuation<EnrolledBrokerTrustAnchor, Error>) {
    self.continuation = continuation
  }

  func timeout() {
    finish(.failure(CoreClientError.requestTimedOut))
  }

  func finish(_ result: Result<EnrolledBrokerTrustAnchor, Error>) {
    let continuation = lock.withLock {
      () -> CheckedContinuation<EnrolledBrokerTrustAnchor, Error>? in
      defer { self.continuation = nil }
      return self.continuation
    }
    continuation?.resume(with: result)
  }
}

private struct ApplyRuntimeControlRequest: Encodable {
  let type: String
  let version: Int
  let control: RuntimeControlAuthorization
}

private struct PrepareCodexRuntimeHomeRequest: Encodable {
  let type: String
  let version: Int
}

private struct PrepareCodexRuntimeHomeResponse: Decodable {
  let runtimeHome: String
  let runtimeDevice: UInt64
  let status: String
  let version: Int
}

private struct CoreLeaseAcquireRequest: Encodable {
  let type: String
  let version: Int
  let corePid: Int32
  let codexPid: Int32
  let coreInstanceNonce: String
}

private struct CoreLeaseAcquireResponse: Decodable {
  let lease: CoreInstanceLease
  let status: String
  let version: Int
}

private struct BrokerStatusRequest: Encodable {
  let type: String
  let version: Int
  let challenge: String
}

private struct BrokerStatusResponse: Decodable {
  let runtimeControl: RuntimeControlAuthorization?
  let runtimeReceipt: RuntimeControlReceipt?
  let status: String
  let version: Int
}

private struct ApplyRuntimeControlResponse: Decodable {
  let runtimeControl: RuntimeControlAuthorization
  let runtimeReceipt: RuntimeControlReceipt
  let status: String
  let version: Int
}

private final class BrokerReply: @unchecked Sendable {
  private let lock = NSLock()
  private var continuation: CheckedContinuation<Data, Error>?
  private let connection: NSXPCConnection

  init(
    continuation: CheckedContinuation<Data, Error>,
    connection: NSXPCConnection
  ) {
    self.continuation = continuation
    self.connection = connection
  }

  func succeed(_ data: Data) {
    finish(.success(data))
  }

  func fail() {
    finish(.failure(CoreClientError.contractViolation("The protected broker disconnected.")))
  }

  func timeout() {
    finish(.failure(CoreClientError.requestTimedOut))
  }

  private func finish(_ result: Result<Data, Error>) {
    let continuation = lock.withLock { () -> CheckedContinuation<Data, Error>? in
      defer { self.continuation = nil }
      return self.continuation
    }
    guard let continuation else { return }
    connection.invalidate()
    continuation.resume(with: result)
  }
}
