use serde::Deserialize;

use super::pricing::PricingRegistry;

#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub uuid: String,
    pub session_id: String,
    pub msg_type: String, // "user", "assistant", "tool_result"
    pub timestamp: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_creation: i64,
    pub cost_usd: f64,
    pub project: String,
    pub provider: String,
    /// True when this entry is a subagent execution (Claude Code's `isSidechain`).
    pub is_sidechain: bool,
    /// The `parentUuid` from the JSONL — links messages into a chain and
    /// links subagents back to the orchestrator's Task tool_use.
    pub parent_uuid: Option<String>,
}

/// A subagent invocation recovered from an assistant message's `Task`
/// tool_use block. Used to render orchestrator → worker edges.
#[derive(Debug, Clone)]
pub struct ParsedAgentSpawn {
    pub session_id: String,
    pub parent_msg_uuid: String,
    pub tool_use_id: String,
    pub subagent_type: Option<String>,
    pub prompt_preview: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone)]
pub struct ParsedSession {
    pub id: String,
    pub project: String,
    pub started_at: String,
    pub updated_at: String,
    pub model: Option<String>,
    pub version: Option<String>,
    pub provider: String,
}

// Raw JSONL structures for deserialization
#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(default)]
    uuid: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "type")]
    entry_type: Option<String>,
    timestamp: Option<String>,
    message: Option<RawMessage>,
    version: Option<String>,
    #[serde(rename = "parentUuid")]
    parent_uuid: Option<String>,
    #[serde(rename = "isSidechain", default)]
    is_sidechain: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    id: Option<String>,
    model: Option<String>,
    #[allow(dead_code)]
    role: Option<String>,
    usage: Option<RawUsage>,
    #[serde(default)]
    content: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawUsage {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
    cache_creation_input_tokens: Option<i64>,
    #[serde(default)]
    cache_creation: Option<CacheCreation>,
}

#[derive(Debug, Deserialize)]
struct CacheCreation {
    #[allow(dead_code)]
    ephemeral_1h_input_tokens: Option<i64>,
    #[allow(dead_code)]
    ephemeral_5m_input_tokens: Option<i64>,
}

pub fn parse_jsonl_line(
    line: &str,
    project: &str,
    pricing: &PricingRegistry,
) -> Option<(
    Option<ParsedSession>,
    Option<ParsedMessage>,
    Vec<ParsedAgentSpawn>,
)> {
    let entry: RawEntry = serde_json::from_str(line).ok()?;

    let entry_type = entry.entry_type.as_deref()?;
    let uuid = entry.uuid.clone()?;
    let session_id = entry.session_id.clone().unwrap_or_default();
    let timestamp = entry.timestamp.clone().unwrap_or_default();
    let is_sidechain = entry.is_sidechain.unwrap_or(false);
    let parent_uuid = entry.parent_uuid.clone();

    match entry_type {
        "user" => {
            // First user message defines the session. Sidechain user
            // messages are subagent prompts, not new sessions.
            if entry.parent_uuid.is_none() && !is_sidechain {
                let session = ParsedSession {
                    id: session_id.clone(),
                    project: project.to_string(),
                    started_at: timestamp.clone(),
                    updated_at: timestamp.clone(),
                    model: None,
                    version: entry.version,
                    provider: "claude".to_string(),
                };
                let msg = ParsedMessage {
                    uuid,
                    session_id,
                    msg_type: "user".to_string(),
                    timestamp,
                    model: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read: 0,
                    cache_creation: 0,
                    cost_usd: 0.0,
                    project: project.to_string(),
                    provider: "claude".to_string(),
                    is_sidechain,
                    parent_uuid,
                };
                Some((Some(session), Some(msg), Vec::new()))
            } else {
                let msg = ParsedMessage {
                    uuid,
                    session_id,
                    msg_type: "user".to_string(),
                    timestamp,
                    model: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read: 0,
                    cache_creation: 0,
                    cost_usd: 0.0,
                    project: project.to_string(),
                    provider: "claude".to_string(),
                    is_sidechain,
                    parent_uuid,
                };
                Some((None, Some(msg), Vec::new()))
            }
        }
        "assistant" => {
            let message = entry.message?;
            let usage = message.usage?;
            let model = message.model.unwrap_or_default();

            let input = usage.input_tokens.unwrap_or(0);
            let output = usage.output_tokens.unwrap_or(0);
            let cache_read = usage.cache_read_input_tokens.unwrap_or(0);
            let cache_creation = usage.cache_creation_input_tokens.unwrap_or(0);

            let cost = pricing.compute_cost(&model, input, output, cache_read, cache_creation);

            // Use message.id as the DB key when available. Claude Code writes
            // multiple JSONL entries per logical message turn (one per tool-use
            // round-trip), all sharing the same message.id but with different
            // UUIDs. Using message.id as the key ensures we deduplicate them
            // in parse_file_content (keeping only the last/final entry).
            let msg_key = message.id.unwrap_or_else(|| uuid.clone());

            let spawns = extract_task_spawns(
                message.content.as_ref(),
                &session_id,
                &msg_key,
                &timestamp,
            );

            let msg = ParsedMessage {
                uuid: msg_key,
                session_id,
                msg_type: "assistant".to_string(),
                timestamp,
                model: Some(model),
                input_tokens: input,
                output_tokens: output,
                cache_read,
                cache_creation,
                cost_usd: cost,
                project: project.to_string(),
                provider: "claude".to_string(),
                is_sidechain,
                parent_uuid,
            };
            Some((None, Some(msg), spawns))
        }
        // "progress" entries duplicate data already in subagent files —
        // skip them to avoid double-counting.
        _ => None,
    }
}

