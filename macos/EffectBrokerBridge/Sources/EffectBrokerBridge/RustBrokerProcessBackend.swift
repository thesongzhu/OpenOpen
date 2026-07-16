import Darwin
import Foundation

enum BrokerWorkerOperation: String {
  case status
  case session
  case enrollCore = "enroll-core"
  case coreLeaseStatus = "core-lease-status"
  case coreLeaseAcquire = "core-lease-acquire"
  case coreLeaseRelease = "core-lease-release"
  case runtimeControl = "runtime-control"
  case put
  case reconcile
}

struct BrokerProcessIdentity: Equatable {
  let pid: pid_t
  let parentPID: pid_t
  let effectiveUserIdentifier: uid_t
  let processGroupIdentifier: pid_t
  let startTimeMicroseconds: UInt64
  let executableURL: URL
}

private struct BrokerAuditedProcess {
  let identity: BrokerProcessIdentity
  let auditTokenHex: String
}

func stableWorkerAuditToken(
  for pid: pid_t,
  expectedParentPID: pid_t,
  expectedEffectiveUserIdentifier: uid_t,
  expectedExecutableURL: URL,
  processInspector: any BrokerProcessInspecting,
  terminationObserved: () -> Bool
) -> String? {
  guard pid > 0, !terminationObserved(),
    let before = processInspector.auditTokenHex(for: pid),
    let identity = processInspector.identity(for: pid),
    let after = processInspector.auditTokenHex(for: pid),
    !terminationObserved(), before == after,
    identity.pid == pid,
    identity.parentPID == expectedParentPID,
    identity.effectiveUserIdentifier == expectedEffectiveUserIdentifier,
    identity.startTimeMicroseconds > 0,
    identity.executableURL.resolvingSymlinksInPath().standardizedFileURL
      == expectedExecutableURL.resolvingSymlinksInPath().standardizedFileURL,
    processInspector.isAlive(auditTokenHex: before)
  else { return nil }
  return before
}

protocol BrokerProcessInspecting {
  func identity(for pid: pid_t) -> BrokerProcessIdentity?
  func auditTokenHex(for pid: pid_t) -> String?
  func isAlive(auditTokenHex: String) -> Bool
  func terminate(auditTokenHex: String) -> Bool
}

struct DarwinBrokerProcessInspector: BrokerProcessInspecting {
  func identity(for pid: pid_t) -> BrokerProcessIdentity? {
    guard pid > 0 else { return nil }
    var info = proc_bsdinfo()
    let count = proc_pidinfo(
      pid,
      PROC_PIDTBSDINFO,
      0,
      &info,
      Int32(MemoryLayout<proc_bsdinfo>.size)
    )
    guard count == Int32(MemoryLayout<proc_bsdinfo>.size) else { return nil }
    let seconds = UInt64(info.pbi_start_tvsec)
    let microseconds = UInt64(info.pbi_start_tvusec)
    guard seconds <= (UInt64.max - microseconds) / 1_000_000 else { return nil }
    var path = [CChar](repeating: 0, count: Int(MAXPATHLEN) * 4)
    guard proc_pidpath(pid, &path, UInt32(path.count)) > 0 else { return nil }
    let pgid = Darwin.getpgid(pid)
    guard pgid > 0 else { return nil }
    let executablePath = path.withUnsafeBufferPointer { buffer in
      buffer.baseAddress!.withMemoryRebound(to: UInt8.self, capacity: buffer.count) {
        String(decodingCString: $0, as: UTF8.self)
      }
    }
    return BrokerProcessIdentity(
      pid: pid,
      parentPID: pid_t(info.pbi_ppid),
      effectiveUserIdentifier: uid_t(info.pbi_uid),
      processGroupIdentifier: pgid,
      startTimeMicroseconds: seconds * 1_000_000 + microseconds,
      executableURL: URL(fileURLWithPath: executablePath).standardizedFileURL
    )
  }

  func auditTokenHex(for pid: pid_t) -> String? {
    guard pid > 0 else { return nil }
    var task = mach_port_name_t(MACH_PORT_NULL)
    guard task_name_for_pid(mach_task_self_, pid, &task) == KERN_SUCCESS else { return nil }
    defer { mach_port_deallocate(mach_task_self_, task) }
    var token = audit_token_t()
    let expectedCount = mach_msg_type_number_t(
      MemoryLayout<audit_token_t>.size / MemoryLayout<natural_t>.size
    )
    var count = expectedCount
    let result = withUnsafeMutablePointer(to: &token) { pointer in
      pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { words in
        task_info(task, task_flavor_t(TASK_AUDIT_TOKEN), words, &count)
      }
    }
    guard result == KERN_SUCCESS, count == expectedCount else { return nil }
    return withUnsafeBytes(of: token) { bytes in
      bytes.map { String(format: "%02x", $0) }.joined()
    }
  }

