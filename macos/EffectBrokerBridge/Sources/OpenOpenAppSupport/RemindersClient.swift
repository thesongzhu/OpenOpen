@preconcurrency import EventKit
import Foundation

@MainActor
protocol RemindersServing {
  func prepareTarget() async throws -> ReminderTarget
  func executeInitialMirror(_ start: ReminderDispatchStart) async throws -> [ReminderLink]
  func recoverMirror(for mission: ConfirmedMission) async throws -> [ReminderLink]
  func completedReminders(for links: [ReminderLink]) async throws
    -> [ReminderCompletionInput]
}

enum RemindersClientError: Error, Equatable, LocalizedError {
  case accessDenied
  case invalidMission(String)
  case invalidLinks
  case targetUnavailable
  case ambiguousCalendar
  case eventKit(String)
  case incompleteMirror(String)
  case reminderMissing(String)
  case reminderChanged(String)
  case completionDateMissing(String)

  public var errorDescription: String? {
    switch self {
    case .accessDenied:
      "OpenOpen needs full Reminders access. Allow it in System Settings, then try again."
    case .invalidMission(let reason):
      "OpenOpen cannot create Reminders because the confirmed Mission is invalid: \(reason)"
    case .invalidLinks:
      "OpenOpen cannot check completion because the saved Reminder links are incomplete or duplicated."
    case .targetUnavailable:
      "Create one Reminders list named OpenOpen, or restore the approved list and account, then try again."
    case .ambiguousCalendar:
      "More than one Reminders list is named OpenOpen in the default account. Keep one list, then try again."
    case .eventKit(let detail):
      "Reminders could not complete the request: \(detail)"
    case .incompleteMirror(let title):
      "The OpenOpen Reminders list does not exactly match the confirmed Mission near “\(title)”."
    case .reminderMissing(let title):
      "The Reminder “\(title)” is missing, so OpenOpen cannot use it as Evidence."
    case .reminderChanged(let title):
      "The Reminder “\(title)” changed, so OpenOpen cannot use it as Evidence."
    case .completionDateMissing(let title):
      "The Reminder “\(title)” is marked complete but has no completion date, so OpenOpen cannot use it as Evidence."
    }
  }
}

@MainActor
final class RemindersClient: RemindersServing {
  private static let calendarName = "OpenOpen"
  private static let markerPrefix = "Created by OpenOpen.\nOpenOpen metadata:"

  private let eventStore: EKEventStore
  private var executionClaims = ReminderExecutionClaims()

  init() {
    eventStore = EKEventStore()
  }

  init(eventStore: EKEventStore) {
    self.eventStore = eventStore
  }

  func prepareTarget() async throws -> ReminderTarget {
    try Task.checkCancellation()
    try await requireFullAccess()
    try Task.checkCancellation()
    let calendars = eventStore.calendars(for: .reminder)
    let snapshots = await fetchReminderSnapshots(in: calendars)
    let ownedIdentifiers = Set(
      snapshots.filter { Self.decodeMarker($0.notes) != nil }.map(\.calendarIdentifier)
    )
    let candidates = try calendars.map { calendar in
      guard let sourceIdentifier = calendar.source?.sourceIdentifier,
        !calendar.calendarIdentifier.isEmpty
      else { throw RemindersClientError.targetUnavailable }
      return ReminderCalendarCandidate(
        sourceIdentifier: sourceIdentifier,
        calendarIdentifier: calendar.calendarIdentifier,
        title: calendar.title,
        containsOpenOpenMarker: ownedIdentifiers.contains(calendar.calendarIdentifier)
      )
    }
    return try selectReminderTarget(candidates: candidates)
  }

