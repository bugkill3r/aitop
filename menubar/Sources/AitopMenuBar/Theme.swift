import SwiftUI

enum Theme {
    // Ember accent — matches aitop TUI
    static let accent = Color(red: 1.0, green: 0.6, blue: 0.2) // amber/orange
    static let live = Color.green
    static let idle = Color.gray

    static let sectionHeader = Color.white.opacity(0.5)
    static let separator = Color.white.opacity(0.1)
    static let barBackground = Color.white.opacity(0.08)
    static let barFill = accent.opacity(0.7)

    static let primaryText = Color.white
    static let secondaryText = Color.white.opacity(0.6)
    static let tertiaryText = Color.white.opacity(0.4)

    static let popoverBackground = Color(nsColor: .windowBackgroundColor)

    static func formatCurrency(_ value: Double) -> String {
        if value >= 1000 {
            return String(format: "$%.0f", value)
        } else if value >= 100 {
            return String(format: "$%.1f", value)
        } else {
            return String(format: "$%.2f", value)
        }
    }

    static func formatRate(_ value: Double) -> String {
        return "\(formatCurrency(value))/hr"
    }
}
