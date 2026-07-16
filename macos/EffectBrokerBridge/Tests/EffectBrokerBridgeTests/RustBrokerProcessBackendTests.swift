import Foundation
import XCTest

@testable import EffectBrokerBridge

final class RustBrokerProcessBackendTests: XCTestCase {
  func testBundledWorkerUsesTheKernelProcessPathAndExactAppLayout() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(
      "OpenOpen-Broker-Path-\(UUID().uuidString)", isDirectory: true
    )
    defer { try? FileManager.default.removeItem(at: root) }
    let macOS = root.appendingPathComponent(
      "OpenOpen.app/Contents/MacOS", isDirectory: true
    )
    try FileManager.default.createDirectory(at: macOS, withIntermediateDirectories: true)
    let broker = macOS.appendingPathComponent("OpenOpenEffectBroker")
    let worker = macOS.appendingPathComponent("OpenOpenEffectBrokerWorker")
    XCTAssertTrue(FileManager.default.createFile(atPath: broker.path, contents: Data()))
    XCTAssertTrue(FileManager.default.createFile(atPath: worker.path, contents: Data()))
    let pid = getpid()
    let inspector = SequencedProcessInspector(
      identities: [
        pid: BrokerProcessIdentity(
          pid: pid,
          parentPID: getppid(),
          effectiveUserIdentifier: geteuid(),
          processGroupIdentifier: getpgid(pid),
          startTimeMicroseconds: 1,
          executableURL: broker
        )
      ],
      liveness: []
    )