  func isAlive(auditTokenHex: String) -> Bool {
    withAuditToken(auditTokenHex) { token in
      guard let current = self.auditTokenHex(for: audit_token_to_pid(token)) else {
        return false
      }
      return current.caseInsensitiveCompare(auditTokenHex) == .orderedSame
    } ?? false
  }

  func terminate(auditTokenHex: String) -> Bool {
    withAuditToken(auditTokenHex) { token in
      var signal = SIGKILL
      if proc_terminate_with_audittoken(&token, &signal) == 0 { return true }
      return errno == ESRCH
    } ?? false
  }

  private func withAuditToken<T>(
    _ hex: String, body: (inout audit_token_t) -> T
  ) -> T? {
    guard hex.count == MemoryLayout<audit_token_t>.size * 2 else { return nil }
    var bytes = [UInt8]()
    bytes.reserveCapacity(MemoryLayout<audit_token_t>.size)
    var index = hex.startIndex
    while index < hex.endIndex {
      let next = hex.index(index, offsetBy: 2)
      guard let byte = UInt8(hex[index..<next], radix: 16) else { return nil }
      bytes.append(byte)
      index = next
    }
    var token = audit_token_t()
    guard bytes.count == MemoryLayout.size(ofValue: token) else { return nil }
    withUnsafeMutableBytes(of: &token) { destination in
      destination.copyBytes(from: bytes)
    }
    return body(&token)
  }
}

protocol CoreBundleValidating {
  func validate(
    appExecutableURL: URL,
    coreExecutableURL: URL,
    coreAuditTokenHex: String,
    codexExecutableURL: URL,
    codexAuditTokenHex: String
  ) -> Bool
}

private struct SignedCoreBundleValidator: CoreBundleValidating {
  func validate(
    appExecutableURL: URL,
    coreExecutableURL: URL,
    coreAuditTokenHex: String,
    codexExecutableURL: URL,
    codexAuditTokenHex: String
  ) -> Bool {
    let macOS = appExecutableURL.deletingLastPathComponent()
    let contents = macOS.deletingLastPathComponent()
    let bundle = contents.deletingLastPathComponent()
    guard appExecutableURL.lastPathComponent == "OpenOpen",
      macOS.lastPathComponent == "MacOS",
      contents.lastPathComponent == "Contents",
      bundle.pathExtension == "app",
      coreExecutableURL == macOS.appendingPathComponent("OpenOpenCore").standardizedFileURL,
      bundle.resolvingSymlinksInPath().standardizedFileURL == bundle.standardizedFileURL,
      let identity = try? SecurityCodeSigningIdentityProvider().currentIdentity(),
      identity.signingIdentifier == EffectBrokerConstants.brokerSigningIdentifier
    else { return false }
    guard
      (try? StaticCodeSigningValidator.validate(
        executableURL: bundle,
        expectedSigningIdentifier: EffectBrokerConstants.hostSigningIdentifier,
        teamIdentifier: identity.teamIdentifier
      )) != nil
    else { return false }
    let expectedCodex = bundle.appendingPathComponent(
      "Contents/Resources/Codex/0.144.0/bin/codex"
    ).standardizedFileURL
    guard codexExecutableURL == expectedCodex else { return false }
    return
      (try? StaticCodeSigningValidator.validateRunningProcess(
        auditTokenHex: coreAuditTokenHex,
        expectedSigningIdentifier: EffectBrokerConstants.coreSigningIdentifier,
        teamIdentifier: identity.teamIdentifier
      )) != nil
      && (try? StaticCodeSigningValidator.validatePinnedCodex(
        auditTokenHex: codexAuditTokenHex
      )) != nil
  }
}

protocol BrokerWorkerRunning: AnyObject {
  func run(
    operation: BrokerWorkerOperation,
    auditEUID: uid_t,
    requestJSON: Data?,
    payload: FileHandle?
  ) throws -> Data
}

enum BrokerWorkerError: Error, Equatable {
  case daemonMustRunAsRoot
  case invalidWorkerBundleLayout
  case invalidProtectedDirectory
  case invalidProtectedExecutable
  case invalidPayloadDescriptor
  case payloadTooLarge
  case malformedWorkerResponse
  case workerLaunchFailed
  case workerTimedOut
}

struct AuditTokenProcessReaper {
  let processInspector: any BrokerProcessInspecting

  func terminateAndConfirm(
    _ auditTokenHex: String,
    completion: DispatchSemaphore,
    waitDeadline: DispatchTime = .now() + .seconds(2)
  ) -> Bool {
    if processInspector.isAlive(auditTokenHex: auditTokenHex),
      !processInspector.terminate(auditTokenHex: auditTokenHex)
    {
      return false
    }
    guard processInspector.isAlive(auditTokenHex: auditTokenHex) else { return true }
    _ = completion.wait(timeout: waitDeadline)
    return !processInspector.isAlive(auditTokenHex: auditTokenHex)
  }
}

