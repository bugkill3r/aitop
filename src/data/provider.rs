use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Gemini,
    OpenClaw,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provider::Claude => write!(f, "claude"),
            Provider::Gemini => write!(f, "gemini"),
            Provider::OpenClaw => write!(f, "openclaw"),
        }
    }
}

impl Provider {
    /// Returns the default data directory for this provider.
    pub fn default_dir(&self) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        match self {
            Provider::Claude => home.join(".claude").join("projects"),
            Provider::Gemini => home.join(".gemini").join("tmp"),
            Provider::OpenClaw => home.join(".openclaw").join("agents"),
        }
    }

    /// All providers that aitop can scan.
    pub fn all() -> &'static [Provider] {
        &[Provider::Claude, Provider::Gemini, Provider::OpenClaw]
    }
}

/// Represents a file belonging to a specific provider.
pub struct ProviderFile {
    pub provider: Provider,
    pub path: PathBuf,
    pub session_id: String,
    pub project: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_display() {
        assert_eq!(Provider::Claude.to_string(), "claude");
        assert_eq!(Provider::Gemini.to_string(), "gemini");
        assert_eq!(Provider::OpenClaw.to_string(), "openclaw");
    }

    #[test]
    fn test_provider_default_dirs() {
        let claude_dir = Provider::Claude.default_dir();
        assert!(claude_dir.to_string_lossy().ends_with(".claude/projects")
            || claude_dir.to_string_lossy().ends_with(".claude\\projects"));

        let gemini_dir = Provider::Gemini.default_dir();
        assert!(gemini_dir.to_string_lossy().ends_with(".gemini/tmp")
            || gemini_dir.to_string_lossy().ends_with(".gemini\\tmp"));

        let openclaw_dir = Provider::OpenClaw.default_dir();
        assert!(openclaw_dir.to_string_lossy().ends_with(".openclaw/agents")
            || openclaw_dir.to_string_lossy().ends_with(".openclaw\\agents"));
    }

    #[test]
    fn test_provider_all() {
        let all = Provider::all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], Provider::Claude);
        assert_eq!(all[1], Provider::Gemini);
        assert_eq!(all[2], Provider::OpenClaw);
    }
}