    XCTAssertEqual(
      try SignedBrokerWorkerRunner.bundledWorkerURL(processInspector: inspector),
      worker
    )
    XCTAssertThrowsError(
      try SignedBrokerWorkerRunner.siblingWorkerURL(
        forBrokerExecutableURL: root.appendingPathComponent("OpenOpenEffectBroker")
      )
    ) { error in
      XCTAssertEqual(error as? BrokerWorkerError, .invalidWorkerBundleLayout)
    }
  }

  func testWorkerReaperUsesOnlyTheExactAuditToken() {
    let inspector = SequencedProcessInspector(identities: [:], liveness: [true, false])
    let completion = DispatchSemaphore(value: 1)
    let token = String(repeating: "a", count: 64)
    XCTAssertTrue(
      AuditTokenProcessReaper(processInspector: inspector).terminateAndConfirm(
        token, completion: completion, waitDeadline: .now()
      )
    )
    XCTAssertEqual(inspector.terminatedTokens, [token])
  }

  func testWorkerAuthorityRejectsImmediateExitAndPIDReuseWithoutTermination() {
    let pid = pid_t(77)
    let expectedURL = URL(fileURLWithPath: "/Library/OpenOpen/OpenOpenEffectBrokerWorker")
    let replacementInspector = SequencedProcessInspector(
      identities: [
        pid: BrokerProcessIdentity(
          pid: pid,
          parentPID: 1,
          effectiveUserIdentifier: 777,
          processGroupIdentifier: pid,
          startTimeMicroseconds: 7_700,
          executableURL: URL(fileURLWithPath: "/tmp/unrelated")
        )
      ],
      liveness: [true]
    )
    XCTAssertNil(
      stableWorkerAuditToken(
        for: pid,
        expectedParentPID: getpid(),
        expectedEffectiveUserIdentifier: 0,
        expectedExecutableURL: expectedURL,
        processInspector: replacementInspector,
        terminationObserved: { false }
      )
    )
    XCTAssertTrue(replacementInspector.terminatedTokens.isEmpty)

    let exactInspector = SequencedProcessInspector(
      identities: [
        pid: BrokerProcessIdentity(
          pid: pid,
          parentPID: getpid(),
          effectiveUserIdentifier: 0,
          processGroupIdentifier: pid,
          startTimeMicroseconds: 7_700,
          executableURL: expectedURL
        )
      ],
      liveness: [true]
    )
    var terminationChecks = [false, true]
    XCTAssertNil(
      stableWorkerAuditToken(
        for: pid,
        expectedParentPID: getpid(),
        expectedEffectiveUserIdentifier: 0,
        expectedExecutableURL: expectedURL,
        processInspector: exactInspector,
        terminationObserved: { terminationChecks.removeFirst() }
      )
    )
    XCTAssertTrue(exactInspector.terminatedTokens.isEmpty)
  }

  func testProductionBrokerContainsNoNumericSignalFallback() throws {
    let source = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .appendingPathComponent("Sources/EffectBrokerBridge/RustBrokerProcessBackend.swift")
    let text = try String(contentsOf: source, encoding: .utf8)
    XCTAssertFalse(text.contains("Darwin.kill("))
    XCTAssertFalse(text.contains("process.terminate()"))
  }

  func testAuditTokenTerminatesTheExactProcessIncarnation() throws {
    let child = Process()
    child.executableURL = URL(fileURLWithPath: "/bin/sleep")
    child.arguments = ["30"]
    try child.run()
    defer {
      if child.isRunning { child.terminate() }
      child.waitUntilExit()
    }
    let inspector = DarwinBrokerProcessInspector()
    let token = try XCTUnwrap(inspector.auditTokenHex(for: child.processIdentifier))
    XCTAssertTrue(inspector.isAlive(auditTokenHex: token))
    XCTAssertTrue(inspector.terminate(auditTokenHex: token))
    child.waitUntilExit()
    XCTAssertFalse(inspector.isAlive(auditTokenHex: token))
  }

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

  func testCoreLeaseDerivesEveryAuthorityFieldAndForwardsOnlyValidatedChild() throws {
    let lease = coreLeaseJSON(appPID: 42, corePID: 43, nonce: String(repeating: "a", count: 64))
    let runner = LeaseWorkerRunner(
      status: Data(#"{"lease":null,"status":"ready","version":1}"#.utf8),
      acquire: lease
    )
    let appURL = URL(fileURLWithPath: "/Applications/OpenOpen.app/Contents/MacOS/OpenOpen")
    let coreURL = appURL.deletingLastPathComponent().appendingPathComponent("OpenOpenCore")
    let inspector = MockProcessInspector(identities: [
      42: BrokerProcessIdentity(
        pid: 42, parentPID: 1, effectiveUserIdentifier: 501,
        processGroupIdentifier: 7, startTimeMicroseconds: 4_200,
        executableURL: appURL
      ),
      43: BrokerProcessIdentity(
        pid: 43, parentPID: 42, effectiveUserIdentifier: 501,
        processGroupIdentifier: 43, startTimeMicroseconds: 4_300,
        executableURL: coreURL
      ),
      44: BrokerProcessIdentity(
        pid: 44, parentPID: 43, effectiveUserIdentifier: 501,
        processGroupIdentifier: 44, startTimeMicroseconds: 4_400,
        executableURL: coreURL.deletingLastPathComponent().appendingPathComponent("codex")
      ),
    ])
    let backend = RustBrokerProcessBackend(
      runner: runner,
      processInspector: inspector,
      coreBundleValidator: AllowingCoreBundleValidator()
    )
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 501,
      processIdentifier: 42,
      auditSessionIdentifier: 7
    )
    let caller = Data(
      #"{"codexPid":44,"coreInstanceNonce":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","corePid":43,"type":"coreLeaseAcquire","version":1}"#
        .utf8
    )
    var response: Data?
    backend.acquireCoreLease(peer: peer, requestJSON: caller) { response = $0 }
    XCTAssertEqual(response, lease)
    XCTAssertEqual(runner.calls.map(\.operation), [.coreLeaseStatus, .coreLeaseAcquire])
    let forwarded = try XCTUnwrap(runner.calls.last?.requestJSON)
    let object = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: forwarded) as? [String: Any]
    )
    XCTAssertEqual(object["appPid"] as? Int, 42)
    XCTAssertEqual(object["appStartTimeUs"] as? Int, 4_200)
    XCTAssertEqual(object["corePid"] as? Int, 43)
    XCTAssertEqual(object["coreStartTimeUs"] as? Int, 4_300)
    XCTAssertEqual(object["codexPid"] as? Int, 44)
    XCTAssertEqual(object["codexStartTimeUs"] as? Int, 4_400)
    XCTAssertNil(object["auditEuid"])
  }

  func testCoreLeaseRejectsPIDReuseAcrossTheAuditTokenSnapshot() throws {
    let runner = LeaseWorkerRunner(
      status: Data(#"{"lease":null,"status":"ready","version":1}"#.utf8),
      acquire: Data()
    )
    let appURL = URL(fileURLWithPath: "/Applications/OpenOpen.app/Contents/MacOS/OpenOpen")
    let coreURL = appURL.deletingLastPathComponent().appendingPathComponent("OpenOpenCore")
    let inspector = MockProcessInspector(
      identities: [
        42: BrokerProcessIdentity(
          pid: 42, parentPID: 1, effectiveUserIdentifier: 501,
          processGroupIdentifier: 7, startTimeMicroseconds: 4_200,
          executableURL: appURL
        ),
        43: BrokerProcessIdentity(
          pid: 43, parentPID: 42, effectiveUserIdentifier: 501,
          processGroupIdentifier: 43, startTimeMicroseconds: 4_300,
          executableURL: coreURL
        ),
        44: BrokerProcessIdentity(
          pid: 44, parentPID: 43, effectiveUserIdentifier: 501,
          processGroupIdentifier: 44, startTimeMicroseconds: 4_400,
          executableURL: coreURL.deletingLastPathComponent().appendingPathComponent("codex")
        ),
      ],
      auditTokens: [
        43: [String(repeating: "a", count: 64), String(repeating: "a", count: 64)],
        44: [String(repeating: "b", count: 64), String(repeating: "c", count: 64)],
      ]
    )
    let backend = RustBrokerProcessBackend(
      runner: runner,
      processInspector: inspector,
      coreBundleValidator: AllowingCoreBundleValidator()
    )
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 501, processIdentifier: 42, auditSessionIdentifier: 7
    )
    let request = Data(
      #"{"codexPid":44,"coreInstanceNonce":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","corePid":43,"type":"coreLeaseAcquire","version":1}"#
        .utf8
    )
    var response: Data?
    backend.acquireCoreLease(peer: peer, requestJSON: request) { response = $0 }
    let object = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: XCTUnwrap(response)) as? [String: Any]
    )
    XCTAssertEqual(object["status"] as? String, "rejected")
    XCTAssertTrue(runner.calls.isEmpty)
  }

  func testCoreLeaseRetiresOnlyAfterBothExactAuditTokensAreDead() {
    let old = coreLeaseJSON(
      appPID: 88, corePID: 99, nonce: String(repeating: "d", count: 64), status: "ready"
    )
    let acquired = coreLeaseJSON(
      appPID: 42, corePID: 43, nonce: String(repeating: "a", count: 64)
    )
    let runner = LeaseWorkerRunner(status: old, acquire: acquired)
    let appURL = URL(fileURLWithPath: "/Applications/OpenOpen.app/Contents/MacOS/OpenOpen")
    let coreURL = appURL.deletingLastPathComponent().appendingPathComponent("OpenOpenCore")
    let inspector = SequencedProcessInspector(
      identities: [
        42: BrokerProcessIdentity(
          pid: 42, parentPID: 1, effectiveUserIdentifier: 501,
          processGroupIdentifier: 7, startTimeMicroseconds: 4_200,
          executableURL: appURL
        ),
        43: BrokerProcessIdentity(
          pid: 43, parentPID: 42, effectiveUserIdentifier: 501,
          processGroupIdentifier: 43, startTimeMicroseconds: 4_300,
          executableURL: coreURL
        ),
        44: BrokerProcessIdentity(
          pid: 44, parentPID: 43, effectiveUserIdentifier: 501,
          processGroupIdentifier: 44, startTimeMicroseconds: 4_400,
          executableURL: coreURL.deletingLastPathComponent().appendingPathComponent("codex")
        ),
      ],
      liveness: [true, false, false, true, false, false, true, true]
    )
    let backend = RustBrokerProcessBackend(
      runner: runner,
      processInspector: inspector,
      coreBundleValidator: AllowingCoreBundleValidator()
    )
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 501, processIdentifier: 42, auditSessionIdentifier: 7
    )
    let caller = Data(
      #"{"codexPid":44,"coreInstanceNonce":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","corePid":43,"type":"coreLeaseAcquire","version":1}"#
        .utf8
    )
    var response: Data?
    backend.acquireCoreLease(peer: peer, requestJSON: caller) { response = $0 }
    XCTAssertEqual(response, acquired)
    XCTAssertEqual(
      runner.calls.map(\.operation),
      [.coreLeaseStatus, .coreLeaseRelease, .coreLeaseAcquire]
    )
    XCTAssertEqual(
      inspector.terminatedTokens,
      [String(format: "%064x", 100), String(format: "%064x", 99)]
    )
  }

  func testReusedCorePIDIsNeverTargetedAndDoesNotWedgeTheOldLease() {
    let old = coreLeaseJSON(
      appPID: 88, corePID: 99, nonce: String(repeating: "d", count: 64), status: "ready"
    )
    let acquired = coreLeaseJSON(
      appPID: 42, corePID: 43, nonce: String(repeating: "a", count: 64)
    )
    let runner = LeaseWorkerRunner(status: old, acquire: acquired)
    let appURL = URL(fileURLWithPath: "/Applications/OpenOpen.app/Contents/MacOS/OpenOpen")
    let coreURL = appURL.deletingLastPathComponent().appendingPathComponent("OpenOpenCore")
    let inspector = SequencedProcessInspector(
      identities: [
        42: BrokerProcessIdentity(
          pid: 42, parentPID: 1, effectiveUserIdentifier: 501,
          processGroupIdentifier: 7, startTimeMicroseconds: 4_200,
          executableURL: appURL
        ),
        43: BrokerProcessIdentity(
          pid: 43, parentPID: 42, effectiveUserIdentifier: 501,
          processGroupIdentifier: 43, startTimeMicroseconds: 4_300,
          executableURL: coreURL
        ),
        44: BrokerProcessIdentity(
          pid: 44, parentPID: 43, effectiveUserIdentifier: 501,
          processGroupIdentifier: 44, startTimeMicroseconds: 4_400,
          executableURL: coreURL.deletingLastPathComponent().appendingPathComponent("codex")
        ),
        99: BrokerProcessIdentity(
          pid: 99, parentPID: 1, effectiveUserIdentifier: 777,
          processGroupIdentifier: 99, startTimeMicroseconds: 9_900,
          executableURL: URL(fileURLWithPath: "/usr/bin/unrelated")
        ),
      ],
      liveness: [false, false, false, false, true, true]
    )
    let backend = RustBrokerProcessBackend(
      runner: runner,
      processInspector: inspector,
      coreBundleValidator: AllowingCoreBundleValidator()
    )
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 501, processIdentifier: 42, auditSessionIdentifier: 7
    )
    let caller = Data(
      #"{"codexPid":44,"coreInstanceNonce":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","corePid":43,"type":"coreLeaseAcquire","version":1}"#
        .utf8
    )
    var response: Data?
    backend.acquireCoreLease(peer: peer, requestJSON: caller) { response = $0 }
    XCTAssertEqual(response, acquired)
    XCTAssertEqual(
      runner.calls.map(\.operation),
      [.coreLeaseStatus, .coreLeaseRelease, .coreLeaseAcquire]
    )
    XCTAssertTrue(inspector.terminatedTokens.isEmpty)
  }

  func testOffTerminatesExactAuditTokensBeforePersistenceAndThenReleasesLease() {
    let lease = coreLeaseJSON(
      appPID: 42, corePID: 43, nonce: String(repeating: "a", count: 64), status: "ready"
    )
    let accepted = Data(#"{"status":"accepted","version":1}"#.utf8)
    let appURL = URL(fileURLWithPath: "/Applications/OpenOpen.app/Contents/MacOS/OpenOpen")
    let coreURL = appURL.deletingLastPathComponent().appendingPathComponent("OpenOpenCore")
    let inspector = SequencedProcessInspector(
      identities: [
        42: BrokerProcessIdentity(
          pid: 42, parentPID: 1, effectiveUserIdentifier: 501,
          processGroupIdentifier: 7, startTimeMicroseconds: 4_200,
          executableURL: appURL
        ),
        43: BrokerProcessIdentity(
          pid: 43, parentPID: 42, effectiveUserIdentifier: 501,
          processGroupIdentifier: 43, startTimeMicroseconds: 4_300,
          executableURL: coreURL
        ),
      ],
      liveness: [true, false, false, true, false, false]
    )
    let runner = LeaseWorkerRunner(status: lease, acquire: accepted) {
      XCTAssertEqual(inspector.terminatedTokens.count, 2)
    }
    let backend = RustBrokerProcessBackend(
      runner: runner,
      processInspector: inspector,
      coreBundleValidator: AllowingCoreBundleValidator()
    )
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 501, processIdentifier: 42, auditSessionIdentifier: 7
    )
    let request = Data(
      """
      {"control":{"authorizationSignatureHex":"\(String(repeating: "a", count: 128))","coreKeyId":"\(String(repeating: "b", count: 64))","enabled":false,"protocolVersion":1,"revision":2,"updatedAtMs":10},"type":"applyRuntimeControl","version":1}
      """.utf8
    )
    var response: Data?
    backend.applyRuntimeControl(peer: peer, requestJSON: request) { response = $0 }
    XCTAssertEqual(response, accepted)
    XCTAssertEqual(
      runner.calls.map(\.operation),
      [.coreLeaseStatus, .runtimeControl, .coreLeaseRelease]
    )
    XCTAssertEqual(
      inspector.terminatedTokens,
      [String(format: "%064x", 44), String(format: "%064x", 43)]
    )
  }

  func testOffNeverPersistsWhenExactAuditTokenTerminationFails() throws {
    let lease = coreLeaseJSON(
      appPID: 42, corePID: 43, nonce: String(repeating: "a", count: 64), status: "ready"
    )
    let accepted = Data(#"{"status":"accepted","version":1}"#.utf8)
    let runner = LeaseWorkerRunner(status: lease, acquire: accepted)
    let appURL = URL(fileURLWithPath: "/Applications/OpenOpen.app/Contents/MacOS/OpenOpen")
    let coreURL = appURL.deletingLastPathComponent().appendingPathComponent("OpenOpenCore")
    let inspector = SequencedProcessInspector(
      identities: [
        43: BrokerProcessIdentity(
          pid: 43, parentPID: 42, effectiveUserIdentifier: 501,
          processGroupIdentifier: 43, startTimeMicroseconds: 4_300,
          executableURL: coreURL
        )
      ],
      liveness: [true],
      terminationResult: false
    )
    let backend = RustBrokerProcessBackend(
      runner: runner,
      processInspector: inspector,
      coreBundleValidator: AllowingCoreBundleValidator()
    )
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 501, processIdentifier: 42, auditSessionIdentifier: 7
    )
    let request = Data(
      """
      {"control":{"authorizationSignatureHex":"\(String(repeating: "a", count: 128))","coreKeyId":"\(String(repeating: "b", count: 64))","enabled":false,"protocolVersion":1,"revision":2,"updatedAtMs":10},"type":"applyRuntimeControl","version":1}
      """.utf8
    )
    var response: Data?
    backend.applyRuntimeControl(peer: peer, requestJSON: request) { response = $0 }
    let object = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: XCTUnwrap(response)) as? [String: Any]
    )
    XCTAssertEqual(object["status"] as? String, "rejected")
    XCTAssertEqual(runner.calls.map(\.operation), [.coreLeaseStatus])
    XCTAssertEqual(inspector.terminatedTokens.count, 1)
  }

  func testOnRejectsWhenAnExactLeasedProcessIncarnationIsGone() throws {
    let lease = coreLeaseJSON(
      appPID: 42, corePID: 43, nonce: String(repeating: "a", count: 64), status: "ready"
    )
    let runner = LeaseWorkerRunner(status: lease, acquire: Data())
    let appURL = URL(fileURLWithPath: "/Applications/OpenOpen.app/Contents/MacOS/OpenOpen")
    let inspector = SequencedProcessInspector(
      identities: [
        42: BrokerProcessIdentity(
          pid: 42, parentPID: 1, effectiveUserIdentifier: 501,
          processGroupIdentifier: 7, startTimeMicroseconds: 4_200,
          executableURL: appURL
        )
      ],
      liveness: [true]
    )
    let backend = RustBrokerProcessBackend(
      runner: runner,
      processInspector: inspector,
      coreBundleValidator: AllowingCoreBundleValidator()
    )
    let peer = AuthenticatedBrokerPeer(
      effectiveUserIdentifier: 501, processIdentifier: 42, auditSessionIdentifier: 7
    )
    let request = Data(
      """
      {"control":{"authorizationSignatureHex":"\(String(repeating: "a", count: 128))","coreKeyId":"\(String(repeating: "b", count: 64))","enabled":true,"protocolVersion":1,"revision":2,"updatedAtMs":10},"type":"applyRuntimeControl","version":1}
      """.utf8
    )
    var response: Data?
    backend.applyRuntimeControl(peer: peer, requestJSON: request) { response = $0 }
    let object = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: XCTUnwrap(response)) as? [String: Any]
    )
    XCTAssertEqual(object["status"] as? String, "rejected")
    XCTAssertEqual(runner.calls.map(\.operation), [.coreLeaseStatus])
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

