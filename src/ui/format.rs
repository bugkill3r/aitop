use chrono::{DateTime, Utc};
use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// Format a token count into a human-readable short string (e.g., "1.5K", "2.3M").
pub fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Shorten a model name for display.
/// Strips "claude-" prefix, date suffixes (-20250514), and "-preview" suffix.
pub fn shorten_model(model: &str) -> String {
    let mut s = model.replace("claude-", "");
    // Strip date suffixes like -20250514 (dash followed by exactly 8 digits)
    if s.len() > 9 {
        let suffix = &s[s.len() - 9..];
        if suffix.starts_with('-') && suffix[1..].chars().all(|c| c.is_ascii_digit()) {
            s = s[..s.len() - 9].to_string();
        }
    }
    // Strip "-preview" suffix (common in Gemini model names)
    if let Some(stripped) = s.strip_suffix("-preview") {
        s = stripped.to_string();
    }
    s
}

/// Format an ISO timestamp into a relative "X ago" string.
pub fn format_relative_time(iso: &str) -> String {
    let Ok(dt) = iso.parse::<DateTime<Utc>>() else {
        return iso.to_string();
    };
    let now = Utc::now();
    let diff = now - dt;

    if diff.num_minutes() < 1 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else {
        format!("{}d ago", diff.num_days())
    }
}

/// Build a braille waveform bar with color based on the item's share.
/// `ratio` = bar fill (0.0–1.0 relative to max item).
/// `share` = this item's percentage of the total (0.0–100.0), determines color.
/// Returns Vec of colored Spans for the bar + empty space.
pub fn braille_bar_spans<'a>(
    ratio: f64,
    width: usize,
    share: f64,
    colors: [Color; 3], // [low, mid, high]
) -> Vec<Span<'a>> {
    let filled_len = (ratio.clamp(0.0, 1.0) * width as f64).round() as usize;
    let remaining = width.saturating_sub(filled_len);

    // Both-column braille — no jitter, all use both dot columns:
    // ⣀ (1 row) → ⣤ (2 rows) → ⣶ (3 rows) → ⣿ (4 rows/full)
    const HEIGHTS: [char; 4] = [
        '\u{28C0}', '\u{28E4}', '\u{28F6}', '\u{28FF}',
    ];
    if filled_len == 0 {
        return vec![Span::raw(" ".repeat(remaining))];
    }

    // How far along the gradient this item reaches based on its share
    // 80% share → gradient goes all the way to red
    // 5% share → gradient stays near green
    let gradient_end = (share / 100.0).clamp(0.0, 1.0).sqrt(); // sqrt for a more visible spread

    // Waveform offsets for organic variation on top of growing base
    const WAVE: [i8; 13] = [0, -1, 0, 1, 0, 0, -1, 1, 0, -1, 0, 1, 0];

    let mut spans = Vec::new();
    for i in 0..filled_len {
        let t = if filled_len > 1 {
            (i as f64 / (filled_len - 1) as f64) * gradient_end
        } else {
            gradient_end
        };

        let color = lerp_color_3(colors, t);

        // Growing base height (thin → thick) with waveform variation
        let progress = if filled_len > 1 { i as f64 / (filled_len - 1) as f64 } else { 1.0 };
        let base = (progress * 3.0) as i8; // grows 0→3
        let h = (base + WAVE[i % WAVE.len()]).clamp(0, 3) as usize;
        let ch = HEIGHTS[h];
        spans.push(Span::styled(
            String::from(ch),
            Style::default().fg(color),
        ));
    }

    if remaining > 0 {
        spans.push(Span::raw(" ".repeat(remaining)));
    }

    spans
}

/// Linearly interpolate across 3 colors. t=0 → colors[0], t=0.5 → colors[1], t=1 → colors[2].
fn lerp_color_3(colors: [Color; 3], t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (from, to, local_t) = if t < 0.5 {
        (colors[0], colors[1], t * 2.0)
    } else {
        (colors[1], colors[2], (t - 0.5) * 2.0)
    };

    if let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (from, to) {
        let r = (r1 as f64 + (r2 as f64 - r1 as f64) * local_t).round() as u8;
        let g = (g1 as f64 + (g2 as f64 - g1 as f64) * local_t).round() as u8;
        let b = (b1 as f64 + (b2 as f64 - b1 as f64) * local_t).round() as u8;
        Color::Rgb(r, g, b)
    } else {
        // Fallback for non-RGB colors: snap to nearest
        if t < 0.33 { colors[0] } else if t < 0.66 { colors[1] } else { colors[2] }
    }
}

/// Truncate a string to `max` characters, appending "\u{2026}" if truncated.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}\u{2026}", &s[..max - 1])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens_small() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn test_format_tokens_thousands() {
        assert_eq!(format_tokens(1_000), "1.0K");
        assert_eq!(format_tokens(1_500), "1.5K");
        assert_eq!(format_tokens(42_000), "42.0K");
    }

    #[test]
    fn test_format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }

    #[test]
    fn test_shorten_model_sonnet() {
        assert_eq!(shorten_model("claude-sonnet-4-6-20250514"), "sonnet-4-6");
    }

    #[test]
    fn test_shorten_model_opus() {
        assert_eq!(shorten_model("claude-opus-4-20250514"), "opus-4");
    }

    #[test]
    fn test_shorten_model_haiku() {
        assert_eq!(shorten_model("claude-3-5-haiku-20241022"), "3-5-haiku");
    }

    #[test]
    fn test_shorten_model_no_date() {
        assert_eq!(shorten_model("claude-sonnet-4"), "sonnet-4");
    }

    #[test]
    fn test_shorten_model_unknown() {
        assert_eq!(shorten_model("gpt-4o"), "gpt-4o");
    }

    #[test]
    fn test_shorten_model_gemini() {
        assert_eq!(shorten_model("gemini-3-pro-preview"), "gemini-3-pro");
        assert_eq!(shorten_model("gemini-2.5-flash"), "gemini-2.5-flash");
    }

    #[test]
    fn test_format_relative_time_invalid() {
        assert_eq!(format_relative_time("not-a-date"), "not-a-date");
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
    }
}
