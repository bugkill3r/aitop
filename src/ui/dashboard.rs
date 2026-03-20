use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph, Row, Table};
use ratatui::Frame;

use super::format::{format_relative_time, format_tokens, shorten_model, truncate};
use super::layout::{dashboard_layout, layout_tier, LayoutTier};
use super::theme::Theme;
use super::widgets::cost_color::cost_color;
use crate::app::AppState;

pub fn render_dashboard(f: &mut Frame, state: &AppState, theme: &Theme) {
    let tier = layout_tier(f.area());

    // If we have a delta banner, render it above the dashboard
    let content_area = if state.delta_banner.is_some() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(state.content_area);
        render_delta_banner(f, state, theme, chunks[0]);
        chunks[1]
    } else {
        state.content_area
    };

    let areas = dashboard_layout(content_area, tier);

    if areas.metrics.height > 0 && areas.metrics.width > 0 {
        render_metrics(f, state, theme, areas.metrics);
    }
    if areas.token_flow.height > 0 && areas.token_flow.width > 0 {
        render_token_flow(f, state, theme, areas.token_flow);
    }
    if areas.model_breakdown.height > 0 && areas.model_breakdown.width > 0 {
        render_model_breakdown(f, state, theme, areas.model_breakdown);
    }
    if areas.sessions.width > 0 && areas.sessions.height > 0 && tier != LayoutTier::Compact {
        render_active_sessions(f, state, theme, areas.sessions);
    }
    if areas.activity.height > 0 && areas.activity.width > 0 {
        render_activity_feed(f, state, theme, areas.activity);
    }
}

