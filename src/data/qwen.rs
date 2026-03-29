use std::io::BufRead;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use super::parser::{ParsedMessage, ParsedSession};
use super::pricing::PricingRegistry;
use super::scanner::SessionFile;

// --- Serde structs for Qwen CLI JSONL format ---

#[derive(Debug, Deserialize)]
struct QwenEntry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default, rename = "usageMetadata")]
    usage_metadata: Option<QwenUsage>,
}

#[derive(Debug, Deserialize)]
struct QwenUsage {
    #[serde(default, rename = "promptTokenCount")]
    prompt_token_count: i64,
    #[serde(default, rename = "candidatesTokenCount")]
    candidates_token_count: i64,
    #[serde(default, rename = "thoughtsTokenCount")]
    thoughts_token_count: i64,
    #[serde(default, rename = "cachedContentTokenCount")]
    cached_content_token_count: i64,
}

// --- Public API ---

/// Scan Qwen CLI projects directory for chat JSONL files.
///
/// Structure: `qwen_dir/<project_path>/chats/*.jsonl`
pub fn scan_qwen_sessions(qwen_dir: &Path) -> Result<Vec<SessionFile>> {
    let mut files = Vec::new();

    if !qwen_dir.exists() {
        return Ok(files);
    }

    scan_qwen_recursive(qwen_dir, qwen_dir, &mut files)?;
    Ok(files)
}

fn scan_qwen_recursive(
    base_dir: &Path,
    current_dir: &Path,
    files: &mut Vec<SessionFile>,
) -> Result<()> {
    let entries = match std::fs::read_dir(current_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("chats") {
                // Found a chats directory — scan its JSONL files
                if let Ok(chat_entries) = std::fs::read_dir(&path) {
                    for chat_entry in chat_entries.flatten() {
                        let chat_path = chat_entry.path();
                        if chat_path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                            let session_id = chat_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string();

                            // Derive project from the relative path between base_dir and chats parent
                            let project = path
                                .parent()
                                .and_then(|p| p.strip_prefix(base_dir).ok())
                                .map(|p| p.to_string_lossy().replace('/', "-"))
                                .unwrap_or_else(|| "qwen".to_string());

                            files.push(SessionFile {
                                path: chat_path,
                                session_id,
                                project,
                            });
                        }
                    }
                }
            } else {
                scan_qwen_recursive(base_dir, &path, files)?;
            }
        }
    }

    Ok(())
}