  func executeInitialMirror(_ start: ReminderDispatchStart) async throws -> [ReminderLink] {
    let mission = start.mission
    try validate(mission)
    try executionClaims.consume(start)
    try Task.checkCancellation()
    try await requireFullAccess()
    try Task.checkCancellation()
    let calendar = try findApprovedCalendar(
      target: mission.reminderAuthorization.target,
      calendars: eventStore.calendars(for: .reminder)
    )
    let existing = try await exactLinks(for: mission, in: calendar, allowMissingAll: true)
    try Task.checkCancellation()
    if !existing.isEmpty { return existing }

    do {
      let dispatchByWorkItem = Dictionary(
        uniqueKeysWithValues: mission.reminderDispatch.map { ($0.workItemId, $0.token) }
      )
      for workItem in mission.workItems {
        try Task.checkCancellation()
        guard let dispatchToken = dispatchByWorkItem[workItem.id] else {
          throw RemindersClientError.invalidMission("its durable dispatch is incomplete")
        }
        let reminder = EKReminder(eventStore: eventStore)
        reminder.calendar = calendar
        reminder.title = workItem.title
        reminder.notes = try Self.marker(
          missionId: mission.missionId,
          workItemId: workItem.id,
          dispatchToken: dispatchToken
        )
        try eventStore.save(reminder, commit: false)
      }
      try Task.checkCancellation()
      try eventStore.commit()
    } catch is CancellationError {
      eventStore.reset()
      throw CancellationError()
    } catch {
      eventStore.reset()
      throw Self.eventKitError(error)
    }

    let links = try await exactLinks(for: mission, in: calendar, allowMissingAll: false)
    try Task.checkCancellation()
    return links
  }

  func recoverMirror(for mission: ConfirmedMission) async throws -> [ReminderLink] {
    try validate(mission)
    try Task.checkCancellation()
    try await requireFullAccess()
    try Task.checkCancellation()
    let calendar = try findApprovedCalendar(
      target: mission.reminderAuthorization.target,
      calendars: eventStore.calendars(for: .reminder)
    )
    let links = try await exactLinks(for: mission, in: calendar, allowMissingAll: false)
    try Task.checkCancellation()
    return links
  }

  func completedReminders(for links: [ReminderLink]) async throws
    -> [ReminderCompletionInput]
  {
    try validate(links)
    try Task.checkCancellation()
    try await requireFullAccess()
    try Task.checkCancellation()

    return try links.compactMap { link in
      try Task.checkCancellation()
      guard
        let reminder = eventStore.calendarItem(withIdentifier: link.calendarItemIdentifier)
          as? EKReminder
      else {
        throw RemindersClientError.reminderMissing(link.title)
      }
      guard reminder.calendar.calendarIdentifier == link.calendarIdentifier,
        reminder.calendar.source?.sourceIdentifier == link.sourceIdentifier,
        reminder.title == link.title,
        reminder.notes
          == (try Self.marker(
            missionId: link.missionId,
            workItemId: link.workItemId,
            dispatchToken: link.dispatchToken
          ))
      else {
        throw RemindersClientError.reminderChanged(link.title)
      }
      guard reminder.isCompleted else { return nil }
      guard let completionDate = reminder.completionDate else {
        throw RemindersClientError.completionDateMissing(link.title)
      }
      let milliseconds = completionDate.timeIntervalSince1970 * 1_000
      guard milliseconds.isFinite, milliseconds >= 0, milliseconds <= Double(Int64.max) else {
        throw RemindersClientError.completionDateMissing(link.title)
      }
      return ReminderCompletionInput(
        workItemId: link.workItemId,
        sourceId: link.calendarItemIdentifier,
        completedAtMs: Int64(milliseconds.rounded(.down))
      )
    }
  }

  private func requireFullAccess() async throws {
    do {
      guard try await eventStore.requestFullAccessToReminders() else {
        throw RemindersClientError.accessDenied
      }
    } catch let error as RemindersClientError {
      throw error
    } catch {
      throw Self.eventKitError(error)
    }
  }

  private func findApprovedCalendar(
    target: ReminderTarget, calendars: [EKCalendar]
  ) throws -> EKCalendar {
    let matches = calendars.filter {
      $0.calendarIdentifier == target.calendarIdentifier
        && $0.source?.sourceIdentifier == target.sourceIdentifier
    }
    guard matches.count <= 1 else { throw RemindersClientError.ambiguousCalendar }
    guard let calendar = matches.first else { throw RemindersClientError.targetUnavailable }
    return calendar
  }

