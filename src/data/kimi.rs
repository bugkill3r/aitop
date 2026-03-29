use std::io::BufRead;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use super::parser::{ParsedMessage, ParsedSession};
use super::pricing::PricingRegistry;
use super::scanner::SessionFile;

// --- Serde structs for Kimi CLI wire.jsonl format ---

#[derive(Debug, Deserialize)]
struct KimiWireEntry {
    #[serde(rename = "type")]
    #[serde(default)]
    entry_type: Option<String>,
    #[serde(default)]
    payload: Option<KimiPayload>,
    #[serde(default)]
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KimiPayload {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    usage: Option<KimiUsage>,
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KimiUsage {
    #[serde(default, rename = "input_other")]
    input_other: i64,
    #[serde(default)]
    output: i64,
    #[serde(default, rename = "input_cache_read")]
    input_cache_read: i64,
    #[serde(default, rename = "cache_creation_input_tokens")]
    cache_creation_input_tokens: i64,
}

// --- Public API ---

/// Scan Kimi CLI sessions directory for wire.jsonl files.
///
/// Structure: `kimi_dir/<group>/<uuid>/wire.jsonl`
pub fn scan_kimi_sessions(kimi_dir: &Path) -> Result<Vec<SessionFile>> {
    let mut files = Vec::new();

    if !kimi_dir.exists() {
        return Ok(files);
    }

    for group_entry in std::fs::read_dir(kimi_dir)? {
        let group_entry = group_entry?;
        let group_path = group_entry.path();

        if !group_path.is_dir() {
            continue;
        }

        let group_name = group_entry
            .file_name()
            .to_string_lossy()
            .to_string();

        for session_entry in std::fs::read_dir(&group_path)? {
            let session_entry = session_entry?;
            let session_path = session_entry.path();

            if !session_path.is_dir() {
                continue;
            }

            let wire_file = session_path.join("wire.jsonl");
            if wire_file.exists() {
                let session_id = session_entry
                    .file_name()
                    .to_string_lossy()
                    .to_string();

                files.push(SessionFile {
                    path: wire_file,
                    session_id,
                    project: group_name.clone(),
                });
            }
        }
    }

    Ok(files)
}

/// Parse a Kimi CLI wire.jsonl file.
pub fn parse_kimi_file(
    path: &Path,
    project: &str,
    pricing: &PricingRegistry,
) -> Result<(ParsedSession, Vec<ParsedMessage>)> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    let session_id = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("kimi-session")
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

        let entry: KimiWireEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // We're looking for StatusPayload entries with usage data
        let payload = match entry.payload {
            Some(p) => p,
            None => continue,
        };

        let usage = match payload.usage {
            Some(u) => u,
            None => continue,
        };

        // Skip zero-token entries
        if usage.input_other == 0 && usage.output == 0 && usage.input_cache_read == 0 {
            continue;
        }

        let ts = entry.timestamp.clone().unwrap_or_default();
        if first_ts.is_none() {
            first_ts = Some(ts.clone());
        }
        last_ts = Some(ts.clone());

        if model.is_none() {
            model = payload.model.clone();
        }

        let msg_model = payload.model.as_deref().unwrap_or("");
        let input_tokens = usage.input_other;
        let output_tokens = usage.output;
        let cache_read = usage.input_cache_read;
        let cache_creation = usage.cache_creation_input_tokens;

        let cost = pricing.compute_cost(
            msg_model,
            input_tokens,
            output_tokens,
            cache_read,
            cache_creation,
        );

        let msg_id = payload.id.unwrap_or_else(|| {
            msg_idx += 1;
            format!("{}-{}", session_id, msg_idx)
        });

        parsed_messages.push(ParsedMessage {
            uuid: msg_id,
            session_id: session_id.clone(),
            msg_type: "assistant".to_string(),
            timestamp: ts,
            model: payload.model.clone(),
            input_tokens,
            output_tokens,
            cache_read,
            cache_creation,
            cost_usd: cost,
            project: project.to_string(),
            provider: "kimi".to_string(),
        });
    }

