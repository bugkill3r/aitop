use std::fs;

use aitop::data::aggregator::Aggregator;
use aitop::data::db::Database;
use aitop::data::scanner::scan_projects;

/// Helper: create a temp dir, ingest sample data, return (db_path, tempdir).
fn setup_fixture(jsonl: &str) -> (std::path::PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = dir.path().join("projects");
    let project_dir = projects_dir.join("-Users-test-myproject");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join("sess1.jsonl"), jsonl).unwrap();

    let db_path = dir.path().join("test.db");
    let db = Database::open(&db_path).unwrap();
    for f in &scan_projects(&projects_dir).unwrap() {
        db.ingest_file(f).unwrap();
    }
    drop(db);
    (db_path, dir)
}

#[test]
fn test_scan_ingest_aggregate() {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = dir.path().join("projects");
    let project_dir = projects_dir.join("-Users-test-myproject");
    fs::create_dir_all(&project_dir).unwrap();

    // Write sample JSONL with a user message (session start) and an assistant response
    let jsonl = concat!(
        r#"{"uuid":"uuid1","sessionId":"sess1","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#,
        "\n",
        r#"{"uuid":"uuid2","sessionId":"sess1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}"#,
    );
    fs::write(project_dir.join("sess1.jsonl"), jsonl).unwrap();

    // Scan
    let files = scan_projects(&projects_dir).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].project, "myproject");
    assert_eq!(files[0].session_id, "sess1");

    // Ingest into SQLite
    let db_path = dir.path().join("test.db");
    let db = Database::open(&db_path).unwrap();
    for f in &files {
        db.ingest_file(f).unwrap();
    }
    drop(db);

    // Aggregate and verify
    let agg = Aggregator::open(&db_path).unwrap();
    let stats = agg.dashboard_stats().unwrap();
    assert_eq!(stats.total_sessions, 1);
    assert_eq!(stats.total_messages, 2); // 1 user + 1 assistant
    assert!(stats.spend_all_time > 0.0, "Expected non-zero spend");

    let models = agg.model_breakdown().unwrap();
    assert_eq!(models.len(), 1);
    assert!(models[0].model.contains("sonnet"));
    assert_eq!(models[0].input_tokens, 1000);
    assert_eq!(models[0].output_tokens, 500);
    assert_eq!(models[0].cache_read, 200);
    assert_eq!(models[0].call_count, 1);

    let sessions = agg.sessions_list(10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].project, "myproject");
    assert_eq!(sessions[0].total_tokens, 1500); // 1000 + 500

    let cache_ratio = agg.cache_hit_ratio().unwrap();
    assert!(cache_ratio > 0.0, "Expected non-zero cache ratio");
}

#[test]
fn test_multiple_sessions_across_projects() {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = dir.path().join("projects");

    // Two different projects
    let proj_a = projects_dir.join("-Users-test-projectA");
    let proj_b = projects_dir.join("-Users-test-projectB");
    fs::create_dir_all(&proj_a).unwrap();
    fs::create_dir_all(&proj_b).unwrap();

    let jsonl_a = concat!(
        r#"{"uuid":"a1","sessionId":"sA","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#,
        "\n",
        r#"{"uuid":"a2","sessionId":"sA","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-opus-4-20250514","role":"assistant","usage":{"input_tokens":500,"output_tokens":200,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#,
    );
    let jsonl_b = concat!(
        r#"{"uuid":"b1","sessionId":"sB","type":"user","timestamp":"2025-01-15T11:00:00Z","parentUuid":null,"message":{"role":"user"}}"#,
        "\n",
        r#"{"uuid":"b2","sessionId":"sB","type":"assistant","timestamp":"2025-01-15T11:00:01Z","message":{"model":"claude-3-5-haiku-20241022","role":"assistant","usage":{"input_tokens":2000,"output_tokens":1000,"cache_read_input_tokens":500,"cache_creation_input_tokens":0}}}"#,
    );
    fs::write(proj_a.join("sA.jsonl"), jsonl_a).unwrap();
    fs::write(proj_b.join("sB.jsonl"), jsonl_b).unwrap();

    let files = scan_projects(&projects_dir).unwrap();
    assert_eq!(files.len(), 2);

    let db_path = dir.path().join("test2.db");
    let db = Database::open(&db_path).unwrap();
    for f in &files {
        db.ingest_file(f).unwrap();
    }
    drop(db);

    let agg = Aggregator::open(&db_path).unwrap();
    let stats = agg.dashboard_stats().unwrap();
    assert_eq!(stats.total_sessions, 2);

    let models = agg.model_breakdown().unwrap();
    assert_eq!(models.len(), 2);

    let project_costs = agg.project_costs().unwrap();
    assert_eq!(project_costs.len(), 2);
}

