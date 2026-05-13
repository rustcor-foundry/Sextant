use chrono::{DateTime, Utc};
use rusqlite::{params, types::Type, Connection, Result};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::Path;
use uuid::Uuid;

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

    pub fn update_status(
        &self,
        id: &Uuid,
        status: LogStatus,
        consent_sig: Option<String>,
    ) -> Result<()> {
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
        let entries = stmt
            .query_map(params![persona_id, limit], |row| {
                let id_raw: String = row.get(0)?;
                let timestamp_raw: String = row.get(1)?;
                let status_raw: String = row.get(7)?;

                let id = Uuid::parse_str(&id_raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        Type::Text,
                        Box::new(error) as Box<dyn Error + Send + Sync>,
                    )
                })?;
                let timestamp = DateTime::parse_from_rfc3339(&timestamp_raw)
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            Type::Text,
                            Box::new(error) as Box<dyn Error + Send + Sync>,
                        )
                    })?
                    .with_timezone(&Utc);
                let status = serde_json::from_str(&status_raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        7,
                        Type::Text,
                        Box::new(error) as Box<dyn Error + Send + Sync>,
                    )
                })?;

                Ok(LogEntry {
                    id,
                    timestamp,
                    persona_id: row.get(2)?,
                    intent: row.get(3)?,
                    plan_json: row.get(4)?,
                    signature: row.get(5)?,
                    consent_signature: row.get(6)?,
                    status,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_persisted_row_returns_error_instead_of_panicking() {
        let log = CaptainsLog::new_in_memory().expect("log should initialize");
        log.conn
            .execute(
                "INSERT INTO audit_trail (id, timestamp, persona_id, intent, plan_json, signature, consent_signature, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    "not-a-uuid",
                    Utc::now().to_rfc3339(),
                    "persona",
                    "intent",
                    "{}",
                    "sig",
                    Option::<String>::None,
                    serde_json::to_string(&LogStatus::Success).unwrap(),
                ],
            )
            .expect("row insert should succeed");

        let result = log.get_entries("persona", 10);
        assert!(result.is_err());
    }

    #[test]
    fn round_trips_log_entry() {
        let log = CaptainsLog::new_in_memory().expect("log should initialize");
        let entry = LogEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            persona_id: "persona".to_string(),
            intent: "open example".to_string(),
            plan_json: "{}".to_string(),
            signature: "sig".to_string(),
            consent_signature: None,
            status: LogStatus::Success,
        };

        log.record(&entry).expect("record should succeed");
        let entries = log
            .get_entries("persona", 10)
            .expect("query should succeed");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, entry.id);
        assert_eq!(entries[0].intent, entry.intent);
    }
}