final class SignedBrokerWorkerRunner: BrokerWorkerRunning {
  private static let maximumPayloadBytes: UInt64 = 512 * 1024 * 1024
  private let executableURL: URL
  private let payloadIdleTimeoutMilliseconds: Int32
  private let operationTimeoutMilliseconds: UInt64
  private let processInspector: any BrokerProcessInspecting

  init(
    identityProvider: any CodeSigningIdentityProviding =
      SecurityCodeSigningIdentityProvider(),
    sourceURL: URL? = nil,
    payloadIdleTimeoutMilliseconds: Int32 = 10_000,
    operationTimeoutMilliseconds: UInt64 = 120_000,
    processInspector: any BrokerProcessInspecting = DarwinBrokerProcessInspector()
  ) throws {
    guard geteuid() == 0 else {
      throw BrokerWorkerError.daemonMustRunAsRoot
    }
    let identity = try identityProvider.currentIdentity()
    guard identity.signingIdentifier == EffectBrokerConstants.brokerSigningIdentifier else {
      throw CodeSigningIdentityError.unexpectedSigningIdentifier(
        expected: EffectBrokerConstants.brokerSigningIdentifier,
        actual: identity.signingIdentifier
      )
    }
    let source = try sourceURL ?? Self.bundledWorkerURL()
    executableURL = try ProtectedWorkerInstaller.install(
      sourceURL: source,
      teamIdentifier: identity.teamIdentifier
    )
    guard payloadIdleTimeoutMilliseconds > 0, operationTimeoutMilliseconds > 0 else {
      throw BrokerWorkerError.invalidPayloadDescriptor
    }
    self.payloadIdleTimeoutMilliseconds = payloadIdleTimeoutMilliseconds
    self.operationTimeoutMilliseconds = operationTimeoutMilliseconds
    self.processInspector = processInspector
  }

  func run(
    operation: BrokerWorkerOperation,
    auditEUID: uid_t,
    requestJSON: Data?,
    payload: FileHandle?
  ) throws -> Data {
    let expectedPayloadBytes = try Self.expectedPayloadBytes(
      operation: operation,
      requestJSON: requestJSON,
      payload: payload
    )
    let process = Process()
    let inputPipe = Pipe()
    let outputPipe = Pipe()
    let completion = DispatchSemaphore(value: 0)
    let deadline = try MonotonicDeadline(millisecondsFromNow: operationTimeoutMilliseconds)
    process.executableURL = executableURL
    process.arguments = [operation.rawValue, String(auditEUID)]
    process.standardInput = inputPipe
    process.standardOutput = outputPipe
    process.standardError = FileHandle.nullDevice
    process.terminationHandler = { _ in completion.signal() }
    var auditTokenHex: String?
    do {
      try process.run()
      guard
        let token = stableWorkerAuditToken(
          for: process.processIdentifier,
          expectedParentPID: getpid(),
          expectedEffectiveUserIdentifier: 0,
          expectedExecutableURL: executableURL,
          processInspector: processInspector,
          terminationObserved: { completion.wait(timeout: .now()) == .success }
        )
      else {
        try? inputPipe.fileHandleForWriting.close()
        _ = completion.wait(timeout: .now() + .seconds(2))
        throw BrokerWorkerError.workerLaunchFailed
      }
      auditTokenHex = token
      if let requestJSON {
        try DeadlineIO.write(
          requestJSON,
          to: inputPipe.fileHandleForWriting,
          deadline: deadline
        )
        try DeadlineIO.write(
          Data([0x0A]),
          to: inputPipe.fileHandleForWriting,
          deadline: deadline
        )
      }
      if let payload {
        var observedBytes = 0 as UInt64
        while let chunk = try DeadlineIO.read(
          from: payload,
          maximumCount: 64 * 1024,
          idleTimeoutMilliseconds: payloadIdleTimeoutMilliseconds,
          deadline: deadline
        ), !chunk.isEmpty {
          observedBytes = try observedBytes.addingWithoutOverflow(UInt64(chunk.count))
          guard observedBytes <= Self.maximumPayloadBytes,
            observedBytes <= expectedPayloadBytes
          else {
            throw BrokerWorkerError.payloadTooLarge
          }
          try DeadlineIO.write(
            chunk,
            to: inputPipe.fileHandleForWriting,
            deadline: deadline
          )
        }
        guard observedBytes == expectedPayloadBytes else {
          throw BrokerWorkerError.invalidPayloadDescriptor
        }
      }
      try inputPipe.fileHandleForWriting.close()
      guard completion.wait(timeout: deadline.dispatchDeadline) == .success else {
        throw BrokerWorkerError.workerTimedOut
      }
      let response = try outputPipe.fileHandleForReading.readToEnd() ?? Data()
      guard response.count <= TypedJSONEnvelope.maximumBytes,
        StrictJSONDocument.object(from: response) != nil
      else {
        throw BrokerWorkerError.malformedWorkerResponse
      }
      return response
    } catch {
      if let auditTokenHex {
        guard
          AuditTokenProcessReaper(processInspector: processInspector).terminateAndConfirm(
            auditTokenHex, completion: completion
          )
        else {
          try? inputPipe.fileHandleForWriting.close()
          throw BrokerWorkerError.workerLaunchFailed
        }
      }
      try? inputPipe.fileHandleForWriting.close()
      if error is BrokerWorkerError {
        throw error
      }
      throw BrokerWorkerError.workerLaunchFailed
    }
  }

