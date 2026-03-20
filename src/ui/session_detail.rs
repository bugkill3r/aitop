use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::format::{format_tokens, shorten_model};
use super::theme::Theme;
use crate::app::AppState;

pub fn render_session_detail(f: &mut Frame, state: &AppState, theme: &Theme) {
    let area = centered_rect(70, 80, f.area());
    f.render_widget(Clear, area);

    // Find the session summary
    let session = state
        .displayed_sessions()
        .iter()
        .find(|s| Some(&s.id) == state.detail_session.as_ref());

    let session = match session {
        Some(s) => s,
        None => {
            let para = Paragraph::new("Session not found")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.accent))
                        .title(" Session Detail (Esc to close) "),
                );
            f.render_widget(para, area);
            return;
        }
    };

    let messages = &state.detail_messages;

    // Compute stats
    let total_input: i64 = messages.iter().map(|m| m.input_tokens).sum();
    let total_output: i64 = messages.iter().map(|m| m.output_tokens).sum();
    let total_cache_read: i64 = messages.iter().map(|m| m.cache_read).sum();
    let total_cache_creation: i64 = messages.iter().map(|m| m.cache_creation).sum();
    let cache_total = total_cache_read + total_input + total_cache_creation;
    let cache_ratio = if cache_total > 0 {
        total_cache_read as f64 / cache_total as f64 * 100.0
    } else {
        0.0
    };

    // Duration
    let duration = compute_duration(&session.started_at, &session.updated_at);

    // Token distribution
    let token_total = total_input + total_output;
    let input_pct = if token_total > 0 {
        total_input as f64 / token_total as f64
    } else {
        0.5
    };
    let output_pct = 1.0 - input_pct;

    let title_text = if state.replay_active {
        let pause_str = if state.replay_paused { "PAUSED " } else { "" };
        let (tokens, cost) = state.replay_running_totals();
        format!(
            " REPLAY {}{}x | {}/{} msgs | {} tok | ${:.4} (Esc:exit Space:pause +/-:speed) ",
            pause_str,
            state.replay_speed,
            state.replay_index + 1,
            state.detail_messages.len(),
            format_tokens(tokens),
            cost,
        )
    } else {
        " Session Detail (Esc to close, j/k to scroll, R:replay) ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            title_text,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Metadata section
    lines.push(Line::from(vec![
        Span::styled(
            "  Project: ",
            Style::default().fg(theme.text_dim),
        ),
        Span::styled(
            &session.project,
            Style::default()
                .fg(theme.text)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Model:   ", Style::default().fg(theme.text_dim)),
        Span::styled(
            shorten_model(&session.model),
            Style::default().fg(theme.secondary),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Duration:", Style::default().fg(theme.text_dim)),
        Span::styled(
            format!(" {}", duration),
            Style::default().fg(theme.text),
        ),
        Span::styled("    Messages: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            session.msg_count.to_string(),
            Style::default().fg(theme.text),
        ),
    ]));

    lines.push(Line::from(""));

    // Cost and token stats
    lines.push(Line::from(vec![
        Span::styled("  Total Cost: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            format!("${:.4}", session.total_cost),
            Style::default()
                .fg(theme.tertiary)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Input:  ", Style::default().fg(theme.text_dim)),
        Span::styled(
            format_tokens(total_input),
            Style::default().fg(theme.secondary),
        ),
        Span::styled("    Output: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            format_tokens(total_output),
            Style::default().fg(theme.tertiary),
        ),
        Span::styled("    Cache Hit: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            format!("{:.0}%", cache_ratio),
            Style::default().fg(theme.success),
        ),
    ]));

    lines.push(Line::from(""));

    // Token distribution bar
    let bar_width = (inner.width as usize).saturating_sub(6);
    let input_bar_len = (input_pct * bar_width as f64) as usize;
    let output_bar_len = bar_width.saturating_sub(input_bar_len);

    lines.push(Line::from(Span::styled(
        "  Token Distribution:",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            "\u{2588}".repeat(input_bar_len),
            Style::default().fg(theme.secondary),
        ),
        Span::styled(
            "\u{2588}".repeat(output_bar_len),
            Style::default().fg(theme.tertiary),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            format!("  Input {:.0}%", input_pct * 100.0),
            Style::default().fg(theme.secondary),
        ),
        Span::styled(" | ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("Output {:.0}%", output_pct * 100.0),
            Style::default().fg(theme.tertiary),
        ),
    ]));

    lines.push(Line::from(""));

    // Message timeline header
    lines.push(Line::from(Span::styled(
        "  Message Timeline:",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::styled(
            format!(
                "  {:>5} {:<8} {:<6} {:<8} {:<8} {:<8}",
                "#", "Time", "Type", "In", "Out", "Cost"
            ),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // In replay mode, show only messages up to replay_index
    let visible_msg_slice = if state.replay_active {
        let end = (state.replay_index + 1).min(messages.len());
        &messages[..end]
    } else {
        messages
    };

    // Scrollable message list
    let max_cost = visible_msg_slice
        .iter()
        .map(|m| m.cost_usd)
        .fold(0.0f64, f64::max);

    let available_height = (inner.height as usize).saturating_sub(lines.len());
    let scroll = if state.replay_active {
        // Auto-scroll to keep current replay message visible
        visible_msg_slice.len().saturating_sub(available_height)
    } else {
        state.detail_scroll.min(messages.len().saturating_sub(1))
    };
    let visible_messages = visible_msg_slice.iter().enumerate().skip(scroll).take(available_height);

    for (i, msg) in visible_messages {
        let time = msg.timestamp.get(11..16).unwrap_or("??:??");
        let type_short = if msg.msg_type == "assistant" {
            "resp"
        } else {
            &msg.msg_type[..msg.msg_type.len().min(4)]
        };

        // Cost bar
        let cost_bar_width: usize = 10;
        let bar_len = if max_cost > 0.0 {
            ((msg.cost_usd / max_cost) * cost_bar_width as f64) as usize
        } else {
            0
        };
        let bar: String = "\u{2588}".repeat(bar_len);
        let empty: String = "\u{2591}".repeat(cost_bar_width.saturating_sub(bar_len));

        let style = if i % 2 == 0 {
            Style::default().fg(theme.text)
        } else {
            Style::default().fg(theme.text_dim)
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "  {:>5} {:<8} {:<6} {:<8} {:<8} ${:<7.4} ",
                    i + 1,
                    time,
                    type_short,
                    format_tokens(msg.input_tokens),
                    format_tokens(msg.output_tokens),
                    msg.cost_usd,
                ),
                style,
            ),
            Span::styled(bar, Style::default().fg(theme.bar_filled)),
            Span::styled(empty, Style::default().fg(theme.bar_empty)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

fn compute_duration(started: &str, updated: &str) -> String {
    use chrono::DateTime;
    let start = started.parse::<DateTime<chrono::Utc>>();
    let end = updated.parse::<DateTime<chrono::Utc>>();

    match (start, end) {
        (Ok(s), Ok(e)) => {
            let diff = e - s;
            let hours = diff.num_hours();
            let mins = diff.num_minutes() % 60;
            if hours > 0 {
                format!("{}h {}m", hours, mins)
            } else {
                format!("{}m", mins.max(1))
            }
        }
        _ => "N/A".to_string(),
    }
}