/// Parse a Qwen CLI JSONL chat file.
pub fn parse_qwen_file(
    path: &Path,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("qwen-session")
        .to_string();

    let mut parsed_messages = Vec::new();
    let mut model: Option<String> = None;
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut msg_idx = 0;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        let entry: QwenEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let usage = match entry.usage_metadata {
            Some(u) => u,
            None => continue,
        };

        // Skip zero-token entries
        if usage.prompt_token_count == 0 && usage.candidates_token_count == 0 {
            continue;
        }

        let ts = entry.timestamp.clone().unwrap_or_default();
        if first_ts.is_none() {
            first_ts = Some(ts.clone());
        }
        last_ts = Some(ts.clone());

        if model.is_none() {
            model = entry.model.clone();
        }

        let msg_model = entry.model.as_deref().unwrap_or("");
        // output = candidates + thoughts (thinking tokens count as output)
        let output_tokens = usage.candidates_token_count + usage.thoughts_token_count;
        let input_tokens = usage.prompt_token_count;
        let cache_read = usage.cached_content_token_count;

        let cost = pricing.compute_cost(
            msg_model,
            input_tokens,
            output_tokens,
            cache_read,
            0, // no cache_creation field in Qwen
        );

        msg_idx += 1;
        let msg_id = entry.id.unwrap_or_else(|| format!("{}-{}", session_id, msg_idx));

        parsed_messages.push(ParsedMessage {
            uuid: msg_id,
            session_id: session_id.clone(),
            msg_type: "assistant".to_string(),
            timestamp: ts,
            model: entry.model.clone(),
            input_tokens,
            output_tokens,
            cache_read,
            cache_creation: 0,
            cost_usd: cost,
            project: project.to_string(),
            provider: "qwen".to_string(),
        });
    }

    let session = ParsedSession {
        id: session_id,
        project: project.to_string(),
        started_at: first_ts.clone().unwrap_or_default(),
        updated_at: last_ts.unwrap_or_else(|| first_ts.unwrap_or_default()),
        model,
        version: None,
        provider: "qwen".to_string(),
    };

    Ok((session, parsed_messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_qwen_file() {
        let tmp = TempDir::new().unwrap();
        let chats_dir = tmp.path().join("myproject").join("chats");
        fs::create_dir_all(&chats_dir).unwrap();
        let chat_path = chats_dir.join("chat-001.jsonl");

        let mut f = fs::File::create(&chat_path).unwrap();
        writeln!(f, r#"{{"id":"msg-1","model":"qwen-max","timestamp":"2026-03-10T10:00:00Z","usageMetadata":{{"promptTokenCount":5000,"candidatesTokenCount":1000,"thoughtsTokenCount":200,"cachedContentTokenCount":2000}}}}"#).unwrap();
        writeln!(f, r#"{{"id":"msg-2","model":"qwen-max","timestamp":"2026-03-10T10:05:00Z","usageMetadata":{{"promptTokenCount":3000,"candidatesTokenCount":500,"thoughtsTokenCount":100,"cachedContentTokenCount":1000}}}}"#).unwrap();

        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_qwen_file(&chat_path, "myproject", &pricing).unwrap();

        assert_eq!(session.id, "chat-001");
        assert_eq!(session.project, "myproject");
        assert_eq!(session.provider, "qwen");
        assert_eq!(session.model, Some("qwen-max".to_string()));

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].uuid, "msg-1");
        assert_eq!(messages[0].input_tokens, 5000);
        assert_eq!(messages[0].output_tokens, 1200); // candidates + thoughts
        assert_eq!(messages[0].cache_read, 2000);
        assert_eq!(messages[0].cache_creation, 0);
        assert_eq!(messages[0].provider, "qwen");
        assert!(messages[0].cost_usd > 0.0);

        assert_eq!(messages[1].input_tokens, 3000);
        assert_eq!(messages[1].output_tokens, 600);
    }

    #[test]
    fn test_parse_qwen_empty() {
        let tmp = TempDir::new().unwrap();
        let chat_path = tmp.path().join("empty.jsonl");
        fs::File::create(&chat_path).unwrap();

        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_qwen_file(&chat_path, "proj", &pricing).unwrap();

        assert_eq!(session.provider, "qwen");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_parse_qwen_skips_no_usage() {
        let tmp = TempDir::new().unwrap();
        let chat_path = tmp.path().join("partial.jsonl");
        let mut f = fs::File::create(&chat_path).unwrap();
        // Line without usageMetadata
        writeln!(f, r#"{{"id":"msg-1","model":"qwen-max","timestamp":"2026-03-10T10:00:00Z"}}"#).unwrap();
        // Line with zero tokens
        writeln!(f, r#"{{"id":"msg-2","model":"qwen-max","timestamp":"2026-03-10T10:01:00Z","usageMetadata":{{"promptTokenCount":0,"candidatesTokenCount":0,"thoughtsTokenCount":0,"cachedContentTokenCount":0}}}}"#).unwrap();
        // Valid line
        writeln!(f, r#"{{"id":"msg-3","model":"qwen-max","timestamp":"2026-03-10T10:02:00Z","usageMetadata":{{"promptTokenCount":1000,"candidatesTokenCount":200,"thoughtsTokenCount":50,"cachedContentTokenCount":0}}}}"#).unwrap();

        let pricing = PricingRegistry::builtin();
        let (_, messages) = parse_qwen_file(&chat_path, "proj", &pricing).unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].uuid, "msg-3");
    }

    #[test]
    fn test_scan_qwen_sessions() {
        let tmp = TempDir::new().unwrap();
        let qwen_dir = tmp.path();

        let p1_chats = qwen_dir.join("project-a").join("chats");
        fs::create_dir_all(&p1_chats).unwrap();
        fs::File::create(p1_chats.join("chat-001.jsonl")).unwrap();
        fs::File::create(p1_chats.join("chat-002.jsonl")).unwrap();

        let p2_chats = qwen_dir.join("project-b").join("chats");
        fs::create_dir_all(&p2_chats).unwrap();
        fs::File::create(p2_chats.join("chat-003.jsonl")).unwrap();

        // Non-jsonl file should be ignored
        fs::File::create(p1_chats.join("notes.txt")).unwrap();

        let files = scan_qwen_sessions(qwen_dir).unwrap();
        assert_eq!(files.len(), 3);

        let ids: Vec<&str> = files.iter().map(|f| f.session_id.as_str()).collect();
        assert!(ids.contains(&"chat-001"));
        assert!(ids.contains(&"chat-002"));
        assert!(ids.contains(&"chat-003"));
    }
}
