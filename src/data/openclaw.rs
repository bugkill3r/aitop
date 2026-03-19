use std::io::BufRead;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use super::parser::{ParsedMessage, ParsedSession};
use super::pricing::PricingRegistry;
use super::scanner::SessionFile;

// ---------------------------------------------------------------------------
// Serde structs for the OpenClaw JSONL format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OpenClawEntry {
    #[serde(rename = "type")]
    entry_type: String,
    id: String,
    timestamp: String,
    #[serde(default)]
    message: Option<OpenClawMessage>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(rename = "modelId")]
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    version: Option<u64>,
    #[serde(rename = "parentId")]
    #[serde(default)]
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawMessage {
    role: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    api: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    usage: Option<OpenClawUsage>,
    #[serde(rename = "stopReason")]
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    content: Option<serde_json::Value>,
    #[serde(default)]
    timestamp: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OpenClawUsage {
    #[serde(default)]
    input: i64,
    #[serde(default)]
    output: i64,
    #[serde(rename = "cacheRead")]
    #[serde(default)]
    cache_read: i64,
    #[serde(rename = "cacheWrite")]
    #[serde(default)]
    cache_write: i64,
    #[serde(rename = "totalTokens")]
    #[serde(default)]
    total_tokens: i64,
    #[serde(default)]
    cost: Option<OpenClawCost>,
}

#[derive(Debug, Deserialize)]
struct OpenClawCost {
    #[serde(default)]
    input: f64,
    #[serde(default)]
    output: f64,
    #[serde(rename = "cacheRead")]
    #[serde(default)]
    cache_read: f64,
    #[serde(rename = "cacheWrite")]
    #[serde(default)]
    cache_write: f64,
    #[serde(default)]
    total: f64,
}

// ---------------------------------------------------------------------------
// Scanner: find OpenClaw session JSONL files
// ---------------------------------------------------------------------------

/// Scans `~/.openclaw/agents/*/sessions/*.jsonl`, skipping `.deleted` files.
pub fn scan_openclaw_sessions(openclaw_dir: &Path) -> Result<Vec<SessionFile>> {
    let mut files = Vec::new();

    if !openclaw_dir.exists() {
        return Ok(files);
    }

    for agent_entry in std::fs::read_dir(openclaw_dir)? {
        let agent_entry = agent_entry?;
        let agent_path = agent_entry.path();

        if !agent_path.is_dir() {
            continue;
        }

        let agent_name = agent_entry
            .file_name()
            .to_string_lossy()
            .to_string();

        let sessions_dir = agent_path.join("sessions");
        if !sessions_dir.is_dir() {
            continue;
        }

        let entries = match std::fs::read_dir(&sessions_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for file_entry in entries.flatten() {
            let file_path = file_entry.path();
            let file_name = file_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("");

            // Skip deleted files and non-jsonl files
            if file_name.contains(".deleted") {
                continue;
            }
            if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            let session_id = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            files.push(SessionFile {
                path: file_path,
                session_id,
                project: agent_name.clone(),
            });
        }
    }

    Ok(files)
}

// ---------------------------------------------------------------------------
// Parser: parse a single OpenClaw JSONL file
// ---------------------------------------------------------------------------

/// Parses an OpenClaw JSONL file line by line, returning a session (if found)
/// and all parsed messages.
pub fn parse_openclaw_file(
    path: &Path,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(Option<ParsedSession>, Vec<ParsedMessage>)> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    let mut session: Option<ParsedSession> = None;
    let mut messages: Vec<ParsedMessage> = Vec::new();
    let mut current_model: Option<String> = None;
    let mut last_timestamp: Option<String> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        let entry: OpenClawEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        match entry.entry_type.as_str() {
            "session" => {
                session = Some(ParsedSession {
                    id: entry.id,
                    project: project.to_string(),
                    started_at: entry.timestamp.clone(),
                    updated_at: entry.timestamp,
                    model: current_model.clone(),
                    version: None,
                    provider: "openclaw".to_string(),
                });
            }
            "model_change" => {
                current_model = entry.model_id.clone();
                // Update session model if we already have a session
                if let Some(ref mut s) = session {
                    s.model = current_model.clone();
                }
            }
            "message" => {
                if let Some(ref msg) = entry.message {
                    last_timestamp = Some(entry.timestamp.clone());

                    match msg.role.as_str() {
                        "user" => {
                            messages.push(ParsedMessage {
                                uuid: entry.id,
                                session_id: session
                                    .as_ref()
                                    .map(|s| s.id.clone())
                                    .unwrap_or_default(),
                                msg_type: "user".to_string(),
                                timestamp: entry.timestamp,
                                model: None,
                                input_tokens: 0,
                                output_tokens: 0,
                                cache_read: 0,
                                cache_creation: 0,
                                cost_usd: 0.0,
                                project: project.to_string(),
                                provider: "openclaw".to_string(),
                            });
                        }
                        "assistant" => {
                            let (input, output, cache_read, cache_write, cost) =
                                if let Some(ref usage) = msg.usage {
                                    let file_cost = usage.cost.as_ref().map(|c| c.total);
                                    let model_name =
                                        msg.model.as_deref().unwrap_or("");
                                    let cost = if let Some(c) = file_cost {
                                        c
                                    } else {
                                        pricing.compute_cost(
                                            model_name,
                                            usage.input,
                                            usage.output,
                                            usage.cache_read,
                                            usage.cache_write,
                                        )
                                    };
                                    (
                                        usage.input,
                                        usage.output,
                                        usage.cache_read,
                                        usage.cache_write,
                                        cost,
                                    )
                                } else {
                                    (0, 0, 0, 0, 0.0)
                                };

                            let model = msg.model.clone().or_else(|| current_model.clone());

                            messages.push(ParsedMessage {
                                uuid: entry.id,
                                session_id: session
                                    .as_ref()
                                    .map(|s| s.id.clone())
                                    .unwrap_or_default(),
                                msg_type: "assistant".to_string(),
                                timestamp: entry.timestamp,
                                model,
                                input_tokens: input,
                                output_tokens: output,
                                cache_read,
                                cache_creation: cache_write,
                                cost_usd: cost,
                                project: project.to_string(),
                                provider: "openclaw".to_string(),
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    // Update session's updated_at to the last message timestamp
    if let Some(ref mut s) = session {
        if let Some(ref ts) = last_timestamp {
            s.updated_at = ts.clone();
        }
    }

    Ok((session, messages))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_openclaw_session_entry() {
        let pricing = PricingRegistry::builtin();
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.jsonl");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, r#"{{"type":"session","version":3,"id":"847bee3d-abcd-1234-5678-abcdef123456","timestamp":"2026-03-13T18:17:38.869Z","cwd":"/home/node/.openclaw/workspace"}}"#).unwrap();

        let (session, messages) = parse_openclaw_file(&file_path, "myagent", &pricing).unwrap();

        let session = session.expect("should have a session");
        assert_eq!(session.id, "847bee3d-abcd-1234-5678-abcdef123456");
        assert_eq!(session.project, "myagent");
        assert_eq!(session.started_at, "2026-03-13T18:17:38.869Z");
        assert_eq!(session.provider, "openclaw");
        assert!(session.version.is_none());
        assert!(messages.is_empty());
    }

    #[test]
    fn test_parse_openclaw_assistant_with_usage() {
        let pricing = PricingRegistry::builtin();
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.jsonl");
        let mut f = std::fs::File::create(&file_path).unwrap();

        // Session line first
        writeln!(f, r#"{{"type":"session","version":3,"id":"sess1","timestamp":"2026-03-13T18:17:38.869Z","cwd":"/tmp"}}"#).unwrap();

        // Assistant message with usage
        writeln!(f, r#"{{"type":"message","id":"fa25a13b","parentId":"b638c0c6","timestamp":"2026-03-13T18:17:41.712Z","message":{{"role":"assistant","content":[],"api":"anthropic-messages","provider":"anthropic","model":"claude-sonnet-4-6","usage":{{"input":3,"output":43,"cacheRead":0,"cacheWrite":20970,"totalTokens":21016,"cost":{{"input":0.000009,"output":0.000645,"cacheRead":0,"cacheWrite":0.0786375,"total":0.0792915}}}},"stopReason":"stop","timestamp":1773425858880}}}}"#).unwrap();

        let (session, messages) = parse_openclaw_file(&file_path, "myagent", &pricing).unwrap();

        assert!(session.is_some());
        assert_eq!(messages.len(), 1);

        let msg = &messages[0];
        assert_eq!(msg.uuid, "fa25a13b");
        assert_eq!(msg.session_id, "sess1");
        assert_eq!(msg.msg_type, "assistant");
        assert_eq!(msg.input_tokens, 3);
        assert_eq!(msg.output_tokens, 43);
        assert_eq!(msg.cache_read, 0);
        assert_eq!(msg.cache_creation, 20970);
        assert_eq!(msg.model, Some("claude-sonnet-4-6".to_string()));
        assert_eq!(msg.provider, "openclaw");
        assert!((msg.cost_usd - 0.0792915).abs() < 0.0001);
    }

    #[test]
    fn test_parse_openclaw_user_message() {
        let pricing = PricingRegistry::builtin();
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.jsonl");
        let mut f = std::fs::File::create(&file_path).unwrap();

        writeln!(f, r#"{{"type":"session","version":3,"id":"sess1","timestamp":"2026-03-13T18:17:38.869Z","cwd":"/tmp"}}"#).unwrap();
        writeln!(f, r#"{{"type":"message","id":"b638c0c6","parentId":"8efcccea","timestamp":"2026-03-13T18:17:38.881Z","message":{{"role":"user","content":[{{"type":"text","text":"hello"}}]}}}}"#).unwrap();

        let (_session, messages) = parse_openclaw_file(&file_path, "myagent", &pricing).unwrap();

        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        assert_eq!(msg.msg_type, "user");
        assert_eq!(msg.input_tokens, 0);
        assert_eq!(msg.output_tokens, 0);
        assert_eq!(msg.cache_read, 0);
        assert_eq!(msg.cache_creation, 0);
        assert_eq!(msg.cost_usd, 0.0);
        assert_eq!(msg.provider, "openclaw");
    }

    #[test]
    fn test_parse_openclaw_model_change() {
        let pricing = PricingRegistry::builtin();
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.jsonl");
        let mut f = std::fs::File::create(&file_path).unwrap();

        writeln!(f, r#"{{"type":"session","version":3,"id":"sess1","timestamp":"2026-03-13T18:17:38.869Z","cwd":"/tmp"}}"#).unwrap();
        writeln!(f, r#"{{"type":"model_change","id":"011fe3d3","parentId":null,"timestamp":"2026-03-13T18:17:38.873Z","provider":"anthropic","modelId":"claude-sonnet-4-6"}}"#).unwrap();

        let (session, _messages) = parse_openclaw_file(&file_path, "myagent", &pricing).unwrap();

        let session = session.expect("should have session");
        assert_eq!(session.model, Some("claude-sonnet-4-6".to_string()));
    }

    #[test]
    fn test_parse_openclaw_uses_file_cost() {
        // The cost from the file (usage.cost.total) should be used directly,
        // NOT recalculated via PricingRegistry.
        let pricing = PricingRegistry::builtin();
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.jsonl");
        let mut f = std::fs::File::create(&file_path).unwrap();

        writeln!(f, r#"{{"type":"session","version":3,"id":"sess1","timestamp":"2026-03-13T18:17:38.869Z","cwd":"/tmp"}}"#).unwrap();

        // Use a known cost that differs from what PricingRegistry would compute.
        // With sonnet pricing: input=1000 => $0.003, output=500 => $0.0075
        // PricingRegistry would give ~$0.0105 total
        // But we set cost.total = 99.99 to prove we use the file cost.
        writeln!(f, r#"{{"type":"message","id":"cost-test","parentId":"p1","timestamp":"2026-03-13T18:18:00.000Z","message":{{"role":"assistant","content":[],"model":"claude-sonnet-4-6","usage":{{"input":1000,"output":500,"cacheRead":0,"cacheWrite":0,"totalTokens":1500,"cost":{{"input":0.003,"output":0.0075,"cacheRead":0,"cacheWrite":0,"total":99.99}}}}}}}}"#).unwrap();

        let (_session, messages) = parse_openclaw_file(&file_path, "myagent", &pricing).unwrap();

        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        // Should use the file's cost.total (99.99), not PricingRegistry calculation
        assert!((msg.cost_usd - 99.99).abs() < 0.001);
    }

    #[test]
    fn test_scan_openclaw_sessions() {
        let dir = TempDir::new().unwrap();

        // Create agent1/sessions/session1.jsonl
        let agent1_sessions = dir.path().join("agent1").join("sessions");
        std::fs::create_dir_all(&agent1_sessions).unwrap();
        std::fs::File::create(agent1_sessions.join("session1.jsonl")).unwrap();

        // Create agent2/sessions/session2.jsonl
        let agent2_sessions = dir.path().join("agent2").join("sessions");
        std::fs::create_dir_all(&agent2_sessions).unwrap();
        std::fs::File::create(agent2_sessions.join("session2.jsonl")).unwrap();

        let files = scan_openclaw_sessions(dir.path()).unwrap();
        assert_eq!(files.len(), 2);

        let projects: Vec<&str> = files.iter().map(|f| f.project.as_str()).collect();
        assert!(projects.contains(&"agent1"));
        assert!(projects.contains(&"agent2"));

        let session_ids: Vec<&str> = files.iter().map(|f| f.session_id.as_str()).collect();
        assert!(session_ids.contains(&"session1"));
        assert!(session_ids.contains(&"session2"));
    }

    #[test]
    fn test_scan_openclaw_skips_deleted() {
        let dir = TempDir::new().unwrap();

        let sessions = dir.path().join("agent1").join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::File::create(sessions.join("active.jsonl")).unwrap();
        std::fs::File::create(sessions.join("old.deleted.jsonl")).unwrap();
        std::fs::File::create(sessions.join("another.jsonl.deleted")).unwrap();

        let files = scan_openclaw_sessions(dir.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].session_id, "active");
        assert_eq!(files[0].project, "agent1");
    }
}
