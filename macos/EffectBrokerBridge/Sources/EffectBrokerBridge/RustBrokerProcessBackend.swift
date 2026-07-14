import Darwin
import Foundation

enum BrokerWorkerOperation: String {
  case status
  case session
  case enrollCore = "enroll-core"
  case put
  case reconcile
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

final class SignedBrokerWorkerRunner: BrokerWorkerRunning {
  private static let maximumPayloadBytes: UInt64 = 512 * 1024 * 1024
  private let executableURL: URL
  private let payloadIdleTimeoutMilliseconds: Int32
  private let operationTimeoutMilliseconds: UInt64

  init(
    identityProvider: any CodeSigningIdentityProviding =
      SecurityCodeSigningIdentityProvider(),
    sourceURL: URL? = nil,
    payloadIdleTimeoutMilliseconds: Int32 = 10_000,
    operationTimeoutMilliseconds: UInt64 = 120_000
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
    do {
      try process.run()
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
            process.terminate()
            throw BrokerWorkerError.payloadTooLarge
          }
          try DeadlineIO.write(
            chunk,
            to: inputPipe.fileHandleForWriting,
            deadline: deadline
          )
        }
        guard observedBytes == expectedPayloadBytes else {
          process.terminate()
          throw BrokerWorkerError.invalidPayloadDescriptor
        }
      }
      try inputPipe.fileHandleForWriting.close()
      guard completion.wait(timeout: deadline.dispatchDeadline) == .success else {
        Self.terminateAndReap(process, completion: completion)
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
      if process.isRunning {
        Self.terminateAndReap(process, completion: completion)
      }
      try? inputPipe.fileHandleForWriting.close()
      if error is BrokerWorkerError {
        throw error
      }
      throw BrokerWorkerError.workerLaunchFailed
    }
  }

  private static func bundledWorkerURL() throws -> URL {
    guard let executablePath = CommandLine.arguments.first, !executablePath.isEmpty else {
      throw BrokerWorkerError.invalidWorkerBundleLayout
    }
    let url = URL(fileURLWithPath: executablePath).standardizedFileURL
      .deletingLastPathComponent()
      .appendingPathComponent("OpenOpenEffectBrokerWorker", isDirectory: false)
    guard FileManager.default.fileExists(atPath: url.path) else {
      throw BrokerWorkerError.invalidWorkerBundleLayout
    }
    return url
  }

  private static func terminateAndReap(
    _ process: Process,
    completion: DispatchSemaphore
  ) {
    guard process.isRunning else {
      return
    }
    process.terminate()
    if completion.wait(timeout: .now() + .seconds(2)) == .timedOut,
      process.isRunning
    {
      _ = Darwin.kill(process.processIdentifier, SIGKILL)
      _ = completion.wait(timeout: .now() + .seconds(2))
    }
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

public final class RustBrokerProcessBackend: EffectBrokerBackend {
  private let runner: any BrokerWorkerRunning
  private let lock = NSLock()

  public convenience init() throws {
    try self.init(runner: SignedBrokerWorkerRunner())
  }

  init(runner: any BrokerWorkerRunning) {
    self.runner = runner
  }

  public func brokerStatus(
    peer: AuthenticatedBrokerPeer,
    requestJSON _: Data,
    reply: @escaping (Data) -> Void
  ) {
    reply(run(.status, peer: peer, requestJSON: nil, payload: nil))
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
      return try runner.run(
        operation: operation,
        auditEUID: peer.effectiveUserIdentifier,
        requestJSON: requestJSON,
        payload: payload
      )
    } catch {
      return Self.rejectedResponse
    }
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
