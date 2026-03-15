use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::Theme;
use super::widgets::cost_color::cost_color;
use crate::app::AppState;

pub fn render_models(f: &mut Frame, state: &AppState, theme: &Theme) {
    let area = state.content_area;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled(" M", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("odels ", Style::default().fg(theme.text)),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.models.is_empty() {
        f.render_widget(
            Paragraph::new("  No model data yet. Start using Claude Code to see stats here.")
                .style(Style::default().fg(theme.text_dim)),
            inner,
        );
        return;
    }

    let total_cost: f64 = state.models.iter().map(|m| m.cost).sum();
    let max_cost = state.models.iter().map(|m| m.cost).fold(0.0f64, f64::max);
    let bar_width = (inner.width as usize).saturating_sub(20);

    // Allocate vertical space per model (4 lines each + 1 blank)
    let mut lines = Vec::new();

    for model in &state.models {
        let short_name = shorten_model(&model.model);
        let pct = if total_cost > 0.0 {
            model.cost / total_cost * 100.0
        } else {
            0.0
        };

        // Line 1: Model name + cost
        let cost_width = inner.width as usize;
        let name_len = short_name.len() + 2; // "  " prefix
        let padding = cost_width.saturating_sub(name_len + 2);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {}", short_name),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>width$}", format!("${:.2}", model.cost), width = padding),
                Style::default().fg(cost_color(model.cost)).add_modifier(Modifier::BOLD),
            ),
        ]));

        // Line 2: Progress bar + percentage
        let bar_len = if max_cost > 0.0 {
            ((model.cost / max_cost) * bar_width as f64) as usize
        } else {
            0
        };
        let bar: String = "█".repeat(bar_len);
        let empty: String = "░".repeat(bar_width.saturating_sub(bar_len));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(bar, Style::default().fg(theme.bar_filled)),
            Span::styled(empty, Style::default().fg(theme.bar_empty)),
            Span::styled(
                format!(" {:.1}%", pct),
                Style::default().fg(theme.text_dim),
            ),
        ]));

        // Line 3: Detail stats
        let cache_total = model.cache_read + model.input_tokens + model.cache_creation;
        let cache_ratio = if cache_total > 0 {
            model.cache_read as f64 / cache_total as f64 * 100.0
        } else {
            0.0
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "  {}in  ·  {}out  ·  {:.0}% cache  ·  {} calls",
                    format_tokens(model.input_tokens),
                    format_tokens(model.output_tokens),
                    cache_ratio,
                    model.call_count,
                ),
                Style::default().fg(theme.text_dim),
            ),
        ]));

        // Blank line separator
        lines.push(Line::from(""));
    }

    // Summary line
    lines.push(Line::from(vec![
        Span::styled("  Total: ", Style::default().fg(theme.text_dim)),
        Span::styled(
            format!("${:.2}", total_cost),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  across {} models",
                state.models.len()
            ),
            Style::default().fg(theme.text_dim),
        ),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn shorten_model(model: &str) -> String {
    model
        .replace("claude-", "")
        .replace("-20241022", "")
        .replace("-20250514", "")
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
