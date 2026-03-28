import Foundation
import GRDB
import Combine

@MainActor
final class DataStore: ObservableObject {
    @Published var stats = DashboardStats()
    @Published var topModels: [ModelStat] = []
    @Published var recentSessions: [RecentSession] = []
    @Published var cacheStats = CacheStats()
    @Published var dbAvailable = false

    private var dbPool: DatabasePool?
    private var timer: AnyCancellable?

    private static var dbPath: String {
        // Match Rust's dirs::data_local_dir() which on macOS = ~/Library/Application Support
        let dataDir: String
        if let xdg = ProcessInfo.processInfo.environment["XDG_DATA_HOME"] {
            dataDir = xdg
        } else {
            let urls = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            dataDir = urls.first?.path ?? (NSHomeDirectory() + "/Library/Application Support")
        }
        return dataDir + "/aitop/sessions.db"
    }

    func start() {
        openDatabase()
        refresh()

        timer = Timer.publish(every: 5, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                Task { @MainActor in
                    self?.refresh()
                }
            }
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }

    private func openDatabase() {
        let path = Self.dbPath
        guard FileManager.default.fileExists(atPath: path) else {
            dbAvailable = false
            return
        }

        do {
            var config = Configuration()
            config.readonly = true
            config.prepareDatabase { db in
                // Match aitop's WAL mode for safe concurrent reads
                try db.execute(sql: "PRAGMA journal_mode=WAL")
            }
            dbPool = try DatabasePool(path: path, configuration: config)
            dbAvailable = true
        } catch {
            dbAvailable = false
        }
    }

    func refresh() {
        // Retry opening if DB wasn't available before
        if dbPool == nil {
            openDatabase()
        }

        guard let db = dbPool else {
            dbAvailable = false
            return
        }

        do {
            // Fetch on GRDB's queue, then assign on MainActor
            let (newStats, newModels, newSessions, newCache) = try db.read { db in
                let s = try Self.fetchDashboardStats(db)
                let m = try Self.fetchTopModels(db)
                let r = try Self.fetchRecentSessions(db)
                let c = try Self.fetchCacheStats(db)
                return (s, m, r, c)
            }
            stats = newStats
            topModels = newModels
            recentSessions = newSessions
            cacheStats = newCache
            dbAvailable = true
        } catch {
            // DB may have been deleted or corrupted — reset
            dbPool = nil
            dbAvailable = false
        }
    }

    // MARK: - Queries

    private static func fetchDashboardStats(_ db: Database) throws -> DashboardStats {
        var stats = DashboardStats()

        // Burn rate: cost in last hour, already per-hour
        if let row = try Row.fetchOne(db, sql: """
            SELECT COALESCE(SUM(cost_usd), 0) FROM messages
            WHERE timestamp > datetime('now', '-1 hour')
            """) {
            stats.burnRatePerHour = row[0]
        }

        // Today's spend (localtime so "today" matches user's timezone)
        if let row = try Row.fetchOne(db, sql: """
            SELECT COALESCE(SUM(cost_usd), 0) FROM messages
            WHERE date(timestamp, 'localtime') = date('now', 'localtime')
            """) {
            stats.spendToday = row[0]
        }

        // This week (localtime)
        if let row = try Row.fetchOne(db, sql: """
            SELECT COALESCE(SUM(cost_usd), 0) FROM messages
            WHERE datetime(timestamp, 'localtime') > datetime('now', 'localtime', '-7 days')
            """) {
            stats.spendThisWeek = row[0]
        }

        // All time
        if let row = try Row.fetchOne(db, sql: """
            SELECT COALESCE(SUM(cost_usd), 0) FROM messages
            """) {
            stats.spendAllTime = row[0]
        }

        // LIVE detection: any message in last 5 minutes
        if let row = try Row.fetchOne(db, sql: """
            SELECT EXISTS(
                SELECT 1 FROM messages
                WHERE timestamp >= datetime('now', '-5 minutes')
            )
            """) {
            stats.isLive = (row[0] as Int) == 1
        }

        return stats
    }

    private static func fetchTopModels(_ db: Database) throws -> [ModelStat] {
        let rows = try Row.fetchAll(db, sql: """
            SELECT COALESCE(model, 'unknown') as model_name,
                   SUM(cost_usd) as total_cost,
                   COALESCE(provider, 'claude') as prov
            FROM messages
            WHERE model IS NOT NULL AND model != ''
            GROUP BY model
            HAVING SUM(cost_usd) > 0
            ORDER BY total_cost DESC
            LIMIT 5
            """)

        let totalCost = rows.reduce(0.0) { $0 + ($1["total_cost"] as Double? ?? 0) }

        return rows.map { row in
            let cost: Double = row["total_cost"] ?? 0
            return ModelStat(
                model: row["model_name"] ?? "unknown",
                cost: cost,
                percentage: totalCost > 0 ? (cost / totalCost) * 100 : 0,
                provider: row["prov"] ?? "claude"
            )
        }
    }

    private static func fetchRecentSessions(_ db: Database) throws -> [RecentSession] {
        let rows = try Row.fetchAll(db, sql: """
            SELECT s.project, COALESCE(s.model, 'unknown') as model,
                   COALESCE(SUM(m.cost_usd), 0) as total_cost,
                   s.updated_at,
                   COALESCE(s.provider, 'claude') as prov
            FROM sessions s
            LEFT JOIN messages m ON s.id = m.session_id
            GROUP BY s.id
            ORDER BY s.updated_at DESC
            LIMIT 5
            """)

        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]

        let fallbackFormatter = ISO8601DateFormatter()
        fallbackFormatter.formatOptions = [.withInternetDateTime]

        return rows.compactMap { row -> RecentSession? in
            let updatedAtStr: String = row["updated_at"] ?? ""
            let date = formatter.date(from: updatedAtStr)
                ?? fallbackFormatter.date(from: updatedAtStr)
                ?? Date.distantPast

            return RecentSession(
                project: Self.shortProject(row["project"] ?? "unknown"),
                model: Self.shortModel(row["model"] ?? "unknown"),
                cost: row["total_cost"] ?? 0,
                updatedAt: date,
                provider: row["prov"] ?? "claude"
            )
        }
    }

    private static func fetchCacheStats(_ db: Database) throws -> CacheStats {
        var stats = CacheStats()

        if let row = try Row.fetchOne(db, sql: """
            SELECT COALESCE(SUM(cache_read), 0) as cr,
                   COALESCE(SUM(input_tokens + cache_read + cache_creation), 0) as total
            FROM messages
            """) {
            let cacheRead: Int64 = row["cr"] ?? 0
            let total: Int64 = row["total"] ?? 0
            stats.hitRatio = total > 0 ? Double(cacheRead) / Double(total) * 100 : 0
        }

        return stats
    }

    // MARK: - Helpers

    /// Shorten project path: "/Users/foo/Dev/myproject" -> "myproject"
    private static func shortProject(_ project: String) -> String {
        let components = project.split(separator: "/")
        return String(components.last ?? Substring(project))
    }

    /// Shorten model name: "claude-opus-4-6" -> "opus-4-6"
    private static func shortModel(_ model: String) -> String {
        var name = model
        for prefix in ["claude-", "gemini-", "gpt-"] {
            if name.hasPrefix(prefix) {
                name = String(name.dropFirst(prefix.count))
                break
            }
        }
        return name
    }
}
