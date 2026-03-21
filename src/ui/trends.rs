use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph};
use ratatui::Frame;

use super::theme::Theme;
use super::widgets::title::{panel_title, shortcut_title};
use crate::app::{AppState, ChartType, TrendRange};

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

    let chart_label = match state.chart_type {
        ChartType::Line => "line",
        ChartType::Bar => "bar",
    };

    let overlay_label = if state.show_token_overlay { " [tokens ON]" } else { "" };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title({
            let mut t = shortcut_title('T', "rends ", theme).spans;
            t.extend([
                Span::styled(format!("({}, {}) ", range_label, chart_label), Style::default().fg(theme.text_dim)),
                Span::styled(overlay_label, Style::default().fg(theme.secondary)),
                Span::styled("  ", Style::default()),
                Span::styled("w", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
                Span::styled("eek ", Style::default().fg(theme.text_dim)),
                Span::styled("m", Style::default().fg(theme.text_dim)),
                Span::styled("o", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
                Span::styled("nth ", Style::default().fg(theme.text_dim)),
                Span::styled("a", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
                Span::styled("ll ", Style::default().fg(theme.text_dim)),
                Span::styled("b", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
                Span::styled("ar/line ", Style::default().fg(theme.text_dim)),
                Span::styled("n", Style::default().fg(theme.tertiary).add_modifier(Modifier::UNDERLINED)),
                Span::styled("toks ", Style::default().fg(theme.text_dim)),
                Span::styled("\u{2190}\u{2192}", Style::default().fg(theme.tertiary)),
                Span::styled(" cycle ", Style::default().fg(theme.text_dim)),
            ]);
            Line::from(t)
        });

    if state.daily_spend.is_empty() {
        f.render_widget(
            Paragraph::new("  No spend data yet")
                .style(Style::default().fg(theme.text_dim))
                .block(block),
            area,
        );
        return;
    }

    // Build token lookup for token mode
    let token_map: std::collections::HashMap<&str, i64> = state
        .daily_tokens
        .iter()
        .map(|t| (t.date.as_str(), t.total_tokens))
        .collect();

    // Primary values: tokens when toggled, cost otherwise
    let values: Vec<f64> = if state.show_token_overlay {
        state
            .daily_spend
            .iter()
            .map(|d| token_map.get(d.date.as_str()).copied().unwrap_or(0) as f64)
            .collect()
    } else {
        state.daily_spend.iter().map(|d| d.cost).collect()
    };

    let max_val = values.iter().cloned().fold(0.0f64, f64::max);
    let y_max = (max_val * 1.2).max(1.0);

    let y_labels = if state.show_token_overlay {
        vec![
            Span::styled("0", Style::default().fg(theme.text_dim)),
            Span::styled(
                format_tokens_short(y_max as i64),
                Style::default().fg(theme.text_dim),
            ),
        ]
    } else {
        vec![
            Span::styled("$0", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.0}", y_max),
                Style::default().fg(theme.text_dim),
            ),
        ]
    };

    match state.chart_type {
        ChartType::Line => {
            let data_points: Vec<(f64, f64)> = values
                .iter()
                .enumerate()
                .map(|(i, &v)| (i as f64, v))
                .collect();

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

            let dataset_name = if state.show_token_overlay { "Tokens" } else { "Cost ($)" };
            let dataset_color = if state.show_token_overlay { theme.secondary } else { theme.accent };

            let dataset = Dataset::default()
                .name(dataset_name)
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(dataset_color))
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
                        .labels(y_labels),
                );

            f.render_widget(chart, area);
        }
        ChartType::Bar => {
            let inner = block.inner(area);
            f.render_widget(block, area);
            render_bar_chart(f, state, theme, inner, y_max, &values);
        }
    }
}

