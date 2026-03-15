#![allow(dead_code)]

mod app;
mod config;
mod data;
mod ui;

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
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};
use ratatui::Terminal;

use app::{AppState, SessionSort, TrendRange, View};
use config::Config;
use data::aggregator::Aggregator;
use data::db::Database;
use data::scanner::scan_claude_projects;
use data::watcher::{watch_directory, FsEvent};

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

    // Ingest data with startup progress
    let db_path = Config::db_path();
    let db = Database::open(&db_path)?;
    let projects_dir = config.claude_projects_dir();
    let files = scan_claude_projects(&projects_dir)?;

    let total_files = files.len();
    if total_files > 0 {
        for (i, file) in files.iter().enumerate() {
            // Show startup progress (overwrite same line)
            eprint!(
                "\r  Indexing sessions... ({}/{} files)",
                i + 1,
                total_files
            );
            io::stderr().flush().ok();

            if let Err(e) = db.ingest_file(file) {
                eprintln!("\nWarning: failed to ingest {:?}: {}", file.path, e);
            }
        }
        // Clear the progress line
        eprint!("\r{}\r", " ".repeat(60));
        io::stderr().flush().ok();
    }

    if args.light {
        drop(db); // Close write connection for light mode
        return print_light_mode(&db_path);
    }

    // For TUI mode, keep write DB open and open a separate read connection
    run_tui(config, db, &db_path, &projects_dir)
}

fn print_light_mode(db_path: &std::path::Path) -> Result<()> {
    let agg = Aggregator::open(db_path)?;
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
            m.model.replace("claude-", ""),
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
        "Total",
        "",
        "",
        "",
        format!("${:.2}", total_cost)
    );
    println!();

    Ok(())
}

fn run_tui(
    config: Config,
    write_db: Database,
    db_path: &std::path::Path,
    projects_dir: &std::path::Path,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let theme = ui::theme::get_theme(&config.theme);
    let mut state = AppState::new(config);

    // Open a separate read-only connection for the aggregator (WAL mode allows concurrent access)
    let agg = Aggregator::open(db_path)?;
    state.refresh_data(&agg);

    // Set up file watcher channel (std::sync::mpsc for non-async event loop)
    let (watcher_tx, watcher_rx) = mpsc::channel::<String>();

    // Start file watcher - convert from tokio mpsc to std mpsc via a bridge
    let (tokio_tx, mut tokio_rx) = tokio::sync::mpsc::unbounded_channel::<FsEvent>();
    let _watcher = if projects_dir.exists() {
        match watch_directory(projects_dir, tokio_tx) {
            Ok(w) => Some(w),
            Err(_) => None,
        }
    } else {
        None
    };

    // Bridge thread: reads from tokio channel, forwards to std channel
    let bridge_tx = watcher_tx;
    std::thread::spawn(move || {
        // Use a blocking recv loop since tokio_rx.blocking_recv() works outside tokio runtime
        while let Some(event) = tokio_rx.blocking_recv() {
            match event {
                FsEvent::Changed(path) => {
                    let _ = bridge_tx.send(path);
                }
            }
        }
    });

    let result = run_event_loop(&mut terminal, &mut state, &write_db, &agg, &theme, &watcher_rx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    write_db: &Database,
    agg: &Aggregator,
    theme: &ui::theme::Theme,
    watcher_rx: &mpsc::Receiver<String>,
) -> Result<()> {
    let mut last_refresh = Instant::now();

    // Maximum refresh interval (fallback): 30 seconds
    let max_refresh_interval = Duration::from_secs(30);
    // Debounce: min 500ms between refreshes
    let debounce_interval = Duration::from_millis(500);

    loop {
        terminal.draw(|f| {
            let (tab_area, content_area) = ui::layout::main_layout(f.area());
            state.content_area = content_area;

            render_tab_bar(f, state, theme, tab_area);

            match state.view {
                View::Dashboard => ui::dashboard::render_dashboard(f, state, theme),
                View::Sessions => ui::sessions::render_sessions(f, state, theme),
                View::Models => ui::models::render_models(f, state, theme),
                View::Trends => ui::trends::render_trends(f, state, theme),
            }

            if state.show_help {
                ui::help::render_help(f, theme);
            }
        })?;

        if state.should_quit {
            break;
        }

        // Poll for keyboard events (short timeout for responsiveness)
        let timeout = Duration::from_millis(100);
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                handle_key(state, key);
            }
        }

        // Check for file watcher events (non-blocking)
        let mut got_fs_event = false;
        while let Ok(path) = watcher_rx.try_recv() {
            // Ingest the changed file incrementally
            match write_db.ingest_file_by_path(&path) {
                Ok((project, _offset)) => {
                    state.last_live_event = Some(Instant::now());
                    state.live_project = Some(project);
                    got_fs_event = true;
                }
                Err(_) => {
                    // Silently ignore ingest errors from watcher events
                }
            }
        }

        // Event-driven refresh: refresh immediately on file change (with debounce)
        if got_fs_event && last_refresh.elapsed() >= debounce_interval {
            state.refresh_data(agg);
            last_refresh = Instant::now();
        }

        // Fallback: refresh every 30s even if no file changes
        if last_refresh.elapsed() >= max_refresh_interval {
            state.refresh_data(agg);
            last_refresh = Instant::now();
        }

        // Manual refresh request (e.g. pressing 'r')
        if state.needs_refresh {
            state.refresh_data(agg);
            last_refresh = Instant::now();
        }
    }

    Ok(())
}

