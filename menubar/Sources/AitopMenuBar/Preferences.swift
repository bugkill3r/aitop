import Foundation
import SwiftUI
import ServiceManagement

enum MenuBarItem: String, CaseIterable, Identifiable {
    case today = "Today"
    case burnRate = "Burn Rate"
    case week = "This Week"
    case allTime = "All Time"

    var id: String { rawValue }

    var shortLabel: String {
        switch self {
        case .today: return "today"
        case .burnRate: return "/hr"
        case .week: return "week"
        case .allTime: return "total"
        }
    }
}

@MainActor
final class Preferences: ObservableObject {
    static let shared = Preferences()

    private let key = "menuBarItems"

    @Published var enabledItems: Set<MenuBarItem> {
        didSet { save() }
    }

    private init() {
        if let saved = UserDefaults.standard.stringArray(forKey: key) {
            enabledItems = Set(saved.compactMap { MenuBarItem(rawValue: $0) })
        } else {
            enabledItems = [.today]
        }
    }

    private func save() {
        UserDefaults.standard.set(enabledItems.map(\.rawValue), forKey: key)
    }

    func toggle(_ item: MenuBarItem) {
        if enabledItems.contains(item) {
            // Don't allow empty — keep at least one
            if enabledItems.count > 1 {
                enabledItems.remove(item)
            }
        } else {
            enabledItems.insert(item)
        }
    }

    func isEnabled(_ item: MenuBarItem) -> Bool {
        enabledItems.contains(item)
    }

    // MARK: - Launch at Login

    @Published var launchAtLogin: Bool = SMAppService.mainApp.status == .enabled {
        didSet {
            do {
                if launchAtLogin {
                    try SMAppService.mainApp.register()
                } else {
                    try SMAppService.mainApp.unregister()
                }
            } catch {
                // Revert on failure
                launchAtLogin = SMAppService.mainApp.status == .enabled
            }
        }
    }
}
