use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Whether the terminal is wide enough for side-by-side panels.
pub fn is_wide(area: Rect) -> bool {
    area.width >= 100
}

/// Split the main area into: tab bar (3 rows) + content area + status bar (1 row).
pub fn main_layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);
    (chunks[0], chunks[1], chunks[2])
}

/// Dashboard: 2x2 grid + bottom activity feed.
pub fn dashboard_layout(area: Rect, wide: bool) -> DashboardAreas {
    if wide {
        // Wide: top row (2 cols) + mid row (2 cols) + bottom activity
        let vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),  // top metrics row
                Constraint::Min(8),    // mid row
                Constraint::Length(8), // activity feed
            ])
            .split(area);

        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(vert[0]);

        let mid = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(vert[1]);

        DashboardAreas {
            metrics: top[0],
            token_flow: top[1],
            model_breakdown: mid[0],
            sessions: mid[1],
            activity: vert[2],
        }
    } else {
        // Compact: stacked vertically, tuned for 80x24 (20 content rows available)
        // Reduce fixed allocations so model_breakdown (Min) always gets space
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),  // metrics
                Constraint::Length(4),  // token flow (compact sparkline)
                Constraint::Min(3),    // model breakdown (gets remaining)
                Constraint::Length(6), // activity feed
            ])
            .split(area);

        DashboardAreas {
            metrics: chunks[0],
            token_flow: chunks[1],
            model_breakdown: chunks[2],
            sessions: Rect::default(), // hidden in compact
            activity: chunks[3],
        }
    }
}

pub struct DashboardAreas {
    pub metrics: Rect,
    pub token_flow: Rect,
    pub model_breakdown: Rect,
    pub sessions: Rect,
    pub activity: Rect,
}
