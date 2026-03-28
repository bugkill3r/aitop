import SwiftUI

struct MenuBarView: View {
    @ObservedObject var store: DataStore
    @State private var showSettings = false

    var body: some View {
        VStack(spacing: 0) {
            if store.dbAvailable {
                if showSettings {
                    SettingsView(isShowing: $showSettings)
                } else {
                    heroSection
                        .padding(.horizontal, 16)
                        .padding(.top, 14)
                        .padding(.bottom, 12)

                    spendGrid
                        .padding(.horizontal, 16)
                        .padding(.bottom, 14)

                    sectionDivider

                    modelsSection
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)

                    sectionDivider

                    sessionsSection
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)

                    sectionDivider

                    footerSection
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                }
            } else {
                noDataView
            }
        }
        .frame(width: 300)
    }

    // MARK: - Divider

    private var sectionDivider: some View {
        Rectangle()
            .fill(Color.white.opacity(0.06))
            .frame(height: 1)
            .padding(.horizontal, 12)
    }

    // MARK: - Hero: Burn Rate + Status

    private var heroSection: some View {
        VStack(spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(Theme.formatRate(store.stats.burnRatePerHour))
                    .font(.system(size: 28, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(Theme.primaryText)

                Spacer()

                HStack(spacing: 5) {
                    Circle()
                        .fill(store.stats.isLive ? Theme.live : Theme.idle)
                        .frame(width: 7, height: 7)
                    Text(store.stats.isLive ? "LIVE" : "IDLE")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(store.stats.isLive ? Theme.live : Theme.idle)
                }
            }

            // Cache efficiency as a subtle subtitle
            HStack {
                Text("Cache \(String(format: "%.0f%%", store.cacheStats.hitRatio))")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.tertiaryText)
                Spacer()
            }
        }
    }

    // MARK: - Spend Grid (2x2)

    private var spendGrid: some View {
        HStack(spacing: 10) {
            spendCard("Today", Theme.formatCurrency(store.stats.spendToday), prominent: true)
            spendCard("Week", Theme.formatCurrency(store.stats.spendThisWeek), prominent: false)
            spendCard("All Time", Theme.formatCurrency(store.stats.spendAllTime), prominent: false)
        }
    }

    private func spendCard(_ label: String, _ value: String, prominent: Bool) -> some View {
        VStack(spacing: 3) {
            Text(value)
                .font(.system(size: prominent ? 16 : 14, weight: .medium, design: .rounded))
                .monospacedDigit()
                .foregroundStyle(prominent ? Theme.accent : Theme.primaryText)
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(Theme.tertiaryText)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(Color.white.opacity(prominent ? 0.07 : 0.04))
        )
    }

    // MARK: - Models

    private var modelsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Models")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(Theme.tertiaryText)
                .textCase(.uppercase)
                .tracking(0.5)

            if store.topModels.isEmpty {
                Text("No data")
                    .font(.caption)
                    .foregroundStyle(Theme.tertiaryText)
            } else {
                ForEach(store.topModels) { model in
                    modelRow(model)
                }
            }
        }
    }

    private func modelRow(_ model: ModelStat) -> some View {
        HStack(spacing: 0) {
            // Bar + name
            HStack(spacing: 8) {
                RoundedRectangle(cornerRadius: 1.5)
                    .fill(Theme.accent.opacity(0.8))
                    .frame(width: max(3, 40 * CGFloat(model.percentage / 100)), height: 14)

                Text(model.model)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(Theme.secondaryText)
                    .lineLimit(1)
            }

            Spacer(minLength: 8)

            Text(Theme.formatCurrency(model.cost))
                .font(.system(size: 11, weight: .medium))
                .monospacedDigit()
                .foregroundStyle(Theme.primaryText)

            Text(String(format: "%.0f%%", model.percentage))
                .font(.system(size: 10))
                .monospacedDigit()
                .foregroundStyle(Theme.tertiaryText)
                .frame(width: 28, alignment: .trailing)
        }
        .frame(height: 20)
    }

    // MARK: - Recent Sessions

    private var sessionsSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Recent")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(Theme.tertiaryText)
                .textCase(.uppercase)
                .tracking(0.5)

            if store.recentSessions.isEmpty {
                Text("No sessions")
                    .font(.caption)
                    .foregroundStyle(Theme.tertiaryText)
            } else {
                ForEach(store.recentSessions) { session in
                    HStack(spacing: 0) {
                        Text(session.project)
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(Theme.primaryText)
                            .lineLimit(1)

                        Text("  \(session.model)")
                            .font(.system(size: 10))
                            .foregroundStyle(Theme.tertiaryText)
                            .lineLimit(1)

                        Spacer(minLength: 8)

                        Text(Theme.formatCurrency(session.cost))
                            .font(.system(size: 11, weight: .medium))
                            .monospacedDigit()
                            .foregroundStyle(Theme.primaryText)

                        Text(session.relativeTime)
                            .font(.system(size: 10))
                            .foregroundStyle(Theme.tertiaryText)
                            .frame(width: 26, alignment: .trailing)
                    }
                    .frame(height: 18)
                }
            }
        }
    }

    // MARK: - Footer

    private var footerSection: some View {
        HStack {
            Button {
                openAitop()
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "terminal")
                        .font(.system(size: 10))
                    Text("Open aitop")
                        .font(.system(size: 11))
                }
            }
            .buttonStyle(.plain)
            .foregroundStyle(Theme.accent)

            Spacer()

            Button {
                withAnimation(.easeInOut(duration: 0.15)) {
                    showSettings = true
                }
            } label: {
                Image(systemName: "gearshape")
                    .font(.system(size: 12))
            }
            .buttonStyle(.plain)
            .foregroundStyle(Theme.tertiaryText)

            Button {
                NSApplication.shared.terminate(nil)
            } label: {
                Text("Quit")
                    .font(.system(size: 11))
            }
            .buttonStyle(.plain)
            .foregroundStyle(Theme.tertiaryText)
            .padding(.leading, 8)
        }
    }

    // MARK: - No Data

    private var noDataView: some View {
        VStack(spacing: 12) {
            Image(systemName: "flame")
                .font(.system(size: 32))
                .foregroundStyle(Theme.tertiaryText)
            Text("No data yet")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(Theme.primaryText)
            Text("Run aitop to start tracking spend")
                .font(.system(size: 12))
                .foregroundStyle(Theme.secondaryText)
        }
        .padding(32)
    }

    // MARK: - Actions

    private func openAitop() {
        let fm = FileManager.default

        let script: String
        if fm.fileExists(atPath: "/Applications/iTerm.app") {
            script = """
            tell application "iTerm"
                activate
                create window with default profile command "aitop"
            end tell
            """
        } else if fm.fileExists(atPath: "/Applications/Warp.app") {
            script = """
            tell application "Warp" to activate
            delay 0.3
            tell application "System Events" to tell process "Warp" to keystroke "t" using command down
            delay 0.2
            tell application "System Events" to tell process "Warp" to keystroke "aitop\n"
            """
        } else {
            script = """
            tell application "Terminal"
                activate
                do script "aitop"
            end tell
            """
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", script]
        try? process.run()
    }
}
