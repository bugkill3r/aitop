use std::io::{self, Write as _};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use rayon::prelude::*;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};
use ratatui::Terminal;

use aitop::app::{AppState, SessionSort, TrendRange, View};
use aitop::config::Config;
use aitop::data::aggregator::Aggregator;
use aitop::data::db::Database;
use aitop::data::gemini::scan_gemini_sessions;
use aitop::data::openclaw::scan_openclaw_sessions;
use aitop::data::pricing::PricingRegistry;
use aitop::data::provider::Provider;
use aitop::data::scanner::scan_projects;
use aitop::data::watcher::{watch_directory, FsEvent};
use aitop::ui;
use aitop::ui::format::{format_tokens, shorten_model};
use aitop::ui::theme::THEME_NAMES;

#[derive(Parser, Debug)]
#[command(name = "aitop", about = "btop for AI — terminal dashboard for token usage and costs")]
struct Args {
    /// Color theme (ember, nord, dracula, gruvbox, catppuccin, mono)
    #[arg(short, long)]
    theme: Option<String>,

    /// Refresh interval in seconds
    #[arg(short, long)]
    refresh: Option<f64>,

    /// Non-interactive table output
    #[arg(long)]
    light: bool,

    /// Output compact one-line summary for tmux status bar
    #[arg(long)]
    tmux_status: bool,

    /// Filter by project name
    #[arg(short, long)]
    project: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut config = Config::load()?;

    if let Some(theme) = args.theme {
        config.theme = theme;
    }
    if let Some(refresh) = args.refresh {
        config.refresh = refresh;
    }

    // Build pricing registry from config (with optional user overrides)
    let pricing = if config.model_pricing.is_empty() {
        PricingRegistry::builtin()
    } else {
        PricingRegistry::with_overrides(&config.model_pricing)
    };

    // Ingest data with startup progress
    let db_path = Config::db_path();
    let db = Database::open_with_pricing(&db_path, pricing.clone())?;
    let projects_dir = config.projects_dir();

    // Scan Claude sessions
    let claude_files = scan_projects(&projects_dir)?;

    // Scan Gemini sessions
    let gemini_dir = Provider::Gemini.default_dir();
    let gemini_files = scan_gemini_sessions(&gemini_dir).unwrap_or_default();

    // Scan OpenClaw sessions
    let openclaw_dir = Provider::OpenClaw.default_dir();
    let openclaw_files = scan_openclaw_sessions(&openclaw_dir).unwrap_or_default();

    let total_files = claude_files.len() + gemini_files.len() + openclaw_files.len();
    if total_files > 0 {
        eprint!("\r  Indexing sessions... (parsing {} files)", total_files);
        io::stderr().flush().ok();

        // Phase 1: Parse Claude files in parallel using rayon
        let file_offsets: Vec<_> = claude_files
            .iter()
            .map(|file| {
                let path_str = file.path.to_string_lossy().to_string();
                db.get_file_offset(&path_str).unwrap_or(0)
            })
            .collect();

        let parsed_files: Vec<_> = claude_files
            .par_iter()
            .zip(file_offsets.par_iter())
            .filter_map(|(file, &offset)| {
                let content = std::fs::read(&file.path).ok()?;
                if (offset as usize) >= content.len() {
                    return None; // already up to date
                }
                let results = aitop::data::parser::parse_file_content(&content, offset, &file.project, &pricing);
                if results.is_empty() {
                    return None;
                }
                Some((file, content.len() as u64, results))
            })
            .collect();

        // Phase 2: Write Claude results to DB sequentially (SQLite single-writer constraint)
        let mut count = 0;
        for (file, new_offset, results) in &parsed_files {
            count += 1;
            eprint!(
                "\r  Indexing sessions... ({}/{} files)",
                count,
                total_files
            );
            io::stderr().flush().ok();

            if let Err(e) = db.write_parsed_results(file, *new_offset, results) {
                eprintln!("\nWarning: failed to write {:?}: {}", file.path, e);
            }
        }

        // Ingest Gemini files sequentially
        for file in &gemini_files {
            count += 1;
            eprint!("\r  Indexing sessions... ({}/{} files)", count, total_files);
            io::stderr().flush().ok();
            if let Err(e) = db.ingest_gemini_file(file) {
                eprintln!("\nWarning: failed to ingest Gemini {:?}: {}", file.path, e);
            }
        }

        // Ingest OpenClaw files sequentially
        for file in &openclaw_files {
            count += 1;
            eprint!("\r  Indexing sessions... ({}/{} files)", count, total_files);
            io::stderr().flush().ok();
            if let Err(e) = db.ingest_openclaw_file(file) {
                eprintln!("\nWarning: failed to ingest OpenClaw {:?}: {}", file.path, e);
            }
        }

        eprint!("\r{}\r", " ".repeat(60));
        io::stderr().flush().ok();
    }

