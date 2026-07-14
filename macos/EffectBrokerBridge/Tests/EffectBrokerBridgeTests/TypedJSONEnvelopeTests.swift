import Foundation
import XCTest

@testable import EffectBrokerBridge

final class TypedJSONEnvelopeTests: XCTestCase {
  func testAcceptsOnlyMatchingBoundedTypedObjects() {
    XCTAssertTrue(
      TypedJSONEnvelope.accepts(
        Data(#"{"type":"session","version":1}"#.utf8),
        kind: .session
      )
    )
    XCTAssertFalse(
      TypedJSONEnvelope.accepts(
        Data(#"{"type":"brokerStatus","version":1}"#.utf8),
        kind: .session
      )
    )
    XCTAssertFalse(
      TypedJSONEnvelope.accepts(Data("[]".utf8), kind: .session)
    )
    XCTAssertFalse(
      TypedJSONEnvelope.accepts(Data("not-json".utf8), kind: .session)
    )
    XCTAssertFalse(
      TypedJSONEnvelope.accepts(
        Data(#"{"type":"session","version":2}"#.utf8),
        kind: .session
      )
    )
  }

  func testRejectsUnknownOperationAndAuthorityAliases() {
    for json in [
      #"{"command":"/bin/sh","type":"session","version":1}"#,
      #"{"destination":"/etc/hosts","type":"session","version":1}"#,
      #"{"padding":"ignored","type":"brokerStatus","version":1}"#,
    ] {
      let kind: BrokerRequestKind =
        json.contains("brokerStatus") ? .brokerStatus : .session
      XCTAssertFalse(TypedJSONEnvelope.accepts(Data(json.utf8), kind: kind))
    }
  }

  func testRejectsDuplicateKeysAndUnrepresentableNumbers() {
    XCTAssertFalse(
      TypedJSONEnvelope.accepts(
        Data(#"{"type":"session","type":"brokerStatus","version":1}"#.utf8),
        kind: .session
      )
    )
    var request = exactPutFileRequest()
    var permit = request["permit"] as! [String: Any]
    permit["expiresAtMs"] = 1e100
    request["permit"] = permit
    let encoded = try! JSONSerialization.data(withJSONObject: request)
    XCTAssertFalse(TypedJSONEnvelope.accepts(encoded, kind: .putMissionFile))
  }

  func testAcceptsExactPutFilePermitAndRejectsNestedUnknownKeys() throws {
    let permit = exactPutFileRequest()
    let encoded = try JSONSerialization.data(withJSONObject: permit, options: [.sortedKeys])
    XCTAssertTrue(TypedJSONEnvelope.accepts(encoded, kind: .putMissionFile))

    var changed = permit
    var nestedPermit = try XCTUnwrap(changed["permit"] as? [String: Any])
    var command = try XCTUnwrap(nestedPermit["command"] as? [String: Any])
    var effect = try XCTUnwrap(command["effect"] as? [String: Any])
    effect["destination"] = "/etc/hosts"
    command["effect"] = effect
    nestedPermit["command"] = command
    changed["permit"] = nestedPermit
    let changedData = try JSONSerialization.data(withJSONObject: changed)
    XCTAssertFalse(TypedJSONEnvelope.accepts(changedData, kind: .putMissionFile))
  }

  func testReconcileRouteAcceptsOnlyReconcilePurpose() throws {
    var request = exactPutFileRequest()
    request["type"] = "reconcileMissionFile"
    var permit = try XCTUnwrap(request["permit"] as? [String: Any])
    permit["purpose"] = "reconcile"
    request["permit"] = permit
    let encoded = try JSONSerialization.data(withJSONObject: request)
    XCTAssertTrue(
      TypedJSONEnvelope.accepts(encoded, kind: .reconcileMissionFile)
    )
    XCTAssertFalse(TypedJSONEnvelope.accepts(encoded, kind: .putMissionFile))

    permit["purpose"] = "execute"
    request["permit"] = permit
    let execute = try JSONSerialization.data(withJSONObject: request)
    XCTAssertFalse(
      TypedJSONEnvelope.accepts(execute, kind: .reconcileMissionFile)
    )
  }

  func testPermitBoundsMatchRustProtocolContract() throws {
    var request = exactPutFileRequest()
    var permit = try XCTUnwrap(request["permit"] as? [String: Any])
    var command = try XCTUnwrap(permit["command"] as? [String: Any])
    command["approvalIds"] = ["approval_with_underscore"]
    permit["command"] = command
    request["permit"] = permit
    XCTAssertFalse(acceptsPut(request))

    request = exactPutFileRequest()
    permit = try XCTUnwrap(request["permit"] as? [String: Any])
    permit["purpose"] = "arbitraryWrite"
    request["permit"] = permit
    XCTAssertFalse(acceptsPut(request))

    request = exactPutFileRequest()
    permit = try XCTUnwrap(request["permit"] as? [String: Any])
    permit["purpose"] = "reattestOnly"
    request["permit"] = permit
    XCTAssertTrue(acceptsPut(request))

    request = exactPutFileRequest()
    permit = try XCTUnwrap(request["permit"] as? [String: Any])
    command = try XCTUnwrap(permit["command"] as? [String: Any])
    command["approvalIds"] = ["approval-1", "approval-1"]
    permit["command"] = command
    request["permit"] = permit
    XCTAssertFalse(acceptsPut(request))

    request = exactPutFileRequest()
    permit = try XCTUnwrap(request["permit"] as? [String: Any])
    permit["issuedAtMs"] = NSNumber(value: UInt64(Int64.max) + 1)
    request["permit"] = permit
    XCTAssertFalse(acceptsPut(request))

    request = exactPutFileRequest()
    permit = try XCTUnwrap(request["permit"] as? [String: Any])
    command = try XCTUnwrap(permit["command"] as? [String: Any])
    var sourceAnchor = try XCTUnwrap(command["sourceAnchor"] as? [String: Any])
    sourceAnchor["sequence"] = 0
    command["sourceAnchor"] = sourceAnchor
    permit["command"] = command
    request["permit"] = permit
    XCTAssertFalse(acceptsPut(request))
  }

  func testRejectsCallerSuppliedAuthorityAtAnyDepth() {
    for json in [
      #"{"type":"session","uid":501,"version":1}"#,
      #"{"type":"session","UID":501,"version":1}"#,
      #"{"type":"putMissionFile","permit":{"missionsRoot":"/tmp"},"version":1}"#,
      #"{"type":"putMissionFile","permit":{"target":{"absolutePath":"/etc"}},"version":1}"#,
    ] {
      let kind: BrokerRequestKind =
        json.contains("putMissionFile")
        ? .putMissionFile
        : .session
      XCTAssertFalse(TypedJSONEnvelope.accepts(Data(json.utf8), kind: kind))
    }
  }

  func testRejectsOversizedEnvelope() {
    let padding = String(repeating: "a", count: TypedJSONEnvelope.maximumBytes)
    let data = Data(
      #"{"padding":"\#(padding)","type":"session","version":1}"#.utf8
    )
    XCTAssertFalse(TypedJSONEnvelope.accepts(data, kind: .session))
  }

  func testConnectionAdapterDoesNotForwardInvalidTypedJSON() throws {
    let backend = FailIfCalledBackend()
    let service = BrokerConnectionService(
      peer: AuthenticatedBrokerPeer(
        effectiveUserIdentifier: 501,
        processIdentifier: 42,
        auditSessionIdentifier: 7
      ),
      backend: backend
    )
    let expectation = expectation(description: "invalid request rejected")
    service.session(Data("[]".utf8)) { response in
      let object = try? JSONSerialization.jsonObject(with: response) as? [String: Any]
      let error = object?["error"] as? [String: Any]
      XCTAssertEqual(object?["status"] as? String, "rejected")
      XCTAssertEqual(object?["version"] as? Int, 1)
      XCTAssertEqual(error?["code"] as? String, "invalidTypedJSON")
      XCTAssertEqual(
        error?["message"] as? String,
        "Request must match the exact typed schema without caller-supplied authority"
      )
      expectation.fulfill()
    }
    wait(for: [expectation], timeout: 1)
  }

  private func exactPutFileRequest() -> [String: Any] {
    [
      "type": "putMissionFile",
      "version": 1,
      "permit": [
        "authorizationAnchor": [
          "entryHash": String(repeating: "9", count: 64),
          "sequence": 8,
          "signatureHex": String(repeating: "8", count: 128),
        ],
        "authorizationSignatureHex": String(repeating: "a", count: 128),
        "brokerSessionNonce": String(repeating: "b", count: 64),
        "command": [
          "approvalIds": ["approval-1"],
          "effect": [
            "actionDigest": String(repeating: "c", count: 64),
            "pathComponents": ["output.xlsx"],
            "payload": [
              "byteLen": 42,
              "sha256": String(repeating: "d", count: 64),
            ],
            "type": "putFile",
          ],
          "effectId": "effect-1",
          "missionId": "mission-1",
          "missionScopeDigest": "scope-v1",
          "missionUpdatedAtMs": 100,
          "protocolVersion": 1,
          "sourceAnchor": [
            "entryHash": String(repeating: "e", count: 64),
            "sequence": 7,
            "signatureHex": String(repeating: "f", count: 128),
          ],
        ],
        "coreKeyId": String(repeating: "1", count: 64),
        "expiresAtMs": 130,
        "issuedAtMs": 100,
        "purpose": "execute",
        "stableEffectHash": String(repeating: "2", count: 64),
      ],
    ]
  }

  private func acceptsPut(_ request: [String: Any]) -> Bool {
    guard let data = try? JSONSerialization.data(withJSONObject: request) else {
      return false
    }
    return TypedJSONEnvelope.accepts(data, kind: .putMissionFile)
  }
}

private final class FailIfCalledBackend: EffectBrokerBackend {
  func brokerStatus(
    peer _: AuthenticatedBrokerPeer,
    requestJSON _: Data,
    reply _: @escaping (Data) -> Void
  ) {
    XCTFail("invalid DTO reached brokerStatus backend")
  }

  func session(
    peer _: AuthenticatedBrokerPeer,
    requestJSON _: Data,
    reply _: @escaping (Data) -> Void
  ) {
    XCTFail("invalid DTO reached session backend")
  }

  func enrollCore(
    peer _: AuthenticatedBrokerPeer,
    requestJSON _: Data,
    reply _: @escaping (Data) -> Void
  ) {
    XCTFail("invalid DTO reached enrollCore backend")
  }

  func putMissionFile(
    peer _: AuthenticatedBrokerPeer,
    permitJSON _: Data,
    payload _: FileHandle,
    reply _: @escaping (Data) -> Void
  ) {
    XCTFail("invalid DTO reached putMissionFile backend")
  }

  func reconcileMissionFile(
    peer _: AuthenticatedBrokerPeer,
    permitJSON _: Data,
    reply _: @escaping (Data) -> Void
  ) {
    XCTFail("invalid DTO reached reconcileMissionFile backend")
  }
}