fn render_bar_chart(
    f: &mut Frame,
    state: &AppState,
    theme: &Theme,
    area: ratatui::layout::Rect,
    y_max: f64,
    values: &[f64],
) {
    use super::widgets::cost_color::cost_color;

    if area.height < 2 || area.width < 4 {
        return;
    }

    let n = values.len();
    if n == 0 {
        return;
    }

    let chart_height = area.height.saturating_sub(2) as usize; // reserve 1 for labels, 1 for y-axis label
    let available_width = area.width.saturating_sub(8) as usize; // reserve left margin for y-axis labels

    // Each bar gets a slot; slot = bar_width + 1 gap
    let slot_width = (available_width / n).max(1);
    let bar_w = slot_width.saturating_sub(1).max(1);

    let y_top_label = if state.show_token_overlay {
        format!(" {:<7}", format_tokens_short(y_max as i64))
    } else {
        format!(" ${:<5.0} ", y_max)
    };

    let y_bottom_label = if state.show_token_overlay { " 0      " } else { " $0     " };

    // Build bar rows from top to bottom
    let mut lines = Vec::new();

    // Y-axis top label
    lines.push(Line::from(vec![
        Span::styled(y_top_label, Style::default().fg(theme.text_dim)),
    ]));

    // Bar rows (top to bottom)
    for row in 0..chart_height {
        let threshold = y_max * (1.0 - (row as f64 / chart_height as f64));
        let mut spans = vec![Span::raw("        ")]; // left margin

        for val in values {
            let ch = if *val >= threshold {
                "\u{2588}".repeat(bar_w) // █
            } else {
                " ".repeat(bar_w)
            };
            let color = if *val >= threshold {
                if state.show_token_overlay {
                    theme.secondary
                } else {
                    cost_color(*val)
                }
            } else {
                theme.bar_empty
            };
            spans.push(Span::styled(ch, Style::default().fg(color)));
            if slot_width > bar_w {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }

    // X-axis labels
    let mut label_spans = vec![Span::styled(y_bottom_label, Style::default().fg(theme.text_dim))];
    for (i, day) in state.daily_spend.iter().enumerate() {
        // Show label for first, last, and every ~7th day
        let label = if i == 0 || i == n - 1 || (n > 14 && i % 7 == 0) {
            // Show just MM/DD
            if day.date.len() >= 10 {
                format!("{:<width$}", &day.date[5..10], width = slot_width)
            } else {
                format!("{:<width$}", &day.date, width = slot_width)
            }
        } else {
            " ".repeat(slot_width)
        };
        label_spans.push(Span::styled(label, Style::default().fg(theme.text_dim)));
    }
    lines.push(Line::from(label_spans));

    f.render_widget(Paragraph::new(lines), area);
}

fn format_tokens_short(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}Mtok", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}Ktok", n as f64 / 1_000.0)
    } else {
        format!("{}tok", n)
    }
}

