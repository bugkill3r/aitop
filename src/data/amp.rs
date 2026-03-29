use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

use super::parser::{ParsedMessage, ParsedSession};
use super::pricing::PricingRegistry;
use super::scanner::SessionFile;

// --- Serde structs for the Amp JSON format ---

#[derive(Debug, Deserialize)]
struct AmpThread {
    #[serde(default)]
    metadata: Option<AmpMetadata>,
    #[serde(default, rename = "usageLedger")]
    usage_ledger: Option<AmpUsageLedger>,
    #[serde(default)]
    messages: Option<Vec<AmpMessage>>,
}

#[derive(Debug, Deserialize)]
struct AmpMetadata {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "createdAt")]
    created_at: Option<String>,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AmpUsageLedger {
    #[serde(default)]
    events: Vec<AmpUsageEvent>,
}

#[derive(Debug, Deserialize)]
struct AmpUsageEvent {
    #[serde(default)]
    model: Option<String>,
    #[serde(default, rename = "inputTokens")]
    input_tokens: i64,
    #[serde(default, rename = "outputTokens")]
    output_tokens: i64,
    #[serde(default, rename = "cacheReadTokens")]
    cache_read_tokens: i64,
    #[serde(default, rename = "cacheCreationTokens")]
    cache_creation_tokens: i64,
    #[serde(default)]
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AmpMessage {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<AmpMessageUsage>,
    #[serde(default)]
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AmpMessageUsage {
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

/// Scan Amp threads directory for all session JSON files.
///
/// Directory structure: `amp_dir/<thread_id>.json` or `amp_dir/<thread_id>/thread.json`
pub fn scan_amp_sessions(amp_dir: &Path) -> Result<Vec<SessionFile>> {
    let mut files = Vec::new();

    if !amp_dir.exists() {
        return Ok(files);
    }

    for entry in std::fs::read_dir(amp_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            files.push(SessionFile {
                path,
                session_id,
                project: "amp".to_string(),
            });
        } else if path.is_dir() {
            let thread_file = path.join("thread.json");
            if thread_file.exists() {
                let session_id = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                files.push(SessionFile {
                    path: thread_file,
                    session_id,
                    project: "amp".to_string(),
                });
            }
        }
    }

    Ok(files)
}

/// Parse a single Amp thread JSON file.
pub fn parse_amp_file(
    path: &Path,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let content = std::fs::read_to_string(path)?;
    parse_amp_str(&content, project, pricing)
}

fn parse_amp_str(
    json: &str,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let thread: AmpThread = serde_json::from_str(json)?;

    let session_id = thread
        .metadata
        .as_ref()
        .and_then(|m| m.id.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let started_at = thread
        .metadata
        .as_ref()
        .and_then(|m| m.created_at.clone())
        .unwrap_or_default();

    let updated_at = thread
        .metadata
        .as_ref()
        .and_then(|m| m.updated_at.clone())
        .unwrap_or_else(|| started_at.clone());

    let mut parsed_messages = Vec::new();
    let mut model: Option<String> = None;

    // Prefer usageLedger.events (more structured)
    if let Some(ref ledger) = thread.usage_ledger {
        for (i, event) in ledger.events.iter().enumerate() {
            if model.is_none() {
                model = event.model.clone();
            }
            let msg_model = event.model.as_deref().unwrap_or("");
            let cost = pricing.compute_cost(
                msg_model,
                event.input_tokens,
                event.output_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
            );

            if event.input_tokens == 0 && event.output_tokens == 0 {
                continue;
            }

            parsed_messages.push(ParsedMessage {
                uuid: format!("{}-usage-{}", session_id, i),
                session_id: session_id.clone(),
                msg_type: "assistant".to_string(),
                timestamp: event.timestamp.clone().unwrap_or_else(|| started_at.clone()),
                model: event.model.clone(),
                input_tokens: event.input_tokens,
                output_tokens: event.output_tokens,
                cache_read: event.cache_read_tokens,
                cache_creation: event.cache_creation_tokens,
                cost_usd: cost,
                project: project.to_string(),
                provider: "amp".to_string(),
            });
        }
    }

    // Fallback to messages[].usage if no ledger events produced results
    if parsed_messages.is_empty() {
        if let Some(ref messages) = thread.messages {
            for (i, msg) in messages.iter().enumerate() {
                if let Some(ref usage) = msg.usage {
                    let msg_model = msg.model.as_deref().unwrap_or("");
                    if model.is_none() {
                        model = msg.model.clone();
                    }
                    let cost = pricing.compute_cost(
                        msg_model,
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.cache_read_tokens,
                        usage.cache_creation_tokens,
                    );

                    if usage.input_tokens == 0 && usage.output_tokens == 0 {
                        continue;
                    }

                    parsed_messages.push(ParsedMessage {
                        uuid: format!("{}-msg-{}", session_id, i),
                        session_id: session_id.clone(),
                        msg_type: msg.role.as_deref().unwrap_or("assistant").to_string(),
                        timestamp: msg.timestamp.clone().unwrap_or_else(|| started_at.clone()),
                        model: msg.model.clone(),
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_read: usage.cache_read_tokens,
                        cache_creation: usage.cache_creation_tokens,
                        cost_usd: cost,
                        project: project.to_string(),
                        provider: "amp".to_string(),
                    });
                }
            }
        }
    }

    let session = ParsedSession {
        id: session_id,
        project: project.to_string(),
        started_at,
        updated_at,
        model,
        version: None,
        provider: "amp".to_string(),
    };

    Ok((session, parsed_messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn sample_ledger_json() -> &'static str {
        r#"{
            "metadata": {
                "id": "thread-001",
                "createdAt": "2026-03-10T10:00:00Z",
                "updatedAt": "2026-03-10T10:30:00Z",
                "title": "Test thread"
            },
            "usageLedger": {
                "events": [
                    {
                        "model": "claude-sonnet-4-6",
                        "inputTokens": 5000,
                        "outputTokens": 1000,
                        "cacheReadTokens": 2000,
                        "cacheCreationTokens": 500,
                        "timestamp": "2026-03-10T10:05:00Z"
                    },
                    {
                        "model": "claude-sonnet-4-6",
                        "inputTokens": 3000,
                        "outputTokens": 800,
                        "cacheReadTokens": 1000,
                        "cacheCreationTokens": 0,
                        "timestamp": "2026-03-10T10:10:00Z"
                    }
                ]
            }
        }"#
    }

    fn sample_messages_json() -> &'static str {
        r#"{
            "metadata": {
                "id": "thread-002",
                "createdAt": "2026-03-10T11:00:00Z",
                "updatedAt": "2026-03-10T11:15:00Z"
            },
            "messages": [
                {
                    "role": "user",
                    "timestamp": "2026-03-10T11:00:00Z"
                },
                {
                    "role": "assistant",
                    "model": "claude-opus-4-6",
                    "usage": {
                        "inputTokens": 10000,
                        "outputTokens": 2000,
                        "cacheReadTokens": 5000,
                        "cacheCreationTokens": 1000
                    },
                    "timestamp": "2026-03-10T11:05:00Z"
                }
            ]
        }"#
    }

    #[test]
    fn test_parse_amp_ledger() {
        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_amp_str(sample_ledger_json(), "myproject", &pricing).unwrap();

        assert_eq!(session.id, "thread-001");
        assert_eq!(session.project, "myproject");
        assert_eq!(session.provider, "amp");
        assert_eq!(session.model, Some("claude-sonnet-4-6".to_string()));
        assert_eq!(session.started_at, "2026-03-10T10:00:00Z");
        assert_eq!(session.updated_at, "2026-03-10T10:30:00Z");

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].input_tokens, 5000);
        assert_eq!(messages[0].output_tokens, 1000);
        assert_eq!(messages[0].cache_read, 2000);
        assert_eq!(messages[0].cache_creation, 500);
        assert_eq!(messages[0].provider, "amp");
        assert!(messages[0].cost_usd > 0.0);