    if args.light {
        drop(db);
        return print_light_mode(&db_path, pricing);
    }

    if args.tmux_status {
        drop(db);
        return print_tmux_status(&db_path);
    }

    run_tui(config, db, &db_path, &projects_dir, pricing)
}

fn print_light_mode(db_path: &std::path::Path, pricing: PricingRegistry) -> Result<()> {
    let agg = Aggregator::open_with_pricing(db_path, pricing)?;
    let stats = agg.dashboard_stats()?;
    let models = agg.model_breakdown()?;

    println!("\n  aitop — AI Token Usage\n");
    println!(
        "  Burn Rate: ${:.2}/hr    Today: ${:.2}    This Week: ${:.2}    All-time: ${:.2}",
        stats.burn_rate_per_hour, stats.spend_today, stats.spend_this_week, stats.spend_all_time,
    );
    println!(
        "  Messages: {}    Sessions: {}    Cache Hit: {:.0}%\n",
        stats.total_messages,
        stats.total_sessions,
        agg.cache_hit_ratio()? * 100.0,
    );

    println!(
        "  {:<20} {:>10} {:>10} {:>10} {:>8}",
        "Model", "Input", "Output", "Calls", "Cost"
    );
    println!("  {}", "\u{2500}".repeat(62));
    for m in &models {
        println!(
            "  {:<20} {:>10} {:>10} {:>10} {:>8}",
            shorten_model(&m.model),
            format_tokens(m.input_tokens),
            format_tokens(m.output_tokens),
            m.call_count,
            format!("${:.2}", m.cost),
        );
    }
    let total_cost: f64 = models.iter().map(|m| m.cost).sum();
    println!("  {}", "\u{2500}".repeat(62));
    println!(
        "  {:<20} {:>10} {:>10} {:>10} {:>8}",
        "Total", "", "", "",
        format!("${:.2}", total_cost)
    );
    println!();

    Ok(())
}

fn print_tmux_status(db_path: &std::path::Path) -> Result<()> {
    let agg = Aggregator::open(db_path)?;
    let stats = agg.dashboard_stats()?;
    print!(
        "{}",
        aitop::app::format_tmux_status(stats.burn_rate_per_hour, stats.spend_today, stats.total_sessions)
    );
    Ok(())
}

