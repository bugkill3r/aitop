use serde::Deserialize;

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
}

#[derive(Debug, Clone)]
pub struct ParsedSession {
    pub id: String,
    pub project: String,
    pub started_at: String,
    pub updated_at: String,
    pub model: Option<String>,
    pub version: Option<String>,
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
    ephemeral_1h_input_tokens: Option<i64>,
    ephemeral_5m_input_tokens: Option<i64>,
}

/// Pricing per million tokens
fn cost_per_mtok(model: &str) -> (f64, f64, f64, f64) {
    // (input, output, cache_read, cache_creation)
    let model_lower = model.to_lowercase();
    if model_lower.contains("opus") {
        (15.0, 75.0, 1.50, 18.75)
    } else if model_lower.contains("haiku") {
        (0.80, 4.0, 0.08, 1.0)
    } else {
        // Default to Sonnet pricing (also covers explicit "sonnet" match)
        (3.0, 15.0, 0.30, 3.75)
    }
}

fn compute_cost(model: &str, input: i64, output: i64, cache_read: i64, cache_creation: i64) -> f64 {
    let (inp_rate, out_rate, cr_rate, cc_rate) = cost_per_mtok(model);
    let scale = 1_000_000.0;
    (input as f64 * inp_rate / scale)
        + (output as f64 * out_rate / scale)
        + (cache_read as f64 * cr_rate / scale)
        + (cache_creation as f64 * cc_rate / scale)
}

pub fn parse_jsonl_line(line: &str, project: &str) -> Option<(Option<ParsedSession>, Option<ParsedMessage>)> {
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

            let cost = compute_cost(&model, input, output, cache_read, cache_creation);

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
            };
            Some((None, Some(msg)))
        }
        _ => None,
    }
}

/// Decode project directory name to human-readable project name.
/// e.g., "-Users-saurabh-Dev-echopad" → "echopad"
pub fn decode_project_name(dir_name: &str) -> String {
    // URL-decode then take the last path component
    let decoded = urlencoding::decode(dir_name).unwrap_or_else(|_| dir_name.into());
    let path = decoded.replace('-', "/");
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(dir_name)
        .to_string()
}