#[test]
fn test_incremental_ingest() {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = dir.path().join("projects");
    let project_dir = projects_dir.join("-Users-test-inc");
    fs::create_dir_all(&project_dir).unwrap();

    let jsonl_path = project_dir.join("sess1.jsonl");

    // First write: just the user message
    let line1 = r#"{"uuid":"u1","sessionId":"s1","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#;
    fs::write(&jsonl_path, format!("{}\n", line1)).unwrap();

    let db_path = dir.path().join("test_inc.db");
    let db = Database::open(&db_path).unwrap();

    let files = scan_projects(&projects_dir).unwrap();
    db.ingest_file(&files[0]).unwrap();

    // Verify 1 session, 1 message
    {
        let agg = Aggregator::open(&db_path).unwrap();
        let stats = agg.dashboard_stats().unwrap();
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.total_messages, 1);
    }

    // Append an assistant response
    let line2 = r#"{"uuid":"u2","sessionId":"s1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":800,"output_tokens":300,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&jsonl_path)
        .unwrap();
    use std::io::Write;
    writeln!(file, "{}", line2).unwrap();

    // Re-ingest (should only process the new line)
    let files = scan_projects(&projects_dir).unwrap();
    db.ingest_file(&files[0]).unwrap();
    drop(db);

    let agg = Aggregator::open(&db_path).unwrap();
    let stats = agg.dashboard_stats().unwrap();
    assert_eq!(stats.total_sessions, 1);
    assert_eq!(stats.total_messages, 2); // Now 2 messages
    assert!(stats.spend_all_time > 0.0);
}

#[test]
fn test_efficiency_stats() {
    let jsonl = concat!(
        r#"{"uuid":"u1","sessionId":"s1","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#,
        "\n",
        r#"{"uuid":"u2","sessionId":"s1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":10000,"output_tokens":2000,"cache_read_input_tokens":5000,"cache_creation_input_tokens":0}}}"#,
    );
    let (db_path, _dir) = setup_fixture(jsonl);
    let agg = Aggregator::open(&db_path).unwrap();

    let eff = agg.efficiency_stats().unwrap();
    // Data is from 2025-01-15, outside the "last 7 days" window, so
    // tokens_per_dollar (which uses the current week) will be 0.
    // Just verify it doesn't panic and returns sensible values.
    assert!(eff.tokens_per_dollar >= 0.0, "tokens_per_dollar should be non-negative");
    assert!(eff.cache_savings_alltime >= 0.0, "cache savings should be >= 0");
}

#[test]
fn test_dashboard_stats_token_totals() {
    let jsonl = concat!(
        r#"{"uuid":"u1","sessionId":"s1","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#,
        "\n",
        r#"{"uuid":"u2","sessionId":"s1","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}"#,
    );
    let (db_path, _dir) = setup_fixture(jsonl);
    let agg = Aggregator::open(&db_path).unwrap();

    let stats = agg.dashboard_stats().unwrap();
    assert_eq!(stats.total_input_tokens, 1000);
    assert_eq!(stats.total_output_tokens, 500);
    assert_eq!(stats.total_cache_read, 200);
    assert!(stats.spend_all_time > 0.0);
    // Historical data won't match "today" or "this week" due to fixed timestamps
    assert_eq!(stats.spend_today, 0.0, "2025-01-15 data is not today");
}

#[test]
fn test_project_costs_aggregation() {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = dir.path().join("projects");

    // Two projects with different costs
    let proj_a = projects_dir.join("-Users-test-alpha");
    let proj_b = projects_dir.join("-Users-test-beta");
    fs::create_dir_all(&proj_a).unwrap();
    fs::create_dir_all(&proj_b).unwrap();

    let jsonl_a = concat!(
        r#"{"uuid":"a1","sessionId":"sA","type":"user","timestamp":"2025-01-15T10:00:00Z","parentUuid":null,"message":{"role":"user"}}"#,
        "\n",
        r#"{"uuid":"a2","sessionId":"sA","type":"assistant","timestamp":"2025-01-15T10:00:01Z","message":{"model":"claude-opus-4-20250514","role":"assistant","usage":{"input_tokens":5000,"output_tokens":2000,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#,
    );
    let jsonl_b = concat!(
        r#"{"uuid":"b1","sessionId":"sB","type":"user","timestamp":"2025-01-15T11:00:00Z","parentUuid":null,"message":{"role":"user"}}"#,
        "\n",
        r#"{"uuid":"b2","sessionId":"sB","type":"assistant","timestamp":"2025-01-15T11:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#,
    );
    fs::write(proj_a.join("sA.jsonl"), jsonl_a).unwrap();
    fs::write(proj_b.join("sB.jsonl"), jsonl_b).unwrap();

    let db_path = dir.path().join("test_proj.db");
    let db = Database::open(&db_path).unwrap();
    for f in &scan_projects(&projects_dir).unwrap() {
        db.ingest_file(f).unwrap();
    }
    drop(db);

    let agg = Aggregator::open(&db_path).unwrap();
    let costs = agg.project_costs().unwrap();
    assert_eq!(costs.len(), 2);
    // Costs should sum to ~100%
    let pct_sum: f64 = costs.iter().map(|c| c.percentage).sum();
    assert!((pct_sum - 100.0).abs() < 0.1, "percentages should sum to 100, got {pct_sum}");
    // Each project should have positive cost
    assert!(costs.iter().all(|c| c.cost > 0.0));
}