  static func bundledWorkerURL(
    processInspector: any BrokerProcessInspecting = DarwinBrokerProcessInspector()
  ) throws -> URL {
    let pid = getpid()
    guard let identity = processInspector.identity(for: pid),
      identity.pid == pid,
      identity.effectiveUserIdentifier == geteuid()
    else {
      throw BrokerWorkerError.invalidWorkerBundleLayout
    }
    return try siblingWorkerURL(forBrokerExecutableURL: identity.executableURL)
  }

  static func siblingWorkerURL(forBrokerExecutableURL broker: URL) throws -> URL {
    let broker = broker.standardizedFileURL
    let macOS = broker.deletingLastPathComponent()
    let contents = macOS.deletingLastPathComponent()
    let app = contents.deletingLastPathComponent()
    guard broker.isFileURL,
      broker.lastPathComponent == "OpenOpenEffectBroker",
      macOS.lastPathComponent == "MacOS",
      contents.lastPathComponent == "Contents",
      app.pathExtension == "app",
      app.resolvingSymlinksInPath().standardizedFileURL == app.standardizedFileURL
    else {
      throw BrokerWorkerError.invalidWorkerBundleLayout
    }
    let worker = macOS.appendingPathComponent(
      "OpenOpenEffectBrokerWorker", isDirectory: false
    )
    guard FileManager.default.fileExists(atPath: worker.path) else {
      throw BrokerWorkerError.invalidWorkerBundleLayout
    }
    return worker
  }

  private static func expectedPayloadBytes(
    operation: BrokerWorkerOperation,
    requestJSON: Data?,
    payload: FileHandle?
  ) throws -> UInt64 {
    guard operation == .put else {
      guard payload == nil else {
        throw BrokerWorkerError.invalidPayloadDescriptor
      }
      return 0
    }
    guard let requestJSON, payload != nil,
      let request = StrictJSONDocument.object(from: requestJSON),
      let permit = request["permit"] as? [String: Any],
      let command = permit["command"] as? [String: Any],
      let effect = command["effect"] as? [String: Any],
      let descriptor = effect["payload"] as? [String: Any],
      let byteLength = descriptor["byteLen"] as? NSNumber,
      CFGetTypeID(byteLength) != CFBooleanGetTypeID(),
      let expected = UInt64(byteLength.stringValue),
      expected <= maximumPayloadBytes
    else {
      throw BrokerWorkerError.invalidPayloadDescriptor
    }
    return expected
  }
}

struct MonotonicDeadline {
  let uptimeNanoseconds: UInt64

  init(millisecondsFromNow: UInt64) throws {
    let delta = try millisecondsFromNow.multipliedWithoutOverflow(1_000_000)
    uptimeNanoseconds = try DispatchTime.now().uptimeNanoseconds.addingWithoutOverflow(delta)
  }

  var dispatchDeadline: DispatchTime {
    DispatchTime(uptimeNanoseconds: uptimeNanoseconds)
  }

  func remainingMilliseconds(cappedAt cap: Int32? = nil) throws -> Int32 {
    let now = DispatchTime.now().uptimeNanoseconds
    guard now < uptimeNanoseconds else {
      throw BrokerWorkerError.workerTimedOut
    }
    let roundedUp = (uptimeNanoseconds - now + 999_999) / 1_000_000
    let bounded = min(roundedUp, UInt64(Int32.max))
    let remaining = Int32(bounded)
    return cap.map { min(remaining, $0) } ?? remaining
  }
}

