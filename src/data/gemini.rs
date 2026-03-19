use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

use super::parser::{ParsedMessage, ParsedSession};
use super::pricing::PricingRegistry;
use super::scanner::SessionFile;

// --- Serde structs for the Gemini JSON format ---

#[derive(Debug, Deserialize)]
struct GeminiSession {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "startTime")]
    start_time: String,
    #[serde(rename = "lastUpdated")]
    last_updated: String,
    #[serde(default)]
    messages: Vec<GeminiMessage>,
}

#[derive(Debug, Deserialize)]
struct GeminiMessage {
    id: String,
    timestamp: String,
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    tokens: Option<GeminiTokens>,
}

#[derive(Debug, Deserialize)]
struct GeminiTokens {
    input: i64,
    output: i64,
    #[serde(default)]
    cached: i64,
    #[serde(default)]
    thoughts: i64,
}

// --- Public API ---

/// Scan the Gemini sessions directory for all session JSON files.
///
/// Directory structure: `gemini_dir/<project>/chats/session-*.json`
/// where `gemini_dir` defaults to `~/.gemini/tmp/`.
pub fn scan_gemini_sessions(gemini_dir: &Path) -> Result<Vec<SessionFile>> {
    let mut files = Vec::new();

    if !gemini_dir.exists() {
        return Ok(files);
    }

    for project_entry in std::fs::read_dir(gemini_dir)? {
        let project_entry = project_entry?;
        let project_path = project_entry.path();

        if !project_path.is_dir() {
            continue;
        }

        let project_name = project_entry
            .file_name()
            .to_string_lossy()
            .to_string();

        let chats_dir = project_path.join("chats");
        if !chats_dir.exists() || !chats_dir.is_dir() {
            continue;
        }

        for file_entry in std::fs::read_dir(&chats_dir)? {
            let file_entry = file_entry?;
            let file_path = file_entry.path();

            if file_path.extension().and_then(|e| e.to_str()) == Some("json") {
                let session_id = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();

                files.push(SessionFile {
                    path: file_path,
                    session_id,
                    project: project_name.clone(),
                });
            }
        }
    }

    Ok(files)
}

/// Parse a single Gemini session JSON file into a ParsedSession and Vec<ParsedMessage>.
///
/// Only messages of type "gemini" with a `tokens` field are included;
/// user/info messages without tokens are skipped.
pub fn parse_gemini_session(
    path: &Path,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let content = std::fs::read_to_string(path)?;
    parse_gemini_session_str(&content, project, pricing)
}

