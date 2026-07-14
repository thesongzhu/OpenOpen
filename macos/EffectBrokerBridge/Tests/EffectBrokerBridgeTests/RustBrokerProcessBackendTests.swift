import Foundation
import XCTest

@testable import EffectBrokerBridge

final class RustBrokerProcessBackendTests: XCTestCase {
  func testPayloadReadWithoutEOFOrBytesTimesOut() throws {
    let pipe = Pipe()
    let deadline = try MonotonicDeadline(millisecondsFromNow: 100)
    XCTAssertThrowsError(
      try DeadlineIO.read(
        from: pipe.fileHandleForReading,
        maximumCount: 64,
        idleTimeoutMilliseconds: 20,
        deadline: deadline
      )
    ) { error in
      XCTAssertEqual(error as? BrokerWorkerError, .workerTimedOut)
    }
    try pipe.fileHandleForWriting.close()
    try pipe.fileHandleForReading.close()
  }

  func testBackendMapsOnlyConnectionDerivedEUIDAndTypedOperation() throws {
    let expected = Data(#"{"status":"ready","version":1}"#.utf8)
    let runner = CapturingWorkerRunner(result: .success(expected))
    let backend = RustBrokerProcessBackend(runner: runner)
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 501,
      processIdentifier: 42,
      auditSessionIdentifier: 7
    )
    let request = Data(#"{"type":"session","version":1}"#.utf8)

    var response: Data?
    backend.session(peer: peer, requestJSON: request) { response = $0 }

    XCTAssertEqual(response, expected)
    XCTAssertEqual(runner.calls.count, 1)
    XCTAssertEqual(runner.calls[0].operation, .session)
    XCTAssertEqual(runner.calls[0].auditEUID, 501)
    XCTAssertNil(runner.calls[0].requestJSON)
    XCTAssertFalse(runner.calls[0].hasPayload)
  }

  func testBackendForwardsCanonicalPutAndFailsClosedOnRunnerError() {
    let runner = CapturingWorkerRunner(result: .failure(TestWorkerError.failed))
    let backend = RustBrokerProcessBackend(runner: runner)
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 502,
      processIdentifier: 43,
      auditSessionIdentifier: 8
    )
    let request = Data(#"{"permit":{},"type":"putMissionFile","version":1}"#.utf8)

    var response: Data?
    backend.putMissionFile(
      peer: peer,
      permitJSON: request,
      payload: .nullDevice
    ) { response = $0 }

    let object =
      try? JSONSerialization.jsonObject(with: response ?? Data())
      as? [String: Any]
    let error = object?["error"] as? [String: Any]
    XCTAssertEqual(object?["status"] as? String, "rejected")
    XCTAssertEqual(error?["code"] as? String, "brokerBackendUnavailable")
    XCTAssertEqual(runner.calls[0].operation, .put)
    XCTAssertEqual(runner.calls[0].auditEUID, 502)
    XCTAssertEqual(runner.calls[0].requestJSON, request)
    XCTAssertTrue(runner.calls[0].hasPayload)
  }

  func testBackendForwardsReconciliationWithoutPayload() {
    let expected = Data(#"{"outcome":"notCommitted"}"#.utf8)
    let runner = CapturingWorkerRunner(result: .success(expected))
    let backend = RustBrokerProcessBackend(runner: runner)
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 503,
      processIdentifier: 44,
      auditSessionIdentifier: 9
    )
    let request = Data(
      #"{"permit":{},"type":"reconcileMissionFile","version":1}"#.utf8
    )
    var response: Data?
    backend.reconcileMissionFile(peer: peer, permitJSON: request) { response = $0 }
    XCTAssertEqual(response, expected)
    XCTAssertEqual(runner.calls[0].operation, .reconcile)
    XCTAssertEqual(runner.calls[0].auditEUID, 503)
    XCTAssertEqual(runner.calls[0].requestJSON, request)
    XCTAssertFalse(runner.calls[0].hasPayload)
  }
}

private enum TestWorkerError: Error {
  case failed
}

private final class CapturingWorkerRunner: BrokerWorkerRunning {
  struct Call {
    let operation: BrokerWorkerOperation
    let auditEUID: uid_t
    let requestJSON: Data?
    let hasPayload: Bool
  }

  let result: Result<Data, Error>
  private(set) var calls = [Call]()

  init(result: Result<Data, Error>) {
    self.result = result
  }

  func run(
    operation: BrokerWorkerOperation,
    auditEUID: uid_t,
    requestJSON: Data?,
    payload: FileHandle?
  ) throws -> Data {
    calls.append(
      Call(
        operation: operation,
        auditEUID: auditEUID,
        requestJSON: requestJSON,
        hasPayload: payload != nil
      )
    )
    return try result.get()
  }
}
