import Darwin
import Foundation

enum CodexRuntimeHomeError: Error, Equatable {
  case invalidCaller
  case invalidDirectory
  case invalidMountHelper
  case mountFailed
  case mountTimedOut
}

struct CodexRuntimeHomeReceipt: Codable, Equatable {
  let runtimeHome: String
  let runtimeDevice: UInt64
  let status: String
  let version: Int
}

protocol CodexRuntimeHomePreparing {
  func prepare(for auditEUID: uid_t) throws -> CodexRuntimeHomeReceipt
}

protocol CodexRuntimeHomeMounting {
  func mount(at path: String) throws
}

struct CodexRuntimeFileSystemSnapshot: Equatable {
  let blockSize: UInt64
  let blocks: UInt64
  let files: UInt64
  let flags: UInt32
  let typeName: String
  let mountPoint: String
}

protocol CodexRuntimeFileSystemInspecting {
  func snapshot(at path: String) -> CodexRuntimeFileSystemSnapshot?
}

struct CodexRuntimePathSnapshot: Equatable {
  let owner: uid_t
  let group: gid_t
  let mode: mode_t
  let device: dev_t
  let isDirectory: Bool
}

protocol CodexRuntimePathOperating {
  func pathSnapshot(at path: String) -> CodexRuntimePathSnapshot?
  func createDirectory(at path: String, mode: mode_t) -> Bool
  func setOwner(_ owner: uid_t, group: gid_t, at path: String) -> Bool
  func setMode(_ mode: mode_t, at path: String) -> Bool
  func directoryIsEmpty(at path: String) throws -> Bool
}

struct DarwinCodexRuntimeFileSystemInspector: CodexRuntimeFileSystemInspecting {
  func snapshot(at path: String) -> CodexRuntimeFileSystemSnapshot? {
    var fileSystem = statfs()
    guard statfs(path, &fileSystem) == 0 else { return nil }
    return CodexRuntimeFileSystemSnapshot(
      blockSize: UInt64(fileSystem.f_bsize),
      blocks: UInt64(fileSystem.f_blocks),
      files: UInt64(fileSystem.f_files),
      flags: fileSystem.f_flags,
      typeName: fileSystemName(fileSystem.f_fstypename),
      mountPoint: fileSystemName(fileSystem.f_mntonname)
    )
  }

  private func fileSystemName<T>(_ value: T) -> String {
    withUnsafeBytes(of: value) { bytes in
      let prefix = bytes.prefix { $0 != 0 }
      return String(decoding: prefix, as: UTF8.self)
    }
  }
}

struct DarwinCodexRuntimePathOperator: CodexRuntimePathOperating {
  func pathSnapshot(at path: String) -> CodexRuntimePathSnapshot? {
    var metadata = stat()
    guard lstat(path, &metadata) == 0 else { return nil }
    return CodexRuntimePathSnapshot(
      owner: metadata.st_uid,
      group: metadata.st_gid,
      mode: metadata.st_mode & 0o777,
      device: metadata.st_dev,
      isDirectory: metadata.st_mode & mode_t(S_IFMT) == mode_t(S_IFDIR)
    )
  }

  func createDirectory(at path: String, mode: mode_t) -> Bool {
    mkdir(path, mode) == 0
  }

  func setOwner(_ owner: uid_t, group: gid_t, at path: String) -> Bool {
    chown(path, owner, group) == 0
  }

  func setMode(_ mode: mode_t, at path: String) -> Bool {
    chmod(path, mode) == 0
  }

  func directoryIsEmpty(at path: String) throws -> Bool {
    try FileManager.default.contentsOfDirectory(atPath: path).isEmpty
  }
}

struct RejectingCodexRuntimeHomePreparer: CodexRuntimeHomePreparing {
  func prepare(for _: uid_t) throws -> CodexRuntimeHomeReceipt {
    throw CodexRuntimeHomeError.invalidCaller
  }
}

struct SystemCodexRuntimeHomeMounter: CodexRuntimeHomeMounting {
  private static let expectedExecutable = URL(
    fileURLWithPath:
      "/System/Library/Filesystems/tmpfs.fs/Contents/Resources/mount_tmpfs"
  ).standardizedFileURL

  private let executable: URL
  private let processInspector: any BrokerProcessInspecting