    let session = ParsedSession {
        id: session_id,
        project: project.to_string(),
        started_at: first_ts.clone().unwrap_or_default(),
        updated_at: last_ts.unwrap_or_else(|| first_ts.unwrap_or_default()),
        model,
        version: None,
        provider: "kimi".to_string(),
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
    fn test_parse_kimi_wire() {
        let tmp = TempDir::new().unwrap();
        let session_dir = tmp.path().join("group1").join("uuid-abc");
        fs::create_dir_all(&session_dir).unwrap();
        let wire_path = session_dir.join("wire.jsonl");

        let mut f = fs::File::create(&wire_path).unwrap();
        // Entry with usage
        writeln!(f, r#"{{"type":"status","timestamp":"2026-03-10T10:00:00Z","payload":{{"model":"claude-sonnet-4-6","status":"complete","id":"msg-001","usage":{{"input_other":5000,"output":1000,"input_cache_read":2000,"cache_creation_input_tokens":500}}}}}}"#).unwrap();
        // Entry without usage (should be skipped)
        writeln!(f, r#"{{"type":"stream","timestamp":"2026-03-10T10:00:01Z","payload":{{"model":"claude-sonnet-4-6","status":"streaming"}}}}"#).unwrap();
        // Another entry with usage
        writeln!(f, r#"{{"type":"status","timestamp":"2026-03-10T10:05:00Z","payload":{{"model":"claude-sonnet-4-6","status":"complete","id":"msg-002","usage":{{"input_other":3000,"output":800,"input_cache_read":1000,"cache_creation_input_tokens":0}}}}}}"#).unwrap();

        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_kimi_file(&wire_path, "group1", &pricing).unwrap();

        assert_eq!(session.id, "uuid-abc");
        assert_eq!(session.project, "group1");
        assert_eq!(session.provider, "kimi");
        assert_eq!(session.model, Some("claude-sonnet-4-6".to_string()));

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].uuid, "msg-001");
        assert_eq!(messages[0].input_tokens, 5000);
        assert_eq!(messages[0].output_tokens, 1000);
        assert_eq!(messages[0].cache_read, 2000);
        assert_eq!(messages[0].cache_creation, 500);
        assert_eq!(messages[0].provider, "kimi");
        assert!(messages[0].cost_usd > 0.0);

        assert_eq!(messages[1].uuid, "msg-002");
        assert_eq!(messages[1].input_tokens, 3000);
    }

    #[test]
    fn test_parse_kimi_empty() {
        let tmp = TempDir::new().unwrap();
        let session_dir = tmp.path().join("group1").join("uuid-empty");
        fs::create_dir_all(&session_dir).unwrap();
        let wire_path = session_dir.join("wire.jsonl");
        fs::File::create(&wire_path).unwrap();

        let pricing = PricingRegistry::builtin();
        let (session, messages) = parse_kimi_file(&wire_path, "group1", &pricing).unwrap();

        assert_eq!(session.provider, "kimi");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_scan_kimi_sessions() {
        let tmp = TempDir::new().unwrap();
        let kimi_dir = tmp.path();

        let s1 = kimi_dir.join("group1").join("uuid-001");
        fs::create_dir_all(&s1).unwrap();
        fs::File::create(s1.join("wire.jsonl")).unwrap();

        let s2 = kimi_dir.join("group1").join("uuid-002");
        fs::create_dir_all(&s2).unwrap();
        fs::File::create(s2.join("wire.jsonl")).unwrap();

        let s3 = kimi_dir.join("group2").join("uuid-003");
        fs::create_dir_all(&s3).unwrap();
        fs::File::create(s3.join("wire.jsonl")).unwrap();

        // Directory without wire.jsonl
        let s4 = kimi_dir.join("group2").join("uuid-004");
        fs::create_dir_all(&s4).unwrap();

        let files = scan_kimi_sessions(kimi_dir).unwrap();
        assert_eq!(files.len(), 3);

        let ids: Vec<&str> = files.iter().map(|f| f.session_id.as_str()).collect();
        assert!(ids.contains(&"uuid-001"));
        assert!(ids.contains(&"uuid-002"));
        assert!(ids.contains(&"uuid-003"));
    }
}