fn render_heatmap(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title({
            let mut t = panel_title(" Time-of-Day ", theme).spans;
            t.push(Span::styled("Heatmap ", Style::default().fg(theme.text_dim)));
            Line::from(t)
        });

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
            let block_char = if val <= q1 {
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
        .title({
            let mut t = panel_title(" Contribution ", theme).spans;
            t.push(Span::styled("Calendar (12 weeks) ", Style::default().fg(theme.text_dim)));
            Line::from(t)
        });

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

        for val in &grid[dow][..12] {
            let val = *val;
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

/// Prepare token overlay data aligned with cost data by date.
/// Returns chart data points and max token count.
pub fn prepare_token_overlay(
    daily_spend: &[crate::data::aggregator::DailySpend],
    daily_tokens: &[crate::data::aggregator::DailyTokenCount],
) -> (Vec<(f64, f64)>, f64) {
    if daily_spend.is_empty() || daily_tokens.is_empty() {
        return (vec![(0.0, 0.0)], 1.0);
    }

    // Build a lookup from date to token count
    let token_map: std::collections::HashMap<&str, i64> = daily_tokens
        .iter()
        .map(|t| (t.date.as_str(), t.total_tokens))
        .collect();

    let data: Vec<(f64, f64)> = daily_spend
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let tokens = token_map.get(d.date.as_str()).copied().unwrap_or(0);
            (i as f64, tokens as f64)
        })
        .collect();

    let max_tokens = data
        .iter()
        .map(|&(_, t)| t)
        .fold(0.0f64, f64::max)
        .max(1.0);

    (data, max_tokens)
}

fn render_stats(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let days = state.daily_spend.len().max(1);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // Build token lookup for token mode
    let token_map: std::collections::HashMap<&str, i64> = state
        .daily_tokens
        .iter()
        .map(|t| (t.date.as_str(), t.total_tokens))
        .collect();

    let left = if state.show_token_overlay {
        let total_tokens: i64 = state
            .daily_spend
            .iter()
            .map(|d| token_map.get(d.date.as_str()).copied().unwrap_or(0))
            .sum();
        let avg_tokens = total_tokens / days as i64;

        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  Avg/day: ", Style::default().fg(theme.text_dim)),
                Span::styled(
                    format_tokens_short(avg_tokens),
                    Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Total: ", Style::default().fg(theme.text_dim)),
                Span::styled(
                    format_tokens_short(total_tokens),
                    Style::default().fg(theme.text),
                ),
            ]),
        ])
    } else {
        let total: f64 = state.daily_spend.iter().map(|d| d.cost).sum();
        let avg = total / days as f64;
        let projected_monthly = avg * 30.0;

        Paragraph::new(vec![
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
        ])
    };

    let right_lines = if state.show_token_overlay {
        let peak = state
            .daily_spend
            .iter()
            .map(|d| {
                let tokens = token_map.get(d.date.as_str()).copied().unwrap_or(0);
                (d.date.as_str(), tokens)
            })
            .max_by_key(|&(_, t)| t);

        if let Some((date, tokens)) = peak {
            vec![
                Line::from(vec![
                    Span::styled("  Peak day: ", Style::default().fg(theme.text_dim)),
                    Span::styled(
                        format!("{} ({})", date, format_tokens_short(tokens)),
                        Style::default().fg(theme.accent),
                    ),
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
        }
    } else {
        let max_day = state.daily_spend.iter().max_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap());
        if let Some(peak) = max_day {
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
        }
    };

    f.render_widget(left, cols[0]);
    f.render_widget(Paragraph::new(right_lines), cols[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::aggregator::{DailySpend, DailyTokenCount};

    #[test]
    fn test_prepare_token_overlay_empty_spend() {
        let (data, max) = prepare_token_overlay(&[], &[]);
        assert_eq!(data.len(), 1);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn test_prepare_token_overlay_empty_tokens() {
        let spend = vec![DailySpend { date: "2025-01-01".into(), cost: 5.0 }];
        let (data, max) = prepare_token_overlay(&spend, &[]);
        assert_eq!(data.len(), 1);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn test_prepare_token_overlay_aligned() {
        let spend = vec![
            DailySpend { date: "2025-01-01".into(), cost: 5.0 },
            DailySpend { date: "2025-01-02".into(), cost: 10.0 },
        ];
        let tokens = vec![
            DailyTokenCount { date: "2025-01-01".into(), total_tokens: 1000 },
            DailyTokenCount { date: "2025-01-02".into(), total_tokens: 2000 },
        ];
        let (data, max) = prepare_token_overlay(&spend, &tokens);
        assert_eq!(data.len(), 2);
        assert_eq!(data[0], (0.0, 1000.0));
        assert_eq!(data[1], (1.0, 2000.0));
        assert_eq!(max, 2000.0);
    }

    #[test]
    fn test_prepare_token_overlay_missing_date() {
        let spend = vec![
            DailySpend { date: "2025-01-01".into(), cost: 5.0 },
            DailySpend { date: "2025-01-02".into(), cost: 10.0 },
            DailySpend { date: "2025-01-03".into(), cost: 8.0 },
        ];
        let tokens = vec![
            DailyTokenCount { date: "2025-01-01".into(), total_tokens: 1000 },
            // 2025-01-02 missing
            DailyTokenCount { date: "2025-01-03".into(), total_tokens: 3000 },
        ];
        let (data, max) = prepare_token_overlay(&spend, &tokens);
        assert_eq!(data.len(), 3);
        assert_eq!(data[1].1, 0.0); // missing date => 0 tokens
        assert_eq!(max, 3000.0);
    }

    #[test]
    fn test_format_tokens_short_millions() {
        assert_eq!(format_tokens_short(1_500_000), "1.5Mtok");
    }

    #[test]
    fn test_format_tokens_short_thousands() {
        assert_eq!(format_tokens_short(42_000), "42Ktok");
    }

    #[test]
    fn test_format_tokens_short_small() {
        assert_eq!(format_tokens_short(500), "500tok");
    }
}