enum DeadlineIO {
  static func read(
    from handle: FileHandle,
    maximumCount: Int,
    idleTimeoutMilliseconds: Int32,
    deadline: MonotonicDeadline
  ) throws -> Data? {
    let fileDescriptor = handle.fileDescriptor
    var descriptor = pollfd(fd: fileDescriptor, events: Int16(POLLIN | POLLHUP), revents: 0)
    let timeout = try deadline.remainingMilliseconds(cappedAt: idleTimeoutMilliseconds)
    let result = Darwin.poll(&descriptor, 1, timeout)
    guard result > 0 else {
      if result == 0 {
        throw BrokerWorkerError.workerTimedOut
      }
      throw BrokerWorkerError.workerLaunchFailed
    }
    guard descriptor.revents & Int16(POLLERR | POLLNVAL) == 0 else {
      throw BrokerWorkerError.workerLaunchFailed
    }
    return try handle.read(upToCount: maximumCount)
  }

  static func write(
    _ data: Data,
    to handle: FileHandle,
    deadline: MonotonicDeadline
  ) throws {
    let fileDescriptor = handle.fileDescriptor
    _ = Darwin.fcntl(fileDescriptor, F_SETNOSIGPIPE, 1)
    try data.withUnsafeBytes { rawBuffer in
      guard let baseAddress = rawBuffer.baseAddress else {
        return
      }
      var offset = 0
      while offset < rawBuffer.count {
        var descriptor = pollfd(fd: fileDescriptor, events: Int16(POLLOUT), revents: 0)
        let result = Darwin.poll(
          &descriptor,
          1,
          try deadline.remainingMilliseconds()
        )
        guard result > 0,
          descriptor.revents & Int16(POLLERR | POLLHUP | POLLNVAL) == 0
        else {
          throw result == 0
            ? BrokerWorkerError.workerTimedOut
            : BrokerWorkerError.workerLaunchFailed
        }
        let count = Darwin.write(
          fileDescriptor,
          baseAddress.advanced(by: offset),
          rawBuffer.count - offset
        )
        guard count > 0 else {
          throw BrokerWorkerError.workerLaunchFailed
        }
        offset += count
      }
    }
  }
}

private struct CoreLeaseAcquireCallerRequest: Decodable {
  let coreInstanceNonce: String
  let corePid: Int32
  let codexPid: Int32
}

private struct CoreInstanceLeaseWire: Codable, Equatable {
  let protocolVersion: UInt32
  let auditEuid: UInt32
  let appPid: Int32
  let appStartTimeUs: UInt64
  let corePid: Int32
  let coreStartTimeUs: UInt64
  let coreAuditTokenHex: String
  let codexPid: Int32
  let codexStartTimeUs: UInt64
  let codexAuditTokenHex: String
  let coreInstanceNonce: String
  let issuedAtMs: Int64
  let brokerKeyId: String
  let brokerSignatureHex: String
}

private struct CoreLeaseStatusResponse: Decodable {
  let lease: CoreInstanceLeaseWire?
  let status: String
  let version: Int
}

private struct CoreLeaseAcquireWorkerRequest: Encodable {
  let type = "coreLeaseAcquire"
  let version = 1
  let appPid: Int32
  let appStartTimeUs: UInt64
  let corePid: Int32
  let coreStartTimeUs: UInt64
  let coreAuditTokenHex: String
  let codexPid: Int32
  let codexStartTimeUs: UInt64
  let codexAuditTokenHex: String
  let coreInstanceNonce: String
}

private struct CoreLeaseReleaseWorkerRequest: Encodable {
  let type = "coreLeaseRelease"
  let version = 1
  let lease: CoreInstanceLeaseWire
}

public final class RustBrokerProcessBackend: EffectBrokerBackend {
  private let runner: any BrokerWorkerRunning
  private let processInspector: any BrokerProcessInspecting
  private let coreBundleValidator: any CoreBundleValidating
  private let lock = NSLock()

  public convenience init() throws {
    try self.init(runner: SignedBrokerWorkerRunner())
  }

  init(
    runner: any BrokerWorkerRunning,
    processInspector: any BrokerProcessInspecting = DarwinBrokerProcessInspector(),
    coreBundleValidator: any CoreBundleValidating = SignedCoreBundleValidator()
  ) {
    self.runner = runner
    self.processInspector = processInspector
    self.coreBundleValidator = coreBundleValidator
  }

  public func brokerStatus(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  ) {
    reply(run(.status, peer: peer, requestJSON: requestJSON, payload: nil))
  }

  public func session(
    peer: AuthenticatedBrokerPeer,
    requestJSON _: Data,
    reply: @escaping (Data) -> Void
  ) {
    reply(run(.session, peer: peer, requestJSON: nil, payload: nil))
  }

  public func enrollCore(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  ) {
    reply(run(.enrollCore, peer: peer, requestJSON: requestJSON, payload: nil))
  }