        assert_eq!(messages[1].input_tokens, 3000);
        assert_eq!(messages[1].output_tokens, 800);
    }

    #[test]
    fn test_parse_amp_messages_fallback() {
        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_amp_str(sample_messages_json(), "myproject", &pricing).unwrap();

        assert_eq!(session.id, "thread-002");
        assert_eq!(session.provider, "amp");
        assert_eq!(session.model, Some("claude-opus-4-6".to_string()));

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].input_tokens, 10000);
        assert_eq!(messages[0].output_tokens, 2000);
        assert_eq!(messages[0].cache_read, 5000);
        assert_eq!(messages[0].cache_creation, 1000);
        assert_eq!(messages[0].model, Some("claude-opus-4-6".to_string()));
        assert!(messages[0].cost_usd > 0.0);
    }

    #[test]
    fn test_parse_amp_empty_session() {
        let pricing = PricingRegistry::builtin();
        let json = r#"{"metadata": {"id": "empty-thread", "createdAt": "2026-01-01T00:00:00Z"}}"#;
        let (session, messages) = parse_amp_str(json, "proj", &pricing).unwrap();

        assert_eq!(session.id, "empty-thread");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_scan_amp_sessions() {
        let tmp = TempDir::new().unwrap();
        let amp_dir = tmp.path();

        fs::write(
            amp_dir.join("thread-abc.json"),
            sample_ledger_json(),
        ).unwrap();

        let subdir = amp_dir.join("thread-def");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("thread.json"), sample_messages_json()).unwrap();

        let files = scan_amp_sessions(amp_dir).unwrap();
        assert_eq!(files.len(), 2);

        let ids: Vec<&str> = files.iter().map(|f| f.session_id.as_str()).collect();
        assert!(ids.contains(&"thread-abc"));
        assert!(ids.contains(&"thread-def"));
    }
}
