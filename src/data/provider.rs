use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Gemini,
    OpenClaw,
    Amp,
    RooCode,
    Mux,
    KimiCli,
    QwenCli,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provider::Claude => write!(f, "claude"),
            Provider::Gemini => write!(f, "gemini"),
            Provider::OpenClaw => write!(f, "openclaw"),
            Provider::Amp => write!(f, "amp"),
            Provider::RooCode => write!(f, "roocode"),
            Provider::Mux => write!(f, "mux"),
            Provider::KimiCli => write!(f, "kimi"),
            Provider::QwenCli => write!(f, "qwen"),
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
            Provider::Amp => home.join(".local").join("share").join("amp").join("threads"),
            Provider::RooCode => home.join(".roo-code").join("tasks"),
            Provider::Mux => home.join(".mux").join("sessions"),
            Provider::KimiCli => home.join(".kimi").join("sessions"),
            Provider::QwenCli => home.join(".qwen").join("projects"),
        }
    }

    /// Check if a file path belongs to this provider (for watcher routing).
    pub fn path_matches(&self, path: &str) -> bool {
        match self {
            Provider::Claude => path.contains("/.claude/"),
            Provider::Gemini => path.contains("/.gemini/"),
            Provider::OpenClaw => path.contains("/.openclaw/"),
            Provider::Amp => path.contains("/.local/share/amp/"),
            Provider::RooCode => path.contains("/.roo-code/"),
            Provider::Mux => path.contains("/.mux/"),
            Provider::KimiCli => path.contains("/.kimi/"),
            Provider::QwenCli => path.contains("/.qwen/"),
        }
    }

    /// All providers that aitop can scan.
    pub fn all() -> &'static [Provider] {
        &[
            Provider::Claude,
            Provider::Gemini,
            Provider::OpenClaw,
            Provider::Amp,
            Provider::RooCode,
            Provider::Mux,
            Provider::KimiCli,
            Provider::QwenCli,
        ]
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
        assert_eq!(Provider::Amp.to_string(), "amp");
        assert_eq!(Provider::RooCode.to_string(), "roocode");
        assert_eq!(Provider::Mux.to_string(), "mux");
        assert_eq!(Provider::KimiCli.to_string(), "kimi");
        assert_eq!(Provider::QwenCli.to_string(), "qwen");
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

        let amp_dir = Provider::Amp.default_dir();
        assert!(amp_dir.to_string_lossy().contains("amp/threads")
            || amp_dir.to_string_lossy().contains("amp\\threads"));

        let roo_dir = Provider::RooCode.default_dir();
        assert!(roo_dir.to_string_lossy().contains(".roo-code/tasks")
            || roo_dir.to_string_lossy().contains(".roo-code\\tasks"));

        let mux_dir = Provider::Mux.default_dir();
        assert!(mux_dir.to_string_lossy().contains(".mux/sessions")
            || mux_dir.to_string_lossy().contains(".mux\\sessions"));

        let kimi_dir = Provider::KimiCli.default_dir();
        assert!(kimi_dir.to_string_lossy().contains(".kimi/sessions")
            || kimi_dir.to_string_lossy().contains(".kimi\\sessions"));

        let qwen_dir = Provider::QwenCli.default_dir();
        assert!(qwen_dir.to_string_lossy().contains(".qwen/projects")
            || qwen_dir.to_string_lossy().contains(".qwen\\projects"));
    }

    #[test]
    fn test_provider_all() {
        let all = Provider::all();
        assert_eq!(all.len(), 8);
        assert_eq!(all[0], Provider::Claude);
        assert_eq!(all[1], Provider::Gemini);
        assert_eq!(all[2], Provider::OpenClaw);
        assert_eq!(all[3], Provider::Amp);
        assert_eq!(all[4], Provider::RooCode);
        assert_eq!(all[5], Provider::Mux);
        assert_eq!(all[6], Provider::KimiCli);
        assert_eq!(all[7], Provider::QwenCli);
    }

    #[test]
    fn test_path_matches() {
        assert!(Provider::Claude.path_matches("/Users/me/.claude/projects/foo/bar.jsonl"));
        assert!(Provider::Gemini.path_matches("/Users/me/.gemini/tmp/proj/chats/s.json"));
        assert!(Provider::OpenClaw.path_matches("/Users/me/.openclaw/agents/a/sessions/s.jsonl"));
        assert!(Provider::Amp.path_matches("/Users/me/.local/share/amp/threads/abc.json"));
        assert!(Provider::RooCode.path_matches("/Users/me/.roo-code/tasks/t1/ui_messages.json"));
        assert!(Provider::Mux.path_matches("/Users/me/.mux/sessions/ws/session-usage.json"));
        assert!(Provider::KimiCli.path_matches("/Users/me/.kimi/sessions/g/uuid/wire.jsonl"));
        assert!(Provider::QwenCli.path_matches("/Users/me/.qwen/projects/p/chats/c.jsonl"));

        assert!(!Provider::Claude.path_matches("/Users/me/.gemini/tmp/foo.json"));
        assert!(!Provider::Gemini.path_matches("/Users/me/.claude/projects/foo.jsonl"));
    }
}