fn handle_key(state: &mut AppState, key: event::KeyEvent) {
    if state.show_help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::F(1) => state.show_help = false,
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') => state.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.should_quit = true;
        }

        // View switching (always global -- d/s/m/t work from any view)
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

        // View-specific
        _ => match state.view {
            View::Dashboard => handle_dashboard_key(state, key),
            View::Sessions => handle_sessions_key(state, key),
            View::Models => handle_models_key(state, key),
            View::Trends => handle_trends_key(state, key),
        },
    }
}

fn handle_dashboard_key(_state: &mut AppState, _key: event::KeyEvent) {
    // Dashboard-specific keys (future: panel focus cycling)
}

fn handle_sessions_key(state: &mut AppState, key: event::KeyEvent) {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => state.next_session(),
        KeyCode::Up | KeyCode::Char('k') => state.prev_session(),
        KeyCode::Char('c') => {
            state.session_sort = SessionSort::Cost;
            state.sort_sessions();
        }
        KeyCode::Char('n') => {
            state.session_sort = SessionSort::Tokens;
            state.sort_sessions();
        }
        KeyCode::Char('p') => {
            state.session_sort = SessionSort::Project;
            state.sort_sessions();
        }
        KeyCode::Char('u') => {
            state.session_sort = SessionSort::Recent;
            state.sort_sessions();
        }
        _ => {}
    }
}

fn handle_models_key(_state: &mut AppState, _key: event::KeyEvent) {
    // Models-specific keys (future: model detail drill-in)
}

fn handle_trends_key(state: &mut AppState, key: event::KeyEvent) {
    match key.code {
        KeyCode::Char('w') => {
            state.trend_range = TrendRange::Week;
            state.needs_refresh = true;
        }
        KeyCode::Char('o') => {
            // mOnth (m is taken by global Models nav)
            state.trend_range = TrendRange::Month;
            state.needs_refresh = true;
        }
        KeyCode::Char('a') => {
            state.trend_range = TrendRange::All;
            state.needs_refresh = true;
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
        _ => {}
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

    // Build the live/idle indicator
    let (is_live, status_text) = state.live_status();
    let live_indicator = if is_live {
        let project_label = state
            .live_project
            .as_deref()
            .unwrap_or("");
        if project_label.is_empty() {
            format!(" \u{25CF} {} ", status_text)
        } else {
            format!(" \u{25CF} {} {} ", status_text, project_label)
        }
    } else {
        format!(" \u{25CB} {} ", status_text)
    };

    let live_style = if is_live {
        Style::default()
            .fg(theme.success)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.muted))
                .title(Span::styled(
                    " aitop ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
                .title(
                    Line::from(Span::styled(live_indicator, live_style))
                        .right_aligned(),
                ),
        )
        .select(state.view.index())
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" \u{2502} ", Style::default().fg(theme.muted)));

    f.render_widget(tabs, area);
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
        Style::default()
            .fg(theme.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_dim)
    };

    Line::from(vec![
        Span::styled(shortcut, shortcut_style),
        Span::styled(rest, rest_style),
    ])
}

fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