#[test]
fn test_sidechain_rollup_end_to_end() {
    // Orchestrator spawns one subagent via Task; the subagent's assistant
    // turn is marked isSidechain=true. The session summary should expose
    // subagent_count=1 and sidechain_cost > 0.
    let jsonl = concat!(
        // 1. Initial user prompt (opens session)
        r#"{"uuid":"u1","sessionId":"sess-x","type":"user","timestamp":"2025-02-01T12:00:00Z","parentUuid":null,"isSidechain":false,"message":{"role":"user"}}"#,
        "\n",
        // 2. Orchestrator assistant turn with a Task tool_use spawning a subagent
        r#"{"uuid":"u2","sessionId":"sess-x","type":"assistant","timestamp":"2025-02-01T12:00:01Z","parentUuid":"u1","isSidechain":false,"message":{"id":"msg_orch","model":"claude-opus-4-7","role":"assistant","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0},"content":[{"type":"tool_use","id":"toolu_sub","name":"Task","input":{"subagent_type":"Explore","prompt":"find call sites"}}]}}"#,
        "\n",
        // 3. Sidechain user message (subagent prompt)
        r#"{"uuid":"u3","sessionId":"sess-x","type":"user","timestamp":"2025-02-01T12:00:02Z","parentUuid":"u2","isSidechain":true,"message":{"role":"user"}}"#,
        "\n",
        // 4. Sidechain assistant turn (subagent's work)
        r#"{"uuid":"u4","sessionId":"sess-x","type":"assistant","timestamp":"2025-02-01T12:00:03Z","parentUuid":"u3","isSidechain":true,"message":{"id":"msg_sub","model":"claude-haiku-4-5-20251001","role":"assistant","usage":{"input_tokens":500,"output_tokens":200,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#,
    );
    let (db_path, _dir) = setup_fixture(jsonl);

    let agg = Aggregator::open(&db_path).unwrap();
    let sessions = agg.sessions_list(10).unwrap();
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.subagent_count, 1, "exactly one Task spawn");
    assert!(s.sidechain_cost > 0.0, "subagent assistant turn must accrue cost");
    assert!(
        s.total_cost > s.sidechain_cost,
        "total cost includes orchestrator on top of sidechain",
    );
}

#[test]
fn test_sidechain_rollup_zero_for_flat_sessions() {
    // A vanilla orchestrator-only session (no Task spawns, no sidechains)
    // must report subagent_count=0 and sidechain_cost=0.
    let jsonl = concat!(
        r#"{"uuid":"u1","sessionId":"sess-y","type":"user","timestamp":"2025-02-01T12:00:00Z","parentUuid":null,"message":{"role":"user"}}"#,
        "\n",
        r#"{"uuid":"u2","sessionId":"sess-y","type":"assistant","timestamp":"2025-02-01T12:00:01Z","message":{"model":"claude-sonnet-4-6-20250514","role":"assistant","usage":{"input_tokens":50,"output_tokens":50}}}"#,
    );
    let (db_path, _dir) = setup_fixture(jsonl);
    let agg = Aggregator::open(&db_path).unwrap();
    let sessions = agg.sessions_list(10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].subagent_count, 0);
    assert_eq!(sessions[0].sidechain_cost, 0.0);
}

#[test]
fn test_empty_db_no_panics() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("empty.db");
    let db = Database::open(&db_path).unwrap();
    drop(db);

    let agg = Aggregator::open(&db_path).unwrap();
    let stats = agg.dashboard_stats().unwrap();
    assert_eq!(stats.total_sessions, 0);
    assert_eq!(stats.total_messages, 0);
    assert_eq!(stats.spend_all_time, 0.0);

    let models = agg.model_breakdown().unwrap();
    assert!(models.is_empty());

    let sessions = agg.sessions_list(10).unwrap();
    assert!(sessions.is_empty());

    let eff = agg.efficiency_stats().unwrap();
    assert_eq!(eff.tokens_per_dollar, 0.0);

    let projects = agg.project_costs().unwrap();
    assert!(projects.is_empty());

    let cache = agg.cache_hit_ratio().unwrap();
    assert_eq!(cache, 0.0);
}