  init(
    source: URL = URL(fileURLWithPath: "/sbin/mount_tmpfs"),
    processInspector: any BrokerProcessInspecting = DarwinBrokerProcessInspector()
  ) throws {
    guard geteuid() == 0 else { throw CodexRuntimeHomeError.invalidCaller }
    let resolved = source.resolvingSymlinksInPath().standardizedFileURL
    var metadata = stat()
    guard resolved == Self.expectedExecutable,
      lstat(resolved.path, &metadata) == 0,
      metadata.st_uid == 0,
      metadata.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
      metadata.st_mode & 0o022 == 0
    else { throw CodexRuntimeHomeError.invalidMountHelper }
    executable = resolved
    self.processInspector = processInspector
  }

  func mount(at path: String) throws {
    let process = Process()
    let completion = DispatchSemaphore(value: 0)
    let termination = ProcessTerminationObservation()
    process.executableURL = executable
    process.arguments = [
      "-o", "nodev,nosuid,noexec", "-e", "-n", "32768", "-s", "256m", path,
    ]
    process.currentDirectoryURL = URL(fileURLWithPath: "/", isDirectory: true)
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    process.terminationHandler = { _ in
      termination.markObserved()
      completion.signal()
    }
    do {
      try process.run()
    } catch {
      throw CodexRuntimeHomeError.mountFailed
    }
    let token = stableWorkerAuditToken(
      for: process.processIdentifier,
      expectedParentPID: getpid(),
      expectedEffectiveUserIdentifier: 0,
      expectedExecutableURL: executable,
      processInspector: processInspector,
      terminationObserved: { termination.isObserved }
    )
    guard let token else {
      guard completion.wait(timeout: .now() + .seconds(2)) == .success else {
        if let lateToken = stableWorkerAuditToken(
          for: process.processIdentifier,
          expectedParentPID: getpid(),
          expectedEffectiveUserIdentifier: 0,
          expectedExecutableURL: executable,
          processInspector: processInspector,
          terminationObserved: { termination.isObserved }
        ) {
          let reaper = AuditTokenProcessReaper(processInspector: processInspector)
          _ = reaper.terminateAndConfirm(lateToken, completion: completion)
        }
        throw CodexRuntimeHomeError.mountTimedOut
      }
      guard process.terminationReason == .exit, process.terminationStatus == 0 else {
        throw CodexRuntimeHomeError.mountFailed
      }
      return
    }
    guard completion.wait(timeout: .now() + .seconds(10)) == .success else {
      let reaper = AuditTokenProcessReaper(processInspector: processInspector)
      _ = reaper.terminateAndConfirm(token, completion: completion)
      throw CodexRuntimeHomeError.mountTimedOut
    }
    guard process.terminationReason == .exit, process.terminationStatus == 0 else {
      throw CodexRuntimeHomeError.mountFailed
    }
  }
}

private final class ProcessTerminationObservation: @unchecked Sendable {
  private let lock = NSLock()
  private var observed = false

  var isObserved: Bool { lock.withLock { observed } }

  func markObserved() { lock.withLock { observed = true } }
}

final class CodexRuntimeHomeManager: CodexRuntimeHomePreparing {
  static let rootPath = "/Library/Application Support/com.thesongzhu.OpenOpenRuntime"
  static let byteCapacity: UInt64 = 256 * 1024 * 1024
  static let inodeCapacity: UInt64 = 32768

  private let mounter: any CodexRuntimeHomeMounting
  private let fileSystemInspector: any CodexRuntimeFileSystemInspecting
  private let pathOperator: any CodexRuntimePathOperating
  private let effectiveUserIdentifier: () -> uid_t

  init(
    mounter: any CodexRuntimeHomeMounting,
    fileSystemInspector: any CodexRuntimeFileSystemInspecting =
      DarwinCodexRuntimeFileSystemInspector(),
    pathOperator: any CodexRuntimePathOperating = DarwinCodexRuntimePathOperator(),
    effectiveUserIdentifier: @escaping () -> uid_t = geteuid
  ) {
    self.mounter = mounter
    self.fileSystemInspector = fileSystemInspector
    self.pathOperator = pathOperator
    self.effectiveUserIdentifier = effectiveUserIdentifier
  }

  static func path(for auditEUID: uid_t) -> String {
    rootPath + "/users/" + String(auditEUID) + "/CodexHome"
  }

