import Foundation

struct DashboardStats: Sendable {
    var burnRatePerHour: Double = 0
    var spendToday: Double = 0
    var spendThisWeek: Double = 0
    var spendAllTime: Double = 0
    var isLive: Bool = false
}

struct ModelStat: Identifiable, Sendable {
    let model: String
    let cost: Double
    let percentage: Double
    let provider: String

    var id: String { model }
}

struct RecentSession: Identifiable, Sendable {
    let project: String
    let model: String
    let cost: Double
    let updatedAt: Date
    let provider: String

    var id: String { "\(project)-\(model)-\(updatedAt.timeIntervalSince1970)" }

    var relativeTime: String {
        let interval = Date().timeIntervalSince(updatedAt)
        if interval < 60 { return "now" }
        if interval < 3600 { return "\(Int(interval / 60))m" }
        if interval < 86400 { return "\(Int(interval / 3600))h" }
        return "\(Int(interval / 86400))d"
    }
}

struct CacheStats: Sendable {
    var hitRatio: Double = 0
}
