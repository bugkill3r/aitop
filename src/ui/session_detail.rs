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

    // Collapse into turns once (a typed prompt + the assistant work it drove).
    let turns = build_turns(messages);
    // Session-level average efficiency across scored turns.
    let scored: Vec<u8> = turns.iter().filter_map(|t| t.efficiency_score()).collect();
    let avg_efficiency: Option<u8> = if scored.is_empty() {
        None
    } else {
        Some((scored.iter().map(|&s| s as u32).sum::<u32>() / scored.len() as u32) as u8)
    };

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
        Span::styled("    Avg Eff: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            match avg_efficiency {
                Some(s) => format!("{}", s),
                None => "—".to_string(),
            },
            Style::default()
                .fg(score_color(avg_efficiency, theme))
                .add_modifier(Modifier::BOLD),
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

    // Prompt timeline header
    lines.push(Line::from(Span::styled(
        "  Prompt Timeline:",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));

    if state.replay_active {
        // Replay keeps the raw per-message view so the animation plays back
        // message-by-message.
        lines.push(Line::from(vec![Span::styled(
            format!(
                "  {:>5} {:<8} {:<6} {:<8} {:<8} {:<8}",
                "#", "Time", "Type", "In", "Out", "Cost"
            ),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]));

        let end = (state.replay_index + 1).min(messages.len());
        let visible_msg_slice = &messages[..end];
        let max_cost = visible_msg_slice
            .iter()
            .map(|m| m.cost_usd)
            .fold(0.0f64, f64::max);
        let available_height = (inner.height as usize).saturating_sub(lines.len());
        let scroll = visible_msg_slice.len().saturating_sub(available_height);

        for (i, msg) in visible_msg_slice.iter().enumerate().skip(scroll).take(available_height) {
            let time = msg.timestamp.get(11..16).unwrap_or("??:??");
            let type_short = if msg.msg_type == "assistant" {
                "resp"
            } else {
                &msg.msg_type[..msg.msg_type.len().min(4)]
            };
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
    } else {
        // Normal view: collapse each turn (a typed prompt + the assistant work
        // it triggered) into a single row showing the prompt, its cost, and an
        // efficiency score derived from behavioral signals.
        lines.push(Line::from(vec![Span::styled(
            format!(
                "  {:>4} {:<6} {:>6} {:>3} {:>9} {:>5}  {}",
                "#", "Time", "Out", "It", "Cost", "Eff", "Prompt"
            ),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]));

        if turns.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No prompt text captured yet — restart aitop to backfill history.",
                Style::default().fg(theme.text_dim),
            )));
        } else {
            let available_height = (inner.height as usize).saturating_sub(lines.len());
            let scroll = state.detail_scroll.min(turns.len().saturating_sub(1));
            // Fixed-width prefix before the prompt column.
            let prefix_width = 2 + 4 + 1 + 6 + 1 + 6 + 1 + 3 + 1 + 9 + 1 + 5 + 2;
            let prompt_width = (inner.width as usize).saturating_sub(prefix_width);

            for (i, turn) in turns.iter().enumerate().skip(scroll).take(available_height) {
                let style = if i % 2 == 0 {
                    Style::default().fg(theme.text)
                } else {
                    Style::default().fg(theme.text_dim)
                };
                let score = turn.efficiency_score();
                let score_label = match score {
                    Some(s) => format!("{:>4}", s),
                    None => "   —".to_string(),
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(
                            "  {:>4} {:<6} {:>6} {:>3} {:>9} ",
                            i + 1,
                            turn.time,
                            format_tokens(turn.output),
                            turn.roundtrips,
                            format!("${:.4}", turn.cost),
                        ),
                        style,
                    ),
                    Span::styled(
                        score_label,
                        Style::default()
                            .fg(score_color(score, theme))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  ", style),
                    Span::styled(clip(&turn.prompt, prompt_width), style),
                ]));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// One conversational turn: a typed user prompt plus the token/cost of all the
/// assistant work it triggered (up to the next typed prompt).
struct Turn {
    time: String,
    prompt: String,
    input: i64,
    output: i64,
    cost: f64,
    /// Assistant responses within the turn — the number of round-trips the
    /// prompt required to resolve. The core friction signal.
    roundtrips: u32,
    cache_read: i64,
    cache_creation: i64,
}

impl Turn {
    /// Behavioral efficiency score (0–100, higher = a cleaner resolution).
    ///
    /// This is a friction / cost-efficiency proxy, NOT a correctness judgment:
    /// a prompt that got a wrong answer in one round-trip still scores high.
    /// Base score decays with the number of round-trips (a well-scoped prompt
    /// resolves in few iterations); it is then modulated by cache efficiency
    /// (poor context reuse signals churn). Returns None when the turn has no
    /// assistant response yet (interrupted or still pending).
    fn efficiency_score(&self) -> Option<u8> {
        if self.roundtrips == 0 {
            return None;
        }
        let base = 100.0 * (-((self.roundtrips as f64 - 1.0) / 10.0)).exp();
        let base = base.clamp(0.0, 100.0);

        let cache_total = self.cache_read + self.input + self.cache_creation;
        let cache_ratio = if cache_total > 0 {
            self.cache_read as f64 / cache_total as f64
        } else {
            0.0
        };
        let modifier = 0.75 + 0.25 * cache_ratio;

        Some((base * modifier).round().clamp(0.0, 100.0) as u8)
    }
}

/// Group a session's messages into turns. A turn begins at each real typed
/// prompt (a `user` message with plain-text content); all subsequent messages
/// — assistant responses and tool-result rows — are attributed to it until the
/// next typed prompt.
fn build_turns(messages: &[crate::data::aggregator::SessionMessage]) -> Vec<Turn> {
    let mut turns: Vec<Turn> = Vec::new();
    for m in messages {
        let is_prompt = m.msg_type == "user"
            && m.content.as_ref().map(|c| !c.trim().is_empty()).unwrap_or(false);
        if is_prompt {
            turns.push(Turn {
                time: m.timestamp.get(11..16).unwrap_or("??:??").to_string(),
                prompt: m
                    .content
                    .clone()
                    .unwrap_or_default()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                input: 0,
                output: 0,
                cost: 0.0,
                roundtrips: 0,
                cache_read: 0,
                cache_creation: 0,
            });
        }
        if let Some(turn) = turns.last_mut() {
            turn.input += m.input_tokens;
            turn.output += m.output_tokens;
            turn.cost += m.cost_usd;
            turn.cache_read += m.cache_read;
            turn.cache_creation += m.cache_creation;
            if m.msg_type == "assistant" {
                turn.roundtrips += 1;
            }
        }
    }
    turns
}

/// Green → amber → red gradient for an efficiency score. `None` (pending turn)
/// renders dim.
fn score_color(score: Option<u8>, theme: &Theme) -> ratatui::style::Color {
    match score {
        Some(s) if s >= 70 => theme.success,
        Some(s) if s >= 40 => theme.bar_mid,
        Some(_) => theme.danger,
        None => theme.text_dim,
    }
}

/// Truncate a string to `max` display characters, appending an ellipsis.
fn clip(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::aggregator::SessionMessage;

    fn msg(msg_type: &str, content: Option<&str>, input: i64, output: i64, cache_read: i64) -> SessionMessage {
        SessionMessage {
            id: "x".to_string(),
            timestamp: "2025-01-01T10:20:30Z".to_string(),
            model: "claude".to_string(),
            msg_type: msg_type.to_string(),
            input_tokens: input,
            output_tokens: output,
            cache_read,
            cache_creation: 0,
            cost_usd: 0.0,
            content: content.map(|c| c.to_string()),
        }
    }

    #[test]
    fn test_build_turns_groups_and_counts_roundtrips() {
        let messages = vec![
            msg("user", Some("fix the bug"), 0, 0, 0),
            msg("assistant", None, 10, 20, 0),
            msg("user", None, 0, 0, 0), // tool result — not a new turn
            msg("assistant", None, 5, 15, 0),
            msg("user", Some("now add a test"), 0, 0, 0),
            msg("assistant", None, 8, 12, 0),
        ];
        let turns = build_turns(&messages);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].prompt, "fix the bug");
        assert_eq!(turns[0].roundtrips, 2); // two assistant round-trips
        assert_eq!(turns[0].output, 35);
        assert_eq!(turns[1].prompt, "now add a test");
        assert_eq!(turns[1].roundtrips, 1);
    }

    #[test]
    fn test_efficiency_score_clean_beats_churny() {
        let clean = Turn {
            time: String::new(), prompt: String::new(),
            input: 0, output: 0, cost: 0.0, roundtrips: 1, cache_read: 0, cache_creation: 0,
        };
        let churny = Turn {
            time: String::new(), prompt: String::new(),
            input: 0, output: 0, cost: 0.0, roundtrips: 40, cache_read: 0, cache_creation: 0,
        };
        let clean_score = clean.efficiency_score().unwrap();
        let churny_score = churny.efficiency_score().unwrap();
        assert!(clean_score > churny_score, "clean {} should beat churny {}", clean_score, churny_score);
        assert!(clean_score >= 70); // one round-trip is high efficiency
        assert!(churny_score < 20); // 40 round-trips is low efficiency
    }

    #[test]
    fn test_efficiency_score_none_when_no_response() {
        let pending = Turn {
            time: String::new(), prompt: String::new(),
            input: 0, output: 0, cost: 0.0, roundtrips: 0, cache_read: 0, cache_creation: 0,
        };
        assert_eq!(pending.efficiency_score(), None);
    }

    #[test]
    fn test_cache_efficiency_lifts_score() {
        let cold = Turn {
            time: String::new(), prompt: String::new(),
            input: 1000, output: 0, cost: 0.0, roundtrips: 5, cache_read: 0, cache_creation: 0,
        };
        let warm = Turn {
            time: String::new(), prompt: String::new(),
            input: 100, output: 0, cost: 0.0, roundtrips: 5, cache_read: 900, cache_creation: 0,
        };
        assert!(warm.efficiency_score().unwrap() > cold.efficiency_score().unwrap());
    }
}