/// Walk an assistant message's `content` array and collect `Task` tool_use
/// invocations — these are the orchestrator → subagent edges.
fn extract_task_spawns(
    content: Option<&serde_json::Value>,
    session_id: &str,
    parent_msg_uuid: &str,
    timestamp: &str,
) -> Vec<ParsedAgentSpawn> {
    let Some(arr) = content.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut spawns = Vec::new();
    for block in arr {
        let Some(obj) = block.as_object() else { continue };
        if obj.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
            continue;
        }
        if obj.get("name").and_then(|v| v.as_str()) != Some("Task") {
            continue;
        }
        let Some(tool_use_id) = obj.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let input = obj.get("input").and_then(|v| v.as_object());
        let subagent_type = input
            .and_then(|m| m.get("subagent_type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let prompt_preview = input
            .and_then(|m| m.get("prompt"))
            .and_then(|v| v.as_str())
            .map(|s| {
                let trimmed: String = s.chars().take(200).collect();
                trimmed
            });
        spawns.push(ParsedAgentSpawn {
            session_id: session_id.to_string(),
            parent_msg_uuid: parent_msg_uuid.to_string(),
            tool_use_id: tool_use_id.to_string(),
            subagent_type,
            prompt_preview,
            timestamp: timestamp.to_string(),
        });
    }
    spawns
}

/// Parse all JSONL lines from file content (from a given offset), returning parsed data.
/// This is a pure function with no DB I/O, suitable for parallel execution.
///
/// Deduplicates assistant messages by message key (message.id from the API).
/// Claude Code writes multiple JSONL entries per logical turn (one per tool-use
/// round-trip), all sharing the same message.id. We keep only the last entry
/// for each key, which has the final accumulated output tokens.
/// Parsed result tuple: (session_open, message, agent_spawns).
pub type ParsedTuple = (
    Option<ParsedSession>,
    Option<ParsedMessage>,
    Vec<ParsedAgentSpawn>,
);

pub fn parse_file_content(
    content: &[u8],
    offset: u64,
    project: &str,
    pricing: &PricingRegistry,
) -> Vec<ParsedTuple> {
    if (offset as usize) >= content.len() {
        return Vec::new();
    }

    let new_content = &content[offset as usize..];
    let text = String::from_utf8_lossy(new_content);
    let mut results: Vec<ParsedTuple> = Vec::new();
    // Track assistant message positions by key for dedup (last wins)
    let mut msg_key_to_idx: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(parsed) = parse_jsonl_line(line, project, pricing) {
            if let Some(ref msg) = parsed.1 {
                if msg.msg_type == "assistant" {
                    if let Some(&prev_idx) = msg_key_to_idx.get(&msg.uuid) {
                        // Replace previous entry with this one (last wins).
                        // Agent spawns are emitted by the first Task tool_use
                        // entry for a message.id, so preserve them across the
                        // dedup swap.
                        let prev_spawns = std::mem::take(&mut results[prev_idx].2);
                        results[prev_idx] = (None, None, Vec::new());
                        let mut combined = prev_spawns;
                        combined.extend(parsed.2.iter().cloned());
                        msg_key_to_idx.insert(msg.uuid.clone(), results.len());
                        results.push((parsed.0, parsed.1, combined));
                        continue;
                    }
                    msg_key_to_idx.insert(msg.uuid.clone(), results.len());
                }
            }
            results.push(parsed);
        }
    }

    // Remove blanked-out entries from dedup
    results.retain(|entry| entry.0.is_some() || entry.1.is_some() || !entry.2.is_empty());

    results
}

