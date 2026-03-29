use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use super::parser::{ParsedMessage, ParsedSession};
use super::pricing::PricingRegistry;
use super::scanner::SessionFile;

// --- Serde structs for the Mux session-usage.json format ---

#[derive(Debug, Deserialize)]
struct MuxUsage {
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default, rename = "byModel")]
    by_model: HashMap<String, MuxModelUsage>,
}

#[derive(Debug, Deserialize)]
struct MuxModelUsage {
    #[serde(default, rename = "inputTokens")]
    input_tokens: i64,
    #[serde(default, rename = "outputTokens")]
    output_tokens: i64,
    #[serde(default, rename = "cacheReadTokens")]
    cache_read_tokens: i64,
    #[serde(default, rename = "cacheCreationTokens")]
    cache_creation_tokens: i64,
}

// --- Public API ---

/// Scan Mux sessions directory for session-usage.json files.
///
/// Structure: `mux_dir/<workspaceId>/session-usage.json`
pub fn scan_mux_sessions(mux_dir: &Path) -> Result<Vec<SessionFile>> {
    let mut files = Vec::new();

    if !mux_dir.exists() {
        return Ok(files);
    }

    for entry in std::fs::read_dir(mux_dir)? {
        let entry = entry?;
        let ws_path = entry.path();

        if !ws_path.is_dir() {
            continue;
        }

        let usage_file = ws_path.join("session-usage.json");
        if usage_file.exists() {
            let session_id = ws_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            files.push(SessionFile {
                path: usage_file,
                session_id,
                project: "mux".to_string(),
            });
        }
    }

    Ok(files)
}

/// Parse a Mux session-usage.json file.
pub fn parse_mux_file(
    path: &Path,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let content = std::fs::read_to_string(path)?;
    parse_mux_str(&content, project, pricing, path)
}

/// Strip "provider:" prefix from model IDs (e.g. "anthropic:claude-opus-4-6" → "claude-opus-4-6")
fn strip_provider_prefix(model: &str) -> String {
    if let Some(pos) = model.find(':') {
        model[pos + 1..].to_string()
    } else {
        model.to_string()
    }
}

fn parse_mux_str(
    json: &str,
    project: &str,
    pricing: &PricingRegistry,
    path: &Path,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let usage: MuxUsage = serde_json::from_str(json)?;

    let session_id = usage.session_id.clone().unwrap_or_else(|| {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("mux-session")
            .to_string()
    });

    let timestamp = usage.timestamp.clone().unwrap_or_default();
    let mut parsed_messages = Vec::new();
    let mut first_model: Option<String> = None;

    for (raw_model, model_usage) in &usage.by_model {
        let model_name = strip_provider_prefix(raw_model);

        if first_model.is_none() {
            first_model = Some(model_name.clone());
        }

        if model_usage.input_tokens == 0 && model_usage.output_tokens == 0 {
            continue;
        }

        let cost = pricing.compute_cost(
            &model_name,
            model_usage.input_tokens,
            model_usage.output_tokens,
            model_usage.cache_read_tokens,
            model_usage.cache_creation_tokens,
        );

        parsed_messages.push(ParsedMessage {
            uuid: format!("{}-{}", session_id, model_name),
            session_id: session_id.clone(),
            msg_type: "assistant".to_string(),
            timestamp: timestamp.clone(),
            model: Some(model_name),
            input_tokens: model_usage.input_tokens,
            output_tokens: model_usage.output_tokens,
            cache_read: model_usage.cache_read_tokens,
            cache_creation: model_usage.cache_creation_tokens,
            cost_usd: cost,
            project: project.to_string(),
            provider: "mux".to_string(),
        });
    }

    let session = ParsedSession {
        id: session_id,
        project: project.to_string(),
        started_at: timestamp.clone(),
        updated_at: timestamp,
        model: first_model,
        version: None,
        provider: "mux".to_string(),
    };

    Ok((session, parsed_messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn sample_usage_json() -> &'static str {
        r#"{
            "sessionId": "ws-12345",
            "timestamp": "2026-03-10T10:00:00Z",
            "byModel": {
                "anthropic:claude-opus-4-6": {
                    "inputTokens": 10000,
                    "outputTokens": 2000,
                    "cacheReadTokens": 5000,
                    "cacheCreationTokens": 1000
                },
                "anthropic:claude-sonnet-4-6": {
                    "inputTokens": 8000,
                    "outputTokens": 1500,
                    "cacheReadTokens": 3000,
                    "cacheCreationTokens": 500
                }
            }
        }"#
    }

    #[test]
    fn test_parse_mux_file() {
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("ws-12345");
        fs::create_dir_all(&ws_dir).unwrap();
        let file_path = ws_dir.join("session-usage.json");
        fs::write(&file_path, sample_usage_json()).unwrap();

        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_mux_file(&file_path, "mux", &pricing).unwrap();

        assert_eq!(session.id, "ws-12345");
        assert_eq!(session.provider, "mux");
        assert_eq!(session.started_at, "2026-03-10T10:00:00Z");

        assert_eq!(messages.len(), 2);

        // Check provider prefix stripping
        for msg in &messages {
            assert!(!msg.model.as_ref().unwrap().contains(':'));
            assert_eq!(msg.provider, "mux");
            assert!(msg.cost_usd > 0.0);
        }
    }

    #[test]
    fn test_strip_provider_prefix() {
        assert_eq!(strip_provider_prefix("anthropic:claude-opus-4-6"), "claude-opus-4-6");
        assert_eq!(strip_provider_prefix("openai:gpt-4o"), "gpt-4o");
        assert_eq!(strip_provider_prefix("claude-sonnet-4-6"), "claude-sonnet-4-6");
    }

    #[test]
    fn test_parse_mux_empty() {
        let tmp = TempDir::new().unwrap();
        let ws_dir = tmp.path().join("ws-empty");
        fs::create_dir_all(&ws_dir).unwrap();
        let file_path = ws_dir.join("session-usage.json");
        fs::write(&file_path, r#"{"byModel": {}}"#).unwrap();

        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_mux_file(&file_path, "mux", &pricing).unwrap();

        assert_eq!(session.provider, "mux");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_scan_mux_sessions() {
        let tmp = TempDir::new().unwrap();
        let mux_dir = tmp.path();

        let ws1 = mux_dir.join("workspace-1");
        fs::create_dir_all(&ws1).unwrap();
        fs::write(ws1.join("session-usage.json"), "{}").unwrap();

        let ws2 = mux_dir.join("workspace-2");
        fs::create_dir_all(&ws2).unwrap();
        fs::write(ws2.join("session-usage.json"), "{}").unwrap();

        // Workspace without usage file
        let ws3 = mux_dir.join("workspace-3");
        fs::create_dir_all(&ws3).unwrap();

        let files = scan_mux_sessions(mux_dir).unwrap();
        assert_eq!(files.len(), 2);
    }
}
