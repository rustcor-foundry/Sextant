use rusqlite::{params, Connection, Result};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use url::Url;
use sextant_engine::DistilledPage;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WakeEntry {
    pub id: Uuid,
    pub persona_id: String,
    pub timestamp: DateTime<Utc>,
    pub url: Url,
    pub title: String,
    pub content: String,
    pub semantic_map_json: String,
    pub metadata_json: String,
    pub importance: f32, // 0.0 to 1.0
    pub usage_count: u32,
    pub last_accessed: Option<DateTime<Utc>>,
    pub embedding: Option<Vec<f32>>,
}

fn bincode_serialize(vec: &Vec<f32>) -> Vec<u8> {
    // Simple manual serialization for the prototype
    let mut res = Vec::with_capacity(vec.len() * 4);
    for f in vec {
        res.extend_from_slice(&f.to_le_bytes());
    }
    res
}

fn bincode_deserialize(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 { return None; }
    let mut res = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let arr: [u8; 4] = chunk.try_into().ok()?;
        res.push(f32::from_le_bytes(arr));
    }
    Some(res)
}

fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    if v1.len() != v2.len() || v1.is_empty() {
        return 0.0;
    }
    let dot_product: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
    let norm_v1: f32 = v1.iter().map(|a| a * a).sum::<f32>().sqrt();
    let norm_v2: f32 = v2.iter().map(|a| a * a).sum::<f32>().sqrt();
    
    if norm_v1 == 0.0 || norm_v2 == 0.0 {
        return 0.0;
    }
    dot_product / (norm_v1 * norm_v2)
}

