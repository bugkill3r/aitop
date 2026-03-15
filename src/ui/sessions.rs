use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Row, Table};
use ratatui::Frame;

use super::theme::Theme;
use crate::app::AppState;
pub fn render_sessions(f: &mut Frame, state: &mut AppState, theme: &Theme) {
    let area = state.content_area;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled(" S", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("essions ", Style::default().fg(theme.text)),
            Span::styled(
                format!("({}) ", state.sessions.len()),
                Style::default().fg(theme.text_dim),
            ),
            Span::raw("  "),
            Span::styled("c", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled("ost ", Style::default().fg(theme.text_dim)),
            Span::styled("n", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled("tokens ", Style::default().fg(theme.text_dim)),
            Span::styled("p", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled("roject ", Style::default().fg(theme.text_dim)),
            Span::styled("u", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled("pdated ", Style::default().fg(theme.text_dim)),
        ]));

    let header = Row::new(vec![
        "#", "Project", "Model", "Tokens", "Cost", "Messages", "Updated",
    ])
    .style(
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(0);

    let sessions = &state.sessions;
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

            Row::new(vec![
                format!("{}", i + 1),
                truncate(&s.project, 14),
                shorten_model(&s.model),
                format_tokens(s.total_tokens),
                format!("${:.2}", s.total_cost),
                s.msg_count.to_string(),
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

fn shorten_model(model: &str) -> String {
    model
        .replace("claude-", "")
        .replace("-20241022", "")
        .replace("-20250514", "")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max - 1])
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
