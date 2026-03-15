use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Row, Table};
use ratatui::Frame;

use super::theme::Theme;
use crate::app::{AppState, SessionSort};

pub fn render_sessions(f: &mut Frame, state: &mut AppState, theme: &Theme) {
    let area = state.content_area;

    let sort_indicator = state.sort_indicator();
    let filter_label = if !state.filter_text.is_empty() {
        format!(" [filter: {}]", state.filter_text)
    } else {
        String::new()
    };

    let cost_suffix = if state.session_sort == SessionSort::Cost { sort_indicator } else { "" };
    let tokens_suffix = if state.session_sort == SessionSort::Tokens { sort_indicator } else { "" };
    let project_suffix = if state.session_sort == SessionSort::Project { sort_indicator } else { "" };
    let updated_suffix = if state.session_sort == SessionSort::Recent { sort_indicator } else { "" };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled(" S", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("essions ", Style::default().fg(theme.text)),
            Span::styled(
                format!("({}) ", state.displayed_sessions().len()),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(
                filter_label,
                Style::default().fg(theme.tertiary),
            ),
            Span::raw("  "),
            Span::styled("c", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled(format!("ost{} ", cost_suffix), Style::default().fg(theme.text_dim)),
            Span::styled("n", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled(format!("tokens{} ", tokens_suffix), Style::default().fg(theme.text_dim)),
            Span::styled("p", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled(format!("roject{} ", project_suffix), Style::default().fg(theme.text_dim)),
            Span::styled("u", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled(format!("pdated{} ", updated_suffix), Style::default().fg(theme.text_dim)),
        ]));

    let header = Row::new(vec![
        "#", "Project", "Model", "Tokens", "Cost", "Msgs", "7d", "Updated",
    ])
    .style(
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(0);

    let sessions = state.displayed_sessions().to_vec();
    let rows: Vec<Row> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let style = if Some(i) == state.session_table_state.selected() {
                Style::default()
                    .fg(theme.text)
                    .bg(ratatui::style::Color::Rgb(40, 40, 50))
                    .add_modifier(Modifier::BOLD)
            } else if i % 2 == 0 {
                Style::default().fg(theme.text)
            } else {
                Style::default().fg(theme.text_dim)
            };

            let sparkline = state
                .session_sparklines
                .get(&s.id)
                .map(|costs| render_sparkline(costs))
                .unwrap_or_else(|| "\u{2581}\u{2581}\u{2581}\u{2581}\u{2581}\u{2581}\u{2581}".to_string());

            Row::new(vec![
                format!("{}", i + 1),
                truncate(&s.project, 14),
                shorten_model(&s.model),
                format_tokens(s.total_tokens),
                format!("${:.2}", s.total_cost),
                s.msg_count.to_string(),
                sparkline,
                format_relative_time(&s.updated_at),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(15),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(
        Style::default()
            .bg(ratatui::style::Color::Rgb(50, 50, 65))
            .add_modifier(Modifier::BOLD),
    );

    f.render_stateful_widget(table, area, &mut state.session_table_state);
}

fn render_sparkline(values: &[f64]) -> String {
    const BARS: [char; 8] = ['\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}'];

    if values.is_empty() {
        return String::new();
    }

    let max = values.iter().cloned().fold(0.0f64, f64::max);
    if max == 0.0 {
        return BARS[0].to_string().repeat(values.len());
    }

    values
        .iter()
        .map(|v| {
            let normalized = (v / max * 7.0).round() as usize;
            BARS[normalized.min(7)]
        })
        .collect()
}

fn shorten_model(model: &str) -> String {
    model
        .replace("claude-", "")
        .replace("-20241022", "")
        .replace("-20250514", "")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}\u{2026}", &s[..max - 1])
    } else {
        s.to_string()
    }
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

fn format_relative_time(iso: &str) -> String {
    use chrono::{DateTime, Utc};
    let Ok(dt) = iso.parse::<DateTime<Utc>>() else {
        return iso.to_string();
    };
    let now = Utc::now();
    let diff = now - dt;

    if diff.num_minutes() < 1 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else {
        format!("{}d ago", diff.num_days())
    }
}
