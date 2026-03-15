use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph};
use ratatui::Frame;

use super::theme::Theme;
use crate::app::{AppState, TrendRange};

pub fn render_trends(f: &mut Frame, state: &AppState, theme: &Theme) {
    let area = state.content_area;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(12), Constraint::Length(5)])
        .split(area);

    render_chart(f, state, theme, chunks[0]);
    render_stats(f, state, theme, chunks[1]);
}

fn render_chart(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let range_label = match state.trend_range {
        TrendRange::Week => "7 days",
        TrendRange::Month => "30 days",
        TrendRange::All => "all time",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled(" T", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("rends ", Style::default().fg(theme.text)),
            Span::styled(format!("({}) ", range_label), Style::default().fg(theme.text_dim)),
            Span::styled("  ", Style::default()),
            Span::styled("w", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled("eek ", Style::default().fg(theme.text_dim)),
            Span::styled("m", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled("o", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled("nth ", Style::default().fg(theme.text_dim)),
            Span::styled("a", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
            Span::styled("ll ", Style::default().fg(theme.text_dim)),
            Span::styled("←→", Style::default().fg(theme.tertiary)),
            Span::styled(" cycle ", Style::default().fg(theme.text_dim)),
        ]));

    if state.daily_spend.is_empty() {
        f.render_widget(
            Paragraph::new("  No spend data yet")
                .style(Style::default().fg(theme.text_dim))
                .block(block),
            area,
        );
        return;
    }

    // Prepare chart data
    let data_points: Vec<(f64, f64)> = state
        .daily_spend
        .iter()
        .enumerate()
        .map(|(i, d)| (i as f64, d.cost))
        .collect();

    let max_cost = state
        .daily_spend
        .iter()
        .map(|d| d.cost)
        .fold(0.0f64, f64::max);
    let y_max = (max_cost * 1.2).max(1.0);

    let x_labels: Vec<Span> = if state.daily_spend.len() > 1 {
        vec![
            Span::styled(
                state.daily_spend.first().map(|d| d.date.clone()).unwrap_or_default(),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(
                state.daily_spend.last().map(|d| d.date.clone()).unwrap_or_default(),
                Style::default().fg(theme.text_dim),
            ),
        ]
    } else {
        vec![Span::styled("today", Style::default().fg(theme.text_dim))]
    };

    let dataset = Dataset::default()
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme.accent))
        .data(&data_points);

    let chart = Chart::new(vec![dataset])
        .block(block)
        .x_axis(
            Axis::default()
                .bounds([0.0, (data_points.len() as f64 - 1.0).max(1.0)])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, y_max])
                .labels(vec![
                    Span::styled("$0", Style::default().fg(theme.text_dim)),
                    Span::styled(
                        format!("${:.0}", y_max),
                        Style::default().fg(theme.text_dim),
                    ),
                ]),
        );

    f.render_widget(chart, area);
}

fn render_stats(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let total: f64 = state.daily_spend.iter().map(|d| d.cost).sum();
    let days = state.daily_spend.len().max(1);
    let avg = total / days as f64;
    let projected_monthly = avg * 30.0;

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let left = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  Avg/day: ", Style::default().fg(theme.text_dim)),
            Span::styled(format!("${:.2}", avg), Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Projected/mo: ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.0}", projected_monthly),
                Style::default().fg(if projected_monthly > 200.0 { theme.danger } else { theme.text }),
            ),
        ]),
    ]);

    let max_day = state.daily_spend.iter().max_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap());
    let right_lines = if let Some(peak) = max_day {
        vec![
            Line::from(vec![
                Span::styled("  Peak day: ", Style::default().fg(theme.text_dim)),
                Span::styled(format!("{} (${:.2})", peak.date, peak.cost), Style::default().fg(theme.accent)),
            ]),
            Line::from(vec![
                Span::styled(
                    format!("  {} days tracked", days),
                    Style::default().fg(theme.text_dim),
                ),
            ]),
        ]
    } else {
        vec![Line::from(Span::styled("  No data", Style::default().fg(theme.text_dim)))]
    };

    f.render_widget(left, cols[0]);
    f.render_widget(Paragraph::new(right_lines), cols[1]);
}
