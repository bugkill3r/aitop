import SwiftUI

struct MenuBarLabel: View {
    @ObservedObject var store: DataStore
    @ObservedObject var prefs: Preferences = .shared

    var body: some View {
        if store.dbAvailable {
            HStack(spacing: 4) {
                Image(systemName: store.stats.isLive ? "flame.fill" : "flame")
                    .imageScale(.medium)

                Text(labelText)
                    .monospacedDigit()
            }
        } else {
            Image(systemName: "flame")
        }
    }

    private var labelText: String {
        let items: [(MenuBarItem, String)] = MenuBarItem.allCases.compactMap { item in
            guard prefs.isEnabled(item) else { return nil }
            return (item, formattedValue(item))
        }

        if items.isEmpty {
            return formattedValue(.today)
        }

        // Single item: show with suffix for context
        // Multiple items: show with suffix to distinguish
        return items.map(\.1).joined(separator: " · ")
    }

    private func formattedValue(_ item: MenuBarItem) -> String {
        switch item {
        case .today:
            return "\(Theme.formatCurrency(store.stats.spendToday)) today"
        case .burnRate:
            return "\(Theme.formatCurrency(store.stats.burnRatePerHour))/hr"
        case .week:
            return "\(Theme.formatCurrency(store.stats.spendThisWeek))/wk"
        case .allTime:
            return "\(Theme.formatCurrency(store.stats.spendAllTime)) total"
        }
    }
}
