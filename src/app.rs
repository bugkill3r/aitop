use std::collections::HashMap;
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
    pub status_bar_area: Rect,

    // Data
    pub dashboard: DashboardStats,
    pub models: Vec<ModelStats>,
    pub sessions: Vec<SessionSummary>,
    pub daily_spend: Vec<DailySpend>,
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
    pub theme_index: usize,

    // Live mode
    pub last_live_event: Option<Instant>,
    pub live_project: Option<String>,

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
            theme_index,

            last_live_event: None,
            live_project: None,

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
}
