use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::theme::Theme;

pub fn render_help(f: &mut Frame, theme: &Theme) {
    let area = centered_rect(60, 80, f.area());

    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from(vec![
            Span::styled("aitop", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(" — btop for AI", Style::default().fg(theme.text_dim)),
        ]),
        Line::from(""),
        // -- Global Navigation --
        Line::from(Span::styled("Navigation", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        shortcut_line("d", "Dashboard view", theme),
        shortcut_line("s", "Sessions view", theme),
        shortcut_line("m", "Models view", theme),
        shortcut_line("t", "Trends view", theme),
        shortcut_line("1-4", "Quick switch view", theme),
        Line::from(""),
        // -- Global Actions --
        Line::from(Span::styled("Actions", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        shortcut_line("r", "Force refresh", theme),
        shortcut_line("p", "Cycle theme", theme),
        shortcut_line("/", "Search / filter", theme),
        shortcut_line("\\", "Toggle split pane", theme),
        shortcut_line("?", "Toggle this help", theme),
        shortcut_line("Esc", "Close overlay / clear", theme),
        shortcut_line("q", "Quit", theme),
        Line::from(""),
        // -- Split Pane --
        Line::from(Span::styled("Split Pane", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        shortcut_line("D/S/M/T", "Set right pane view", theme),
        Line::from(""),
        // -- Sessions View --
        Line::from(Span::styled("Sessions View", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        shortcut_line("↑/↓ j/k", "Navigate rows", theme),
        shortcut_line("Enter", "Drill into detail", theme),
        shortcut_line("c", "Sort by cost", theme),
        shortcut_line("n", "Sort by tokens", theme),
        shortcut_line("p", "Sort by project", theme),
        shortcut_line("u", "Sort by updated", theme),
        shortcut_line("y", "Copy session to clipboard", theme),
        shortcut_line("Y", "Copy all as TSV", theme),
        Line::from(""),
        // -- Detail / Replay --
        Line::from(Span::styled("Detail / Replay", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        shortcut_line("R", "Start replay", theme),
        shortcut_line("Space", "Pause / resume replay", theme),
        shortcut_line("+/-", "Replay speed", theme),
        Line::from(""),
        // -- Trends View --
        Line::from(Span::styled("Trends View", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
        shortcut_line("w", "Last week", theme),
        shortcut_line("o", "Last month", theme),
        shortcut_line("a", "All time", theme),
        shortcut_line("←/→", "Cycle time range", theme),
        shortcut_line("n", "Toggle token overlay", theme),
        shortcut_line("b", "Bar / line chart", theme),
    ];

    let para = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent))
            .title(" Help (? to close) ")
            .title_style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
    );

    f.render_widget(para, area);
}

fn shortcut_line<'a>(key: &'a str, desc: &'a str, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("  {:>10}", key),
            Style::default().fg(theme.tertiary).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {}", desc), Style::default().fg(theme.text)),
    ])
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
