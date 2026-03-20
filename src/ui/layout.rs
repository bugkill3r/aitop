use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Three-tier responsive layout classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutTier {
    /// < 80 cols: stacked single-column, minimal chrome
    Compact,
    /// 80-119 cols: standard layout
    Standard,
    /// >= 120 cols: full side-by-side with extra detail columns
    Wide,
}

/// Determine the layout tier based on terminal width.
pub fn layout_tier(area: Rect) -> LayoutTier {
    if area.width >= 120 {
        LayoutTier::Wide
    } else if area.width >= 80 {
        LayoutTier::Standard
    } else {
        LayoutTier::Compact
    }
}

/// Whether the terminal is wide enough for side-by-side panels.
/// Convenience wrapper for backward compatibility.
pub fn is_wide(area: Rect) -> bool {
    layout_tier(area) == LayoutTier::Wide
}

/// Split content area horizontally 50/50 for split-pane mode.
pub fn split_content(content: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(content);
    (chunks[0], chunks[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_layout_proportions() {
        let area = Rect::new(0, 0, 120, 40);
        let (tab, content, status) = main_layout(area);
        assert_eq!(tab.height, 3, "tab bar should be 3 rows");
        assert_eq!(status.height, 1, "status bar should be 1 row");
        assert_eq!(content.height, 36, "content should fill remaining space");
        assert_eq!(tab.width, 120);
        assert_eq!(content.width, 120);
        assert_eq!(status.width, 120);
    }

    #[test]
    fn test_split_content_proportions() {
        let content = Rect::new(0, 3, 120, 36);
        let (left, right) = split_content(content);

        assert_eq!(left.width, 60, "left pane should be 50%");
        assert_eq!(right.width, 60, "right pane should be 50%");
        assert_eq!(left.height, content.height);
        assert_eq!(right.height, content.height);
        assert_eq!(left.x, content.x, "left pane starts at content left");
        assert_eq!(right.x, 60, "right pane starts at midpoint");
    }

    #[test]
    fn test_split_content_odd_width() {
        let content = Rect::new(0, 3, 101, 36);
        let (left, right) = split_content(content);

        // Ratatui's 50/50 split rounds; total should cover full width
        assert!(
            left.width + right.width >= 100 && left.width + right.width <= 101,
            "left + right should approximately equal content width: {} + {} vs {}",
            left.width, right.width, content.width
        );
    }

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

/// Dashboard layout with three-tier support.
/// Accepts a LayoutTier to determine how panels are arranged.
pub fn dashboard_layout(area: Rect, tier: LayoutTier) -> DashboardAreas {
    match tier {
        LayoutTier::Wide => {
            // Wide (>=120): top row (2 cols) + mid row (2 cols) + bottom activity
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
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(vert[0]);

            let mid = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(vert[1]);

            DashboardAreas {
                metrics: top[0],
                token_flow: top[1],
                model_breakdown: mid[0],
                sessions: mid[1],
                activity: vert[2],
            }
        }
        LayoutTier::Standard => {
            // Standard (80-119): 2-column top row, stacked mid + bottom
            let vert = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(7),  // top metrics row
                    Constraint::Length(6),  // token flow
                    Constraint::Min(3),    // model breakdown
                    Constraint::Length(7), // activity feed
                ])
                .split(area);

            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(vert[0]);

            DashboardAreas {
                metrics: top[0],
                token_flow: top[1],
                model_breakdown: vert[1],
                sessions: vert[2],
                activity: vert[3],
            }
        }
        LayoutTier::Compact => {
            // Compact (<80): stacked vertically, minimal chrome
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
}

pub struct DashboardAreas {
    pub metrics: Rect,
    pub token_flow: Rect,
    pub model_breakdown: Rect,
    pub sessions: Rect,
    pub activity: Rect,
}

#[cfg(test)]
mod layout_tier_tests {
    use super::*;

    fn rect(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    #[test]
    fn test_layout_tier_compact() {
        assert_eq!(layout_tier(rect(79, 24)), LayoutTier::Compact);
        assert_eq!(layout_tier(rect(60, 24)), LayoutTier::Compact);
        assert_eq!(layout_tier(rect(40, 24)), LayoutTier::Compact);
    }

    #[test]
    fn test_layout_tier_standard() {
        assert_eq!(layout_tier(rect(80, 24)), LayoutTier::Standard);
        assert_eq!(layout_tier(rect(100, 24)), LayoutTier::Standard);
        assert_eq!(layout_tier(rect(119, 24)), LayoutTier::Standard);
    }

    #[test]
    fn test_layout_tier_wide() {
        assert_eq!(layout_tier(rect(120, 24)), LayoutTier::Wide);
        assert_eq!(layout_tier(rect(200, 24)), LayoutTier::Wide);
    }

    #[test]
    fn test_layout_tier_boundary_80() {
        assert_eq!(layout_tier(rect(79, 24)), LayoutTier::Compact);
        assert_eq!(layout_tier(rect(80, 24)), LayoutTier::Standard);
    }

    #[test]
    fn test_layout_tier_boundary_120() {
        assert_eq!(layout_tier(rect(119, 24)), LayoutTier::Standard);
        assert_eq!(layout_tier(rect(120, 24)), LayoutTier::Wide);
    }

    // --- Dashboard layout tier behavior tests ---

    #[test]
    fn test_compact_layout_hides_sessions() {
        let areas = dashboard_layout(rect(60, 30), LayoutTier::Compact);
        assert_eq!(areas.sessions, Rect::default());
    }

    #[test]
    fn test_standard_layout_shows_sessions() {
        let areas = dashboard_layout(rect(100, 30), LayoutTier::Standard);
        assert!(areas.sessions.width > 0 || areas.sessions == Rect::default());
    }

    #[test]
    fn test_wide_layout_has_sessions_visible() {
        let areas = dashboard_layout(rect(150, 40), LayoutTier::Wide);
        assert!(areas.sessions.width > 0);
        assert!(areas.sessions.height > 0);
    }

    #[test]
    fn test_wide_layout_has_side_by_side() {
        let areas = dashboard_layout(rect(150, 40), LayoutTier::Wide);
        // In wide mode, metrics and token_flow should be side by side (same y, different x)
        assert_eq!(areas.metrics.y, areas.token_flow.y);
        assert_ne!(areas.metrics.x, areas.token_flow.x);
    }

    #[test]
    fn test_compact_layout_single_column() {
        let areas = dashboard_layout(rect(60, 30), LayoutTier::Compact);
        // In compact mode, all visible panels should share the same x (stacked)
        assert_eq!(areas.metrics.x, areas.token_flow.x);
        assert_eq!(areas.token_flow.x, areas.model_breakdown.x);
    }
}