  private func exactLinks(
    for mission: ConfirmedMission,
    in calendar: EKCalendar,
    allowMissingAll: Bool
  ) async throws -> [ReminderLink] {
    let reminders = await fetchReminderSnapshots(in: [calendar])
    var remindersByWorkItem: [String: [ReminderSnapshot]] = [:]
    var missionReminderCount = 0
    let dispatchByWorkItem = Dictionary(
      uniqueKeysWithValues: mission.reminderDispatch.map { ($0.workItemId, $0.token) }
    )

    for reminder in reminders {
      guard let marker = Self.decodeMarker(reminder.notes),
        marker.missionId == mission.missionId
      else { continue }
      missionReminderCount += 1
      guard dispatchByWorkItem[marker.workItemId] == marker.dispatchToken else {
        throw RemindersClientError.reminderChanged(reminder.title)
      }
      remindersByWorkItem[marker.workItemId, default: []].append(reminder)
    }

    if allowMissingAll, missionReminderCount == 0 { return [] }

    let expectedIds = Set(mission.workItems.map(\.id))
    guard missionReminderCount == mission.workItems.count,
      Set(remindersByWorkItem.keys) == expectedIds
    else {
      throw RemindersClientError.incompleteMirror(mission.title)
    }

    return try mission.workItems.map { workItem in
      guard let matches = remindersByWorkItem[workItem.id], matches.count == 1,
        let reminder = matches.first
      else {
        throw RemindersClientError.incompleteMirror(workItem.title)
      }
      guard !reminder.identifier.isEmpty,
        reminder.title == workItem.title,
        reminder.notes
          == (try Self.marker(
            missionId: mission.missionId,
            workItemId: workItem.id,
            dispatchToken: dispatchByWorkItem[workItem.id] ?? ""
          ))
      else {
        throw RemindersClientError.reminderChanged(workItem.title)
      }
      guard let sourceIdentifier = calendar.source?.sourceIdentifier else {
        throw RemindersClientError.targetUnavailable
      }
      return ReminderLink(
        missionId: mission.missionId,
        workItemId: workItem.id,
        sourceIdentifier: sourceIdentifier,
        calendarIdentifier: calendar.calendarIdentifier,
        calendarItemIdentifier: reminder.identifier,
        dispatchToken: dispatchByWorkItem[workItem.id] ?? "",
        title: workItem.title
      )
    }
  }

  private func fetchReminderSnapshots(in calendars: [EKCalendar]) async -> [ReminderSnapshot] {
    guard !calendars.isEmpty else { return [] }
    let predicate = eventStore.predicateForReminders(in: calendars)
    return await withCheckedContinuation { continuation in
      eventStore.fetchReminders(matching: predicate) { reminders in
        continuation.resume(
          returning: (reminders ?? []).map {
            ReminderSnapshot(
              identifier: $0.calendarItemIdentifier,
              calendarIdentifier: $0.calendar.calendarIdentifier,
              title: $0.title,
              notes: $0.notes
            )
          }
        )
      }
    }
  }

