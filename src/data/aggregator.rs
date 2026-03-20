use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

use super::pricing::PricingRegistry;
use crate::ui::format::shorten_model;

/// Top-level stats for the dashboard.
#[derive(Debug, Clone, Default)]
pub struct DashboardStats {
    pub burn_rate_per_hour: f64,
    pub spend_today: f64,
    pub spend_this_week: f64,
    pub spend_all_time: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cache_read: i64,
    pub total_messages: i64,
    pub total_sessions: i64,
}

/// Per-model breakdown.
#[derive(Debug, Clone)]
pub struct ModelStats {
    pub model: String,
    pub cost: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_creation: i64,
    pub call_count: i64,
    pub provider: String,
}

/// Session summary for the sessions list.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: String,
    pub project: String,
    pub model: String,
    pub total_cost: f64,
    pub total_tokens: i64,
    pub msg_count: i64,
    pub started_at: String,
    pub updated_at: String,
    pub provider: String,
}

/// Daily spend data point.
#[derive(Debug, Clone)]
pub struct DailySpend {
    pub date: String,
    pub cost: f64,
}

/// Hourly token flow data point (for sparkline).
#[derive(Debug, Clone)]
pub struct TokenFlowPoint {
    pub minute: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
}

/// Recent activity entry.
#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub timestamp: String,
    pub project: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cost_usd: f64,
    pub provider: String,
}

/// Individual message within a session (for detail popup).
#[derive(Debug, Clone)]
pub struct SessionMessage {
    pub id: String,
    pub timestamp: String,
    pub model: String,
    pub msg_type: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_creation: i64,
    pub cost_usd: f64,
}

/// Delta banner data: changes since last check.
#[derive(Debug, Clone, Default)]
pub struct DeltaBanner {
    pub last_checked_label: String,  // e.g., "2h ago"
    pub spend_delta: f64,
    pub new_sessions: i64,
    pub model_changes: Vec<ModelChange>,
}

/// Per-model change since last check.
#[derive(Debug, Clone)]
pub struct ModelChange {
    pub model: String,
    pub pct_change: f64, // positive = increase
}

/// Per-project cost attribution.
#[derive(Debug, Clone)]
pub struct ProjectCost {
    pub name: String,
    pub cost: f64,
    pub percentage: f64,
}

/// Token efficiency stats.
#[derive(Debug, Clone, Default)]
pub struct EfficiencyStats {
    pub tokens_per_dollar: f64,
    pub tokens_per_dollar_last_week: f64,
    pub efficiency_change_pct: f64,
    pub cache_savings_usd: f64,
}

/// Daily token count for the token overlay in trends.
#[derive(Debug, Clone)]
pub struct DailyTokenCount {
    pub date: String,
    pub total_tokens: i64,
}

/// Contribution calendar day.
#[derive(Debug, Clone)]
pub struct ContributionDay {
    pub date: String,
    pub cost: f64,
}

pub struct Aggregator {
    conn: Connection,
    pricing: PricingRegistry,
}

impl Aggregator {
    pub fn open(db_path: &Path) -> Result<Self> {
        Self::open_with_pricing(db_path, PricingRegistry::builtin())
    }

    pub fn open_with_pricing(db_path: &Path, pricing: PricingRegistry) -> Result<Self> {
        let conn = Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Ok(Aggregator { conn, pricing })
    }

