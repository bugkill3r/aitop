use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use super::format::{braille_bar_spans, format_relative_time, format_tokens, shorten_model, truncate};
use super::layout::{dashboard_layout, layout_tier, LayoutTier};
use super::theme::Theme;
use super::widgets::cost_color::cost_color;
use super::widgets::title::panel_title;
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
    let eff = &state.efficiency;

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
        .title(panel_title("Burn Rate ", theme));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 3-column layout: hero burn rate | spend breakdown | efficiency
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(13),     // burn rate hero
            Constraint::Percentage(40), // spend grid
            Constraint::Percentage(50), // efficiency (wider — cache values can be long)
        ])
        .split(inner);

    // Column 1: Hero burn rate
    let burn_arrow = if stats.burn_rate_per_hour > 0.0 { " \u{25B2}" } else { "" };
    let hero = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!(" ${:.2}", stats.burn_rate_per_hour),
                Style::default().fg(burn_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(burn_arrow, Style::default().fg(burn_color)),
        ]),
        Line::from(vec![
            Span::styled(" per hour", Style::default().fg(theme.text_dim)),
        ]),
    ];
    f.render_widget(Paragraph::new(hero), cols[0]);

    // Column 2: Spend breakdown (vertically centered)
    let mut spend_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" Today ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.2}", stats.spend_today),
                Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Week  ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.2}", stats.spend_this_week),
                Style::default().fg(theme.text),
            ),
        ]),
    ];

    if let Some(budget) = state.config.weekly_budget {
        let pct = (stats.spend_this_week / budget).min(1.0);
        let gauge_color = if pct < 0.60 {
            theme.success
        } else if pct < 0.85 {
            theme.tertiary
        } else {
            theme.danger
        };
        let bar_total = 10usize;
        let filled = ((pct * bar_total as f64) as usize).min(bar_total);
        let empty = bar_total - filled;
        spend_lines.push(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled("\u{2588}".repeat(filled), Style::default().fg(gauge_color)),
            Span::styled("\u{2591}".repeat(empty), Style::default().fg(theme.bar_empty)),
            Span::styled(
                format!(" {:.0}%", pct * 100.0),
                Style::default().fg(gauge_color),
            ),
        ]));
    } else {
        spend_lines.push(Line::from(vec![
            Span::styled(" Total ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.2}", stats.spend_all_time),
                Style::default().fg(theme.text),
            ),
        ]));
    }

    f.render_widget(Paragraph::new(spend_lines), cols[1]);

    // Column 3: Efficiency stats
    let eff_arrow = if eff.efficiency_change_pct > 0.0 { "\u{2191}" } else if eff.efficiency_change_pct < 0.0 { "\u{2193}" } else { "" };
    let eff_color = if eff.efficiency_change_pct >= 0.0 { theme.success } else { theme.danger };

    let eff_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!(" {:.0} tok/$", eff.tokens_per_dollar),
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
            Span::styled(" Cache ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.2}", eff.cache_savings_today),
                Style::default().fg(theme.success),
            ),
            Span::styled(" today", Style::default().fg(theme.text_dim)),
        ]),
        Line::from(vec![
            Span::styled(" Cache ", Style::default().fg(theme.text_dim)),
            Span::styled(
                format!("${:.2}", eff.cache_savings_alltime),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(" total", Style::default().fg(theme.text_dim)),
        ]),
    ];
    f.render_widget(Paragraph::new(eff_lines), cols[2]);
}