  func prepare(for auditEUID: uid_t) throws -> CodexRuntimeHomeReceipt {
    guard effectiveUserIdentifier() == 0, auditEUID != 0 else {
      throw CodexRuntimeHomeError.invalidCaller
    }
    let root = Self.rootPath
    let users = root + "/users"
    let user = users + "/" + String(auditEUID)
    let home = Self.path(for: auditEUID)
    try ensureDirectory(root, owner: 0, mode: 0o711)
    try ensureDirectory(users, owner: 0, mode: 0o711)
    try ensureDirectory(user, owner: 0, mode: 0o711)

    if !isExactTmpfsMount(home, auditEUID: auditEUID) {
      // Never mount over an existing but nonconforming filesystem. Only the
      // underlying root-owned empty directory is eligible for a new mount.
      guard !isFileSystemMounted(at: home) else {
        throw CodexRuntimeHomeError.invalidDirectory
      }
      try ensureDirectory(home, owner: 0, mode: 0o700)
      guard try pathOperator.directoryIsEmpty(at: home), sameDevice(home, user) else {
        throw CodexRuntimeHomeError.invalidDirectory
      }
      try mounter.mount(at: home)
      guard pathOperator.setOwner(auditEUID, group: 0, at: home),
        pathOperator.setMode(0o700, at: home)
      else { throw CodexRuntimeHomeError.invalidDirectory }
    }
    guard isExactTmpfsMount(home, auditEUID: auditEUID) else {
      throw CodexRuntimeHomeError.invalidDirectory
    }
    guard let metadata = pathOperator.pathSnapshot(at: home) else {
      throw CodexRuntimeHomeError.invalidDirectory
    }
    return CodexRuntimeHomeReceipt(
      runtimeHome: home,
      runtimeDevice: UInt64(metadata.device),
      status: "ready",
      version: 1
    )
  }

  private func ensureDirectory(_ path: String, owner: uid_t, mode: mode_t) throws {
    if pathOperator.pathSnapshot(at: path) == nil {
      guard pathOperator.createDirectory(at: path, mode: mode),
        pathOperator.setOwner(owner, group: 0, at: path),
        pathOperator.setMode(mode, at: path)
      else { throw CodexRuntimeHomeError.invalidDirectory }
    }
    guard let metadata = pathOperator.pathSnapshot(at: path), metadata.isDirectory,
      metadata.owner == owner,
      metadata.group == 0,
      metadata.mode == mode
    else { throw CodexRuntimeHomeError.invalidDirectory }
  }

  private func sameDevice(_ first: String, _ second: String) -> Bool {
    guard let firstMetadata = pathOperator.pathSnapshot(at: first),
      let secondMetadata = pathOperator.pathSnapshot(at: second)
    else { return false }
    return firstMetadata.device == secondMetadata.device
  }

  private func isExactTmpfsMount(_ path: String, auditEUID: uid_t) -> Bool {
    guard isExactTmpfsMountFileSystem(path),
      let metadata = pathOperator.pathSnapshot(at: path),
      metadata.isDirectory,
      metadata.owner == auditEUID,
      metadata.group == 0,
      metadata.mode == 0o700
    else { return false }
    return true
  }

  private func isFileSystemMounted(at path: String) -> Bool {
    fileSystemInspector.snapshot(at: path)?.mountPoint == path
  }

  private func isExactTmpfsMountFileSystem(_ path: String) -> Bool {
    let parent = URL(fileURLWithPath: path).deletingLastPathComponent().path
    guard let snapshot = fileSystemInspector.snapshot(at: path),
      let metadata = pathOperator.pathSnapshot(at: path),
      let parentMetadata = pathOperator.pathSnapshot(at: parent),
      metadata.device != parentMetadata.device,
      Self.isExactFileSystem(snapshot, mountedAt: path)
    else { return false }
    return true
  }

  static func isExactFileSystem(
    _ snapshot: CodexRuntimeFileSystemSnapshot, mountedAt path: String
  ) -> Bool {
    let (capacity, overflow) = snapshot.blockSize.multipliedReportingOverflow(by: snapshot.blocks)
    return !overflow
      && capacity == byteCapacity
      && snapshot.files == inodeCapacity
      && snapshot.flags & UInt32(MNT_NODEV | MNT_NOEXEC | MNT_NOSUID)
        == UInt32(MNT_NODEV | MNT_NOEXEC | MNT_NOSUID)
      && snapshot.typeName == "tmpfs"
      && snapshot.mountPoint == path
  }
}
