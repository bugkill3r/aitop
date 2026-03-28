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
        // Build label from enabled items in a stable order
        let ordered: [(MenuBarItem, String)] = MenuBarItem.allCases.compactMap { item in
            guard prefs.isEnabled(item) else { return nil }
            let value: String
            switch item {
            case .today:
                value = "\(Theme.formatCurrency(store.stats.spendToday)) today"
            case .burnRate:
                value = Theme.formatRate(store.stats.burnRatePerHour)
            case .week:
                value = "\(Theme.formatCurrency(store.stats.spendThisWeek)) wk"
            case .allTime:
                value = Theme.formatCurrency(store.stats.spendAllTime)
            }
            return (item, value)
        }

        if ordered.isEmpty {
            return Theme.formatCurrency(store.stats.spendToday)
        }

        return ordered.map(\.1).joined(separator: "  ")
    }
}