/// Decode project directory name to human-readable project name.
/// e.g., "-Users-saurabh-Dev-echopad" -> "echopad"
pub fn decode_project_name(dir_name: &str) -> String {
    // URL-decode then take the last path component
    let decoded = urlencoding::decode(dir_name).unwrap_or_else(|_| dir_name.into());
    let path = decoded.replace('-', "/");
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(dir_name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_line(uuid: &str, session_id: &str) -> String {
        format!(
            r#"{{"uuid":"{}","sessionId":"{}","type":"user","timestamp":"2025-01-01T00:00:00Z","message":{{"role":"user"}}}}"#,
            uuid, session_id
        )
    }

    fn make_assistant_line(uuid: &str, session_id: &str) -> String {
        format!(
            r#"{{"uuid":"{}","sessionId":"{}","type":"assistant","timestamp":"2025-01-01T00:01:00Z","message":{{"role":"assistant","model":"claude-3-5-sonnet-20241022","usage":{{"input_tokens":100,"output_tokens":200}}}}}}"#,
            uuid, session_id
        )
    }

    #[test]
    fn test_decode_project_name() {
        assert_eq!(decode_project_name("-Users-saurabh-Dev-echopad"), "echopad");
        assert_eq!(decode_project_name("-Users-test-myproject"), "myproject");
        assert_eq!(decode_project_name("simple"), "simple");
    }

    #[test]
    fn test_parse_user_message() {
        let pricing = PricingRegistry::builtin();
        let line = r#"{"uuid":"u1","sessionId":"s1","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#;
        let result = parse_jsonl_line(line, "testproject", &pricing);
        assert!(result.is_some());
        let (session, msg, spawns) = result.unwrap();
        assert!(session.is_some());
        assert!(msg.is_some());
        assert!(spawns.is_empty());
        let session = session.unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.project, "testproject");
        assert_eq!(session.provider, "claude");
        let msg = msg.unwrap();
        assert!(!msg.is_sidechain);
        assert!(msg.parent_uuid.is_none());
    }

    #[test]
    fn test_parse_assistant_message() {
        let pricing = PricingRegistry::builtin();
        let line = r#"{"uuid":"u2","sessionId":"s1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}"#;
        let result = parse_jsonl_line(line, "testproject", &pricing);
        assert!(result.is_some());
        let (session, msg, _spawns) = result.unwrap();
        assert!(session.is_none());
        let msg = msg.unwrap();
        assert_eq!(msg.msg_type, "assistant");
        assert_eq!(msg.input_tokens, 1000);
        assert_eq!(msg.output_tokens, 500);
        assert_eq!(msg.cache_read, 200);
        assert!(msg.cost_usd > 0.0);
        assert_eq!(msg.provider, "claude");
        assert!(!msg.is_sidechain);
    }

    #[test]
    fn test_parse_invalid_json() {
        let pricing = PricingRegistry::builtin();
        let result = parse_jsonl_line("not json", "test", &pricing);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_subsequent_user_message() {
        let pricing = PricingRegistry::builtin();
        let line = r#"{"uuid":"u3","sessionId":"s1","type":"user","timestamp":"2025-01-15T10:01:00Z","parentUuid":"u2","message":{"role":"user"}}"#;
        let result = parse_jsonl_line(line, "testproject", &pricing);
        assert!(result.is_some());
        let (session, msg, _spawns) = result.unwrap();
        assert!(session.is_none()); // Not the first message, so no session
        let msg = msg.unwrap();
        assert_eq!(msg.parent_uuid.as_deref(), Some("u2"));
    }

    #[test]
    fn test_parsed_message_default_provider() {
        let pricing = PricingRegistry::builtin();
        // User message
        let line = r#"{"uuid":"u1","sessionId":"s1","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#;
        let result = parse_jsonl_line(line, "testproject", &pricing).unwrap();
        let (session, msg, _) = result;
        assert_eq!(session.unwrap().provider, "claude");
        assert_eq!(msg.unwrap().provider, "claude");

        // Assistant message
        let line = r#"{"uuid":"u2","sessionId":"s1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#;
        let result = parse_jsonl_line(line, "testproject", &pricing).unwrap();
        let (_, msg, _) = result;
        assert_eq!(msg.unwrap().provider, "claude");
    }

    #[test]
    fn test_parse_sidechain_assistant_message() {
        let pricing = PricingRegistry::builtin();
        let line = r#"{"uuid":"u9","sessionId":"s1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","parentUuid":"u8","isSidechain":true,"message":{"model":"claude-haiku-4-5-20251001","role":"assistant","usage":{"input_tokens":100,"output_tokens":50}}}"#;
        let result = parse_jsonl_line(line, "proj", &pricing).unwrap();
        let (_, msg, _) = result;
        let msg = msg.unwrap();
        assert!(msg.is_sidechain);
        assert_eq!(msg.parent_uuid.as_deref(), Some("u8"));
    }

    #[test]
    fn test_parse_sidechain_first_user_does_not_open_session() {
        let pricing = PricingRegistry::builtin();
        let line = r#"{"uuid":"u10","sessionId":"s1","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"isSidechain":true,"message":{"role":"user"}}"#;
        let result = parse_jsonl_line(line, "proj", &pricing).unwrap();
        let (session, msg, _) = result;
        assert!(session.is_none(), "sidechain user must not open a session");
        assert!(msg.is_some());
    }

    #[test]
    fn test_parse_task_tool_spawn() {
        let pricing = PricingRegistry::builtin();
        // Assistant turn with a Task tool_use block (Claude Code subagent spawn)
        let line = r#"{"uuid":"u20","sessionId":"sess-1","type":"assistant","timestamp":"2025-02-01T12:00:00Z","message":{"id":"msg_99","model":"claude-opus-4-7","role":"assistant","usage":{"input_tokens":10,"output_tokens":20},"content":[{"type":"text","text":"spawning"},{"type":"tool_use","id":"toolu_1","name":"Task","input":{"subagent_type":"Explore","prompt":"find all references to foo bar"}}]}}"#;
        let (_, msg, spawns) = parse_jsonl_line(line, "proj", &pricing).unwrap();
        let msg = msg.unwrap();
        assert_eq!(spawns.len(), 1);
        let s = &spawns[0];
        assert_eq!(s.session_id, "sess-1");
        assert_eq!(s.parent_msg_uuid, msg.uuid);
        assert_eq!(s.tool_use_id, "toolu_1");
        assert_eq!(s.subagent_type.as_deref(), Some("Explore"));
        assert!(s.prompt_preview.as_deref().unwrap().contains("find all references"));
    }

    #[test]
    fn test_parse_assistant_no_task_yields_no_spawns() {
        let pricing = PricingRegistry::builtin();
        let line = r#"{"uuid":"u30","sessionId":"sess-1","type":"assistant","timestamp":"2025-02-01T12:00:00Z","message":{"id":"msg_77","model":"claude-opus-4-7","role":"assistant","usage":{"input_tokens":5,"output_tokens":7},"content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"toolu_x","name":"Bash","input":{"command":"ls"}}]}}"#;
        let (_, _, spawns) = parse_jsonl_line(line, "proj", &pricing).unwrap();
        assert!(spawns.is_empty());
    }

    #[test]
    fn test_parse_file_content_empty() {
        let pricing = PricingRegistry::builtin();
        let results = parse_file_content(b"", 0, "test", &pricing);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_file_content_offset_past_end() {
        let pricing = PricingRegistry::builtin();
        let content = b"some content";
        let results = parse_file_content(content, 100, "test", &pricing);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_file_content_single_line() {
        let pricing = PricingRegistry::builtin();
        let line = make_user_line("uuid1", "session1");
        let results = parse_file_content(line.as_bytes(), 0, "myproject", &pricing);
        assert_eq!(results.len(), 1);
        let (session, msg, _) = &results[0];
        assert!(session.is_some());
        assert!(msg.is_some());
        assert_eq!(msg.as_ref().unwrap().session_id, "session1");
        assert_eq!(msg.as_ref().unwrap().project, "myproject");
    }

    #[test]
    fn test_parse_file_content_multiple_lines() {
        let pricing = PricingRegistry::builtin();
        let content = format!(
            "{}\n{}\n",
            make_user_line("u1", "s1"),
            make_assistant_line("u2", "s1")
        );
        let results = parse_file_content(content.as_bytes(), 0, "proj", &pricing);
        assert_eq!(results.len(), 2);
        // First line: user message with session
        assert!(results[0].0.is_some());
        // Second line: assistant message without session
        assert!(results[1].0.is_none());
        assert!(results[1].1.is_some());
        let msg = results[1].1.as_ref().unwrap();
        assert_eq!(msg.input_tokens, 100);
        assert_eq!(msg.output_tokens, 200);
        assert!(msg.cost_usd > 0.0);
    }

    #[test]
    fn test_parse_file_content_collects_task_spawns() {
        let pricing = PricingRegistry::builtin();
        let user = make_user_line("u1", "s1");
        // Two Task spawns from one assistant turn
        let assistant = r#"{"uuid":"u2","sessionId":"s1","type":"assistant","timestamp":"2025-02-01T12:00:00Z","message":{"id":"msg_a","model":"claude-opus-4-7","role":"assistant","usage":{"input_tokens":5,"output_tokens":7},"content":[{"type":"tool_use","id":"toolu_1","name":"Task","input":{"subagent_type":"Explore","prompt":"p1"}},{"type":"tool_use","id":"toolu_2","name":"Task","input":{"subagent_type":"Plan","prompt":"p2"}}]}}"#;
        let content = format!("{}\n{}\n", user, assistant);
        let results = parse_file_content(content.as_bytes(), 0, "proj", &pricing);
        let total_spawns: usize = results.iter().map(|r| r.2.len()).sum();
        assert_eq!(total_spawns, 2);
    }

    #[test]
    fn test_parse_file_content_dedup_preserves_spawns() {
        let pricing = PricingRegistry::builtin();
        // Two entries share the same message.id; only the second has a
        // Task tool_use. Dedup keeps the last entry but must not drop spawns
        // that arrived on an earlier round-trip.
        let earlier = r#"{"uuid":"ua","sessionId":"s1","type":"assistant","timestamp":"2025-02-01T12:00:00Z","message":{"id":"msg_dup","model":"claude-opus-4-7","role":"assistant","usage":{"input_tokens":1,"output_tokens":1},"content":[{"type":"tool_use","id":"toolu_keep","name":"Task","input":{"subagent_type":"Explore","prompt":"early"}}]}}"#;
        let later = r#"{"uuid":"ub","sessionId":"s1","type":"assistant","timestamp":"2025-02-01T12:00:01Z","message":{"id":"msg_dup","model":"claude-opus-4-7","role":"assistant","usage":{"input_tokens":2,"output_tokens":99}}}"#;
        let content = format!("{}\n{}\n", earlier, later);
        let results = parse_file_content(content.as_bytes(), 0, "proj", &pricing);
        // Single assistant entry after dedup, and its spawns survived.
        let kept: Vec<_> = results.iter().filter(|r| r.1.is_some()).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].1.as_ref().unwrap().output_tokens, 99);
        assert_eq!(kept[0].2.len(), 1);
        assert_eq!(kept[0].2[0].tool_use_id, "toolu_keep");
    }

    #[test]
    fn test_parse_file_content_with_offset() {
        let pricing = PricingRegistry::builtin();
        let line1 = make_user_line("u1", "s1");
        let line2 = make_assistant_line("u2", "s1");
        let content = format!("{}\n{}\n", line1, line2);
        let offset = (line1.len() + 1) as u64; // skip first line + newline
        let results = parse_file_content(content.as_bytes(), offset, "proj", &pricing);
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_some());
        assert_eq!(results[0].1.as_ref().unwrap().msg_type, "assistant");
    }

    #[test]
    fn test_parse_file_content_skips_blank_lines() {
        let pricing = PricingRegistry::builtin();
        let content = format!("\n\n{}\n\n", make_user_line("u1", "s1"));
        let results = parse_file_content(content.as_bytes(), 0, "proj", &pricing);
        assert_eq!(results.len(), 1);
    }

    fn make_assistant_line_with_msg_id(uuid: &str, session_id: &str, msg_id: &str, output: i64) -> String {
        format!(
            r#"{{"uuid":"{}","sessionId":"{}","type":"assistant","timestamp":"2025-01-01T00:01:00Z","message":{{"id":"{}","role":"assistant","model":"claude-opus-4-6","usage":{{"input_tokens":5,"output_tokens":{},"cache_read_input_tokens":17000,"cache_creation_input_tokens":6000}}}}}}"#,
            uuid, session_id, msg_id, output
        )
    }

    #[test]
    fn test_parse_file_content_deduplicates_by_message_id() {
        let pricing = PricingRegistry::builtin();
        // Simulate 3 entries for the same message.id (tool-use round trips)
        let content = format!(
            "{}\n{}\n{}\n{}\n",
            make_user_line("u1", "s1"),
            make_assistant_line_with_msg_id("uuid-a", "s1", "msg_01ABC", 12),
            make_assistant_line_with_msg_id("uuid-b", "s1", "msg_01ABC", 12),
            make_assistant_line_with_msg_id("uuid-c", "s1", "msg_01ABC", 462),
        );
        let results = parse_file_content(content.as_bytes(), 0, "proj", &pricing);
        // Should have 2 entries: 1 user + 1 assistant (deduped)
        assert_eq!(results.len(), 2);
        let msg = results[1].1.as_ref().unwrap();
        // Should keep the last entry (highest output_tokens)
        assert_eq!(msg.output_tokens, 462);
        // Key should be message.id, not uuid
        assert_eq!(msg.uuid, "msg_01ABC");
    }

    #[test]
    fn test_parse_file_content_different_message_ids_not_deduped() {
        let pricing = PricingRegistry::builtin();
        let content = format!(
            "{}\n{}\n",
            make_assistant_line_with_msg_id("uuid-a", "s1", "msg_01AAA", 100),
            make_assistant_line_with_msg_id("uuid-b", "s1", "msg_01BBB", 200),
        );
        let results = parse_file_content(content.as_bytes(), 0, "proj", &pricing);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_parse_file_content_parallel_matches_sequential() {
        use rayon::prelude::*;

        let pricing = PricingRegistry::builtin();

        // Create multiple "files" (byte slices)
        let files: Vec<(String, &str)> = (0..10)
            .map(|i| {
                let content = format!(
                    "{}\n{}\n",
                    make_user_line(&format!("u{i}a"), &format!("s{i}")),
                    make_assistant_line(&format!("u{i}b"), &format!("s{i}"))
                );
                (content, "project")
            })
            .collect();

        // Sequential parse
        let sequential: Vec<_> = files
            .iter()
            .flat_map(|(content, project)| parse_file_content(content.as_bytes(), 0, project, &pricing))
            .collect();

        // Parallel parse
        let parallel: Vec<_> = files
            .par_iter()
            .flat_map(|(content, project)| parse_file_content(content.as_bytes(), 0, project, &pricing))
            .collect();

        // Same number of results
        assert_eq!(sequential.len(), parallel.len());
        assert_eq!(sequential.len(), 20); // 2 per file * 10 files

        // Both should produce sessions for user messages and messages for all
        let seq_sessions: usize = sequential.iter().filter(|(s, _, _)| s.is_some()).count();
        let par_sessions: usize = parallel.iter().filter(|(s, _, _)| s.is_some()).count();
        assert_eq!(seq_sessions, par_sessions);
        assert_eq!(seq_sessions, 10); // one session per file
    }
}
