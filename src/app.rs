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

    // Data
    pub dashboard: DashboardStats,
    pub models: Vec<ModelStats>,
    pub sessions: Vec<SessionSummary>,
    pub daily_spend: Vec<DailySpend>,
    pub token_flow: Vec<TokenFlowPoint>,
    pub activity: Vec<ActivityEntry>,
    pub cache_hit_ratio: f64,

    // UI state
    pub session_table_state: TableState,
    pub session_sort: SessionSort,
    pub trend_range: TrendRange,

    // Live mode
    pub last_live_event: Option<Instant>,
    pub live_project: Option<String>,

    // Control
    pub should_quit: bool,
    pub needs_refresh: bool,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));

        Self {
            view: View::Dashboard,
            show_help: false,
            config,
            content_area: Rect::default(),

            dashboard: DashboardStats::default(),
            models: Vec::new(),
            sessions: Vec::new(),
            daily_spend: Vec::new(),
            token_flow: Vec::new(),
            activity: Vec::new(),
            cache_hit_ratio: 0.0,

            session_table_state: table_state,
            session_sort: SessionSort::Recent,
            trend_range: TrendRange::Month,

            last_live_event: None,
            live_project: None,

            should_quit: false,
            needs_refresh: true,
        }
    }

    /// Returns the live/idle status text and whether it's "live" (true) or "idle" (false).
    pub fn live_status(&self) -> (bool, &str) {
        match self.last_live_event {
            Some(t) if t.elapsed().as_secs() < 60 => (true, "LIVE"),
            _ => (false, "IDLE"),
        }
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
        if let Ok(flow) = agg.token_flow_last_hour() {
            self.token_flow = flow;
        }
        if let Ok(activity) = agg.recent_activity(50) {
            self.activity = activity;
        }
        if let Ok(ratio) = agg.cache_hit_ratio() {
            self.cache_hit_ratio = ratio;
        }
        self.needs_refresh = false;
    }

    pub fn sort_sessions(&mut self) {
        match self.session_sort {
            SessionSort::Cost => self.sessions.sort_by(|a, b| b.total_cost.partial_cmp(&a.total_cost).unwrap()),
            SessionSort::Tokens => self.sessions.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens)),
            SessionSort::Project => self.sessions.sort_by(|a, b| a.project.cmp(&b.project)),
            SessionSort::Recent => self.sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
        }
    }

    pub fn next_session(&mut self) {
        let len = self.sessions.len();
        if len == 0 { return; }
        let i = self.session_table_state.selected().unwrap_or(0);
        self.session_table_state.select(Some((i + 1).min(len - 1)));
    }

    pub fn prev_session(&mut self) {
        let i = self.session_table_state.selected().unwrap_or(0);
        self.session_table_state.select(Some(i.saturating_sub(1)));
    }
}