fn normalize_fts_query(query: &str) -> String {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct DigitalWake {
    conn: Connection,
}

impl DigitalWake {
    /// Initializes the Wake memory. In production, this would be a persistent file.
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let wake = Self { conn };
        wake.setup_tables()?;
        Ok(wake)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let wake = Self { conn };
        wake.setup_tables()?;
        Ok(wake)
    }

    fn setup_tables(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS wake_entries (
                id TEXT PRIMARY KEY,
                persona_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                semantic_map_json TEXT,
                metadata_json TEXT,
                importance REAL DEFAULT 0.5,
                usage_count INTEGER DEFAULT 0,
                last_accessed TEXT,
                embedding BLOB
            )",
            [],
        )?;

        // Associative Memory Table: Links related memories
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS wake_associations (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                strength REAL NOT NULL,
                association_type TEXT NOT NULL,
                PRIMARY KEY (source_id, target_id)
            )",
            [],
        )?;
        
        // FTS5 Virtual Table for high-performance text search
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS wake_fts USING fts5(
                title,
                content,
                content='wake_entries',
                content_rowid='rowid'
            )",
            [],
        )?;

        // Triggers to keep FTS in sync
        self.conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS wake_ai AFTER INSERT ON wake_entries BEGIN
                INSERT INTO wake_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
            END;
            CREATE TRIGGER IF NOT EXISTS wake_ad AFTER DELETE ON wake_entries BEGIN
                INSERT INTO wake_fts(wake_fts, rowid, title, content) VALUES('delete', old.rowid, old.title, old.content);
            END;
            CREATE TRIGGER IF NOT EXISTS wake_au AFTER UPDATE ON wake_entries BEGIN
                INSERT INTO wake_fts(wake_fts, rowid, title, content) VALUES('delete', old.rowid, old.title, old.content);
                INSERT INTO wake_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
            END;"
        )?;

        // Index for fast semantic/temporal retrieval
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_wake_timestamp ON wake_entries(timestamp)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_wake_persona ON wake_entries(persona_id)",
            [],
        )?;
        Ok(())
    }

    /// Records a new experience into the Digital Wake.
    pub fn record(&self, persona_id: &str, page: &DistilledPage, embedding: Option<Vec<f32>>) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();
        let semantic_map = serde_json::to_string(&page.semantic_map).unwrap_or_default();
        let metadata = serde_json::to_string(&page.metadata).unwrap_or_default();
        
        // Importance heuristic:
        // - Longer content is generally more important (up to a point)
        // - Presence of specific metadata (e.g., 'author', 'priority') increases importance
        let mut importance: f32 = 0.3;
        
        if page.content.len() > 1000 { importance += 0.2; }
        if page.content.len() > 5000 { importance += 0.2; }
        if page.metadata.contains_key("author") { importance += 0.1; }
        if page.metadata.get("priority").map(|v| v == "high").unwrap_or(false) { importance += 0.2; }
        
        importance = importance.min(1.0_f32);

        let embedding_blob = embedding.map(|v| bincode_serialize(&v));

        self.conn.execute(
            "INSERT INTO wake_entries (id, persona_id, timestamp, url, title, content, semantic_map_json, metadata_json, importance, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                persona_id,
                timestamp,
                page.url.to_string(),
                page.title,
                page.content,
                semantic_map,
                metadata,
                importance,
                embedding_blob
            ],
        )?;
        Ok(())
    }

    /// Performs a hybrid search (FTS5 + Semantic Re-ranking) isolated by persona.
    pub fn search_hybrid(&self, persona_id: &str, query: &str, query_vector: Option<&[f32]>) -> Result<Vec<WakeEntry>> {
        let normalized_query = normalize_fts_query(query);
        if normalized_query.is_empty() {
            return self.get_recent(persona_id, 10);
        }

        // 1. Fetch candidates using FTS5 + Persona Filter
        let mut stmt = self.conn.prepare(
            "SELECT wake_entries.id,
                    wake_entries.persona_id,
                    wake_entries.timestamp,
                    wake_entries.url,
                    wake_entries.title,
                    wake_entries.content,
                    wake_entries.semantic_map_json,
                    wake_entries.metadata_json,
                    wake_entries.importance,
                    wake_entries.usage_count,
                    wake_entries.last_accessed,
                    wake_entries.embedding
             FROM wake_entries 
             JOIN wake_fts ON wake_entries.rowid = wake_fts.rowid
             WHERE wake_entries.persona_id = ?1 AND wake_fts MATCH ?2 
             ORDER BY rank LIMIT 50"
        )?;
        
        let entries = stmt.query_map(params![persona_id, normalized_query], |row| {
            self.map_row_to_entry(row)
        })?;

        let mut candidates = Vec::new();
        for entry in entries {
            candidates.push(entry?);
        }

        // 2. If a query vector is provided, re-rank candidates using cosine similarity
        if let Some(q_vec) = query_vector {
            candidates.sort_by(|a, b| {
                let score_a = a.embedding.as_ref().map(|v| cosine_similarity(v, q_vec)).unwrap_or(0.0);
                let score_b = b.embedding.as_ref().map(|v| cosine_similarity(v, q_vec)).unwrap_or(0.0);
                score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        // 3. Increment usage count and update importance for top results
        let now = Utc::now().to_rfc3339();
        for entry in candidates.iter().take(5) {
            self.conn.execute(
                "UPDATE wake_entries 
                 SET usage_count = usage_count + 1, 
                     last_accessed = ?1,
                     importance = MIN(1.0, importance + 0.05)
                 WHERE id = ?2",
                params![now, entry.id.to_string()],
            )?;
        }

        // 4. Return top 10
        candidates.truncate(10);
        Ok(candidates)
    }

    /// Performs a simple text search isolated by persona.
    pub fn search(&self, persona_id: &str, query: &str) -> Result<Vec<WakeEntry>> {
        self.search_hybrid(persona_id, query, None)
    }

    fn get_recent(&self, persona_id: &str, limit: usize) -> Result<Vec<WakeEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, persona_id, timestamp, url, title, content, semantic_map_json, metadata_json, importance, usage_count, last_accessed, embedding 
             FROM wake_entries 
             WHERE persona_id = ?1
             ORDER BY timestamp DESC LIMIT ?2"
        )?;
        
        let entries = stmt.query_map(params![persona_id, limit], |row| {
            self.map_row_to_entry(row)
        })?;

        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }
        Ok(result)
    }

    fn map_row_to_entry(&self, row: &rusqlite::Row) -> Result<WakeEntry> {
        let id_str: String = row.get(0)?;
        let persona_id: String = row.get(1)?;
        let ts_str: String = row.get(2)?;
        let url_str: String = row.get(3)?;
        let last_acc_str: Option<String> = row.get(10)?;
        let embedding_blob: Option<Vec<u8>> = row.get(11)?;
        
        let embedding = embedding_blob.and_then(|b| bincode_deserialize(&b));
        let last_accessed = last_acc_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        Ok(WakeEntry {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::nil()),
            persona_id,
            timestamp: DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            url: Url::parse(&url_str).unwrap_or_else(|_| Url::parse("about:blank").unwrap()),
            title: row.get(4)?,
            content: row.get(5)?,
            semantic_map_json: row.get(6)?,
            metadata_json: row.get(7)?,
            importance: row.get(8)?,
            usage_count: row.get(9)?,
            last_accessed,
            embedding,
        })
    }

    /// Creates associations between semantically related memories.
    pub fn associate(&self, persona_id: &str) -> Result<usize> {
        let entries = self.get_recent(persona_id, 50)?;
        let mut associations_created = 0;

        for i in 0..entries.len() {
            for j in i+1..entries.len() {
                let e1 = &entries[i];
                let e2 = &entries[j];
                
                if let (Some(v1), Some(v2)) = (&e1.embedding, &e2.embedding) {
                    let similarity = cosine_similarity(v1, v2);
                    if similarity > 0.8 {
                        self.conn.execute(
                            "INSERT OR REPLACE INTO wake_associations (source_id, target_id, strength, association_type)
                             VALUES (?1, ?2, ?3, ?4)",
                            params![e1.id.to_string(), e2.id.to_string(), similarity, "semantic"],
                        )?;
                        associations_created += 1;
                    }
                }
            }
        }
        Ok(associations_created)
    }

    /// Retrieves associated memories for a given entry.
    pub fn get_associations(&self, entry_id: &Uuid) -> Result<Vec<WakeEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, persona_id, timestamp, url, title, content, semantic_map_json, metadata_json, importance, usage_count, last_accessed, embedding 
             FROM wake_entries 
             WHERE id IN (
                 SELECT target_id FROM wake_associations WHERE source_id = ?1
                 UNION
                 SELECT source_id FROM wake_associations WHERE target_id = ?1
             )
             ORDER BY importance DESC LIMIT 5"
        )?;

        let entries = stmt.query_map(params![entry_id.to_string()], |row| {
            self.map_row_to_entry(row)
        })?;

        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }
        Ok(result)
    }

    /// "Distills" the wake by removing low-importance or redundant entries.
    /// This is the "Memory Consolidation" phase.
    pub fn consolidate(&self, persona_id: &str) -> Result<usize> {
        let mut deleted_count = 0;

        // 1. Build associations before pruning
        let _ = self.associate(persona_id);

        // 2. Temporal Decay: Reduce importance of all entries in this persona over time
        // We simulate this by reducing importance by 5% for entries older than 7 days
        self.conn.execute(
            "UPDATE wake_entries 
             SET importance = importance * 0.95 
             WHERE persona_id = ?1 AND timestamp < datetime('now', '-7 days')",
            params![persona_id],
        )?;

        // 2. Remove low-importance old entries
        deleted_count += self.conn.execute(
            "DELETE FROM wake_entries 
             WHERE persona_id = ?1 AND timestamp < datetime('now', '-30 days') 
             AND importance < 0.2",
            params![persona_id],
        )?;

        // 3. Semantic Deduplication
        // Fetch recent entries for this persona to check for redundancy
        let entries = self.get_recent(persona_id, 100)?;
        let mut to_delete = Vec::new();

        for i in 0..entries.len() {
            for j in i+1..entries.len() {
                let e1 = &entries[i];
                let e2 = &entries[j];
                
                if let (Some(v1), Some(v2)) = (&e1.embedding, &e2.embedding) {
                    if cosine_similarity(v1, v2) > 0.95 {
                        // Keep the one with higher importance or the newer one
                        if e1.importance >= e2.importance {
                            to_delete.push(e2.id.to_string());
                        } else {
                            to_delete.push(e1.id.to_string());
                        }
                    }
                }
            }
        }

        for id in to_delete {
            deleted_count += self.conn.execute(
                "DELETE FROM wake_entries WHERE id = ?1",
                params![id],
            )?;
        }

        Ok(deleted_count)
    }

    /// Explicitly prunes a specific memory entry from the wake.
    pub fn prune(&self, entry_id: &Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM wake_entries WHERE id = ?1",
            params![entry_id.to_string()],
        )?;
        self.conn.execute(
            "DELETE FROM wake_associations WHERE source_id = ?1 OR target_id = ?1",
            params![entry_id.to_string()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_wake_persistence() {
        let wake = DigitalWake::new_in_memory().unwrap();
        let persona_id = "test-persona";
        let page = DistilledPage {
            title: "Test Page".to_string(),
            url: Url::parse("https://example.com").unwrap(),
            content: "This is a test of the digital wake system.".to_string(),
            semantic_map: vec![],
            metadata: HashMap::new(),
        };

        wake.record(persona_id, &page, None).unwrap();
        let results = wake.search(persona_id, "digital wake").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Test Page");
        assert_eq!(results[0].persona_id, persona_id);
    }
}