  private func validate(_ mission: ConfirmedMission) throws {
    guard !mission.missionId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      throw RemindersClientError.invalidMission("its identifier is empty")
    }
    guard !mission.workItems.isEmpty else {
      throw RemindersClientError.invalidMission("it has no work items")
    }
    guard Set(mission.workItems.map(\.id)).count == mission.workItems.count else {
      throw RemindersClientError.invalidMission("work item identifiers are not unique")
    }
    guard
      mission.workItems.allSatisfy({
        !$0.id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          && !$0.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      })
    else {
      throw RemindersClientError.invalidMission("a work item identifier or title is empty")
    }
    let authorization = mission.reminderAuthorization
    guard
      authorization.validates(
        missionId: mission.missionId, workItems: mission.workItems
      )
    else {
      throw RemindersClientError.invalidMission(
        "its Core authorization does not match the exact Reminder payload"
      )
    }
    guard mission.reminderDispatch.count == mission.workItems.count,
      Set(mission.reminderDispatch.map(\.workItemId)) == Set(mission.workItems.map(\.id)),
      Set(mission.reminderDispatch.map(\.token)).count == mission.reminderDispatch.count,
      mission.reminderDispatch.allSatisfy({
        !$0.token.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      })
    else {
      throw RemindersClientError.invalidMission("its durable dispatch is incomplete")
    }
  }

  private func validate(_ links: [ReminderLink]) throws {
    guard !links.isEmpty,
      Set(links.map(\.missionId)).count == 1,
      Set(links.map(\.workItemId)).count == links.count,
      Set(links.map(\.sourceIdentifier)).count == 1,
      Set(links.map(\.calendarIdentifier)).count == 1,
      Set(links.map(\.calendarItemIdentifier)).count == links.count,
      Set(links.map(\.dispatchToken)).count == links.count,
      links.allSatisfy({
        !$0.missionId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          && !$0.workItemId.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          && !$0.sourceIdentifier.isEmpty
          && !$0.calendarIdentifier.isEmpty
          && !$0.calendarItemIdentifier.isEmpty
          && !$0.dispatchToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          && !$0.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      })
    else {
      throw RemindersClientError.invalidLinks
    }
  }

  private static func marker(
    missionId: String, workItemId: String, dispatchToken: String
  ) throws -> String {
    let encoded: Data
    do {
      let encoder = JSONEncoder()
      encoder.outputFormatting = [.sortedKeys]
      encoded = try encoder.encode(
        ReminderMarker(
          version: 2,
          missionId: missionId,
          workItemId: workItemId,
          dispatchToken: dispatchToken
        )
      )
    } catch {
      throw eventKitError(error)
    }
    return "\(markerPrefix)\(encoded.base64EncodedString())"
  }

  private static func decodeMarker(_ notes: String?) -> ReminderMarker? {
    guard let notes, notes.hasPrefix(markerPrefix) else { return nil }
    let payload = String(notes.dropFirst(markerPrefix.count))
    guard let data = Data(base64Encoded: payload),
      let marker = try? JSONDecoder().decode(ReminderMarker.self, from: data),
      marker.version == 2
    else { return nil }
    return marker
  }

  private static func eventKitError(_ error: Error) -> RemindersClientError {
    if let error = error as? RemindersClientError { return error }
    let description = (error as NSError).localizedDescription
    return .eventKit(description.isEmpty ? "EventKit returned an unknown error." : description)
  }
}

struct ReminderCalendarCandidate: Equatable, Sendable {
  let sourceIdentifier: String
  let calendarIdentifier: String
  let title: String
  let containsOpenOpenMarker: Bool
}

func selectReminderTarget(candidates: [ReminderCalendarCandidate]) throws -> ReminderTarget {
  let owned = candidates.filter(\.containsOpenOpenMarker)
  guard owned.count <= 1 else { throw RemindersClientError.ambiguousCalendar }
  if let calendar = owned.first {
    return ReminderTarget(
      sourceIdentifier: calendar.sourceIdentifier,
      calendarIdentifier: calendar.calendarIdentifier
    )
  }
  let named = candidates.filter { $0.title == "OpenOpen" }
  guard named.count <= 1 else { throw RemindersClientError.ambiguousCalendar }
  if let calendar = named.first {
    return ReminderTarget(
      sourceIdentifier: calendar.sourceIdentifier,
      calendarIdentifier: calendar.calendarIdentifier
    )
  }
  throw RemindersClientError.targetUnavailable
}

struct ReminderExecutionClaims {
  private var consumedMissionIds: Set<String> = []

  mutating func consume(_ start: ReminderDispatchStart) throws {
    guard start.executeNow, start.mission.reminderLinks.isEmpty else {
      throw RemindersClientError.invalidMission("Core did not issue initial execution authority")
    }
    guard consumedMissionIds.insert(start.mission.missionId).inserted else {
      throw RemindersClientError.incompleteMirror(start.mission.title)
    }
  }
}

private struct ReminderMarker: Codable, Equatable {
  let version: Int
  let missionId: String
  let workItemId: String
  let dispatchToken: String
}

private struct ReminderSnapshot: Sendable {
  let identifier: String
  let calendarIdentifier: String
  let title: String
  let notes: String?
}