/// Parse Gemini session JSON from a string (used by tests and parse_gemini_session).
fn parse_gemini_session_str(
    json: &str,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let session: GeminiSession = serde_json::from_str(json)?;

    // Determine the model from the first gemini message that has one
    let model = session
        .messages
        .iter()
        .find_map(|m| m.model.clone());

    let parsed_session = ParsedSession {
        id: session.session_id.clone(),
        project: project.to_string(),
        started_at: session.start_time,
        updated_at: session.last_updated,
        model: model.clone(),
        version: None,
        provider: "gemini".to_string(),
    };

    let mut parsed_messages = Vec::new();

    for msg in &session.messages {
        // Only process messages that have token data
        let tokens = match &msg.tokens {
            Some(t) => t,
            None => continue,
        };

        let msg_model = msg.model.as_deref().unwrap_or("");
        let input_tokens = tokens.input;
        let output_tokens = tokens.output + tokens.thoughts;
        let cache_read = tokens.cached;
        let cache_creation = 0;

        let cost = pricing.compute_cost(
            msg_model,
            input_tokens,
            output_tokens,
            cache_read,
            cache_creation,
        );

        parsed_messages.push(ParsedMessage {
            uuid: msg.id.clone(),
            session_id: session.session_id.clone(),
            msg_type: msg.msg_type.clone(),
            timestamp: msg.timestamp.clone(),
            model: msg.model.clone(),
            input_tokens,
            output_tokens,
            cache_read,
            cache_creation,
            cost_usd: cost,
            project: project.to_string(),
            provider: "gemini".to_string(),
        });
    }

    Ok((parsed_session, parsed_messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn minimal_session_json() -> String {
        r#"{
            "sessionId": "a3bf4d75-85d9-4a7b-8852-d6e9d46818f1",
            "projectHash": "40f7f8b30cc6b7939a5f9f9086cc2906b089baa43d79b5acb44ab22aede631df",
            "startTime": "2026-02-24T14:28:39.443Z",
            "lastUpdated": "2026-02-24T14:42:30.767Z",
            "messages": [
                {
                    "id": "eacc6a7f-0001",
                    "timestamp": "2026-02-24T14:28:39.443Z",
                    "type": "user",
                    "content": [{"text": "Hello"}]
                },
                {
                    "id": "924f1a78-0002",
                    "timestamp": "2026-02-24T14:30:38.291Z",
                    "type": "gemini",
                    "content": "Hi there!",
                    "model": "gemini-2.5-pro",
                    "tokens": {
                        "input": 5092,
                        "output": 94,
                        "cached": 2663,
                        "thoughts": 494,
                        "tool": 0,
                        "total": 5680
                    }
                }
            ]
        }"#
        .to_string()
    }

    #[test]
    fn test_parse_gemini_session() {
        let pricing = PricingRegistry::builtin();
        let json = minimal_session_json();
        let (session, messages) =
            parse_gemini_session_str(&json, "myproject", &pricing).unwrap();

        // Session fields
        assert_eq!(session.id, "a3bf4d75-85d9-4a7b-8852-d6e9d46818f1");
        assert_eq!(session.project, "myproject");
        assert_eq!(session.started_at, "2026-02-24T14:28:39.443Z");
        assert_eq!(session.updated_at, "2026-02-24T14:42:30.767Z");
        assert_eq!(session.model.as_deref(), Some("gemini-2.5-pro"));
        assert!(session.version.is_none());
        assert_eq!(session.provider, "gemini");

        // Only the gemini message with tokens should be parsed (user message is skipped)
        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        assert_eq!(msg.uuid, "924f1a78-0002");
        assert_eq!(msg.session_id, "a3bf4d75-85d9-4a7b-8852-d6e9d46818f1");
        assert_eq!(msg.msg_type, "gemini");
        assert_eq!(msg.timestamp, "2026-02-24T14:30:38.291Z");
        assert_eq!(msg.model.as_deref(), Some("gemini-2.5-pro"));
        assert_eq!(msg.input_tokens, 5092);
        assert_eq!(msg.output_tokens, 94 + 494); // output + thoughts
        assert_eq!(msg.cache_read, 2663);
        assert_eq!(msg.cache_creation, 0);
        assert_eq!(msg.provider, "gemini");
        assert!(msg.cost_usd > 0.0);
    }

    #[test]
    fn test_parse_gemini_no_tokens() {
        let pricing = PricingRegistry::builtin();
        let json = r#"{
            "sessionId": "sess-1",
            "projectHash": "abc",
            "startTime": "2026-02-24T14:28:39.443Z",
            "lastUpdated": "2026-02-24T14:42:30.767Z",
            "messages": [
                {
                    "id": "msg-1",
                    "timestamp": "2026-02-24T14:28:39.443Z",
                    "type": "user",
                    "content": [{"text": "Hello"}]
                },
                {
                    "id": "msg-2",
                    "timestamp": "2026-02-24T14:29:00.000Z",
                    "type": "info",
                    "content": "System info message"
                }
            ]
        }"#;

        let (session, messages) =
            parse_gemini_session_str(json, "testproj", &pricing).unwrap();

        assert_eq!(session.id, "sess-1");
        assert_eq!(session.provider, "gemini");
        // No messages have tokens, so all are skipped
        assert!(messages.is_empty());
    }

    #[test]
    fn test_parse_gemini_empty_messages() {
        let pricing = PricingRegistry::builtin();
        let json = r#"{
            "sessionId": "sess-empty",
            "projectHash": "abc",
            "startTime": "2026-01-01T00:00:00Z",
            "lastUpdated": "2026-01-01T00:00:00Z",
            "messages": []
        }"#;

        let (session, messages) =
            parse_gemini_session_str(json, "emptyproj", &pricing).unwrap();

        assert_eq!(session.id, "sess-empty");
        assert_eq!(session.project, "emptyproj");
        assert_eq!(session.provider, "gemini");
        assert!(session.model.is_none());
        assert!(messages.is_empty());
    }

    #[test]
    fn test_parse_gemini_cost_computation() {
        let pricing = PricingRegistry::builtin();
        let json = r#"{
            "sessionId": "cost-test",
            "projectHash": "abc",
            "startTime": "2026-01-01T00:00:00Z",
            "lastUpdated": "2026-01-01T00:01:00Z",
            "messages": [
                {
                    "id": "m1",
                    "timestamp": "2026-01-01T00:00:30Z",
                    "type": "gemini",
                    "model": "gemini-2.5-pro",
                    "content": "response",
                    "tokens": {
                        "input": 1000000,
                        "output": 0,
                        "cached": 0,
                        "thoughts": 0
                    }
                }
            ]
        }"#;

        let (_, messages) =
            parse_gemini_session_str(json, "costproj", &pricing).unwrap();

        assert_eq!(messages.len(), 1);
        let msg = &messages[0];

        // gemini-2.5-pro: input rate = 1.25 per million tokens
        // 1,000,000 input tokens * 1.25 / 1,000,000 = 1.25
        let expected_cost = 1.25;
        assert!(
            (msg.cost_usd - expected_cost).abs() < 0.001,
            "Expected cost ~{}, got {}",
            expected_cost,
            msg.cost_usd
        );
    }

    #[test]
    fn test_scan_gemini_sessions() {
        let tmp = TempDir::new().unwrap();
        let gemini_dir = tmp.path();

        // Create project dirs with chats subdirs
        let proj1_chats = gemini_dir.join("project-alpha").join("chats");
        let proj2_chats = gemini_dir.join("project-beta").join("chats");
        fs::create_dir_all(&proj1_chats).unwrap();
        fs::create_dir_all(&proj2_chats).unwrap();

        // Write session files
        fs::write(
            proj1_chats.join("session-001.json"),
            minimal_session_json(),
        )
        .unwrap();
        fs::write(
            proj1_chats.join("session-002.json"),
            minimal_session_json(),
        )
        .unwrap();
        fs::write(
            proj2_chats.join("session-003.json"),
            minimal_session_json(),
        )
        .unwrap();

        // Write a non-json file that should be ignored
        fs::write(proj1_chats.join("notes.txt"), "not a session").unwrap();

        let files = scan_gemini_sessions(gemini_dir).unwrap();

        assert_eq!(files.len(), 3);

        // Check that project names are directory names
        let projects: Vec<&str> = files.iter().map(|f| f.project.as_str()).collect();
        assert!(projects.contains(&"project-alpha"));
        assert!(projects.contains(&"project-beta"));

        // Check session IDs are file stems
        let mut session_ids: Vec<&str> =
            files.iter().map(|f| f.session_id.as_str()).collect();
        session_ids.sort();
        assert_eq!(session_ids, vec!["session-001", "session-002", "session-003"]);
    }
}