  public func acquireCoreLease(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  ) {
    lock.lock()
    defer { lock.unlock() }
    do {
      let caller = try JSONDecoder().decode(CoreLeaseAcquireCallerRequest.self, from: requestJSON)
      guard let app = processInspector.identity(for: peer.processIdentifier),
        let auditedCore = auditedProcess(for: caller.corePid),
        let auditedCodex = auditedProcess(for: caller.codexPid)
      else {
        reply(Self.rejectedResponse)
        return
      }
      let core = auditedCore.identity
      let codex = auditedCodex.identity
      let coreAuditTokenHex = auditedCore.auditTokenHex
      let codexAuditTokenHex = auditedCodex.auditTokenHex
      guard
        app.pid == peer.processIdentifier,
        app.effectiveUserIdentifier == peer.effectiveUserIdentifier,
        core.parentPID == app.pid,
        codex.parentPID == core.pid,
        core.effectiveUserIdentifier == peer.effectiveUserIdentifier,
        codex.effectiveUserIdentifier == peer.effectiveUserIdentifier,
        core.processGroupIdentifier == core.pid,
        app.executableURL.lastPathComponent == "OpenOpen",
        core.executableURL.lastPathComponent == "OpenOpenCore",
        app.executableURL.deletingLastPathComponent()
          == core.executableURL.deletingLastPathComponent(),
        coreBundleValidator.validate(
          appExecutableURL: app.executableURL,
          coreExecutableURL: core.executableURL,
          coreAuditTokenHex: coreAuditTokenHex,
          codexExecutableURL: codex.executableURL,
          codexAuditTokenHex: codexAuditTokenHex
        )
      else {
        reply(Self.rejectedResponse)
        return
      }

      let statusData = try runUnlocked(
        .coreLeaseStatus, peer: peer, requestJSON: nil, payload: nil
      )
      let status = try JSONDecoder().decode(CoreLeaseStatusResponse.self, from: statusData)
      guard status.version == 1, status.status == "ready" else {
        reply(Self.rejectedResponse)
        return
      }
      if let existing = status.lease {
        if existing.auditEuid == peer.effectiveUserIdentifier,
          existing.appPid == app.pid,
          existing.appStartTimeUs == app.startTimeMicroseconds,
          existing.corePid == core.pid,
          existing.coreStartTimeUs == core.startTimeMicroseconds,
          existing.coreAuditTokenHex == coreAuditTokenHex,
          existing.codexPid == codex.pid,
          existing.codexStartTimeUs == codex.startTimeMicroseconds,
          existing.codexAuditTokenHex == codexAuditTokenHex,
          existing.coreInstanceNonce == caller.coreInstanceNonce
        {
          reply(try Self.acceptedLeaseResponse(existing))
          return
        }
        guard try retireLeaseAfterTerminatingExactProcesses(existing, peer: peer) else {
          reply(Self.rejectedResponse)
          return
        }
      }

      let workerRequest = CoreLeaseAcquireWorkerRequest(
        appPid: app.pid,
        appStartTimeUs: app.startTimeMicroseconds,
        corePid: core.pid,
        coreStartTimeUs: core.startTimeMicroseconds,
        coreAuditTokenHex: coreAuditTokenHex,
        codexPid: codex.pid,
        codexStartTimeUs: codex.startTimeMicroseconds,
        codexAuditTokenHex: codexAuditTokenHex,
        coreInstanceNonce: caller.coreInstanceNonce
      )
      let acquired = try runUnlocked(
        .coreLeaseAcquire,
        peer: peer,
        requestJSON: try JSONEncoder().encode(workerRequest),
        payload: nil
      )
      guard processInspector.isAlive(auditTokenHex: coreAuditTokenHex),
        processInspector.isAlive(auditTokenHex: codexAuditTokenHex)
      else {
        // The durable record deliberately remains occupied. A later acquire
        // retires only these exact audit-token process incarnations.
        reply(Self.rejectedResponse)
        return
      }
      reply(acquired)
    } catch {
      reply(Self.rejectedResponse)
    }
  }

