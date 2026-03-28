import SwiftUI

struct SettingsView: View {
    @ObservedObject var prefs: Preferences = .shared
    @Binding var isShowing: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Menu Bar Display")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Theme.primaryText)
                Spacer()
                Button {
                    withAnimation(.easeInOut(duration: 0.15)) {
                        isShowing = false
                    }
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(Theme.tertiaryText)
                }
                .buttonStyle(.plain)
            }

            Text("Choose what to show in the menu bar")
                .font(.system(size: 11))
                .foregroundStyle(Theme.tertiaryText)

            VStack(spacing: 2) {
                ForEach(MenuBarItem.allCases) { item in
                    settingsRow(item)
                }
            }

            Rectangle()
                .fill(Color.white.opacity(0.06))
                .frame(height: 1)
                .padding(.vertical, 4)

            Button {
                prefs.launchAtLogin.toggle()
            } label: {
                HStack {
                    Image(systemName: prefs.launchAtLogin ? "checkmark.circle.fill" : "circle")
                        .font(.system(size: 14))
                        .foregroundStyle(prefs.launchAtLogin ? Theme.accent : Theme.tertiaryText)

                    Text("Launch at Login")
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.primaryText)

                    Spacer()
                }
                .padding(.vertical, 6)
                .padding(.horizontal, 8)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(prefs.launchAtLogin ? Color.white.opacity(0.04) : Color.clear)
                )
            }
            .buttonStyle(.plain)
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color.white.opacity(0.05))
        )
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }

    private func settingsRow(_ item: MenuBarItem) -> some View {
        Button {
            withAnimation(.easeInOut(duration: 0.15)) {
                prefs.toggle(item)
            }
        } label: {
            HStack {
                Image(systemName: prefs.isEnabled(item) ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 14))
                    .foregroundStyle(prefs.isEnabled(item) ? Theme.accent : Theme.tertiaryText)

                Text(item.rawValue)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.primaryText)

                Spacer()

                Text(previewValue(item))
                    .font(.system(size: 11))
                    .monospacedDigit()
                    .foregroundStyle(Theme.tertiaryText)
            }
            .padding(.vertical, 6)
            .padding(.horizontal, 8)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(prefs.isEnabled(item) ? Color.white.opacity(0.04) : Color.clear)
            )
        }
        .buttonStyle(.plain)
    }

    private func previewValue(_ item: MenuBarItem) -> String {
        switch item {
        case .today: return "$59 today"
        case .burnRate: return "$49/hr"
        case .week: return "$257/wk"
        case .allTime: return "$1184 total"
        }
    }
}
