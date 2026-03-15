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
        .constraints([
            Constraint::Min(10),     // chart
            Constraint::Length(5),   // stats
            Constraint::Length(11),  // heatmap (7 days + 2 header + 2 borders)
            Constraint::Length(11),  // contribution calendar
        ])
        .split(area);

    render_chart(f, state, theme, chunks[0]);
    render_stats(f, state, theme, chunks[1]);
    render_heatmap(f, state, theme, chunks[2]);
    render_contribution_calendar(f, state, theme, chunks[3]);
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

fn render_heatmap(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled(" Time-of-Day ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled("Heatmap ", Style::default().fg(theme.text_dim)),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Collect all non-zero values for quartile computation
    let mut all_vals: Vec<f64> = state.heatmap.iter()
        .flat_map(|row| row.iter())
        .copied()
        .filter(|v| *v > 0.0)
        .collect();
    all_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let q1 = percentile(&all_vals, 25);
    let q2 = percentile(&all_vals, 50);
    let q3 = percentile(&all_vals, 75);

    let mut lines = Vec::new();

    // Header row: hours
    let mut header_spans = vec![Span::styled("      ", Style::default().fg(theme.text_dim))];
    for h in (0..24).step_by(2) {
        header_spans.push(Span::styled(
            format!("{:02} ", h),
            Style::default().fg(theme.text_dim),
        ));
    }
    lines.push(Line::from(header_spans));

    // Data rows: one per day of week
    // Reorder so Monday is first: indices 1,2,3,4,5,6,0
    let day_order = [1usize, 2, 3, 4, 5, 6, 0];
    let day_labels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

    for (i, &dow) in day_order.iter().enumerate() {
        let mut spans = vec![Span::styled(
            format!("  {} ", day_labels[i]),
            Style::default().fg(theme.text_dim),
        )];

        for h in (0..24).step_by(2) {
            // Average the two hours in each slot
            let val = (state.heatmap[dow][h] + state.heatmap[dow][(h + 1).min(23)]) / 2.0;
            let block_char = if val <= 0.0 {
                "\u{2591}\u{2591}" // ░░
            } else if val <= q1 {
                "\u{2591}\u{2591}" // ░░
            } else if val <= q2 {
                "\u{2592}\u{2592}" // ▒▒
            } else if val <= q3 {
                "\u{2593}\u{2593}" // ▓▓
            } else {
                "\u{2588}\u{2588}" // ██
            };

            let color = if val <= 0.0 {
                theme.bar_empty
            } else if val <= q1 {
                theme.muted
            } else if val <= q2 {
                theme.secondary
            } else if val <= q3 {
                theme.tertiary
            } else {
                theme.accent
            };

            spans.push(Span::styled(
                format!("{} ", block_char),
                Style::default().fg(color),
            ));
        }
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_contribution_calendar(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled(" Contribution ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled("Calendar (12 weeks) ", Style::default().fg(theme.text_dim)),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.contribution_calendar.is_empty() {
        f.render_widget(
            Paragraph::new("  No data yet").style(Style::default().fg(theme.text_dim)),
            inner,
        );
        return;
    }

    // Build a grid: 7 rows (Mon-Sun) x 12 weeks
    // We need to map dates to (week_col, day_row)
    use chrono::Datelike;

    let today = chrono::Local::now().date_naive();
    let start = today - chrono::Duration::days(83); // 12 weeks = 84 days

    // Build a lookup of date -> cost
    let mut date_costs: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for day in &state.contribution_calendar {
        date_costs.insert(day.date.clone(), day.cost);
    }

    // Build the grid: 7 rows x 12 cols
    let mut grid = vec![vec![0.0f64; 12]; 7];
    for d in 0..84 {
        let date = start + chrono::Duration::days(d);
        let date_str = date.format("%Y-%m-%d").to_string();
        let cost = date_costs.get(&date_str).copied().unwrap_or(0.0);
        let week = d as usize / 7;
        let dow = date.weekday().num_days_from_monday() as usize; // 0=Mon
        if week < 12 && dow < 7 {
            grid[dow][week] = cost;
        }
    }

    // Quartile computation for coloring
    let mut all_vals: Vec<f64> = grid.iter()
        .flat_map(|row| row.iter())
        .copied()
        .filter(|v| *v > 0.0)
        .collect();
    all_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let q1 = percentile(&all_vals, 25);
    let q2 = percentile(&all_vals, 50);
    let q3 = percentile(&all_vals, 75);

    let day_labels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

    let mut lines = Vec::new();

    // Week number header
    let mut header_spans = vec![Span::styled("      ", Style::default().fg(theme.text_dim))];
    for w in 0..12 {
        let week_date = start + chrono::Duration::days((w * 7) as i64);
        if w % 3 == 0 {
            header_spans.push(Span::styled(
                format!("{:<6}", week_date.format("%m/%d")),
                Style::default().fg(theme.text_dim),
            ));
        } else {
            header_spans.push(Span::styled("  ", Style::default().fg(theme.text_dim)));
        }
    }
    lines.push(Line::from(header_spans));

    for (dow, label) in day_labels.iter().enumerate() {
        let mut spans = vec![Span::styled(
            format!("  {} ", label),
            Style::default().fg(theme.text_dim),
        )];

        for week in 0..12 {
            let val = grid[dow][week];
            let (ch, color) = if val <= 0.0 {
                ("\u{2591}", theme.bar_empty)
            } else if val <= q1 {
                ("\u{2591}", theme.muted)
            } else if val <= q2 {
                ("\u{2592}", theme.secondary)
            } else if val <= q3 {
                ("\u{2593}", theme.tertiary)
            } else {
                ("\u{2588}", theme.accent)
            };
            spans.push(Span::styled(
                format!("{} ", ch),
                Style::default().fg(color),
            ));
        }
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Compute percentile from sorted values.
fn percentile(sorted: &[f64], p: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p as f64 / 100.0 * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
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