  public func applyRuntimeControl(
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data,
    reply: @escaping (Data) -> Void
  ) {
    guard
      let object = try? JSONSerialization.jsonObject(with: requestJSON) as? [String: Any],
      let control = object["control"] as? [String: Any],
      let enabled = control["enabled"] as? Bool
    else {
      reply(Self.rejectedResponse)
      return
    }
    lock.lock()
    defer { lock.unlock() }
    do {
      let statusData = try runUnlocked(
        .coreLeaseStatus, peer: peer, requestJSON: nil, payload: nil
      )
      let status = try JSONDecoder().decode(CoreLeaseStatusResponse.self, from: statusData)
      guard status.version == 1, status.status == "ready" else {
        reply(Self.rejectedResponse)
        return
      }
      if enabled {
        guard let lease = status.lease,
          let app = processInspector.identity(for: peer.processIdentifier),
          lease.auditEuid == peer.effectiveUserIdentifier,
          lease.appPid == app.pid,
          lease.appStartTimeUs == app.startTimeMicroseconds,
          processInspector.isAlive(auditTokenHex: lease.coreAuditTokenHex),
          processInspector.isAlive(auditTokenHex: lease.codexAuditTokenHex)
        else {
          reply(Self.rejectedResponse)
          return
        }
        reply(
          try runUnlocked(
            .runtimeControl, peer: peer, requestJSON: requestJSON, payload: nil
          ))
        return
      }

      // Keep the old lease occupied while SIGKILL is delivered to the exact
      // leased Codex and Core audit-token incarnations. Only after both are
      // proven dead may protected Off become durable. A daemon crash can
      // therefore occur either before Off persistence (no Off acceptance,
      // old lease still occupied) or after both exact processes are dead.
      if let lease = status.lease {
        guard retireExactLeaseProcessesBeforeOff(lease) else {
          reply(Self.rejectedResponse)
          return
        }
      }
      let acceptedOff = try runUnlocked(
        .runtimeControl, peer: peer, requestJSON: requestJSON, payload: nil
      )
      guard Self.isAcceptedRuntimeResponse(acceptedOff) else {
        reply(acceptedOff)
        return
      }
      if let lease = status.lease {
        guard try releaseLease(lease, peer: peer) else {
          reply(Self.rejectedResponse)
          return
        }
      }
      reply(acceptedOff)
    } catch {
      reply(Self.rejectedResponse)
    }
  }

  public func putMissionFile(
    peer: AuthenticatedBrokerPeer,
    permitJSON: Data,
    payload: FileHandle,
    reply: @escaping (Data) -> Void
  ) {
    reply(run(.put, peer: peer, requestJSON: permitJSON, payload: payload))
  }

  public func reconcileMissionFile(
    peer: AuthenticatedBrokerPeer,
    permitJSON: Data,
    reply: @escaping (Data) -> Void
  ) {
    reply(run(.reconcile, peer: peer, requestJSON: permitJSON, payload: nil))
  }

  private func run(
    _ operation: BrokerWorkerOperation,
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data?,
    payload: FileHandle?
  ) -> Data {
    lock.lock()
    defer { lock.unlock() }
    do {
      return try runUnlocked(operation, peer: peer, requestJSON: requestJSON, payload: payload)
    } catch {
      return Self.rejectedResponse
    }
  }

  private func runUnlocked(
    _ operation: BrokerWorkerOperation,
    peer: AuthenticatedBrokerPeer,
    requestJSON: Data?,
    payload: FileHandle?
  ) throws -> Data {
    try runner.run(
      operation: operation,
      auditEUID: peer.effectiveUserIdentifier,
      requestJSON: requestJSON,
      payload: payload
    )
  }

  private func retireLeaseAfterTerminatingExactProcesses(
    _ lease: CoreInstanceLeaseWire,
    peer: AuthenticatedBrokerPeer
  ) throws -> Bool {
    guard retireExactLeaseProcessesBeforeOff(lease) else { return false }
    return try releaseLease(lease, peer: peer)
  }

  private func auditedProcess(for pid: pid_t) -> BrokerAuditedProcess? {
    guard let before = processInspector.auditTokenHex(for: pid),
      let identity = processInspector.identity(for: pid),
      let after = processInspector.auditTokenHex(for: pid),
      before == after
    else { return nil }
    return BrokerAuditedProcess(identity: identity, auditTokenHex: before)
  }

  private func retireExactLeaseProcessesBeforeOff(_ lease: CoreInstanceLeaseWire) -> Bool {
    for token in [lease.codexAuditTokenHex, lease.coreAuditTokenHex] {
      if processInspector.isAlive(auditTokenHex: token) {
        guard processInspector.terminate(auditTokenHex: token) else { return false }
        waitForAuditTokenToExit(token)
      }
      guard !processInspector.isAlive(auditTokenHex: token) else { return false }
    }
    return true
  }

  private func waitForAuditTokenToExit(_ token: String) {
    for _ in 0..<40 {
      guard processInspector.isAlive(auditTokenHex: token) else { return }
      usleep(50_000)
    }
  }

  private func releaseLease(
    _ lease: CoreInstanceLeaseWire,
    peer: AuthenticatedBrokerPeer
  ) throws -> Bool {
    let release = CoreLeaseReleaseWorkerRequest(lease: lease)
    let response = try runUnlocked(
      .coreLeaseRelease,
      peer: peer,
      requestJSON: try JSONEncoder().encode(release),
      payload: nil
    )
    guard let object = try JSONSerialization.jsonObject(with: response) as? [String: Any],
      object["version"] as? Int == 1,
      object["status"] as? String == "released"
    else { return false }
    return true
  }

