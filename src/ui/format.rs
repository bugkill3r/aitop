use chrono::{DateTime, Utc};

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
