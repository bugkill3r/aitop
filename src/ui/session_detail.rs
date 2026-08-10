use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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

    // Turns are built once when the detail view opens (see `AppState::open_detail`),
    // not per frame — the key handler needs the same count to clamp scrolling.
    let turns = &state.detail_turns;
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
                "  {:>4} {:<6} {:>6} {:>3} {:>9} {:>4}  {}",
                "#", "Time", "Out", "It", "Cost", "Eff", "Prompt"
            ),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]));

        // A session with only a `(pre-prompt)` bucket has no captured prompt
        // text at all — explain why rather than showing a lone unnamed row.
        let has_prompt = turns.iter().any(|t| !t.synthetic);
        if !has_prompt {
            // Only Claude sessions carry prompt text; the Gemini and OpenClaw
            // parsers have no prompt field to read, so no restart will help.
            let hint = if session.provider == "claude" {
                "  No prompt text captured yet — restart aitop to backfill history."
            } else {
                "  Prompt capture is Claude-only — this provider records no prompt text."
            };
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(theme.text_dim),
            )));
        } else {
            let available_height = (inner.height as usize).saturating_sub(lines.len());
            let scroll = state.detail_scroll.min(turns.len().saturating_sub(1));
            // Fixed-width prefix before the prompt column: the 35-cell row
            // format below, plus the 4-cell score, plus a 2-cell gap.
            let prefix_width = 2 + 4 + 1 + 6 + 1 + 6 + 1 + 3 + 1 + 9 + 1 + 4 + 2;
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

/// Label for the synthetic turn holding messages that precede the first typed
/// prompt (resumed sessions, or history whose prompt text was never captured).
pub const PRE_PROMPT_LABEL: &str = "(pre-prompt)";

/// One conversational turn: a typed user prompt plus the token/cost of all the
/// assistant work it triggered (up to the next typed prompt).
pub struct Turn {
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
    /// True for the `(pre-prompt)` bucket, which has no typed prompt behind it.
    /// It exists so those messages' tokens and cost still appear in the timeline
    /// and the rows reconcile against the session header, but it is not scored.
    synthetic: bool,
}

impl Turn {
    /// Behavioral efficiency score (0–100, higher = a cleaner resolution).
    ///
    /// This is a friction / cost-efficiency proxy, NOT a correctness judgment:
    /// a prompt that got a wrong answer in one round-trip still scores high.
    /// Base score decays with the number of round-trips (a well-scoped prompt
    /// resolves in few iterations); it is then modulated by cache efficiency
    /// (poor context reuse signals churn). Returns None when the turn has no
    /// assistant response yet (interrupted or still pending), and for the
    /// synthetic `(pre-prompt)` bucket — scoring it would attribute friction to
    /// a prompt that does not exist.
    fn efficiency_score(&self) -> Option<u8> {
        if self.roundtrips == 0 || self.synthetic {
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
///
/// Messages that arrive before the first typed prompt (resumed sessions, or
/// history predating prompt capture) go into a leading synthetic `(pre-prompt)`
/// turn rather than being dropped, so the rows sum to the session total shown
/// in the header above them.
pub fn build_turns(messages: &[crate::data::aggregator::SessionMessage]) -> Vec<Turn> {
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
                synthetic: false,
            });
        } else if turns.is_empty() {
            turns.push(Turn {
                time: m.timestamp.get(11..16).unwrap_or("??:??").to_string(),
                prompt: PRE_PROMPT_LABEL.to_string(),
                input: 0,
                output: 0,
                cost: 0.0,
                roundtrips: 0,
                cache_read: 0,
                cache_creation: 0,
                synthetic: true,
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

/// Truncate a string to `max` terminal cells, appending an ellipsis.
///
/// Width is measured in display cells, not `char`s: a CJK codepoint occupies two
/// cells, so a character-count truncation would overrun the column by up to 2x.
fn clip(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.width() <= max {
        return s.to_string();
    }
    // Reserve one cell for the ellipsis.
    let budget = max.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0usize;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('\u{2026}');
    out
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

    /// A scored (non-synthetic) turn with the given friction inputs.
    fn turn(roundtrips: u32, input: i64, cache_read: i64) -> Turn {
        Turn {
            time: String::new(),
            prompt: String::new(),
            input,
            output: 0,
            cost: 0.0,
            roundtrips,
            cache_read,
            cache_creation: 0,
            synthetic: false,
        }
    }

    #[test]
    fn test_efficiency_score_clean_beats_churny() {
        let clean = turn(1, 0, 0);
        let churny = turn(40, 0, 0);
        let clean_score = clean.efficiency_score().unwrap();
        let churny_score = churny.efficiency_score().unwrap();
        assert!(clean_score > churny_score, "clean {} should beat churny {}", clean_score, churny_score);
        assert!(clean_score >= 70); // one round-trip is high efficiency
        assert!(churny_score < 20); // 40 round-trips is low efficiency
    }

    #[test]
    fn test_efficiency_score_none_when_no_response() {
        assert_eq!(turn(0, 0, 0).efficiency_score(), None);
    }

    #[test]
    fn test_cache_efficiency_lifts_score() {
        let cold = turn(5, 1000, 0);
        let warm = turn(5, 100, 900);
        assert!(warm.efficiency_score().unwrap() > cold.efficiency_score().unwrap());
    }

    #[test]
    fn test_pre_prompt_bucket_is_not_scored() {
        let mut synthetic = turn(5, 100, 900);
        synthetic.synthetic = true;
        assert_eq!(
            synthetic.efficiency_score(),
            None,
            "a bucket with no typed prompt behind it must not be scored"
        );
    }

    #[test]
    fn test_messages_before_first_prompt_go_to_pre_prompt_turn() {
        // A resumed session: assistant work arrives before any typed prompt.
        let messages = vec![
            msg("assistant", None, 10, 20, 0),
            msg("assistant", None, 5, 15, 0),
            msg("user", Some("now fix it"), 0, 0, 0),
            msg("assistant", None, 1, 2, 0),
        ];
        let turns = build_turns(&messages);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].prompt, PRE_PROMPT_LABEL);
        assert!(turns[0].synthetic);
        assert_eq!(turns[0].output, 35, "orphan tokens must not vanish");
        assert_eq!(turns[1].prompt, "now fix it");
        assert_eq!(turns[1].output, 2);
    }

    #[test]
    fn test_turn_totals_reconcile_with_message_totals() {
        let mut messages = vec![
            msg("assistant", None, 10, 20, 5),
            msg("user", Some("first"), 0, 0, 0),
            msg("assistant", None, 7, 9, 3),
            msg("user", None, 0, 0, 0),
            msg("assistant", None, 4, 6, 1),
            msg("user", Some("second"), 0, 0, 0),
            msg("assistant", None, 2, 3, 0),
        ];
        for (i, m) in messages.iter_mut().enumerate() {
            m.cost_usd = (i as f64 + 1.0) * 0.25;
        }

        let turns = build_turns(&messages);
        let turn_output: i64 = turns.iter().map(|t| t.output).sum();
        let turn_cost: f64 = turns.iter().map(|t| t.cost).sum();
        let msg_output: i64 = messages.iter().map(|m| m.output_tokens).sum();
        let msg_cost: f64 = messages.iter().map(|m| m.cost_usd).sum();

        assert_eq!(turn_output, msg_output);
        assert!(
            (turn_cost - msg_cost).abs() < 1e-9,
            "turn rows ({}) must sum to the session total in the header ({})",
            turn_cost,
            msg_cost
        );
    }

    #[test]
    fn test_clip_measures_display_width_not_char_count() {
        // Each CJK codepoint is 2 cells: 4 chars would be 8 cells, over budget.
        let clipped = clip("日本語テスト", 6);
        assert!(
            clipped.width() <= 6,
            "clipped {:?} is {} cells, over the 6-cell column",
            clipped,
            clipped.width()
        );
        // ASCII is unchanged when it fits.
        assert_eq!(clip("hello", 10), "hello");
        assert_eq!(clip("hello world", 5), "hell\u{2026}");
    }
}