  private static func acceptedLeaseResponse(_ lease: CoreInstanceLeaseWire) throws -> Data {
    try JSONSerialization.data(
      withJSONObject: [
        "lease": try JSONSerialization.jsonObject(with: JSONEncoder().encode(lease)),
        "status": "accepted",
        "version": 1,
      ],
      options: [.sortedKeys]
    )
  }

  private static func isAcceptedRuntimeResponse(_ data: Data) -> Bool {
    guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else { return false }
    return object["version"] as? Int == 1 && object["status"] as? String == "accepted"
  }

  private static let rejectedResponse = Data(
    #"{"error":{"code":"brokerBackendUnavailable"},"status":"rejected","version":1}"#.utf8
  )
}

private enum ProtectedWorkerInstaller {
  private static let systemRoot = URL(
    fileURLWithPath: "/Library/Application Support/com.thesongzhu.OpenOpen",
    isDirectory: true
  )
  private static let installRoot = URL(
    fileURLWithPath:
      "/Library/Application Support/com.thesongzhu.OpenOpen/EffectBroker/bin",
    isDirectory: true
  )

  static func install(sourceURL: URL, teamIdentifier: String) throws -> URL {
    let fileManager = FileManager.default
    try ensurePrivateDirectory(systemRoot)
    try ensurePrivateDirectory(installRoot.deletingLastPathComponent())
    try ensurePrivateDirectory(installRoot)
    let temporary = installRoot.appendingPathComponent(
      ".OpenOpenEffectBrokerWorker.\(UUID().uuidString)",
      isDirectory: false
    )
    let destination = installRoot.appendingPathComponent(
      "OpenOpenEffectBrokerWorker",
      isDirectory: false
    )
    defer { try? fileManager.removeItem(at: temporary) }
    try fileManager.copyItem(at: sourceURL, to: temporary)
    try fileManager.setAttributes(
      [.ownerAccountID: 0, .groupOwnerAccountID: 0, .posixPermissions: 0o500],
      ofItemAtPath: temporary.path
    )
    let handle = try FileHandle(forWritingTo: temporary)
    try handle.synchronize()
    try handle.close()
    try requirePrivateExecutable(temporary)
    try StaticCodeSigningValidator.validate(
      executableURL: temporary,
      expectedSigningIdentifier: EffectBrokerConstants.workerSigningIdentifier,
      teamIdentifier: teamIdentifier
    )
    let result = temporary.path.withCString { source in
      destination.path.withCString { target in
        Darwin.rename(source, target)
      }
    }
    guard result == 0 else {
      throw BrokerWorkerError.invalidProtectedExecutable
    }
    try requirePrivateExecutable(destination)
    try StaticCodeSigningValidator.validate(
      executableURL: destination,
      expectedSigningIdentifier: EffectBrokerConstants.workerSigningIdentifier,
      teamIdentifier: teamIdentifier
    )
    return destination
  }

  private static func ensurePrivateDirectory(_ url: URL) throws {
    let fileManager = FileManager.default
    if !fileManager.fileExists(atPath: url.path) {
      try fileManager.createDirectory(
        at: url,
        withIntermediateDirectories: false,
        attributes: [
          .ownerAccountID: 0,
          .groupOwnerAccountID: 0,
          .posixPermissions: 0o700,
        ]
      )
    }
    let attributes = try fileManager.attributesOfItem(atPath: url.path)
    guard attributes[.type] as? FileAttributeType == .typeDirectory,
      (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == 0,
      (attributes[.posixPermissions] as? NSNumber)?.uint16Value == 0o700,
      url.resolvingSymlinksInPath().standardizedFileURL == url.standardizedFileURL
    else {
      throw BrokerWorkerError.invalidProtectedDirectory
    }
  }

  private static func requirePrivateExecutable(_ url: URL) throws {
    let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
    guard attributes[.type] as? FileAttributeType == .typeRegular,
      (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == 0,
      (attributes[.posixPermissions] as? NSNumber)?.uint16Value == 0o500,
      url.resolvingSymlinksInPath().standardizedFileURL == url.standardizedFileURL
    else {
      throw BrokerWorkerError.invalidProtectedExecutable
    }
  }
}

extension UInt64 {
  fileprivate func addingWithoutOverflow(_ other: UInt64) throws -> UInt64 {
    let (value, overflow) = addingReportingOverflow(other)
    guard !overflow else {
      throw BrokerWorkerError.payloadTooLarge
    }
    return value
  }

  fileprivate func multipliedWithoutOverflow(_ other: UInt64) throws -> UInt64 {
    let (value, overflow) = multipliedReportingOverflow(by: other)
    guard !overflow else {
      throw BrokerWorkerError.workerTimedOut
    }
    return value
  }
}