fn run_tui(
    config: Config,
    write_db: Database,
    db_path: &std::path::Path,
    projects_dir: &std::path::Path,
    pricing: PricingRegistry,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut theme = ui::theme::get_theme(&config.theme);
    let mut state = AppState::new(config);

    let agg = Aggregator::open_with_pricing(db_path, pricing)?;
    state.refresh_data(&agg);

    // Delta banner: load last_checked_at and compute deltas
    {
        let meta_db = Database::open(db_path)?;
        if let Ok(Some(last_ts)) = meta_db.get_last_checked_at() {
            if let Ok(banner) = agg.delta_since(&last_ts) {
                if banner.spend_delta > 0.0 || banner.new_sessions > 0 {
                    state.banner_shown_at = Some(Instant::now());
                    state.delta_banner = Some(banner);
                }
            }
        }
    }

    // Set up file watchers for all providers
    let (watcher_tx, watcher_rx) = mpsc::channel::<String>();
    let (tokio_tx, mut tokio_rx) = tokio::sync::mpsc::unbounded_channel::<FsEvent>();

    let mut _watchers: Vec<notify::RecommendedWatcher> = Vec::new();

    // Watch Claude projects dir
    if projects_dir.exists() {
        if let Ok(w) = watch_directory(projects_dir, tokio_tx.clone()) {
            _watchers.push(w);
        }
    }
    // Watch Gemini dir
    let gemini_dir = Provider::Gemini.default_dir();
    if gemini_dir.exists() {
        if let Ok(w) = watch_directory(&gemini_dir, tokio_tx.clone()) {
            _watchers.push(w);
        }
    }
    // Watch OpenClaw dir
    let openclaw_dir = Provider::OpenClaw.default_dir();
    if openclaw_dir.exists() {
        if let Ok(w) = watch_directory(&openclaw_dir, tokio_tx) {
            _watchers.push(w);
        }
    }

    let bridge_tx = watcher_tx;
    std::thread::spawn(move || {
        while let Some(event) = tokio_rx.blocking_recv() {
            match event {
                FsEvent::Changed(path) => {
                    let _ = bridge_tx.send(path);
                }
            }
        }
    });

    let result = run_event_loop(
        &mut terminal, &mut state, &write_db, &agg, &mut theme, &watcher_rx,
    );

    // Save last_checked_at on quit
    {
        let meta_db = Database::open(db_path)?;
        let now = chrono::Utc::now().to_rfc3339();
        let _ = meta_db.set_last_checked_at(&now);
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    write_db: &Database,
    agg: &Aggregator,
    theme: &mut ui::theme::Theme,
    watcher_rx: &mpsc::Receiver<String>,
) -> Result<()> {
    let mut last_refresh = Instant::now();
    let mut last_replay_tick = Instant::now();
    let max_refresh_interval = Duration::from_secs(30);
    let debounce_interval = Duration::from_millis(500);

    loop {
        let secs_since = last_refresh.elapsed().as_secs();
        let secs_until = 30u64.saturating_sub(secs_since);

        terminal.draw(|f| {
            let (tab_area, content_area, status_bar_area) = ui::layout::main_layout(f.area());
            state.status_bar_area = status_bar_area;

            render_tab_bar(f, state, theme, tab_area);

            if state.split_mode {
                let (left_area, right_area) = ui::layout::split_content(content_area);

                // Left pane: current view
                state.content_area = left_area;
                render_view(f, state, theme, state.view);

                // Right pane: split view
                if let Some(right_view) = state.split_view {
                    state.content_area = right_area;
                    render_view(f, state, theme, right_view);
                }

                // Restore content_area for other overlays
                state.content_area = content_area;
            } else {
                state.content_area = content_area;
                render_view(f, state, theme, state.view);
            }

            render_status_bar(f, state, theme, status_bar_area, secs_until);

            if state.show_help {
                ui::help::render_help(f, theme);
            }

            if state.detail_session.is_some() {
                ui::session_detail::render_session_detail(f, state, theme);
            }

            if state.filter_active {
                ui::filter::render_filter(f, state, theme);
            }
        })?;

        if state.should_quit {
            break;
        }

        state.advance_pulse();
        state.check_banner_timeout();

        // Clear expired alert shimmer and reset terminal title
        if let Some((_, t)) = &state.alert_flash {
            if t.elapsed().as_secs() >= 5 {
                state.alert_flash = None;
                aitop::app::reset_terminal_title();
            }
        }

        // Replay auto-advance
        if state.replay_active && !state.replay_paused {
            let replay_interval = Duration::from_millis(1000 / state.replay_speed as u64);
            if last_replay_tick.elapsed() >= replay_interval {
                state.replay_advance();
                last_replay_tick = Instant::now();
            }
        }

        let timeout = Duration::from_millis(100);
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if state.delta_banner.is_some() {
                    state.dismiss_banner();
                }
                handle_key(state, key, theme, agg);
            }
        }

        // Check for file watcher events
        let mut got_fs_event = false;
        while let Ok(path) = watcher_rx.try_recv() {
            let result = if path.contains("/.gemini/") {
                // Gemini session file — derive project from path
                let file_path = std::path::PathBuf::from(&path);
                let project = file_path
                    .ancestors()
                    .find(|p| p.parent().map(|pp| pp.file_name().and_then(|n| n.to_str()) == Some("tmp")).unwrap_or(false))
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("gemini")
                    .to_string();
                let sf = aitop::data::scanner::SessionFile {
                    path: file_path.clone(),
                    session_id: file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string(),
                    project: project.clone(),
                };
                write_db.ingest_gemini_file(&sf).map(|_| (project, 0u64))
            } else if path.contains("/.openclaw/") {
                let file_path = std::path::PathBuf::from(&path);
                let project = file_path
                    .ancestors()
                    .find(|p| p.parent().map(|pp| pp.file_name().and_then(|n| n.to_str()) == Some("agents")).unwrap_or(false))
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("openclaw")
                    .to_string();
                let sf = aitop::data::scanner::SessionFile {
                    path: file_path.clone(),
                    session_id: file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string(),
                    project: project.clone(),
                };
                write_db.ingest_openclaw_file(&sf).map(|_| (project, 0u64))
            } else {
                write_db.ingest_file_by_path(&path)
            };
            if let Ok((project, _offset)) = result {
                state.last_live_event = Some(Instant::now());
                state.live_project = Some(project);
                got_fs_event = true;
            }
        }

        if got_fs_event && last_refresh.elapsed() >= debounce_interval {
            state.refresh_data(agg);
            last_refresh = Instant::now();
        }

        if last_refresh.elapsed() >= max_refresh_interval {
            state.refresh_data(agg);
            last_refresh = Instant::now();
        }

        if state.needs_refresh {
            state.refresh_data(agg);
            last_refresh = Instant::now();
        }

        // Check budget notifications after data refresh
        state.check_budget_notifications();
    }

    Ok(())
}

