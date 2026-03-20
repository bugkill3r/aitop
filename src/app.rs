use std::collections::{HashMap, HashSet};
use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::widgets::TableState;

use crate::config::Config;
use crate::data::aggregator::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Sessions,
    Models,
    Trends,
}

impl View {
    pub fn index(&self) -> usize {
        match self {
            View::Dashboard => 0,
            View::Sessions => 1,
            View::Models => 2,
            View::Trends => 3,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => View::Dashboard,
            1 => View::Sessions,
            2 => View::Models,
            3 => View::Trends,
            _ => View::Dashboard,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendRange {
    Week,
    Month,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Line,
    Bar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSort {
    Cost,
    Tokens,
    Project,
    Recent,
}

pub struct AppState {
    pub view: View,
    pub show_help: bool,
    pub config: Config,
    pub content_area: Rect,
    pub status_bar_area: Rect,

    // Data
    pub dashboard: DashboardStats,
    pub models: Vec<ModelStats>,
    pub sessions: Vec<SessionSummary>,
    pub daily_spend: Vec<DailySpend>,
    pub daily_tokens: Vec<DailyTokenCount>,
    pub token_flow: Vec<TokenFlowPoint>,
    pub activity: Vec<ActivityEntry>,
    pub cache_hit_ratio: f64,

    // Session detail popup
    pub detail_session: Option<String>,
    pub detail_messages: Vec<SessionMessage>,
    pub detail_scroll: usize,

    // Filter/search
    pub filter_text: String,
    pub filter_active: bool,
    pub filtered_sessions: Vec<SessionSummary>,

    // Sparklines
    pub session_sparklines: HashMap<String, Vec<f64>>,

    // Creative features
    pub delta_banner: Option<DeltaBanner>,
    pub banner_shown_at: Option<Instant>,
    pub heatmap: Vec<Vec<f64>>,
    pub project_costs: Vec<ProjectCost>,
    pub efficiency: EfficiencyStats,
    pub contribution_calendar: Vec<ContributionDay>,

    // UI state
    pub session_table_state: TableState,
    pub session_sort: SessionSort,
    pub sort_ascending: bool,
    pub trend_range: TrendRange,
    pub chart_type: ChartType,
    pub show_token_overlay: bool,
    pub theme_index: usize,

    // Live mode
    pub last_live_event: Option<Instant>,
    pub live_project: Option<String>,
    pub pulse_tick: u8,

    // Theme flash
    pub theme_flash: Option<Instant>,

    // Split panes
    pub split_mode: bool,
    pub split_view: Option<View>,

    // Clipboard flash
    pub copy_flash: Option<Instant>,

    // Budget notification tracking
    pub notified_thresholds: HashSet<u8>,

    // Session replay
    pub replay_active: bool,
    pub replay_index: usize,
    pub replay_speed: u8,
    pub replay_paused: bool,

    // Control
    pub should_quit: bool,
    pub needs_refresh: bool,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        use crate::ui::theme::THEME_NAMES;

        let mut table_state = TableState::default();
        table_state.select(Some(0));

        let theme_index = THEME_NAMES
            .iter()
            .position(|&n| n == config.theme)
            .unwrap_or(0);

        Self {
            view: View::Dashboard,
            show_help: false,
            config,
            content_area: Rect::default(),
            status_bar_area: Rect::default(),

            dashboard: DashboardStats::default(),
            models: Vec::new(),
            sessions: Vec::new(),
            daily_spend: Vec::new(),
            daily_tokens: Vec::new(),
            token_flow: Vec::new(),
            activity: Vec::new(),
            cache_hit_ratio: 0.0,

            detail_session: None,
            detail_messages: Vec::new(),
            detail_scroll: 0,

            filter_text: String::new(),
            filter_active: false,
            filtered_sessions: Vec::new(),

            session_sparklines: HashMap::new(),

            delta_banner: None,
            banner_shown_at: None,
            heatmap: vec![vec![0.0; 24]; 7],
            project_costs: Vec::new(),
            efficiency: EfficiencyStats::default(),
            contribution_calendar: Vec::new(),

            session_table_state: table_state,
            session_sort: SessionSort::Recent,
            sort_ascending: false,
            trend_range: TrendRange::Month,
            chart_type: ChartType::Bar,
            show_token_overlay: false,
            theme_index,

            last_live_event: None,
            live_project: None,
            pulse_tick: 0,

            theme_flash: None,

            split_mode: false,
            split_view: None,

            copy_flash: None,

            notified_thresholds: HashSet::new(),

            replay_active: false,
            replay_index: 0,
            replay_speed: 1,
            replay_paused: false,

            should_quit: false,
            needs_refresh: true,
        }
    }

    pub fn live_status(&self) -> (bool, &str) {
        match self.last_live_event {
            Some(t) if t.elapsed().as_secs() < 60 => (true, "LIVE"),
            _ => (false, "IDLE"),
        }
    }

    /// Returns the animated pulse indicator character for the LIVE status.
    /// Cycles through varying intensity dot characters based on pulse_tick.
    pub fn pulse_indicator(&self) -> &str {
        let (is_live, _) = self.live_status();
        if !is_live {
            return "\u{25CB}"; // ○ hollow circle for IDLE
        }
        // 8-frame pulse animation cycle
        match self.pulse_tick % 8 {
            0 => "\u{2219}",     // ∙ (dim)
            1 => "\u{00B7}",     // · (small dot)
            2 => "\u{2022}",     // • (bullet)
            3 => "\u{25CF}",     // ● (full circle)
            4 => "\u{2B24}",     // ⬤ (large circle)
            5 => "\u{25CF}",     // ● (full circle)
            6 => "\u{2022}",     // • (bullet)
            7 => "\u{00B7}",     // · (small dot)
            _ => "\u{25CF}",     // ● fallback
        }
    }

    /// Advance the pulse tick counter (called each render cycle).
    pub fn advance_pulse(&mut self) {
        self.pulse_tick = self.pulse_tick.wrapping_add(1);
    }

    pub fn check_banner_timeout(&mut self) {
        if let Some(shown_at) = self.banner_shown_at {
            if shown_at.elapsed() >= std::time::Duration::from_secs(10) {
                self.delta_banner = None;
                self.banner_shown_at = None;
            }
        }
    }

    pub fn dismiss_banner(&mut self) {
        self.delta_banner = None;
        self.banner_shown_at = None;
    }

    pub fn refresh_data(&mut self, agg: &Aggregator) {
        if let Ok(stats) = agg.dashboard_stats() {
            self.dashboard = stats;
        }
        if let Ok(models) = agg.model_breakdown() {
            self.models = models;
        }
        if let Ok(sessions) = agg.sessions_list(500) {
            self.sessions = sessions;
            self.sort_sessions();
        }
        let days = match self.trend_range {
            TrendRange::Week => 7,
            TrendRange::Month => 30,
            TrendRange::All => 365,
        };
        if let Ok(spend) = agg.daily_spend(days) {
            self.daily_spend = spend;
        }
        if let Ok(tokens) = agg.daily_tokens(days) {
            self.daily_tokens = tokens;
        }
        if let Ok(flow) = agg.token_flow_last_hour() {
            self.token_flow = flow;
        }
        if let Ok(activity) = agg.recent_activity(50) {
            self.activity = activity;
        }
        if let Ok(ratio) = agg.cache_hit_ratio() {
            self.cache_hit_ratio = ratio;
        }
        if let Ok(sparklines) = agg.session_daily_costs() {
            self.session_sparklines = sparklines;
        }
        if let Ok(heatmap) = agg.hourly_heatmap() {
            self.heatmap = heatmap;
        }
        if let Ok(pc) = agg.project_costs() {
            self.project_costs = pc;
        }
        if let Ok(eff) = agg.efficiency_stats() {
            self.efficiency = eff;
        }
        if let Ok(cal) = agg.contribution_calendar() {
            self.contribution_calendar = cal;
        }
        self.apply_filter();
        self.needs_refresh = false;
    }

    pub fn sort_sessions(&mut self) {
        match self.session_sort {
            SessionSort::Cost => self.sessions.sort_by(|a, b| {
                if self.sort_ascending {
                    a.total_cost.partial_cmp(&b.total_cost).unwrap()
                } else {
                    b.total_cost.partial_cmp(&a.total_cost).unwrap()
                }
            }),
            SessionSort::Tokens => self.sessions.sort_by(|a, b| {
                if self.sort_ascending {
                    a.total_tokens.cmp(&b.total_tokens)
                } else {
                    b.total_tokens.cmp(&a.total_tokens)
                }
            }),
            SessionSort::Project => self.sessions.sort_by(|a, b| {
                if self.sort_ascending {
                    a.project.cmp(&b.project)
                } else {
                    b.project.cmp(&a.project)
                }
            }),
            SessionSort::Recent => self.sessions.sort_by(|a, b| {
                if self.sort_ascending {
                    a.updated_at.cmp(&b.updated_at)
                } else {
                    b.updated_at.cmp(&a.updated_at)
                }
            }),
        }
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        if self.filter_text.is_empty() {
            self.filtered_sessions = self.sessions.clone();
        } else {
            let lower = self.filter_text.to_lowercase();
            self.filtered_sessions = self
                .sessions
                .iter()
                .filter(|s| s.project.to_lowercase().contains(&lower))
                .cloned()
                .collect();
        }
    }

    pub fn displayed_sessions(&self) -> &[SessionSummary] {
        &self.filtered_sessions
    }

    pub fn next_session(&mut self) {
        let len = self.displayed_sessions().len();
        if len == 0 { return; }
        let i = self.session_table_state.selected().unwrap_or(0);
        self.session_table_state.select(Some((i + 1).min(len - 1)));
    }

    pub fn prev_session(&mut self) {
        let i = self.session_table_state.selected().unwrap_or(0);
        self.session_table_state.select(Some(i.saturating_sub(1)));
    }

    pub fn selected_session_id(&self) -> Option<String> {
        let idx = self.session_table_state.selected()?;
        self.displayed_sessions().get(idx).map(|s| s.id.clone())
    }

    pub fn sort_indicator(&self) -> &str {
        if self.sort_ascending { " \u{25B2}" } else { " \u{25BC}" }
    }

    /// Check budget thresholds and send notifications for newly crossed ones.
    /// Returns the list of newly notified thresholds (for testing).
    pub fn check_budget_notifications(&mut self) -> Vec<u8> {
        let budget = match self.config.budget {
            Some(b) if b > 0.0 => b,
            _ => return Vec::new(),
        };

        let new_thresholds = check_budget_thresholds(
            budget,
            self.dashboard.spend_today,
            &self.notified_thresholds,
        );

        for &threshold in &new_thresholds {
            let msg = format!(
                "Spend today: ${:.2} ({:.0}% of ${:.2} budget)",
                self.dashboard.spend_today,
                self.dashboard.spend_today / budget * 100.0,
                budget,
            );
            send_desktop_notification("aitop Budget Alert", &msg);
            self.notified_thresholds.insert(threshold);
        }

        new_thresholds
    }

    /// Toggle split mode on/off.
    pub fn toggle_split(&mut self) {
        self.split_mode = !self.split_mode;
        if self.split_mode {
            // Default right pane to a different view than current
            self.split_view = Some(match self.view {
                View::Dashboard => View::Sessions,
                View::Sessions => View::Dashboard,
                View::Models => View::Dashboard,
                View::Trends => View::Dashboard,
            });
        } else {
            self.split_view = None;
        }
    }

    /// Enter replay mode for the current session detail.
    pub fn start_replay(&mut self) {
        if self.detail_session.is_some() && !self.detail_messages.is_empty() {
            self.replay_active = true;
            self.replay_index = 0;
            self.replay_speed = 1;
            self.replay_paused = false;
        }
    }

    /// Exit replay mode.
    pub fn stop_replay(&mut self) {
        self.replay_active = false;
        self.replay_index = 0;
        self.replay_speed = 1;
        self.replay_paused = false;
    }

    /// Toggle replay pause/resume.
    pub fn toggle_replay_pause(&mut self) {
        if self.replay_active {
            self.replay_paused = !self.replay_paused;
        }
    }

    /// Increase replay speed: 1 -> 2 -> 5 -> 10.
    pub fn replay_speed_up(&mut self) {
        if self.replay_active {
            self.replay_speed = match self.replay_speed {
                1 => 2,
                2 => 5,
                5 => 10,
                _ => 10,
            };
        }
    }

    /// Decrease replay speed: 10 -> 5 -> 2 -> 1.
    pub fn replay_speed_down(&mut self) {
        if self.replay_active {
            self.replay_speed = match self.replay_speed {
                10 => 5,
                5 => 2,
                2 => 1,
                _ => 1,
            };
        }
    }

    /// Advance replay by one message. Returns true if advanced, false if at end.
    pub fn replay_advance(&mut self) -> bool {
        if !self.replay_active || self.replay_paused {
            return false;
        }
        if self.replay_index < self.detail_messages.len().saturating_sub(1) {
            self.replay_index += 1;
            true
        } else {
            false
        }
    }

    /// Get running totals up to and including `replay_index`.
    pub fn replay_running_totals(&self) -> (i64, f64) {
        if !self.replay_active || self.detail_messages.is_empty() {
            return (0, 0.0);
        }
        let end = (self.replay_index + 1).min(self.detail_messages.len());
        let msgs = &self.detail_messages[..end];
        let tokens: i64 = msgs.iter().map(|m| m.input_tokens + m.output_tokens).sum();
        let cost: f64 = msgs.iter().map(|m| m.cost_usd).sum();
        (tokens, cost)
    }
}

/// Budget threshold percentages to check for notifications.
const BUDGET_THRESHOLDS: &[u8] = &[50, 75, 90, 100];

/// Determine which budget thresholds have been crossed based on current spend.
/// Returns thresholds that are newly crossed (not already in `already_notified`).
pub fn check_budget_thresholds(
    budget: f64,
    current_spend: f64,
    already_notified: &HashSet<u8>,
) -> Vec<u8> {
    if budget <= 0.0 {
        return Vec::new();
    }
    let pct = (current_spend / budget * 100.0) as u8;
    BUDGET_THRESHOLDS
        .iter()
        .filter(|&&threshold| pct >= threshold && !already_notified.contains(&threshold))
        .copied()
        .collect()
}

/// Send a desktop notification using platform-specific commands.
pub fn send_desktop_notification(title: &str, message: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            message.replace('"', "\\\""),
            title.replace('"', "\\\""),
        );
        Command::new("osascript")
            .args(["-e", &script])
            .output()
            .is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        use std::process::Command;
        Command::new("notify-send")
            .args([title, message])
            .output()
            .is_ok()
    }
}

/// Format a single session as a human-readable clipboard string.
pub fn format_session_for_clipboard(session: &SessionSummary) -> String {
    format!(
        "Session: {}\nProject: {}\nModel: {}\nTokens: {}\nCost: ${:.4}\nMessages: {}\nStarted: {}\nUpdated: {}",
        session.id, session.project, session.model, session.total_tokens,
        session.total_cost, session.msg_count, session.started_at, session.updated_at,
    )
}

/// Format a list of sessions as TSV (tab-separated values) for clipboard.
pub fn format_sessions_as_tsv(sessions: &[SessionSummary]) -> String {
    let mut lines = Vec::with_capacity(sessions.len() + 1);
    lines.push("Project\tModel\tTokens\tCost\tMessages\tStarted\tUpdated".to_string());
    for s in sessions {
        lines.push(format!(
            "{}\t{}\t{}\t${:.4}\t{}\t{}\t{}",
            s.project, s.model, s.total_tokens, s.total_cost, s.msg_count, s.started_at, s.updated_at,
        ));
    }
    lines.join("\n")
}

/// Copy text to system clipboard using platform-specific commands.
pub fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            return child.wait().is_ok();
        }
        false
    }
    #[cfg(not(target_os = "macos"))]
    {
        use std::process::{Command, Stdio};
        // Try xclip first, then xsel
        let cmds = ["xclip", "xsel"];
        for cmd in &cmds {
            if let Ok(mut child) = Command::new(cmd)
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()
            {
                if let Some(stdin) = child.stdin.as_mut() {
                    use std::io::Write;
                    let _ = stdin.write_all(text.as_bytes());
                }
                if child.wait().is_ok() {
                    return true;
                }
            }
        }
        false
    }
}

