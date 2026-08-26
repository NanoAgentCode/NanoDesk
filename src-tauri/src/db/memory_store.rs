use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;
use zerocopy::IntoBytes;

use super::{
    add_rrf_scores, build_fts_prefix_query, clean_or_default, ensure_affected,
    extract_memory_entities, memory_content_hash, memory_query_tokens, memory_vector_table,
    normalize_entity_name, score_memory_relevance, Database,
};
use crate::error::{AppError, AppResult};
use crate::models::{Memory, MemoryDraft, MemoryPatch};

impl Database {
    pub fn list_memories(&self) -> AppResult<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, content, tags_json, enabled, created_at, updated_at
            FROM memories
            ORDER BY enabled DESC, updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn list_enabled_memories(&self) -> AppResult<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, content, tags_json, enabled, created_at, updated_at
            FROM memories
            WHERE enabled = 1
            ORDER BY updated_at DESC
            LIMIT 30
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn list_relevant_memories(
        &self,
        query: &str,
        limit: Option<i64>,
    ) -> AppResult<Vec<Memory>> {
        let limit = limit.unwrap_or(8).clamp(1, 30) as usize;
        let trimmed = query.trim();
        if trimmed.is_empty() {
            let mut memories = self.list_enabled_memories()?;
            memories.truncate(limit);
            return Ok(memories);
        }

        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, content, tags_json, enabled, created_at, updated_at
            FROM memories
            WHERE enabled = 1
            ORDER BY updated_at DESC
            ",
        )?;
        let memories = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        let tokens = memory_query_tokens(trimmed);
        let query_lower = trimmed.to_lowercase();
        let mut scored = memories
            .into_iter()
            .filter_map(|memory| {
                let score = score_memory_relevance(&memory, &query_lower, &tokens);
                (score > 0).then_some((score, memory))
            })
            .collect::<Vec<_>>();

        scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.updated_at.cmp(&left.1.updated_at))
        });
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, memory)| memory).collect())
    }

    pub fn list_memories_missing_embedding(
        &self,
        model: &str,
        limit: i64,
    ) -> AppResult<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT m.id, m.title, m.content, m.tags_json, m.enabled, m.created_at, m.updated_at
            FROM memories m
            WHERE m.enabled = 1
            ORDER BY m.updated_at DESC
            ",
        )?;

        let candidates = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut missing = candidates
            .into_iter()
            .filter(|memory| {
                self.memory_embedding_is_stale(memory, model)
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        missing.truncate(limit.clamp(1, 512) as usize);
        Ok(missing)
    }

    pub fn memory_needs_embedding(&self, memory: &Memory, model: &str) -> AppResult<bool> {
        self.memory_embedding_is_stale(memory, model)
    }

    pub fn upsert_memory_embedding(
        &self,
        memory: &Memory,
        model: &str,
        embedding: &[f32],
    ) -> AppResult<()> {
        if embedding.is_empty() || embedding.len() > 65_536 {
            return Err(AppError::Message(
                "memory embedding dimensions are invalid".to_string(),
            ));
        }

        let dimensions = embedding.len() as i64;
        let table = memory_vector_table(dimensions)?;
        self.conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {table} USING vec0(embedding float[{dimensions}]);"
        ))?;

        let existing = self
            .conn
            .query_row(
                "SELECT vector_rowid, dimensions FROM memory_embeddings WHERE memory_id = ?1",
                params![memory.id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let vector_rowid = existing.map(|value| value.0).unwrap_or_else(|| {
            self.conn
                .query_row(
                    "SELECT COALESCE(MAX(vector_rowid), 0) + 1 FROM memory_embeddings",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(1)
        });

        self.conn.execute_batch("SAVEPOINT memory_vector_upsert")?;
        let result = (|| -> AppResult<()> {
            if let Some((_, old_dimensions)) = existing {
                let old_table = memory_vector_table(old_dimensions)?;
                self.conn.execute(
                    &format!("DELETE FROM {old_table} WHERE rowid = ?1"),
                    params![vector_rowid],
                )?;
            }
            self.conn.execute(
                &format!("INSERT INTO {table}(rowid, embedding) VALUES (?1, ?2)"),
                params![vector_rowid, embedding.as_bytes()],
            )?;
            self.conn.execute(
                "
                INSERT INTO memory_embeddings
                    (vector_rowid, memory_id, model, dimensions, content_hash, indexed_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(memory_id) DO UPDATE SET
                    vector_rowid = excluded.vector_rowid,
                    model = excluded.model,
                    dimensions = excluded.dimensions,
                    content_hash = excluded.content_hash,
                    indexed_at = excluded.indexed_at
                ",
                params![
                    vector_rowid,
                    memory.id,
                    model,
                    dimensions,
                    memory_content_hash(memory),
                    Utc::now().to_rfc3339()
                ],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.conn.execute_batch("RELEASE memory_vector_upsert")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.conn.execute_batch(
                    "ROLLBACK TO memory_vector_upsert; RELEASE memory_vector_upsert;",
                );
                Err(error)
            }
        }
    }

    pub fn search_hybrid_memories(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        query_embedding_model: Option<&str>,
        limit: i64,
    ) -> AppResult<Vec<Memory>> {
        let limit = limit.clamp(1, 30) as usize;
        let candidate_limit = (limit * 6).clamp(24, 180);
        let mut scores = HashMap::<String, f64>::new();

        let lexical = self.list_relevant_memories(query, Some(candidate_limit as i64))?;
        add_rrf_scores(
            &mut scores,
            lexical.iter().map(|memory| memory.id.as_str()),
            1.0,
        );

        if let Some(embedding) = query_embedding.filter(|value| !value.is_empty()) {
            let vector_ids = self.search_memory_vectors(
                embedding,
                query_embedding_model,
                candidate_limit as i64,
            )?;
            add_rrf_scores(&mut scores, vector_ids.iter().map(String::as_str), 1.15);
        }

        let graph_ids = self.search_memory_graph(query, candidate_limit as i64)?;
        add_rrf_scores(&mut scores, graph_ids.iter().map(String::as_str), 0.9);

        let mut ranked = scores.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });

        let always_limit = limit.div_ceil(3).clamp(1, 3);
        let mut memories = self.list_always_personalization_memories(always_limit)?;
        let mut selected_ids = memories
            .iter()
            .map(|memory| memory.id.clone())
            .collect::<std::collections::HashSet<_>>();
        for (id, _) in ranked {
            if memories.len() >= limit {
                break;
            }
            if selected_ids.contains(&id) {
                continue;
            }
            if let Some(memory) = self.get_memory(&id)? {
                if memory.enabled {
                    selected_ids.insert(memory.id.clone());
                    memories.push(memory);
                }
            }
        }
        Ok(memories)
    }

    fn list_always_personalization_memories(&self, limit: usize) -> AppResult<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT m.id, m.title, m.content, m.tags_json, m.enabled, m.created_at, m.updated_at
            FROM memory_entities e
            JOIN memory_entity_links l ON l.entity_id = e.id
            JOIN memories m ON m.id = l.memory_id
            WHERE e.kind = 'tag'
              AND e.normalized_name = 'personalization:always'
              AND m.enabled = 1
            ORDER BY m.updated_at DESC
            LIMIT ?1
            ",
        )?;
        let rows = stmt
            .query_map([limit as i64], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub(super) fn memory_embedding_is_stale(
        &self,
        memory: &Memory,
        model: &str,
    ) -> AppResult<bool> {
        let indexed = self
            .conn
            .query_row(
                "SELECT model, content_hash FROM memory_embeddings WHERE memory_id = ?1",
                params![memory.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(match indexed {
            Some((indexed_model, content_hash)) => {
                indexed_model != model || content_hash != memory_content_hash(memory)
            }
            None => true,
        })
    }

    fn search_memory_vectors(
        &self,
        query_embedding: &[f32],
        model: Option<&str>,
        limit: i64,
    ) -> AppResult<Vec<String>> {
        let dimensions = query_embedding.len() as i64;
        let table = memory_vector_table(dimensions)?;
        let exists = self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(Vec::new());
        }

        let sql = format!(
            "
            SELECT e.memory_id
            FROM {table} v
            JOIN memory_embeddings e ON e.vector_rowid = v.rowid
            JOIN memories m ON m.id = e.memory_id
            WHERE v.embedding MATCH ?1
              AND k = ?2
              AND (?3 IS NULL OR e.model = ?3)
              AND m.enabled = 1
            ORDER BY v.distance
            "
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let ids = stmt
            .query_map(
                params![query_embedding.as_bytes(), limit.clamp(1, 180), model],
                |row| row.get(0),
            )?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    fn search_memory_graph(&self, query: &str, limit: i64) -> AppResult<Vec<String>> {
        let tokens = memory_query_tokens(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let graph_query = tokens
            .iter()
            .take(12)
            .map(|token| format!("\"{}\"*", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut seed_stmt = self.conn.prepare(
            "
            SELECT e.id
            FROM memory_entities_fts f
            JOIN memory_entities e ON e.id = f.id
            WHERE memory_entities_fts MATCH ?1
            LIMIT 24
            ",
        )?;
        let seed_ids = seed_stmt
            .query_map([graph_query], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if seed_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut scores = HashMap::<String, f64>::new();
        let mut direct_stmt = self
            .conn
            .prepare("SELECT memory_id, weight FROM memory_entity_links WHERE entity_id = ?1")?;
        let mut neighbor_stmt = self.conn.prepare(
            "
            SELECT CASE WHEN source_entity_id = ?1 THEN target_entity_id ELSE source_entity_id END,
                   weight
            FROM memory_relations
            WHERE source_entity_id = ?1 OR target_entity_id = ?1
            LIMIT 64
            ",
        )?;
        for seed_id in seed_ids {
            let direct = direct_stmt
                .query_map([&seed_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (memory_id, weight) in direct {
                *scores.entry(memory_id).or_default() += weight;
            }

            let neighbors = neighbor_stmt
                .query_map([&seed_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (neighbor_id, relation_weight) in neighbors {
                let linked = direct_stmt
                    .query_map([neighbor_id], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                for (memory_id, link_weight) in linked {
                    *scores.entry(memory_id).or_default() += relation_weight * link_weight * 0.45;
                }
            }
        }

        let mut ranked = scores.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(limit.clamp(1, 180) as usize);
        Ok(ranked.into_iter().map(|(id, _)| id).collect())
    }

    pub(super) fn rebuild_missing_memory_graphs(&self) -> AppResult<()> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, content, tags_json, enabled, created_at, updated_at
            FROM memories m
            WHERE NOT EXISTS (
                SELECT 1 FROM memory_entity_links l WHERE l.memory_id = m.id
            )
            ",
        )?;
        let memories = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for memory in memories {
            self.sync_memory_graph(&memory)?;
        }
        Ok(())
    }

    pub(super) fn sync_memory_graph(&self, memory: &Memory) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM memory_relations WHERE evidence_memory_id = ?1",
            params![memory.id],
        )?;
        self.conn.execute(
            "DELETE FROM memory_entity_links WHERE memory_id = ?1",
            params![memory.id],
        )?;

        let entities = extract_memory_entities(memory);
        let mut entity_ids = Vec::with_capacity(entities.len());
        for entity in entities {
            let normalized = normalize_entity_name(&entity.name);
            if normalized.is_empty() {
                continue;
            }
            let existing_id = self
                .conn
                .query_row(
                    "SELECT id FROM memory_entities WHERE kind = ?1 AND normalized_name = ?2",
                    params![entity.kind, normalized],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let entity_id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());
            let inserted = self.conn.execute(
                "
                INSERT OR IGNORE INTO memory_entities
                    (id, kind, name, normalized_name, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    entity_id,
                    entity.kind,
                    entity.name,
                    normalized,
                    Utc::now().to_rfc3339()
                ],
            )?;
            if inserted > 0 {
                self.conn.execute(
                    "INSERT INTO memory_entities_fts (id, name) VALUES (?1, ?2)",
                    params![entity_id, entity.name],
                )?;
            }
            self.conn.execute(
                "INSERT OR REPLACE INTO memory_entity_links (memory_id, entity_id, weight) VALUES (?1, ?2, ?3)",
                params![memory.id, entity_id, entity.weight],
            )?;
            entity_ids.push((entity_id, entity.weight));
        }

        for left in 0..entity_ids.len() {
            for right in (left + 1)..entity_ids.len() {
                let (source_id, source_weight) = &entity_ids[left];
                let (target_id, target_weight) = &entity_ids[right];
                self.conn.execute(
                    "
                    INSERT OR REPLACE INTO memory_relations
                        (source_entity_id, target_entity_id, kind, weight, evidence_memory_id)
                    VALUES (?1, ?2, 'co_occurs', ?3, ?4)
                    ",
                    params![
                        source_id,
                        target_id,
                        source_weight.min(*target_weight),
                        memory.id
                    ],
                )?;
            }
        }
        self.remove_orphan_memory_entities()
    }

    fn remove_orphan_memory_entities(&self) -> AppResult<()> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id FROM memory_entities e
            WHERE NOT EXISTS (SELECT 1 FROM memory_entity_links l WHERE l.entity_id = e.id)
              AND NOT EXISTS (
                  SELECT 1 FROM memory_relations r
                  WHERE r.source_entity_id = e.id OR r.target_entity_id = e.id
              )
            ",
        )?;
        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for id in ids {
            self.conn
                .execute("DELETE FROM memory_entities_fts WHERE id = ?1", params![id])?;
            self.conn
                .execute("DELETE FROM memory_entities WHERE id = ?1", params![id])?;
        }
        Ok(())
    }

    pub fn search_memories(&self, query: &str) -> AppResult<Vec<Memory>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_memories();
        }

        let Some(fts_query) = build_fts_prefix_query(trimmed) else {
            return Ok(Vec::new());
        };

        let mut stmt = self.conn.prepare(
            "
            SELECT m.id, m.title, m.content, m.tags_json, m.enabled, m.created_at, m.updated_at
            FROM memories_fts f
            JOIN memories m ON m.id = f.id
            WHERE memories_fts MATCH ?1
            ORDER BY rank
            LIMIT 100
            ",
        )?;

        let rows = stmt
            .query_map([fts_query], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn create_memory(&self, draft: MemoryDraft) -> AppResult<Memory> {
        let now = Utc::now();
        let memory = Memory {
            id: Uuid::new_v4().to_string(),
            title: clean_or_default(draft.title, "New memory"),
            content: draft.content,
            tags: draft.tags,
            enabled: draft.enabled.unwrap_or(true),
            created_at: now,
            updated_at: now,
        };

        self.upsert_memory(&memory)?;
        Ok(memory)
    }

    pub fn update_memory(&self, patch: MemoryPatch) -> AppResult<Memory> {
        let current = self
            .get_memory(&patch.id)?
            .ok_or_else(|| AppError::Message("memory not found".to_string()))?;

        let memory = Memory {
            id: current.id,
            title: patch.title.unwrap_or(current.title),
            content: patch.content.unwrap_or(current.content),
            tags: patch.tags.unwrap_or(current.tags),
            enabled: patch.enabled.unwrap_or(current.enabled),
            created_at: current.created_at,
            updated_at: Utc::now(),
        };

        self.upsert_memory(&memory)?;
        Ok(memory)
    }

    pub fn delete_memory(&self, id: &str) -> AppResult<()> {
        self.with_savepoint("memory_delete", || {
            if let Some((vector_rowid, dimensions)) = self
                .conn
                .query_row(
                    "SELECT vector_rowid, dimensions FROM memory_embeddings WHERE memory_id = ?1",
                    params![id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?
            {
                let table = memory_vector_table(dimensions)?;
                self.conn.execute(
                    &format!("DELETE FROM {table} WHERE rowid = ?1"),
                    params![vector_rowid],
                )?;
            }
            self.conn
                .execute("DELETE FROM memories_fts WHERE id = ?1", params![id])?;
            let affected = self
                .conn
                .execute("DELETE FROM memories WHERE id = ?1", params![id])?;
            ensure_affected(affected, "memory not found")?;
            self.remove_orphan_memory_entities()
        })
    }
}