fn handle_key(
    state: &mut AppState,
    key: event::KeyEvent,
    theme: &mut ui::theme::Theme,
    agg: &Aggregator,
) {
    if state.show_help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::F(1) => state.show_help = false,
            _ => {}
        }
        return;
    }

    if state.filter_active {
        handle_filter_key(state, key);
        return;
    }

    if state.replay_active {
        handle_replay_key(state, key);
        return;
    }

    if state.detail_session.is_some() {
        handle_detail_key(state, key);
        return;
    }

    match key.code {
        KeyCode::Char('q') => state.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
        }

        KeyCode::Char('d') => state.view = View::Dashboard,
        KeyCode::Char('s') => state.view = View::Sessions,
        KeyCode::Char('m') => state.view = View::Models,
        KeyCode::Char('t') => state.view = View::Trends,
        KeyCode::Char('1') => state.view = View::Dashboard,
        KeyCode::Char('2') => state.view = View::Sessions,
        KeyCode::Char('3') => state.view = View::Models,
        KeyCode::Char('4') => state.view = View::Trends,

        KeyCode::Char('?') | KeyCode::F(1) => state.show_help = true,
        KeyCode::Char('r') => state.needs_refresh = true,
        KeyCode::Char('\\') => state.toggle_split(),
        KeyCode::Char('/') => {
            state.filter_active = true;
        }

        // Split pane: Shift+letter changes right pane view
        KeyCode::Char('D') if state.split_mode => {
            state.split_view = Some(View::Dashboard);
        }
        KeyCode::Char('S') if state.split_mode => {
            state.split_view = Some(View::Sessions);
        }
        KeyCode::Char('M') if state.split_mode => {
            state.split_view = Some(View::Models);
        }
        KeyCode::Char('T') if state.split_mode => {
            state.split_view = Some(View::Trends);
        }

        KeyCode::Char('p') if state.view != View::Sessions => {
            state.theme_index = (state.theme_index + 1) % THEME_NAMES.len();
            *theme = ui::theme::get_theme(THEME_NAMES[state.theme_index]);
            state.theme_flash = Some(std::time::Instant::now());
        }

        _ => match state.view {
            View::Dashboard => handle_dashboard_key(state, key),
            View::Sessions => handle_sessions_key(state, key, agg),
            View::Models => handle_models_key(state, key),
            View::Trends => handle_trends_key(state, key),
        },
    }
}

fn handle_dashboard_key(_state: &mut AppState, _key: event::KeyEvent) {}

fn handle_sessions_key(state: &mut AppState, key: event::KeyEvent, agg: &Aggregator) {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => state.next_session(),
        KeyCode::Up | KeyCode::Char('k') => state.prev_session(),
        KeyCode::Enter => {
            if let Some(session_id) = state.selected_session_id() {
                if let Ok(messages) = agg.session_detail(&session_id) {
                    state.detail_messages = messages;
                }
                state.detail_session = Some(session_id);
                state.detail_scroll = 0;
            }
        }
        KeyCode::Char('y') => {
            // Copy selected session info to clipboard
            if let Some(idx) = state.session_table_state.selected() {
                if let Some(session) = state.displayed_sessions().get(idx).cloned() {
                    let text = aitop::app::format_session_for_clipboard(&session);
                    if aitop::app::copy_to_clipboard(&text) {
                        state.copy_flash = Some(Instant::now());
                    }
                }
            }
        }
        KeyCode::Char('Y') => {
            // Copy all visible sessions as TSV
            let sessions = state.displayed_sessions().to_vec();
            let tsv = aitop::app::format_sessions_as_tsv(&sessions);
            if aitop::app::copy_to_clipboard(&tsv) {
                state.copy_flash = Some(Instant::now());
            }
        }
        KeyCode::Char('c') => toggle_sort(state, SessionSort::Cost),
        KeyCode::Char('n') => toggle_sort(state, SessionSort::Tokens),
        KeyCode::Char('p') => toggle_sort(state, SessionSort::Project),
        KeyCode::Char('u') => toggle_sort(state, SessionSort::Recent),
        _ => {}
    }
}

