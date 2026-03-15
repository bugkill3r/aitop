use ratatui::style::Color;

/// Maps a dollar cost amount to a color gradient:
/// - $0.00-0.99 -> dim green
/// - $1.00-4.99 -> yellow
/// - $5.00-9.99 -> orange
/// - $10.00+    -> bright red
pub fn cost_color(amount: f64) -> Color {
    if amount < 1.0 {
        Color::Rgb(80, 200, 80) // dim green
    } else if amount < 5.0 {
        Color::Rgb(230, 220, 50) // yellow
    } else if amount < 10.0 {
        Color::Rgb(255, 150, 50) // orange
    } else {
        Color::Rgb(255, 80, 80) // bright red
    }
}
