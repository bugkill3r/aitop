use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

use super::parser::{ParsedMessage, ParsedSession};
use super::pricing::PricingRegistry;
use super::scanner::SessionFile;

pub struct Database {
    conn: Connection,
    pricing: PricingRegistry,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_pricing(path, PricingRegistry::builtin())
    }

    pub fn open_with_pricing(path: &Path, pricing: PricingRegistry) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let db = Database { conn, pricing };
        db.create_tables()?;
        db.migrate()?;
        Ok(db)
    }

    fn create_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY,
                project     TEXT NOT NULL,
                started_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                model       TEXT,
                version     TEXT,
                provider    TEXT DEFAULT 'claude'
            );

            CREATE TABLE IF NOT EXISTS messages (
                id              TEXT PRIMARY KEY,
                session_id      TEXT NOT NULL,
                type            TEXT NOT NULL,
                timestamp       TEXT NOT NULL,
                model           TEXT,
                input_tokens    INTEGER DEFAULT 0,
                output_tokens   INTEGER DEFAULT 0,
                cache_read      INTEGER DEFAULT 0,
                cache_creation  INTEGER DEFAULT 0,
                cost_usd        REAL DEFAULT 0.0,
                provider        TEXT DEFAULT 'claude'
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
            CREATE INDEX IF NOT EXISTS idx_messages_model ON messages(model);

            CREATE TABLE IF NOT EXISTS file_index (
                path        TEXT PRIMARY KEY,
                last_offset INTEGER DEFAULT 0,
                last_mtime  TEXT
            );

            CREATE TABLE IF NOT EXISTS metadata (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    /// Run schema migrations.
    fn migrate(&self) -> Result<()> {
        let version = self.get_schema_version()?;

        if version < 2 {
            // Add provider column to sessions and messages if not already present
            let _ = self.conn.execute_batch(
                "ALTER TABLE sessions ADD COLUMN provider TEXT DEFAULT 'claude';"
            );
            let _ = self.conn.execute_batch(
                "ALTER TABLE messages ADD COLUMN provider TEXT DEFAULT 'claude';"
            );
            self.set_schema_version(2)?;
        }

        Ok(())
    }

    fn get_schema_version(&self) -> Result<i64> {
        let result = self.conn.query_row(
            "SELECT value FROM metadata WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(val.parse::<i64>().unwrap_or(1)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(1),
            Err(e) => Err(e.into()),
        }
    }

    fn set_schema_version(&self, version: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO metadata (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![version.to_string()],
        )?;
        Ok(())
    }

    /// Get the last parsed offset for a file, or 0 if never parsed.
    pub fn get_file_offset(&self, path: &str) -> Result<u64> {
        let result = self.conn.query_row(
            "SELECT last_offset FROM file_index WHERE path = ?1",
            params![path],
            |row| row.get::<_, i64>(0),
        );
        match result {
            Ok(offset) => Ok(offset as u64),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    /// Update the file index after parsing.
    pub fn set_file_offset(&self, path: &str, offset: u64, mtime: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO file_index (path, last_offset, last_mtime) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET last_offset = ?2, last_mtime = ?3",
            params![path, offset as i64, mtime],
        )?;
        Ok(())
    }

    pub fn upsert_session(&self, session: &ParsedSession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (id, project, started_at, updated_at, model, version, provider)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                updated_at = MAX(sessions.updated_at, ?4),
                model = COALESCE(?5, sessions.model),
                version = COALESCE(?6, sessions.version)",
            params![
                session.id,
                session.project,
                session.started_at,
                session.updated_at,
                session.model,
                session.version,
                session.provider,
            ],
        )?;
        Ok(())
    }

    pub fn insert_message(&self, msg: &ParsedMessage) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO messages (id, session_id, type, timestamp, model, input_tokens, output_tokens, cache_read, cache_creation, cost_usd, provider)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                msg.uuid,
                msg.session_id,
                msg.msg_type,
                msg.timestamp,
                msg.model,
                msg.input_tokens,
                msg.output_tokens,
                msg.cache_read,
                msg.cache_creation,
                msg.cost_usd,
                msg.provider,
            ],
        )?;
        Ok(())
    }

    pub fn get_last_checked_at(&self) -> Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT value FROM metadata WHERE key = 'last_checked_at'",
            [],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_last_checked_at(&self, ts: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO metadata (key, value) VALUES ('last_checked_at', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![ts],
        )?;
        Ok(())
    }

    /// Ingest a JSONL file by its raw path string.
    /// Derives session_id and project from the path structure.
    /// Returns the project name (for live indicator) and new offset.
    pub fn ingest_file_by_path(&self, path: &str) -> Result<(String, u64)> {
        let file_path = std::path::PathBuf::from(path);
        let session_id = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        // Derive project name from parent directory name
        let project = file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(super::parser::decode_project_name)
            .unwrap_or_else(|| "unknown".to_string());

        let sf = SessionFile {
            path: file_path,
            session_id,
            project: project.clone(),
        };

        let offset = self.ingest_file(&sf)?;
        Ok((project, offset))
    }

    /// Ingest a JSONL file, starting from the given byte offset.
    pub fn ingest_file(&self, file: &SessionFile) -> Result<u64> {
        let path_str = file.path.to_string_lossy().to_string();
        let offset = self.get_file_offset(&path_str)?;

        let content = std::fs::read(&file.path)?;
        if (offset as usize) >= content.len() {
            return Ok(offset);
        }

        let new_content = &content[offset as usize..];
        let text = String::from_utf8_lossy(new_content);

        let tx = self.conn.unchecked_transaction()?;

        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some((session, message)) =
                super::parser::parse_jsonl_line(line, &file.project, &self.pricing)
            {
                if let Some(s) = session {
                    tx.execute(
                        "INSERT INTO sessions (id, project, started_at, updated_at, model, version, provider)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                         ON CONFLICT(id) DO UPDATE SET
                            updated_at = MAX(sessions.updated_at, ?4),
                            model = COALESCE(?5, sessions.model),
                            version = COALESCE(?6, sessions.version)",
                        params![s.id, s.project, s.started_at, s.updated_at, s.model, s.version, s.provider],
                    )?;
                }
                if let Some(m) = message {
                    // Update session's model and updated_at
                    if let Some(ref model) = m.model {
                        tx.execute(
                            "UPDATE sessions SET model = ?1, updated_at = MAX(updated_at, ?2) WHERE id = ?3",
                            params![model, m.timestamp, m.session_id],
                        )?;
                    }
                    tx.execute(
                        "INSERT OR IGNORE INTO messages (id, session_id, type, timestamp, model, input_tokens, output_tokens, cache_read, cache_creation, cost_usd, provider)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                        params![m.uuid, m.session_id, m.msg_type, m.timestamp, m.model, m.input_tokens, m.output_tokens, m.cache_read, m.cache_creation, m.cost_usd, m.provider],
                    )?;
                }
            }
        }

        let new_offset = content.len() as u64;
        let mtime = std::fs::metadata(&file.path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();

        tx.execute(
            "INSERT INTO file_index (path, last_offset, last_mtime) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET last_offset = ?2, last_mtime = ?3",
            params![path_str, new_offset as i64, mtime],
        )?;

        tx.commit()?;
        Ok(new_offset)
    }

    /// Ingest pre-parsed session and messages (used by Gemini/OpenClaw parsers).
    /// Uses file mtime to skip files that haven't changed since last ingest.
    pub fn ingest_parsed(
        &self,
        path: &std::path::Path,
        session: Option<&ParsedSession>,
        messages: &[ParsedMessage],
    ) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();

        // Check mtime to skip unchanged files
        let mtime = std::fs::metadata(path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();

        let last_mtime = self.conn.query_row(
            "SELECT last_mtime FROM file_index WHERE path = ?1",
            params![path_str],
            |row| row.get::<_, String>(0),
        );
        if let Ok(ref saved_mtime) = last_mtime {
            if saved_mtime == &mtime {
                return Ok(()); // File hasn't changed
            }
        }

        let tx = self.conn.unchecked_transaction()?;

        if let Some(s) = session {
            tx.execute(
                "INSERT INTO sessions (id, project, started_at, updated_at, model, version, provider)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    updated_at = MAX(sessions.updated_at, ?4),
                    model = COALESCE(?5, sessions.model),
                    version = COALESCE(?6, sessions.version)",
                params![s.id, s.project, s.started_at, s.updated_at, s.model, s.version, s.provider],
            )?;
        }

        for m in messages {
            if let Some(ref model) = m.model {
                tx.execute(
                    "UPDATE sessions SET model = ?1, updated_at = MAX(updated_at, ?2) WHERE id = ?3",
                    params![model, m.timestamp, m.session_id],
                )?;
            }
            tx.execute(
                "INSERT OR IGNORE INTO messages (id, session_id, type, timestamp, model, input_tokens, output_tokens, cache_read, cache_creation, cost_usd, provider)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![m.uuid, m.session_id, m.msg_type, m.timestamp, m.model, m.input_tokens, m.output_tokens, m.cache_read, m.cache_creation, m.cost_usd, m.provider],
            )?;
        }

        tx.execute(
            "INSERT INTO file_index (path, last_offset, last_mtime) VALUES (?1, 0, ?2)
             ON CONFLICT(path) DO UPDATE SET last_mtime = ?2",
            params![path_str, mtime],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Ingest a Gemini session JSON file.
    pub fn ingest_gemini_file(&self, file: &SessionFile) -> Result<()> {
        let (session, messages) = super::gemini::parse_gemini_session(
            &file.path,
            &file.project,
            &self.pricing,
        )?;
        self.ingest_parsed(&file.path, Some(&session), &messages)
    }

    /// Ingest an OpenClaw JSONL session file.
    pub fn ingest_openclaw_file(&self, file: &SessionFile) -> Result<()> {
        let (session, messages) = super::openclaw::parse_openclaw_file(
            &file.path,
            &file.project,
            &self.pricing,
        )?;
        self.ingest_parsed(&file.path, session.as_ref(), &messages)
    }
}