fn toggle_sort(state: &mut AppState, sort: SessionSort) {
    if state.session_sort == sort {
        state.sort_ascending = !state.sort_ascending;
    } else {
        state.session_sort = sort;
        state.sort_ascending = false;
    }
    state.sort_sessions();
}

fn handle_detail_key(state: &mut AppState, key: event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            state.detail_session = None;
            state.detail_messages.clear();
            state.detail_scroll = 0;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = state.detail_messages.len().saturating_sub(1);
            state.detail_scroll = (state.detail_scroll + 1).min(max);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.detail_scroll = state.detail_scroll.saturating_sub(1);
        }
        KeyCode::Char('R') => {
            state.start_replay();
        }
        _ => {}
    }
}

fn handle_replay_key(state: &mut AppState, key: event::KeyEvent) {
    match key.code {
        KeyCode::Esc => state.stop_replay(),
        KeyCode::Char(' ') => state.toggle_replay_pause(),
        KeyCode::Char('+') | KeyCode::Char('=') => state.replay_speed_up(),
        KeyCode::Char('-') => state.replay_speed_down(),
        _ => {}
    }
}

fn handle_filter_key(state: &mut AppState, key: event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            state.filter_active = false;
        }
        KeyCode::Enter => {
            state.filter_active = false;
            state.apply_filter();
            state.session_table_state.select(Some(0));
        }
        KeyCode::Backspace => {
            state.filter_text.pop();
            state.apply_filter();
        }
        KeyCode::Char(c) => {
            state.filter_text.push(c);
            state.apply_filter();
        }
        _ => {}
    }
}

fn handle_models_key(_state: &mut AppState, _key: event::KeyEvent) {}

fn handle_trends_key(state: &mut AppState, key: event::KeyEvent) {
    match key.code {
        KeyCode::Char('w') => {
            state.trend_range = TrendRange::Week;
            state.needs_refresh = true;
        }
        KeyCode::Char('o') => {
            state.trend_range = TrendRange::Month;
            state.needs_refresh = true;
        }
        KeyCode::Char('a') => {
            state.trend_range = TrendRange::All;
            state.needs_refresh = true;
        }
        KeyCode::Char('n') => {
            state.show_token_overlay = !state.show_token_overlay;
        }
        KeyCode::Left => {
            state.trend_range = match state.trend_range {
                TrendRange::Month => TrendRange::Week,
                TrendRange::All => TrendRange::Month,
                TrendRange::Week => TrendRange::Week,
            };
            state.needs_refresh = true;
        }
        KeyCode::Right => {
            state.trend_range = match state.trend_range {
                TrendRange::Week => TrendRange::Month,
                TrendRange::Month => TrendRange::All,
                TrendRange::All => TrendRange::All,
            };
            state.needs_refresh = true;
        }
        KeyCode::Char('b') => {
            state.chart_type = match state.chart_type {
                aitop::app::ChartType::Line => aitop::app::ChartType::Bar,
                aitop::app::ChartType::Bar => aitop::app::ChartType::Line,
            };
        }
        _ => {}
    }
}

fn render_view(
    f: &mut ratatui::Frame,
    state: &mut AppState,
    theme: &ui::theme::Theme,
    view: View,
) {
    match view {
        View::Dashboard => ui::dashboard::render_dashboard(f, state, theme),
        View::Sessions => ui::sessions::render_sessions(f, state, theme),
        View::Models => ui::models::render_models(f, state, theme),
        View::Trends => ui::trends::render_trends(f, state, theme),
    }
}