private final class LeaseWorkerRunner: BrokerWorkerRunning {
  struct Call {
    let operation: BrokerWorkerOperation
    let requestJSON: Data?
  }
  let status: Data
  let acquire: Data
  let onRuntimeControl: () -> Void
  private(set) var calls = [Call]()

  init(status: Data, acquire: Data, onRuntimeControl: @escaping () -> Void = {}) {
    self.status = status
    self.acquire = acquire
    self.onRuntimeControl = onRuntimeControl
  }

  func run(
    operation: BrokerWorkerOperation,
    auditEUID _: uid_t,
    requestJSON: Data?,
    payload _: FileHandle?
  ) throws -> Data {
    calls.append(Call(operation: operation, requestJSON: requestJSON))
    switch operation {
    case .coreLeaseStatus: return status
    case .coreLeaseAcquire: return acquire
    case .coreLeaseRelease: return Data(#"{"status":"released","version":1}"#.utf8)
    case .runtimeControl:
      onRuntimeControl()
      return acquire
    default: throw TestWorkerError.failed
    }
  }
}

private final class SequencedProcessInspector: BrokerProcessInspecting {
  let identities: [pid_t: BrokerProcessIdentity]
  var liveness: [Bool]
  let terminationResult: Bool
  private(set) var terminatedTokens = [String]()

