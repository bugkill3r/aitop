use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::theme::Theme;
use crate::app::AppState;

pub fn render_filter(f: &mut Frame, state: &AppState, theme: &Theme) {
    let area = centered_rect(50, 5, f.area());
    f.render_widget(Clear, area);

    let match_count = state.filtered_sessions.len();
    let total_count = state.sessions.len();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            " Filter (Enter to apply, Esc to cancel) ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("  / ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled(
                &state.filter_text,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "\u{2588}",
                Style::default().fg(theme.accent),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  {} / {} sessions match", match_count, total_count),
                Style::default().fg(theme.text_dim),
            ),
        ]),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

fn centered_rect(percent_x: u16, lines: u16, area: Rect) -> Rect {
    let height = lines + 2; // +2 for borders
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