fn render_tab_bar(
    f: &mut ratatui::Frame,
    state: &AppState,
    theme: &ui::theme::Theme,
    area: Rect,
) {
    let tab_titles: Vec<Line> = vec![
        tab_label("D", "ashboard", state.view == View::Dashboard, theme),
        tab_label("S", "essions", state.view == View::Sessions, theme),
        tab_label("M", "odels", state.view == View::Models, theme),
        tab_label("T", "rends", state.view == View::Trends, theme),
    ];

    let (is_live, status_text) = state.live_status();
    let pulse_char = state.pulse_indicator();

    // Check for active alert shimmer (5-second duration)
    let alert_info = state.alert_flash.as_ref().and_then(|(msg, t)| {
        let elapsed = t.elapsed();
        if elapsed.as_secs() < 5 {
            Some((msg.clone(), elapsed))
        } else {
            None
        }
    });

    let live_indicator = if is_live {
        let project_label = state.live_project.as_deref().unwrap_or("");
        if project_label.is_empty() {
            format!(" {} {} ", pulse_char, status_text)
        } else {
            format!(" {} {} {} ", pulse_char, status_text, project_label)
        }
    } else {
        format!(" {} {} ", pulse_char, status_text)
    };

    let live_style = if is_live {
        Style::default().fg(theme.success).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };

    // Right-aligned content: alert shimmer takes priority over live indicator
    let right_title = if let Some((ref alert_msg, elapsed)) = alert_info {
        // Shimmer: pulse between danger and accent based on tick
        let phase = (elapsed.as_millis() / 300) % 2;
        let alert_color = if phase == 0 { theme.danger } else { theme.tertiary };
        let alert_style = Style::default()
            .fg(alert_color)
            .add_modifier(Modifier::BOLD);
        Line::from(Span::styled(format!(" {} ", alert_msg), alert_style)).right_aligned()
    } else {
        Line::from(Span::styled(live_indicator, live_style)).right_aligned()
    };

    // Border color shimmers during alert
    let border_color = if alert_info.is_some() {
        let phase = (state.pulse_tick / 2) % 2;
        if phase == 0 { theme.danger } else { theme.tertiary }
    } else {
        theme.muted
    };

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    " aitop ",
                    Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
                ))
                .title(right_title),
        )
        .select(state.view.index())
        .highlight_style(
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" \u{2502} ", Style::default().fg(theme.muted)));

    f.render_widget(tabs, area);
}

fn render_status_bar(
    f: &mut ratatui::Frame,
    state: &mut AppState,
    theme: &ui::theme::Theme,
    area: Rect,
    secs_until_refresh: u64,
) {
    // Show "Copied!" flash for 2 seconds
    let flash_active = state.copy_flash.is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(2));

    let left_text = if flash_active {
        format!(
            "Copied! \u{2502} aitop v{} \u{2502} {} sessions \u{2502} ${:.2} all-time",
            env!("CARGO_PKG_VERSION"),
            state.dashboard.total_sessions, state.dashboard.spend_all_time
        )
    } else {
        format!(
            "aitop v{} \u{2502} {} sessions \u{2502} ${:.2} all-time",
            env!("CARGO_PKG_VERSION"),
            state.dashboard.total_sessions, state.dashboard.spend_all_time
        )
    };

    // Show theme name flash for 2 seconds after cycling
    let theme_flash_active = state
        .theme_flash
        .map(|t| t.elapsed().as_secs() < 2)
        .unwrap_or(false);
    if !theme_flash_active && state.theme_flash.is_some() {
        state.theme_flash = None;
    }

    let hints = if theme_flash_active {
        format!("theme: {}  (p to cycle)", theme.name)
    } else {
        match state.view {
            View::Dashboard => "d:dashboard  s:sessions  m:models  t:trends  ?:help  p:theme".to_string(),
            View::Sessions => "j/k:navigate  c:cost  n:tokens  p:project  u:updated  /:filter  ?:help".to_string(),
            View::Models => "d:dashboard  s:sessions  t:trends  p:theme  ?:help".to_string(),
            View::Trends => "w:week  o:month  a:all  b:bar/line  n:tokens  \u{2190}\u{2192}:cycle  p:theme  ?:help".to_string(),
        }
    };

    let right_text = format!("{}  \u{27f3} {}s", hints, secs_until_refresh);

    let available = area.width as usize;
    let left_len = left_text.len();
    let right_len = right_text.len();

    let mut spans = vec![Span::styled(left_text, Style::default().fg(theme.text_dim))];

    if left_len + right_len < available {
        spans.push(Span::raw(" ".repeat(available - left_len - right_len)));
    } else {
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(right_text, Style::default().fg(theme.muted)));

    let bar = ratatui::widgets::Paragraph::new(Line::from(spans))
        .style(Style::default().bg(ratatui::style::Color::Rgb(30, 30, 35)));

    f.render_widget(bar, area);
}

fn tab_label<'a>(
    shortcut: &'a str,
    rest: &'a str,
    active: bool,
    theme: &ui::theme::Theme,
) -> Line<'a> {
    let shortcut_style = if active {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::UNDERLINED)
    };

    let rest_style = if active {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_dim)
    };

    Line::from(vec![
        Span::styled(shortcut, shortcut_style),
        Span::styled(rest, rest_style),
    ])
}