    pub fn dashboard_stats(&self) -> Result<DashboardStats> {
        let mut stats = DashboardStats::default();

        // All-time totals
        self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0), COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0), COALESCE(SUM(cache_read), 0),
                    COUNT(*)
             FROM messages",
            [],
            |row| {
                stats.spend_all_time = row.get(0)?;
                stats.total_input_tokens = row.get(1)?;
                stats.total_output_tokens = row.get(2)?;
                stats.total_cache_read = row.get(3)?;
                stats.total_messages = row.get(4)?;
                Ok(())
            },
        )?;

        // Total sessions
        stats.total_sessions = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions",
            [],
            |row| row.get(0),
        )?;

        // Today's spend
        stats.spend_today = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM messages WHERE date(timestamp) = date('now')",
            [],
            |row| row.get(0),
        )?;

        // This week's spend (last 7 days)
        stats.spend_this_week = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM messages WHERE timestamp > datetime('now', '-7 days')",
            [],
            |row| row.get(0),
        )?;

        // Burn rate: cost in last hour extrapolated to $/hr
        stats.burn_rate_per_hour = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM messages WHERE timestamp > datetime('now', '-1 hour')",
            [],
            |row| row.get(0),
        )?;

        Ok(stats)
    }

    pub fn model_breakdown(&self) -> Result<Vec<ModelStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(model, 'unknown'), SUM(cost_usd), SUM(input_tokens),
                    SUM(output_tokens), SUM(cache_read), SUM(cache_creation), COUNT(*),
                    COALESCE(provider, 'claude')
             FROM messages
             WHERE model IS NOT NULL AND model != ''
             GROUP BY model
             HAVING SUM(cost_usd) > 0 OR SUM(input_tokens) > 0 OR SUM(output_tokens) > 0
             ORDER BY SUM(cost_usd) DESC",
        )?;

        let raw_rows: Vec<ModelStats> = stmt.query_map([], |row| {
            Ok(ModelStats {
                model: row.get(0)?,
                cost: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                cache_read: row.get(4)?,
                cache_creation: row.get(5)?,
                call_count: row.get(6)?,
                provider: row.get(7)?,
            })
        })?.filter_map(|r| r.ok()).collect();

        // Merge rows with the same shortened model name (e.g. claude-haiku-4-5-20251001
        // and claude-haiku-4-5 both shorten to haiku-4-5)
        let mut merged: std::collections::HashMap<String, ModelStats> =
            std::collections::HashMap::new();
        for row in raw_rows {
            let key = shorten_model(&row.model);
            merged
                .entry(key.clone())
                .and_modify(|existing| {
                    existing.cost += row.cost;
                    existing.input_tokens += row.input_tokens;
                    existing.output_tokens += row.output_tokens;
                    existing.cache_read += row.cache_read;
                    existing.cache_creation += row.cache_creation;
                    existing.call_count += row.call_count;
                })
                .or_insert(ModelStats {
                    model: key,
                    ..row
                });
        }

        let mut result: Vec<ModelStats> = merged.into_values().collect();
        result.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
        Ok(result)
    }

    pub fn sessions_list(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.project, COALESCE(s.model, 'unknown'), s.started_at, s.updated_at,
                    COALESCE(SUM(m.cost_usd), 0),
                    COALESCE(SUM(m.input_tokens + m.output_tokens), 0),
                    COUNT(m.id),
                    COALESCE(s.provider, 'claude')
             FROM sessions s
             LEFT JOIN messages m ON s.id = m.session_id
             GROUP BY s.id
             ORDER BY s.updated_at DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                project: row.get(1)?,
                model: row.get(2)?,
                started_at: row.get(3)?,
                updated_at: row.get(4)?,
                total_cost: row.get(5)?,
                total_tokens: row.get(6)?,
                msg_count: row.get(7)?,
                provider: row.get(8)?,
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Daily total tokens for the token overlay chart.
    pub fn daily_tokens(&self, days: i32) -> Result<Vec<DailyTokenCount>> {
        let mut stmt = self.conn.prepare(
            "SELECT date(timestamp) as day, SUM(input_tokens + output_tokens)
             FROM messages
             WHERE timestamp > datetime('now', ?1)
             GROUP BY day
             ORDER BY day",
        )?;

        let range = format!("-{} days", days);
        let rows = stmt.query_map(params![range], |row| {
            Ok(DailyTokenCount {
                date: row.get(0)?,
                total_tokens: row.get(1)?,
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn daily_spend(&self, days: i32) -> Result<Vec<DailySpend>> {
        let mut stmt = self.conn.prepare(
            "SELECT date(timestamp) as day, SUM(cost_usd)
             FROM messages
             WHERE timestamp > datetime('now', ?1)
             GROUP BY day
             ORDER BY day",
        )?;

        let range = format!("-{} days", days);
        let rows = stmt.query_map(params![range], |row| {
            Ok(DailySpend {
                date: row.get(0)?,
                cost: row.get(1)?,
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn token_flow_last_hour(&self) -> Result<Vec<TokenFlowPoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT strftime('%H:%M', timestamp) as minute,
                    SUM(input_tokens), SUM(output_tokens),
                    SUM(input_tokens + output_tokens)
             FROM messages
             WHERE timestamp > datetime('now', '-1 hour')
             GROUP BY minute
             ORDER BY minute",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(TokenFlowPoint {
                minute: row.get(0)?,
                input_tokens: row.get(1)?,
                output_tokens: row.get(2)?,
                total_tokens: row.get(3)?,
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn recent_activity(&self, limit: usize) -> Result<Vec<ActivityEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.timestamp, s.project, COALESCE(m.model, 'unknown'),
                    m.input_tokens, m.output_tokens, m.cache_read, m.cost_usd,
                    COALESCE(m.provider, 'claude')
             FROM messages m
             JOIN sessions s ON m.session_id = s.id
             WHERE m.type = 'assistant' AND m.model IS NOT NULL
             ORDER BY m.timestamp DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ActivityEntry {
                timestamp: row.get(0)?,
                project: row.get(1)?,
                model: row.get(2)?,
                input_tokens: row.get(3)?,
                output_tokens: row.get(4)?,
                cache_read: row.get(5)?,
                cost_usd: row.get(6)?,
                provider: row.get(7)?,
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn cache_hit_ratio(&self) -> Result<f64> {
        let result: (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(cache_read), 0), COALESCE(SUM(input_tokens + cache_read + cache_creation), 0)
             FROM messages",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if result.1 == 0 {
            Ok(0.0)
        } else {
            Ok(result.0 as f64 / result.1 as f64)
        }
    }

    /// Compute delta banner data since a given ISO timestamp.
    pub fn delta_since(&self, since_ts: &str) -> Result<DeltaBanner> {
        // Compute how long ago
        let last_checked_label = format_since_label(since_ts);

        // Spend delta
        let spend_delta: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM messages WHERE timestamp > ?1",
            params![since_ts],
            |row| row.get(0),
        )?;

        // New sessions since
        let new_sessions: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE started_at > ?1",
            params![since_ts],
            |row| row.get(0),
        )?;

        // Model cost breakdown: current period vs previous period of same length
        let mut model_changes = Vec::new();

        // Get model costs in the "since" period
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(model, 'unknown'), SUM(cost_usd)
             FROM messages
             WHERE timestamp > ?1 AND model IS NOT NULL AND model != ''
             GROUP BY model
             ORDER BY SUM(cost_usd) DESC
             LIMIT 5",
        )?;
        let current_models: Vec<(String, f64)> = stmt.query_map(params![since_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?.filter_map(|r| r.ok()).collect();

        let total_current: f64 = current_models.iter().map(|(_, c)| c).sum();

        // Get total cost from previous equivalent period for comparison
        let total_previous: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM messages
             WHERE timestamp <= ?1
               AND timestamp > datetime(?1, '-' || (strftime('%s', 'now') - strftime('%s', ?1)) || ' seconds')",
            params![since_ts],
            |row| row.get(0),
        ).unwrap_or(0.0);

        // Get model costs in previous period
        let mut stmt_prev = self.conn.prepare(
            "SELECT COALESCE(model, 'unknown'), SUM(cost_usd)
             FROM messages
             WHERE timestamp <= ?1
               AND timestamp > datetime(?1, '-' || (strftime('%s', 'now') - strftime('%s', ?1)) || ' seconds')
               AND model IS NOT NULL AND model != ''
             GROUP BY model",
        )?;
        let prev_models: Vec<(String, f64)> = stmt_prev.query_map(params![since_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?.filter_map(|r| r.ok()).collect();

        // Compute per-model percentage share changes
        for (model, cost) in &current_models {
            let current_pct = if total_current > 0.0 { cost / total_current * 100.0 } else { 0.0 };
            let prev_cost = prev_models.iter().find(|(m, _)| m == model).map(|(_, c)| *c).unwrap_or(0.0);
            let prev_pct = if total_previous > 0.0 { prev_cost / total_previous * 100.0 } else { 0.0 };
            let pct_change = current_pct - prev_pct;
            if pct_change.abs() > 1.0 {
                model_changes.push(ModelChange {
                    model: shorten_model(model),
                    pct_change,
                });
            }
        }
        // Sort by absolute change descending, limit to top 3
        model_changes.sort_by(|a, b| b.pct_change.abs().partial_cmp(&a.pct_change.abs()).unwrap_or(std::cmp::Ordering::Equal));
        model_changes.truncate(3);

        Ok(DeltaBanner {
            last_checked_label,
            spend_delta,
            new_sessions,
            model_changes,
        })
    }

    /// Per-project cost attribution.
    pub fn project_costs(&self) -> Result<Vec<ProjectCost>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.project, COALESCE(SUM(m.cost_usd), 0) as total_cost
             FROM sessions s
             JOIN messages m ON s.id = m.session_id
             GROUP BY s.project
             ORDER BY total_cost DESC",
        )?;

        let rows: Vec<(String, f64)> = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?.filter_map(|r| r.ok()).collect();

        let grand_total: f64 = rows.iter().map(|(_, c)| c).sum();

        let mut results = Vec::new();
        for (name, cost) in rows {
            let percentage = if grand_total > 0.0 { cost / grand_total * 100.0 } else { 0.0 };
            results.push(ProjectCost { name, cost, percentage });
        }

        Ok(results)
    }

    /// Hourly heatmap: 7 rows (dow 0=Sun..6=Sat) x 24 cols (hours).
    pub fn hourly_heatmap(&self) -> Result<Vec<Vec<f64>>> {
        let mut heatmap = vec![vec![0.0f64; 24]; 7];

        let mut stmt = self.conn.prepare(
            "SELECT CAST(strftime('%w', timestamp) AS INTEGER) as dow,
                    CAST(strftime('%H', timestamp) AS INTEGER) as hour,
                    SUM(cost_usd)
             FROM messages
             GROUP BY dow, hour",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, f64>(2)?))
        })?;

        for row in rows.flatten() {
            let (dow, hour, cost) = row;
            if (0..7).contains(&dow) && (0..24).contains(&hour) {
                heatmap[dow as usize][hour as usize] = cost;
            }
        }

        Ok(heatmap)
    }

    /// Token efficiency stats.
    pub fn efficiency_stats(&self) -> Result<EfficiencyStats> {
        // Current week
        let (this_week_tokens, this_week_cost): (i64, f64) = self.conn.query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0),
                    COALESCE(SUM(cost_usd), 0)
             FROM messages
             WHERE timestamp > datetime('now', '-7 days')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        // Last week (7-14 days ago)
        let (last_week_tokens, last_week_cost): (i64, f64) = self.conn.query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0),
                    COALESCE(SUM(cost_usd), 0)
             FROM messages
             WHERE timestamp > datetime('now', '-14 days') AND timestamp <= datetime('now', '-7 days')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let tokens_per_dollar = if this_week_cost > 0.0 {
            this_week_tokens as f64 / this_week_cost
        } else {
            0.0
        };
        let tokens_per_dollar_last_week = if last_week_cost > 0.0 {
            last_week_tokens as f64 / last_week_cost
        } else {
            0.0
        };
        let efficiency_change_pct = if tokens_per_dollar_last_week > 0.0 {
            (tokens_per_dollar - tokens_per_dollar_last_week) / tokens_per_dollar_last_week * 100.0
        } else {
            0.0
        };

        // Cache savings: compute per-model using the pricing registry
        let mut cache_stmt = self.conn.prepare(
            "SELECT COALESCE(model, 'unknown'), COALESCE(SUM(cache_read), 0)
             FROM messages
             WHERE model IS NOT NULL
             GROUP BY model",
        )?;
        let cache_rows = cache_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
        let mut cache_savings = 0.0;
        for row in cache_rows.flatten() {
            let (model, cache_read) = row;
            let price = self.pricing.lookup(&model);
            cache_savings += cache_read as f64 * (price.input - price.cache_read) / 1_000_000.0;
        }

        Ok(EfficiencyStats {
            tokens_per_dollar,
            tokens_per_dollar_last_week,
            efficiency_change_pct,
            cache_savings_usd: cache_savings,
        })
    }

    /// Contribution calendar: daily spend for last 84 days (12 weeks).
    pub fn contribution_calendar(&self) -> Result<Vec<ContributionDay>> {
        let mut stmt = self.conn.prepare(
            "SELECT date(timestamp) as day, SUM(cost_usd)
             FROM messages
             WHERE timestamp > datetime('now', '-84 days')
             GROUP BY day
             ORDER BY day",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ContributionDay {
                date: row.get(0)?,
                cost: row.get(1)?,
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Get detailed messages for a specific session (for session detail popup).
    pub fn session_detail(&self, session_id: &str) -> Result<Vec<SessionMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.timestamp, COALESCE(m.model, 'unknown'), m.type,
                    m.input_tokens, m.output_tokens, m.cache_read, m.cache_creation, m.cost_usd
             FROM messages m
             WHERE m.session_id = ?1
             ORDER BY m.timestamp ASC",
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            Ok(SessionMessage {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                model: row.get(2)?,
                msg_type: row.get(3)?,
                input_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                cache_read: row.get(6)?,
                cache_creation: row.get(7)?,
                cost_usd: row.get(8)?,
            })
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Get daily costs per session for the last 7 days (for sparklines).
    pub fn session_daily_costs(&self) -> Result<std::collections::HashMap<String, Vec<f64>>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.session_id, date(m.timestamp) as day, SUM(m.cost_usd)
             FROM messages m
             WHERE m.timestamp > datetime('now', '-7 days')
             GROUP BY m.session_id, day
             ORDER BY m.session_id, day",
        )?;

        let mut result: std::collections::HashMap<String, Vec<(String, f64)>> =
            std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?;

        for row in rows.flatten() {
            result.entry(row.0).or_default().push((row.1, row.2));
        }

        let today = chrono::Utc::now().date_naive();
        let dates: Vec<String> = (0..7)
            .rev()
            .map(|i| (today - chrono::Duration::days(i)).format("%Y-%m-%d").to_string())
            .collect();

        let mut output: std::collections::HashMap<String, Vec<f64>> =
            std::collections::HashMap::new();
        for (session_id, day_costs) in &result {
            let mut costs = Vec::with_capacity(7);
            for date in &dates {
                let cost = day_costs
                    .iter()
                    .find(|(d, _)| d == date)
                    .map(|(_, c)| *c)
                    .unwrap_or(0.0);
                costs.push(cost);
            }
            output.insert(session_id.clone(), costs);
        }

        Ok(output)
    }
}

/// Format a timestamp into a human-readable "since" label.
fn format_since_label(iso: &str) -> String {
    use chrono::{DateTime, Utc};
    let Ok(dt) = iso.parse::<DateTime<Utc>>() else {
        return "unknown".to_string();
    };
    let now = Utc::now();
    let diff = now - dt;

    if diff.num_minutes() < 1 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}d ago", diff.num_days())
    } else {
        format!("{}w ago", diff.num_days() / 7)
    }
}


