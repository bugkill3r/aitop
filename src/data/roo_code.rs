use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

use super::parser::{ParsedMessage, ParsedSession};
use super::pricing::PricingRegistry;
use super::scanner::SessionFile;

// --- Serde structs for Roo Code ui_messages.json ---

#[derive(Debug, Deserialize)]
struct RooMessage {
    #[serde(default)]
    ts: Option<u64>,
    #[serde(rename = "type")]
    #[serde(default)]
    msg_type: Option<String>,
    #[serde(default)]
    say: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RooApiReq {
    #[serde(default)]
    cost: Option<f64>,
    #[serde(default, rename = "tokensIn")]
    tokens_in: i64,
    #[serde(default, rename = "tokensOut")]
    tokens_out: i64,
    #[serde(default, rename = "cacheReads")]
    cache_reads: i64,
    #[serde(default, rename = "cacheWrites")]
    cache_writes: i64,
    #[serde(default, rename = "modelId")]
    model_id: Option<String>,
}

// --- Public API ---

/// Scan Roo Code tasks directory for ui_messages.json files.
///
/// Structure: `roo_dir/<taskId>/ui_messages.json`
pub fn scan_roo_code_sessions(roo_dir: &Path) -> Result<Vec<SessionFile>> {
    let mut files = Vec::new();

    if !roo_dir.exists() {
        return Ok(files);
    }

    for entry in std::fs::read_dir(roo_dir)? {
        let entry = entry?;
        let task_path = entry.path();

        if !task_path.is_dir() {
            continue;
        }

        let ui_file = task_path.join("ui_messages.json");
        if ui_file.exists() {
            let session_id = task_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            files.push(SessionFile {
                path: ui_file,
                session_id,
                project: "roo-code".to_string(),
            });
        }
    }

    Ok(files)
}

/// Parse a Roo Code ui_messages.json file.
pub fn parse_roo_code_file(
    path: &Path,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let content = std::fs::read_to_string(path)?;
    parse_roo_code_str(&content, project, pricing, path)
}

fn parse_roo_code_str(
    json: &str,
    project: &str,
    pricing: &PricingRegistry,
    path: &Path,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let messages: Vec<RooMessage> = serde_json::from_str(json)?;

    let session_id = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("roo-session")
        .to_string();

    let mut parsed_messages = Vec::new();
    let mut model: Option<String> = None;
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;

    for (i, msg) in messages.iter().enumerate() {
        // Filter: type == "say" and say == "api_req_started"
        if msg.msg_type.as_deref() != Some("say") || msg.say.as_deref() != Some("api_req_started") {
            continue;
        }

        // The text field contains a JSON string with the API request data
        let text = match &msg.text {
            Some(t) => t,
            None => continue,
        };

        let api_req: RooApiReq = match serde_json::from_str(text) {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Skip zero-token entries
        if api_req.tokens_in == 0 && api_req.tokens_out == 0 {
            continue;
        }

        let ts = msg.ts.map(|t| {
            chrono::DateTime::from_timestamp_millis(t as i64)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| t.to_string())
        }).unwrap_or_default();

        if first_ts.is_none() {
            first_ts = Some(ts.clone());
        }
        last_ts = Some(ts.clone());

        if model.is_none() {
            model = api_req.model_id.clone();
        }

        let msg_model = api_req.model_id.as_deref().unwrap_or("");
        let cost = api_req.cost.unwrap_or_else(|| {
            pricing.compute_cost(
                msg_model,
                api_req.tokens_in,
                api_req.tokens_out,
                api_req.cache_reads,
                api_req.cache_writes,
            )
        });

        parsed_messages.push(ParsedMessage {
            uuid: format!("{}-{}", session_id, i),
            session_id: session_id.clone(),
            msg_type: "assistant".to_string(),
            timestamp: ts,
            model: api_req.model_id.clone(),
            input_tokens: api_req.tokens_in,
            output_tokens: api_req.tokens_out,
            cache_read: api_req.cache_reads,
            cache_creation: api_req.cache_writes,
            cost_usd: cost,
            project: project.to_string(),
            provider: "roocode".to_string(),
        });
    }

    let session = ParsedSession {
        id: session_id,
        project: project.to_string(),
        started_at: first_ts.clone().unwrap_or_default(),
        updated_at: last_ts.unwrap_or_else(|| first_ts.unwrap_or_default()),
        model,
        version: None,
        provider: "roocode".to_string(),
    };

    Ok((session, parsed_messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn sample_ui_messages() -> &'static str {
        r#"[
            {
                "ts": 1710072000000,
                "type": "say",
                "say": "api_req_started",
                "text": "{\"cost\": 0.05, \"tokensIn\": 5000, \"tokensOut\": 1000, \"cacheReads\": 2000, \"cacheWrites\": 500, \"modelId\": \"claude-sonnet-4-6\"}"
            },
            {
                "ts": 1710072060000,
                "type": "say",
                "say": "text",
                "text": "Some user message"
            },
            {
                "ts": 1710072120000,
                "type": "say",
                "say": "api_req_started",
                "text": "{\"cost\": 0.03, \"tokensIn\": 3000, \"tokensOut\": 500, \"cacheReads\": 1000, \"cacheWrites\": 0, \"modelId\": \"claude-sonnet-4-6\"}"
            }
        ]"#
    }

    #[test]
    fn test_parse_roo_code() {
        let tmp = TempDir::new().unwrap();
        let task_dir = tmp.path().join("task-123");
        fs::create_dir_all(&task_dir).unwrap();
        let file_path = task_dir.join("ui_messages.json");
        fs::write(&file_path, sample_ui_messages()).unwrap();

        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_roo_code_file(&file_path, "roo-code", &pricing).unwrap();

        assert_eq!(session.id, "task-123");
        assert_eq!(session.provider, "roocode");
        assert_eq!(session.model, Some("claude-sonnet-4-6".to_string()));

        // Only api_req_started messages (2 of 3)
        assert_eq!(messages.len(), 2);

        assert_eq!(messages[0].input_tokens, 5000);
        assert_eq!(messages[0].output_tokens, 1000);
        assert_eq!(messages[0].cache_read, 2000);
        assert_eq!(messages[0].cache_creation, 500);
        assert!((messages[0].cost_usd - 0.05).abs() < 0.001);
        assert_eq!(messages[0].provider, "roocode");

        assert_eq!(messages[1].input_tokens, 3000);
        assert_eq!(messages[1].output_tokens, 500);
        assert!((messages[1].cost_usd - 0.03).abs() < 0.001);
    }

    #[test]
    fn test_parse_roo_code_no_cost_uses_pricing() {
        let tmp = TempDir::new().unwrap();
        let task_dir = tmp.path().join("task-456");
        fs::create_dir_all(&task_dir).unwrap();
        let file_path = task_dir.join("ui_messages.json");

        let json = r#"[{
            "ts": 1710072000000,
            "type": "say",
            "say": "api_req_started",
            "text": "{\"tokensIn\": 1000000, \"tokensOut\": 0, \"cacheReads\": 0, \"cacheWrites\": 0, \"modelId\": \"claude-sonnet-4-6\"}"
        }]"#;
        fs::write(&file_path, json).unwrap();