fn render_delta_banner(f: &mut Frame, state: &AppState, theme: &Theme, area: Rect) {
    let banner = match &state.delta_banner {
        Some(b) => b,
        None => return,
    };

    let mut spans = vec![
        Span::styled(
            format!("  +${:.2} spent", banner.spend_delta),
            Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} new session{}", banner.new_sessions, if banner.new_sessions != 1 { "s" } else { "" }),
            Style::default().fg(theme.text),
        ),
    ];

    for mc in &banner.model_changes {
        let arrow = if mc.pct_change > 0.0 { "\u{2191}" } else { "\u{2193}" };
        let color = if mc.pct_change > 0.0 { theme.danger } else { theme.success };
        spans.push(Span::styled("  |  ", Style::default().fg(theme.muted)));
        spans.push(Span::styled(
            format!("{} {}{:.0}%", mc.model, arrow, mc.pct_change.abs()),
            Style::default().fg(color),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(Line::from(vec![
            Span::styled(
                format!(" Since you last checked ({}) ", banner.last_checked_label),
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
        ]));

    let para = Paragraph::new(Line::from(spans)).block(block);
    f.render_widget(para, area);
}

fn render_metrics(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let stats = &state.dashboard;

    let burn_arrow = if stats.burn_rate_per_hour > 0.0 { "\u{25B2}" } else { "\u{00B7}" };
    let burn_color = if stats.burn_rate_per_hour > 10.0 {
        theme.danger
    } else if stats.burn_rate_per_hour > 5.0 {
        theme.tertiary
    } else {
        theme.success
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled("B", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("urn Rate ", Style::default().fg(theme.text)),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // Left: burn rate + efficiency
    let eff = &state.efficiency;
    let eff_arrow = if eff.efficiency_change_pct > 0.0 { "\u{2191}" } else if eff.efficiency_change_pct < 0.0 { "\u{2193}" } else { "" };
    let eff_color = if eff.efficiency_change_pct >= 0.0 { theme.success } else { theme.danger };

    let burn_text = vec![
        Line::from(vec![
            Span::styled(
                format!("${:.2}/hr {}", stats.burn_rate_per_hour, burn_arrow),
                Style::default().fg(burn_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Today ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.2}", stats.spend_today),
                Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{:.0}tok/$", eff.tokens_per_dollar),
                Style::default().fg(theme.secondary),
            ),
            Span::styled(
                if eff.efficiency_change_pct.abs() > 0.1 {
                    format!(" {}{:.0}%", eff_arrow, eff.efficiency_change_pct.abs())
                } else {
                    String::new()
                },
                Style::default().fg(eff_color),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("Cache saved ${:.2}", eff.cache_savings_usd),
                Style::default().fg(theme.text_dim),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(burn_text), cols[0]);

    // Right: week spend + budget gauge
    let mut right_lines = vec![
        Line::from(vec![
            Span::styled("This Week ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.2}", stats.spend_this_week),
                Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    if let Some(budget) = state.config.weekly_budget {
        let pct = (stats.spend_this_week / budget).min(1.0);
        let pct_int = (pct * 100.0) as u16;
        let gauge_color = if pct < 0.60 {
            theme.success
        } else if pct < 0.85 {
            theme.tertiary
        } else {
            theme.danger
        };

        let bar_total = 12usize;
        let filled = ((pct * bar_total as f64) as usize).min(bar_total);
        let empty = bar_total - filled;
        let bar_filled: String = "\u{2588}".repeat(filled);
        let bar_empty: String = "\u{2591}".repeat(empty);

        right_lines.push(Line::from(vec![
            Span::styled("Budget ", Style::default().fg(theme.text_dim)),
            Span::styled(bar_filled, Style::default().fg(gauge_color)),
            Span::styled(bar_empty, Style::default().fg(theme.bar_empty)),
            Span::styled(
                format!(" {}% (${:.0} / ${:.0})", pct_int, stats.spend_this_week, budget),
                Style::default().fg(gauge_color),
            ),
        ]));
    } else {
        right_lines.push(Line::from(vec![
            Span::styled("All-time ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.2}", stats.spend_all_time),
                Style::default().fg(theme.text)),
        ]));
    }

    f.render_widget(Paragraph::new(right_lines), cols[1]);
}

fn render_token_flow(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled(" T", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("oken Flow ", Style::default().fg(theme.text)),
            Span::styled("(last hour) ", Style::default().fg(theme.text_dim)),
            Span::styled("in", Style::default().fg(theme.secondary)),
            Span::styled("/", Style::default().fg(theme.muted)),
            Span::styled("out", Style::default().fg(theme.tertiary)),
        ]));

    if state.token_flow.is_empty() {
        f.render_widget(
            Paragraph::new("  No token flow data")
                .style(Style::default().fg(theme.text_dim))
                .block(block),
            area,
        );
        return;
    }

    let chart_data = prepare_token_flow_data(&state.token_flow);
    let y_max = chart_data.max_value * 1.2;
    let x_max = (state.token_flow.len() as f64 - 1.0).max(1.0);

    let input_dataset = Dataset::default()
        .name("Input")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme.secondary))
        .data(&chart_data.input_data);

    let output_dataset = Dataset::default()
        .name("Output")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme.tertiary))
        .data(&chart_data.output_data);

    let chart = Chart::new(vec![input_dataset, output_dataset])
        .block(block)
        .x_axis(
            Axis::default()
                .bounds([0.0, x_max]),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, y_max]),
        );

    f.render_widget(chart, area);
}

fn render_model_breakdown(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    // Split area: models on top, project costs below
    let has_projects = !state.project_costs.is_empty();
    let sub_areas = if has_projects {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled("M", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("odels ", Style::default().fg(theme.text)),
        ]));

    let inner = block.inner(sub_areas.0);
    f.render_widget(block, sub_areas.0);

    if state.models.is_empty() {
        f.render_widget(
            Paragraph::new("  No data yet").style(Style::default().fg(theme.text_dim)),
            inner,
        );
    } else {
        let max_cost = state.models.iter().map(|m| m.cost).fold(0.0f64, f64::max);
        // Compute model name column width dynamically
        let name_col = state.models.iter()
            .map(|m| shorten_model(&m.model).len())
            .max()
            .unwrap_or(10)
            .max(10);
        // 2 (indent) + name_col + 1 (space) + cost ~12 chars = overhead
        let bar_width = (inner.width as usize).saturating_sub(name_col + 16);

        let mut lines = Vec::new();
        for model in &state.models {
            let short_name = shorten_model(&model.model);
            let bar_len = if max_cost > 0.0 {
                ((model.cost / max_cost) * bar_width as f64) as usize
            } else {
                0
            };
            let bar: String = "\u{2588}".repeat(bar_len);
            let empty: String = "\u{2591}".repeat(bar_width.saturating_sub(bar_len));

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<width$}", short_name, width = name_col),
                    Style::default().fg(theme.text),
                ),
                Span::styled(bar, Style::default().fg(theme.bar_filled)),
                Span::styled(empty, Style::default().fg(theme.bar_empty)),
                Span::styled(
                    format!(" ${:.2}", model.cost),
                    Style::default().fg(cost_color(model.cost)).add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        f.render_widget(Paragraph::new(lines), inner);
    }

    // Project costs section
    if let Some(proj_area) = sub_areas.1 {
        render_project_costs(f, state, theme, proj_area);
    }
}

fn render_project_costs(f: &mut Frame, state: &AppState, theme: &Theme, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled("P", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("rojects ", Style::default().fg(theme.text)),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.project_costs.is_empty() {
        f.render_widget(
            Paragraph::new("  No project data").style(Style::default().fg(theme.text_dim)),
            inner,
        );
        return;
    }

    let max_cost = state.project_costs.iter().map(|p| p.cost).fold(0.0f64, f64::max);
    let bar_width = (inner.width as usize).saturating_sub(34);

    let mut lines = Vec::new();
    for pc in state.project_costs.iter().take(inner.height as usize) {
        let bar_len = if max_cost > 0.0 {
            ((pc.cost / max_cost) * bar_width as f64) as usize
        } else {
            0
        };
        let bar: String = "\u{2588}".repeat(bar_len);
        let empty: String = " ".repeat(bar_width.saturating_sub(bar_len));

        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<14}", truncate(&pc.name, 14)),
                Style::default().fg(theme.text),
            ),
            Span::styled(bar, Style::default().fg(theme.secondary)),
            Span::styled(empty, Style::default()),
            Span::styled(
                format!(" ${:<6.0}", pc.cost),
                Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({:.0}%)", pc.percentage),
                Style::default().fg(theme.text_dim),
            ),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_active_sessions(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled("S", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("essions ", Style::default().fg(theme.text)),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows: Vec<Row> = state
        .sessions
        .iter()
        .take(inner.height as usize)
        .enumerate()
        .map(|(i, s)| {
            let base_fg = if i % 2 == 0 { theme.text } else { theme.text_dim };
            Row::new(vec![
                Line::from(Span::styled(
                    format!("{:<12}", truncate(&s.project, 12)),
                    Style::default().fg(base_fg),
                )),
                Line::from(Span::styled(
                    shorten_model(&s.model),
                    Style::default().fg(base_fg),
                )),
                Line::from(Span::styled(
                    format!("${:.2}", s.total_cost),
                    Style::default().fg(cost_color(s.total_cost)),
                )),
                Line::from(Span::styled(
                    format_relative_time(&s.updated_at),
                    Style::default().fg(base_fg),
                )),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(13),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec!["Project", "Model", "Cost", "When"])
            .style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
    );

    f.render_widget(table, inner);
}

fn render_activity_feed(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(Line::from(vec![
            Span::styled("R", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
            Span::styled("ecent Activity ", Style::default().fg(theme.text)),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows: Vec<Row> = state
        .activity
        .iter()
        .take(inner.height as usize)
        .map(|a| {
            let time = a.timestamp.get(11..16).unwrap_or("??:??");
            let base = Style::default().fg(theme.text_dim);
            Row::new(vec![
                Line::from(Span::styled(time.to_string(), base)),
                Line::from(Span::styled(format!("{:<12}", truncate(&a.project, 12)), base)),
                Line::from(Span::styled(shorten_model(&a.model), base)),
                Line::from(Span::styled(
                    format!("{}in/{}out", format_tokens(a.input_tokens), format_tokens(a.output_tokens)),
                    base,
                )),
                Line::from(Span::styled(format!("{}c", format_tokens(a.cache_read)), base)),
                Line::from(Span::styled(
                    format!("${:.3}", a.cost_usd),
                    Style::default().fg(cost_color(a.cost_usd)),
                )),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(13),
            Constraint::Length(10),
            Constraint::Length(18),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new(vec!["Time", "Project", "Model", "Tokens", "Cache", "Cost"])
            .style(Style::default().fg(theme.accent)),
    );

    f.render_widget(table, inner);
}

/// Prepared dual-line chart data for the token flow widget.
pub struct TokenFlowChartData {
    pub input_data: Vec<(f64, f64)>,
    pub output_data: Vec<(f64, f64)>,
    pub max_value: f64,
}

/// Prepare dual-line chart data from token flow points.
pub fn prepare_token_flow_data(
    flow: &[crate::data::aggregator::TokenFlowPoint],
) -> TokenFlowChartData {
    if flow.is_empty() {
        return TokenFlowChartData {
            input_data: vec![(0.0, 0.0)],
            output_data: vec![(0.0, 0.0)],
            max_value: 1.0,
        };
    }

    let input_data: Vec<(f64, f64)> = flow
        .iter()
        .enumerate()
        .map(|(i, p)| (i as f64, p.input_tokens as f64))
        .collect();

    let output_data: Vec<(f64, f64)> = flow
        .iter()
        .enumerate()
        .map(|(i, p)| (i as f64, p.output_tokens as f64))
        .collect();

    let max_val = flow
        .iter()
        .map(|p| p.input_tokens.max(p.output_tokens))
        .max()
        .unwrap_or(1) as f64;

    TokenFlowChartData {
        input_data,
        output_data,
        max_value: max_val.max(1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::aggregator::TokenFlowPoint;

    #[test]
    fn test_prepare_token_flow_empty() {
        let data = prepare_token_flow_data(&[]);
        assert_eq!(data.input_data.len(), 1);
        assert_eq!(data.output_data.len(), 1);
        assert_eq!(data.max_value, 1.0);
    }

    #[test]
    fn test_prepare_token_flow_single_point() {
        let flow = vec![TokenFlowPoint {
            minute: "12:00".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
        }];
        let data = prepare_token_flow_data(&flow);
        assert_eq!(data.input_data.len(), 1);
        assert_eq!(data.output_data.len(), 1);
        assert_eq!(data.input_data[0], (0.0, 100.0));
        assert_eq!(data.output_data[0], (0.0, 50.0));
        assert_eq!(data.max_value, 100.0);
    }

    #[test]
    fn test_prepare_token_flow_multiple_points() {
        let flow = vec![
            TokenFlowPoint {
                minute: "12:00".to_string(),
                input_tokens: 100,
                output_tokens: 200,
                total_tokens: 300,
            },
            TokenFlowPoint {
                minute: "12:01".to_string(),
                input_tokens: 300,
                output_tokens: 100,
                total_tokens: 400,
            },
            TokenFlowPoint {
                minute: "12:02".to_string(),
                input_tokens: 50,
                output_tokens: 400,
                total_tokens: 450,
            },
        ];
        let data = prepare_token_flow_data(&flow);
        assert_eq!(data.input_data.len(), 3);
        assert_eq!(data.output_data.len(), 3);
        // Max is output_tokens 400 from the 3rd point
        assert_eq!(data.max_value, 400.0);
        // Verify x-axis indices
        assert_eq!(data.input_data[1].0, 1.0);
        assert_eq!(data.output_data[2].0, 2.0);
        // Verify values
        assert_eq!(data.input_data[0].1, 100.0);
        assert_eq!(data.output_data[2].1, 400.0);
    }

    #[test]
    fn test_prepare_token_flow_max_never_zero() {
        let flow = vec![TokenFlowPoint {
            minute: "12:00".to_string(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
        }];
        let data = prepare_token_flow_data(&flow);
        assert!(data.max_value >= 1.0, "max_value should be at least 1.0 to avoid division by zero");
    }
}