  init(
    identities: [pid_t: BrokerProcessIdentity], liveness: [Bool],
    terminationResult: Bool = true
  ) {
    self.identities = identities
    self.liveness = liveness
    self.terminationResult = terminationResult
  }

  func identity(for pid: pid_t) -> BrokerProcessIdentity? { identities[pid] }
  func auditTokenHex(for pid: pid_t) -> String? { String(format: "%064x", pid) }
  func isAlive(auditTokenHex _: String) -> Bool {
    liveness.isEmpty ? false : liveness.removeFirst()
  }
  func terminate(auditTokenHex: String) -> Bool {
    terminatedTokens.append(auditTokenHex)
    return terminationResult
  }
}

private final class MockProcessInspector: BrokerProcessInspecting {
  let identities: [pid_t: BrokerProcessIdentity]
  var auditTokens: [pid_t: [String]]
  init(
    identities: [pid_t: BrokerProcessIdentity], auditTokens: [pid_t: [String]] = [:]
  ) {
    self.identities = identities
    self.auditTokens = auditTokens
  }
  func identity(for pid: pid_t) -> BrokerProcessIdentity? { identities[pid] }
  func auditTokenHex(for pid: pid_t) -> String? {
    guard var tokens = auditTokens[pid], !tokens.isEmpty else {
      return String(format: "%064x", pid)
    }
    let token = tokens.removeFirst()
    auditTokens[pid] = tokens
    return token
  }
  func isAlive(auditTokenHex _: String) -> Bool { true }
  func terminate(auditTokenHex _: String) -> Bool { true }
}

private struct AllowingCoreBundleValidator: CoreBundleValidating {
  func validate(
    appExecutableURL _: URL,
    coreExecutableURL _: URL,
    coreAuditTokenHex _: String,
    codexExecutableURL _: URL,
    codexAuditTokenHex _: String
  ) -> Bool {
    true
  }
}

private func coreLeaseJSON(
  appPID: Int32,
  corePID: Int32,
  nonce: String,
  status: String = "accepted"
) -> Data {
  let coreAuditToken = String(format: "%064x", corePID)
  let codexPID = corePID + 1
  let codexAuditToken = String(format: "%064x", codexPID)
  return Data(
    """
    {"lease":{"appPid":\(appPID),"appStartTimeUs":4200,"auditEuid":501,"brokerKeyId":"\(String(repeating: "b", count: 64))","brokerSignatureHex":"\(String(repeating: "c", count: 128))","codexAuditTokenHex":"\(codexAuditToken)","codexPid":\(codexPID),"codexStartTimeUs":4400,"coreAuditTokenHex":"\(coreAuditToken)","coreInstanceNonce":"\(nonce)","corePid":\(corePID),"coreStartTimeUs":4300,"issuedAtMs":5000,"protocolVersion":1},"status":"\(status)","version":1}
    """.utf8
  )
}