        let pricing = PricingRegistry::builtin();
        let (_, messages) = parse_roo_code_file(&file_path, "roo-code", &pricing).unwrap();

        assert_eq!(messages.len(), 1);
        // Sonnet: 3.0 per MTok input, 1M tokens = $3.00
        assert!((messages[0].cost_usd - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_roo_code_empty() {
        let tmp = TempDir::new().unwrap();
        let task_dir = tmp.path().join("task-empty");
        fs::create_dir_all(&task_dir).unwrap();
        let file_path = task_dir.join("ui_messages.json");
        fs::write(&file_path, "[]").unwrap();

        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_roo_code_file(&file_path, "roo-code", &pricing).unwrap();

        assert_eq!(session.provider, "roocode");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_scan_roo_code_sessions() {
        let tmp = TempDir::new().unwrap();
        let roo_dir = tmp.path();

        let task1 = roo_dir.join("task-abc");
        fs::create_dir_all(&task1).unwrap();
        fs::write(task1.join("ui_messages.json"), "[]").unwrap();

        let task2 = roo_dir.join("task-def");
        fs::create_dir_all(&task2).unwrap();
        fs::write(task2.join("ui_messages.json"), "[]").unwrap();

        // Task without ui_messages.json should be skipped
        let task3 = roo_dir.join("task-ghi");
        fs::create_dir_all(&task3).unwrap();

        let files = scan_roo_code_sessions(roo_dir).unwrap();
        assert_eq!(files.len(), 2);

        let ids: Vec<&str> = files.iter().map(|f| f.session_id.as_str()).collect();
        assert!(ids.contains(&"task-abc"));
        assert!(ids.contains(&"task-def"));
    }
}
