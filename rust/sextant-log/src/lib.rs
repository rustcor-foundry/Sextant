use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use rusqlite::{params, Connection, Result};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum LogStatus {
    Success,
    Failure(String),
    Aborted,
    AwaitingConsent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub persona_id: String,
    pub intent: String,
    pub plan_json: String,
    pub signature: String,
    pub consent_signature: Option<String>,
    pub status: LogStatus,
}

pub struct CaptainsLog {
    conn: Connection,
}

impl CaptainsLog {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS audit_trail (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                persona_id TEXT NOT NULL,
                intent TEXT NOT NULL,
                plan_json TEXT NOT NULL,
                signature TEXT NOT NULL,
                consent_signature TEXT,
                status TEXT NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS audit_trail (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                persona_id TEXT NOT NULL,
                intent TEXT NOT NULL,
                plan_json TEXT NOT NULL,
                signature TEXT NOT NULL,
                consent_signature TEXT,
                status TEXT NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn record(&self, entry: &LogEntry) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_trail (id, timestamp, persona_id, intent, plan_json, signature, consent_signature, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.id.to_string(),
                entry.timestamp.to_rfc3339(),
                entry.persona_id,
                entry.intent,
                entry.plan_json,
                entry.signature,
                entry.consent_signature,
                serde_json::to_string(&entry.status).unwrap(),
            ],
        )?;
        Ok(())
    }

    pub fn update_status(&self, id: &Uuid, status: LogStatus, consent_sig: Option<String>) -> Result<()> {
        self.conn.execute(
            "UPDATE audit_trail SET status = ?1, consent_signature = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&status).unwrap(),
                consent_sig,
                id.to_string(),
            ],
        )?;
        Ok(())
    }

    pub fn get_entries(&self, persona_id: &str, limit: usize) -> Result<Vec<LogEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, persona_id, intent, plan_json, signature, consent_signature, status 
             FROM audit_trail 
             WHERE persona_id = ?1 
             ORDER BY timestamp DESC 
             LIMIT ?2"
        )?;
        let entries = stmt.query_map(params![persona_id, limit], |row| {
            Ok(LogEntry {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?).unwrap().with_timezone(&Utc),
                persona_id: row.get(2)?,
                intent: row.get(3)?,
                plan_json: row.get(4)?,
                signature: row.get(5)?,
                consent_signature: row.get(6)?,
                status: serde_json::from_str(&row.get::<_, String>(7)?).unwrap(),
            })
        })?.collect::<Result<Vec<_>>>()?;
        Ok(entries)
    }
}