/// Format a compact one-line tmux status bar string from dashboard stats.
pub fn format_tmux_status(burn_rate: f64, spend_today: f64, total_sessions: i64) -> String {
    format!(
        "#[fg=colour208]aitop:#[fg=colour255] ${:.2}/hr #[fg=colour240]|#[fg=colour255] ${:.2} today #[fg=colour240]|#[fg=colour255] {} sessions",
        burn_rate, spend_today, total_sessions
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tmux_status_format() {
        let output = format_tmux_status(1.23, 45.67, 12);
        assert!(output.contains("$1.23/hr"), "should contain burn rate");
        assert!(output.contains("$45.67 today"), "should contain today's spend");
        assert!(output.contains("12 sessions"), "should contain session count");
        assert!(output.contains("#[fg="), "should contain tmux color codes");
    }

    #[test]
    fn test_tmux_status_zero_values() {
        let output = format_tmux_status(0.0, 0.0, 0);
        assert!(output.contains("$0.00/hr"));
        assert!(output.contains("$0.00 today"));
        assert!(output.contains("0 sessions"));
    }

    fn make_test_state() -> AppState {
        let config = Config::default();
        AppState::new(config)
    }

    fn make_test_messages(n: usize) -> Vec<crate::data::aggregator::SessionMessage> {
        (0..n)
            .map(|i| crate::data::aggregator::SessionMessage {
                id: format!("msg-{}", i),
                timestamp: format!("2025-01-01T{:02}:00:00Z", i),
                model: "claude-3-sonnet".to_string(),
                msg_type: if i % 2 == 0 { "human".to_string() } else { "assistant".to_string() },
                input_tokens: 100 * (i as i64 + 1),
                output_tokens: 50 * (i as i64 + 1),
                cache_read: 0,
                cache_creation: 0,
                cost_usd: 0.01 * (i as f64 + 1.0),
            })
            .collect()
    }

    #[test]
    fn test_replay_start_requires_session() {
        let mut state = make_test_state();
        // No detail session - should not activate
        state.start_replay();
        assert!(!state.replay_active);

        // With session but no messages - should not activate
        state.detail_session = Some("session-1".to_string());
        state.start_replay();
        assert!(!state.replay_active);

        // With session and messages - should activate
        state.detail_messages = make_test_messages(5);
        state.start_replay();
        assert!(state.replay_active);
        assert_eq!(state.replay_index, 0);
        assert_eq!(state.replay_speed, 1);
        assert!(!state.replay_paused);
    }

    #[test]
    fn test_replay_advance() {
        let mut state = make_test_state();
        state.detail_session = Some("session-1".to_string());
        state.detail_messages = make_test_messages(3);
        state.start_replay();

        assert!(state.replay_advance()); // 0 -> 1
        assert_eq!(state.replay_index, 1);
        assert!(state.replay_advance()); // 1 -> 2
        assert_eq!(state.replay_index, 2);
        assert!(!state.replay_advance()); // at end, no advance
        assert_eq!(state.replay_index, 2);
    }

    #[test]
    fn test_replay_pause_blocks_advance() {
        let mut state = make_test_state();
        state.detail_session = Some("session-1".to_string());
        state.detail_messages = make_test_messages(5);
        state.start_replay();

        state.toggle_replay_pause();
        assert!(state.replay_paused);
        assert!(!state.replay_advance()); // blocked
        assert_eq!(state.replay_index, 0);

        state.toggle_replay_pause();
        assert!(!state.replay_paused);
        assert!(state.replay_advance()); // unblocked
        assert_eq!(state.replay_index, 1);
    }

    #[test]
    fn test_replay_speed_changes() {
        let mut state = make_test_state();
        state.detail_session = Some("session-1".to_string());
        state.detail_messages = make_test_messages(5);
        state.start_replay();

        assert_eq!(state.replay_speed, 1);
        state.replay_speed_up();
        assert_eq!(state.replay_speed, 2);
        state.replay_speed_up();
        assert_eq!(state.replay_speed, 5);
        state.replay_speed_up();
        assert_eq!(state.replay_speed, 10);
        state.replay_speed_up();
        assert_eq!(state.replay_speed, 10); // capped

        state.replay_speed_down();
        assert_eq!(state.replay_speed, 5);
        state.replay_speed_down();
        assert_eq!(state.replay_speed, 2);
        state.replay_speed_down();
        assert_eq!(state.replay_speed, 1);
        state.replay_speed_down();
        assert_eq!(state.replay_speed, 1); // capped
    }

    #[test]
    fn test_replay_running_totals() {
        let mut state = make_test_state();
        state.detail_session = Some("session-1".to_string());
        state.detail_messages = make_test_messages(3);
        state.start_replay();

        // At index 0: first message only
        let (tokens, cost) = state.replay_running_totals();
        assert_eq!(tokens, 150); // 100 + 50
        assert!((cost - 0.01).abs() < 1e-9);

        state.replay_advance(); // index 1
        let (tokens, cost) = state.replay_running_totals();
        assert_eq!(tokens, 150 + 300); // (100+50) + (200+100)
        assert!((cost - 0.03).abs() < 1e-9);

        state.replay_advance(); // index 2
        let (tokens, cost) = state.replay_running_totals();
        assert_eq!(tokens, 150 + 300 + 450); // + (300+150)
        assert!((cost - 0.06).abs() < 1e-9);
    }

    #[test]
    fn test_budget_threshold_detection_no_budget() {
        let notified = HashSet::new();
        let result = check_budget_thresholds(0.0, 10.0, &notified);
        assert!(result.is_empty(), "no thresholds when budget is 0");
    }

    #[test]
    fn test_budget_threshold_detection_below_first() {
        let notified = HashSet::new();
        let result = check_budget_thresholds(100.0, 40.0, &notified);
        assert!(result.is_empty(), "no thresholds at 40% of budget");
    }

    #[test]
    fn test_budget_threshold_detection_at_50() {
        let notified = HashSet::new();
        let result = check_budget_thresholds(100.0, 50.0, &notified);
        assert_eq!(result, vec![50], "should trigger 50% threshold");
    }

    #[test]
    fn test_budget_threshold_detection_at_80() {
        let notified = HashSet::new();
        let result = check_budget_thresholds(100.0, 80.0, &notified);
        assert_eq!(result, vec![50, 75], "should trigger 50% and 75% thresholds");
    }

    #[test]
    fn test_budget_threshold_detection_at_100() {
        let notified = HashSet::new();
        let result = check_budget_thresholds(100.0, 100.0, &notified);
        assert_eq!(result, vec![50, 75, 90, 100], "should trigger all thresholds");
    }

    #[test]
    fn test_budget_threshold_no_duplicates() {
        let mut notified = HashSet::new();
        notified.insert(50);
        notified.insert(75);
        let result = check_budget_thresholds(100.0, 95.0, &notified);
        assert_eq!(result, vec![90], "should only trigger new thresholds");
    }

    #[test]
    fn test_budget_notification_state_machine() {
        let mut state = make_test_state();
        state.config.budget = Some(100.0);

        // Below any threshold
        state.dashboard.spend_today = 30.0;
        let result = state.check_budget_notifications();
        assert!(result.is_empty());
        assert!(state.notified_thresholds.is_empty());

        // Cross 50%
        state.dashboard.spend_today = 55.0;
        let result = state.check_budget_notifications();
        assert_eq!(result, vec![50]);
        assert!(state.notified_thresholds.contains(&50));

        // Still at 55%, no new notifications
        let result = state.check_budget_notifications();
        assert!(result.is_empty());

        // Cross 75% and 90%
        state.dashboard.spend_today = 92.0;
        let result = state.check_budget_notifications();
        assert!(result.contains(&75));
        assert!(result.contains(&90));
        assert!(!result.contains(&50), "50 already notified");
    }

    fn make_test_session(id: &str, project: &str, cost: f64) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            project: project.to_string(),
            model: "claude-3-sonnet".to_string(),
            total_cost: cost,
            total_tokens: 1500,
            msg_count: 10,
            started_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T01:00:00Z".to_string(),
            provider: "claude".to_string(),
        }
    }

    #[test]
    fn test_clipboard_single_session_format() {
        let session = make_test_session("sess-1", "my-project", 1.2345);
        let output = format_session_for_clipboard(&session);
        assert!(output.contains("my-project"), "should contain project name");
        assert!(output.contains("$1.2345"), "should contain cost");
        assert!(output.contains("1500"), "should contain token count");
        assert!(output.contains("Session: sess-1"), "should contain session id");
    }

    #[test]
    fn test_clipboard_tsv_format() {
        let sessions = vec![
            make_test_session("s1", "proj-a", 0.50),
            make_test_session("s2", "proj-b", 1.25),
        ];
        let tsv = format_sessions_as_tsv(&sessions);
        let lines: Vec<&str> = tsv.lines().collect();
        assert_eq!(lines.len(), 3, "header + 2 data rows");
        assert!(lines[0].contains("Project"), "header should contain Project");
        assert!(lines[0].contains('\t'), "header should be tab-separated");
        assert!(lines[1].contains("proj-a"), "first row should contain proj-a");
        assert!(lines[2].contains("proj-b"), "second row should contain proj-b");
        assert!(lines[1].contains("$0.5000"), "cost should be formatted to 4 decimal places");
    }

    #[test]
    fn test_clipboard_tsv_empty() {
        let tsv = format_sessions_as_tsv(&[]);
        let lines: Vec<&str> = tsv.lines().collect();
        assert_eq!(lines.len(), 1, "only header for empty sessions");
    }

    #[test]
    fn test_copy_flash_timeout() {
        let mut state = make_test_state();
        state.copy_flash = Some(Instant::now() - std::time::Duration::from_secs(5));
        // After 2 seconds, flash should be considered expired
        if let Some(flash_at) = state.copy_flash {
            assert!(flash_at.elapsed() >= std::time::Duration::from_secs(2));
        }
    }

    #[test]
    fn test_split_toggle() {
        let mut state = make_test_state();
        assert!(!state.split_mode);
        assert!(state.split_view.is_none());

        state.toggle_split();
        assert!(state.split_mode);
        assert!(state.split_view.is_some());
        // Default view is Dashboard, so split_view should be Sessions
        assert_eq!(state.split_view, Some(View::Sessions));

        state.toggle_split();
        assert!(!state.split_mode);
        assert!(state.split_view.is_none());
    }

    #[test]
    fn test_split_default_view_differs() {
        let mut state = make_test_state();

        // When on Sessions, right pane should default to Dashboard
        state.view = View::Sessions;
        state.toggle_split();
        assert_eq!(state.split_view, Some(View::Dashboard));
        state.toggle_split();

        // When on Models, right pane should default to Dashboard
        state.view = View::Models;
        state.toggle_split();
        assert_eq!(state.split_view, Some(View::Dashboard));
    }

    #[test]
    fn test_replay_stop() {
        let mut state = make_test_state();
        state.detail_session = Some("session-1".to_string());
        state.detail_messages = make_test_messages(5);
        state.start_replay();
        state.replay_advance();
        state.replay_speed_up();

        state.stop_replay();
        assert!(!state.replay_active);
        assert_eq!(state.replay_index, 0);
        assert_eq!(state.replay_speed, 1);
        assert!(!state.replay_paused);
    }

    // --- v0.4 tests (pulse, layout) ---

    #[test]
    fn test_pulse_tick_wraps() {
        let mut state = make_test_state();
        state.pulse_tick = 255;
        state.advance_pulse();
        assert_eq!(state.pulse_tick, 0);
    }

    #[test]
    fn test_pulse_tick_advances() {
        let mut state = make_test_state();
        assert_eq!(state.pulse_tick, 0);
        state.advance_pulse();
        assert_eq!(state.pulse_tick, 1);
        state.advance_pulse();
        assert_eq!(state.pulse_tick, 2);
    }

    #[test]
    fn test_pulse_indicator_idle() {
        let state = make_test_state();
        // No live event => IDLE
        assert_eq!(state.pulse_indicator(), "\u{25CB}"); // ○
    }

    #[test]
    fn test_pulse_indicator_live_cycles() {
        let mut state = make_test_state();
        state.last_live_event = Some(Instant::now());

        // Collect indicators for one full cycle
        let mut indicators = Vec::new();
        for _ in 0..8 {
            indicators.push(state.pulse_indicator().to_string());
            state.advance_pulse();
        }

        // Should have 8 distinct frames (some may repeat due to symmetry)
        assert_eq!(indicators.len(), 8);
        // Frame 0 and 7 should be the same (dim dots)
        // Frame 3 and 5 should be the same (full circle)
        assert_eq!(indicators[3], indicators[5]);
        // Frame 3 should be the full circle ●
        assert_eq!(indicators[3], "\u{25CF}");
    }

    #[test]
    fn test_pulse_indicator_periodicity() {
        let mut state = make_test_state();
        state.last_live_event = Some(Instant::now());

        // After 8 ticks, should cycle back
        let first = state.pulse_indicator().to_string();
        for _ in 0..8 {
            state.advance_pulse();
        }
        assert_eq!(state.pulse_indicator(), first);
    }

    #[test]
    fn test_live_status_recent_event() {
        let mut state = make_test_state();
        state.last_live_event = Some(Instant::now());
        let (is_live, label) = state.live_status();
        assert!(is_live);
        assert_eq!(label, "LIVE");
    }

    #[test]
    fn test_live_status_no_event() {
        let state = make_test_state();
        let (is_live, label) = state.live_status();
        assert!(!is_live);
        assert_eq!(label, "IDLE");
    }
}