fn render_token_flow(f: &mut Frame, state: &AppState, theme: &Theme, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title({
            let mut t = panel_title("Token Flow ", theme).spans;
            t.extend([
                Span::styled("(last hour) ", Style::default().fg(theme.text_dim)),
                Span::styled("in", Style::default().fg(theme.secondary)),
                Span::styled("/", Style::default().fg(theme.muted)),
                Span::styled("out", Style::default().fg(theme.tertiary)),
            ]);
            Line::from(t)
        });

    if state.token_flow.is_empty() {
        f.render_widget(
            Paragraph::new("  No token flow data")
                .style(Style::default().fg(theme.text_dim))
                .block(block),
            area,
        );
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chart_data = prepare_token_flow_data(&state.token_flow);

    // Stacked area: input on bottom, output on top
    let stacked_max = state.token_flow.iter()
        .map(|p| (p.input_tokens + p.output_tokens) as f64)
        .fold(0.0f64, f64::max)
        .max(1.0);
    let y_max = stacked_max * 1.1;

    let w = inner.width as usize;
    let h = inner.height as usize;
    if w == 0 || h == 0 {
        return;
    }

    let dot_cols = w * 2;
    let dot_rows = h * 4;

    let n = chart_data.input_data.len();
    let interpolate = |data: &[(f64, f64)], x: usize| -> f64 {
        if n <= 1 {
            return data.first().map(|d| d.1).unwrap_or(0.0);
        }
        let fx = x as f64 * (n - 1) as f64 / (dot_cols - 1).max(1) as f64;
        let i = (fx as usize).min(n - 2);
        let t = fx - i as f64;
        data[i].1 * (1.0 - t) + data[i + 1].1 * t
    };

    // Pre-compute stacked heights per character column: (input_share, total_h)
    // input_share is the fraction of total that is input (0.0–1.0)
    let mut col_info: Vec<(f64, usize)> = vec![(0.0, 0); w];
    for (char_col, info) in col_info.iter_mut().enumerate() {
        let mut max_total = 0usize;
        let mut sum_iv = 0.0f64;
        let mut sum_total = 0.0f64;
        for dc in 0..2 {
            let x = char_col * 2 + dc;
            if x >= dot_cols { continue; }
            let iv = interpolate(&chart_data.input_data, x);
            let ov = interpolate(&chart_data.output_data, x);
            let ih = (iv / y_max * dot_rows as f64).round() as usize;
            let oh = (ov / y_max * dot_rows as f64).round() as usize;
            max_total = max_total.max(ih + oh);
            sum_iv += iv;
            sum_total += iv + ov;
        }
        info.0 = if sum_total > 0.0 { sum_iv / sum_total } else { 0.0 };
        info.1 = max_total;
    }

    let dot_bits: [[u16; 4]; 2] = [
        [0x01, 0x02, 0x04, 0x40],
        [0x08, 0x10, 0x20, 0x80],
    ];

    let mut lines = Vec::with_capacity(h);
    for char_row in 0..h {
        let mut spans = Vec::new();
        for (char_col, &(input_share, total_h)) in col_info.iter().enumerate() {
            let mut braille: u16 = 0x2800;
            let mut has_data = false;

            for (dc, col_bits) in dot_bits.iter().enumerate() {
                let x = char_col * 2 + dc;
                if x >= dot_cols { continue; }

                let iv = interpolate(&chart_data.input_data, x);
                let ov = interpolate(&chart_data.output_data, x);
                let th = (iv / y_max * dot_rows as f64).round() as usize
                    + (ov / y_max * dot_rows as f64).round() as usize;

                for (dr, &bit) in col_bits.iter().enumerate() {
                    let from_bottom = dot_rows.saturating_sub(1).saturating_sub(char_row * 4 + dr);
                    if from_bottom < th {
                        braille |= bit;
                        has_data = true;
                    }
                }
            }

            let ch = char::from_u32(braille as u32).unwrap_or(' ');
            let color = if has_data {
                // Color boundary based on actual input share of total
                let cell_bottom = dot_rows.saturating_sub((char_row + 1) * 4);
                let input_boundary = (input_share * total_h as f64).round() as usize;
                let cell_center = cell_bottom + 2;
                let (base_color, region_bot, region_h) = if cell_center < input_boundary {
                    (theme.secondary, 0, input_boundary)
                } else {
                    (theme.tertiary, input_boundary, total_h.saturating_sub(input_boundary))
                };
                // Brightness gradient within region: dim at bottom, bright at top
                let pos = cell_bottom.saturating_sub(region_bot);
                let t = if region_h > 0 { (pos as f64 / region_h as f64).clamp(0.0, 1.0) } else { 1.0 };
                dim_color(base_color, 0.35 + t * 0.65)
            } else {
                theme.bar_empty
            };

            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Scale a color's brightness: t=0 → black, t=1 → full color.
fn dim_color(color: ratatui::style::Color, t: f64) -> ratatui::style::Color {
    if let ratatui::style::Color::Rgb(r, g, b) = color {
        ratatui::style::Color::Rgb(
            (r as f64 * t).round() as u8,
            (g as f64 * t).round() as u8,
            (b as f64 * t).round() as u8,
        )
    } else {
        color
    }
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
        .title(panel_title("Models ", theme));

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
        let gradient = [theme.bar_low, theme.bar_mid, theme.bar_high];
        let total_cost: f64 = state.models.iter().map(|m| m.cost).sum();
        for model in &state.models {
            let short_name = shorten_model(&model.model);
            let ratio = if max_cost > 0.0 { model.cost / max_cost } else { 0.0 };
            let share = if total_cost > 0.0 { model.cost / total_cost * 100.0 } else { 0.0 };

            let mut spans = vec![
                Span::styled(
                    format!("  {:<width$}", short_name, width = name_col),
                    Style::default().fg(theme.text),
                ),
            ];
            spans.extend(braille_bar_spans(ratio, bar_width, share, gradient));
            spans.push(Span::styled(
                format!(" ${:.2}", model.cost),
                Style::default().fg(cost_color(model.cost)).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::from(spans));
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
        .title(panel_title("Projects ", theme));

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
    let gradient = [theme.bar_low, theme.bar_mid, theme.bar_high];
    for pc in state.project_costs.iter().take(inner.height as usize) {
        let ratio = if max_cost > 0.0 { pc.cost / max_cost } else { 0.0 };

        let mut spans = vec![
            Span::styled(
                format!("  {:<14}", truncate(&pc.name, 14)),
                Style::default().fg(theme.text),
            ),
        ];
        spans.extend(braille_bar_spans(ratio, bar_width, pc.percentage, gradient));
        spans.push(Span::styled(
            format!(" ${:<6.0}", pc.cost),
            Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("({:.0}%)", pc.percentage),
            Style::default().fg(theme.text_dim),
        ));
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_active_sessions(f: &mut Frame, state: &AppState, theme: &Theme, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.muted))
        .title(panel_title("Sessions ", theme));

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
        .title(panel_title("Recent Activity ", theme));

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
/// Pads to 60 minutes so gaps without activity show as zero.
pub fn prepare_token_flow_data(
    flow: &[crate::data::aggregator::TokenFlowPoint],
) -> TokenFlowChartData {
    if flow.is_empty() {
        return TokenFlowChartData {
            input_data: vec![(0.0, 0.0); 60],
            output_data: vec![(0.0, 0.0); 60],
            max_value: 1.0,
        };
    }

    // Build a lookup from "HH:MM" → (input, output)
    let mut minute_map: std::collections::HashMap<&str, (i64, i64)> =
        std::collections::HashMap::new();
    for p in flow {
        minute_map.insert(&p.minute, (p.input_tokens, p.output_tokens));
    }

    // Generate 60 minutes ending at now
    let now = chrono::Local::now();
    let mut input_data = Vec::with_capacity(60);
    let mut output_data = Vec::with_capacity(60);
    let mut max_val = 1i64;

    for i in 0..60 {
        let t = now - chrono::Duration::minutes(59 - i);
        let key = t.format("%H:%M").to_string();
        let (inp, out) = minute_map
            .iter()
            .find(|(k, _)| **k == key.as_str())
            .map(|(_, v)| *v)
            .unwrap_or((0, 0));
        input_data.push((i as f64, inp as f64));
        output_data.push((i as f64, out as f64));
        max_val = max_val.max(inp.max(out));
    }

    TokenFlowChartData {
        input_data,
        output_data,
        max_value: max_val as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::aggregator::TokenFlowPoint;

    #[test]
    fn test_prepare_token_flow_empty() {
        let data = prepare_token_flow_data(&[]);
        assert_eq!(data.input_data.len(), 60);
        assert_eq!(data.output_data.len(), 60);
        assert_eq!(data.max_value, 1.0);
    }

    #[test]
    fn test_prepare_token_flow_single_point() {
        let now = chrono::Local::now();
        let minute_key = now.format("%H:%M").to_string();
        let flow = vec![TokenFlowPoint {
            minute: minute_key,
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
        }];
        let data = prepare_token_flow_data(&flow);
        assert_eq!(data.input_data.len(), 60);
        assert_eq!(data.output_data.len(), 60);
        // Last element should have the data (it's the current minute)
        assert_eq!(data.input_data[59].1, 100.0);
        assert_eq!(data.output_data[59].1, 50.0);
        assert_eq!(data.max_value, 100.0);
    }

    #[test]
    fn test_prepare_token_flow_multiple_points() {
        let now = chrono::Local::now();
        let flow = vec![
            TokenFlowPoint {
                minute: (now - chrono::Duration::minutes(2)).format("%H:%M").to_string(),
                input_tokens: 100,
                output_tokens: 200,
                total_tokens: 300,
            },
            TokenFlowPoint {
                minute: (now - chrono::Duration::minutes(1)).format("%H:%M").to_string(),
                input_tokens: 300,
                output_tokens: 100,
                total_tokens: 400,
            },
            TokenFlowPoint {
                minute: now.format("%H:%M").to_string(),
                input_tokens: 50,
                output_tokens: 400,
                total_tokens: 450,
            },
        ];
        let data = prepare_token_flow_data(&flow);
        assert_eq!(data.input_data.len(), 60);
        assert_eq!(data.output_data.len(), 60);
        assert_eq!(data.max_value, 400.0);
        // Last 3 minutes should have data, rest zeros
        assert_eq!(data.output_data[59].1, 400.0);
        assert_eq!(data.input_data[58].1, 300.0);
        assert_eq!(data.input_data[57].1, 100.0);
        // A minute without data should be zero
        assert_eq!(data.input_data[50].1, 0.0);
    }

    #[test]
    fn test_prepare_token_flow_max_never_zero() {
        let now = chrono::Local::now();
        let flow = vec![TokenFlowPoint {
            minute: now.format("%H:%M").to_string(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
        }];
        let data = prepare_token_flow_data(&flow);
        assert!(data.max_value >= 1.0, "max_value should be at least 1.0 to avoid division by zero");
    }
}
