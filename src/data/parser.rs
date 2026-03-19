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
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    model: Option<String>,
    #[allow(dead_code)]
    role: Option<String>,
    usage: Option<RawUsage>,
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
) -> Option<(Option<ParsedSession>, Option<ParsedMessage>)> {
    let entry: RawEntry = serde_json::from_str(line).ok()?;

    let entry_type = entry.entry_type.as_deref()?;
    let uuid = entry.uuid.clone()?;
    let session_id = entry.session_id.clone().unwrap_or_default();
    let timestamp = entry.timestamp.clone().unwrap_or_default();

    match entry_type {
        "user" => {
            // First user message defines the session
            if entry.parent_uuid.is_none() {
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
                };
                Some((Some(session), Some(msg)))
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
                };
                Some((None, Some(msg)))
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

            let msg = ParsedMessage {
                uuid,
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
            };
            Some((None, Some(msg)))
        }
        _ => None,
    }
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
        let (session, msg) = result.unwrap();
        assert!(session.is_some());
        assert!(msg.is_some());
        let session = session.unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.project, "testproject");
        assert_eq!(session.provider, "claude");
    }

    #[test]
    fn test_parse_assistant_message() {
        let pricing = PricingRegistry::builtin();
        let line = r#"{"uuid":"u2","sessionId":"s1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}"#;
        let result = parse_jsonl_line(line, "testproject", &pricing);
        assert!(result.is_some());
        let (session, msg) = result.unwrap();
        assert!(session.is_none());
        let msg = msg.unwrap();
        assert_eq!(msg.msg_type, "assistant");
        assert_eq!(msg.input_tokens, 1000);
        assert_eq!(msg.output_tokens, 500);
        assert_eq!(msg.cache_read, 200);
        assert!(msg.cost_usd > 0.0);
        assert_eq!(msg.provider, "claude");
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
        let (session, msg) = result.unwrap();
        assert!(session.is_none()); // Not the first message, so no session
        assert!(msg.is_some());
    }

    #[test]
    fn test_parsed_message_default_provider() {
        let pricing = PricingRegistry::builtin();
        // User message
        let line = r#"{"uuid":"u1","sessionId":"s1","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#;
        let result = parse_jsonl_line(line, "testproject", &pricing).unwrap();
        let (session, msg) = result;
        assert_eq!(session.unwrap().provider, "claude");
        assert_eq!(msg.unwrap().provider, "claude");

        // Assistant message
        let line = r#"{"uuid":"u2","sessionId":"s1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#;
        let result = parse_jsonl_line(line, "testproject", &pricing).unwrap();
        let (_, msg) = result;
        assert_eq!(msg.unwrap().provider, "claude");
    }
}
