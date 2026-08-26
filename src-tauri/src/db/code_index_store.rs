use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::{
    code_search_terms, cosine_similarity, decode_embedding, dedupe_code_search_results,
    encode_embedding, row_to_code_index_run, trim_code_snippet, Database,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    CodeChunk, CodeEntity, CodeIndexRun, CodeIndexStats, CodeRelation, CodeSearchResult,
};

impl Database {
    pub fn replace_code_index(
        &self,
        project_path: &str,
        file_count: i64,
        entities: &[CodeEntity],
        relations: &[CodeRelation],
        chunks: &[CodeChunk],
        chunk_embeddings: Option<(&[Vec<f32>], &str)>,
    ) -> AppResult<CodeIndexRun> {
        let now = Utc::now();
        if let Some((embeddings, _)) = chunk_embeddings {
            if embeddings.len() != chunks.len() {
                return Err(AppError::Message(
                    "代码 chunk 与 embedding 数量不一致".to_string(),
                ));
            }
        }
        self.conn.execute(
            "DELETE FROM code_chunks_fts WHERE project_path = ?1",
            params![project_path],
        )?;
        self.conn.execute(
            "DELETE FROM code_embeddings WHERE project_path = ?1",
            params![project_path],
        )?;
        self.conn.execute(
            "DELETE FROM code_relations WHERE project_path = ?1",
            params![project_path],
        )?;
        self.conn.execute(
            "DELETE FROM code_entities WHERE project_path = ?1",
            params![project_path],
        )?;
        self.conn.execute(
            "DELETE FROM code_chunks WHERE project_path = ?1",
            params![project_path],
        )?;

        for entity in entities {
            self.conn.execute(
                "
                INSERT INTO code_entities
                    (id, project_path, file_path, name, kind, language, start_line,
                     end_line, signature, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    &entity.id,
                    project_path,
                    &entity.file_path,
                    &entity.name,
                    &entity.kind,
                    &entity.language,
                    entity.start_line,
                    entity.end_line,
                    &entity.signature,
                    now.to_rfc3339()
                ],
            )?;
        }

        for relation in relations {
            self.conn.execute(
                "
                INSERT INTO code_relations
                    (id, project_path, source_entity_id, source_name, target_entity_id,
                     target_name, kind, file_path, line, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    &relation.id,
                    project_path,
                    &relation.source_entity_id,
                    &relation.source_name,
                    &relation.target_entity_id,
                    &relation.target_name,
                    &relation.kind,
                    &relation.file_path,
                    relation.line,
                    now.to_rfc3339()
                ],
            )?;
        }

        for (chunk_index, chunk) in chunks.iter().enumerate() {
            self.conn.execute(
                "
                INSERT INTO code_chunks
                    (id, project_path, file_path, language, chunk_index, start_line,
                     end_line, text, content_hash, token_count, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ",
                params![
                    &chunk.id,
                    project_path,
                    &chunk.file_path,
                    &chunk.language,
                    chunk.chunk_index,
                    chunk.start_line,
                    chunk.end_line,
                    &chunk.text,
                    &chunk.content_hash,
                    chunk.token_count,
                    now.to_rfc3339()
                ],
            )?;
            self.conn.execute(
                "
                INSERT INTO code_chunks_fts
                    (chunk_id, project_path, file_path, language, text)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    &chunk.id,
                    project_path,
                    &chunk.file_path,
                    &chunk.language,
                    &chunk.text
                ],
            )?;
            if let Some((embeddings, embedding_model)) = chunk_embeddings {
                let embedding = &embeddings[chunk_index];
                self.conn.execute(
                    "
                    INSERT INTO code_embeddings
                        (chunk_id, project_path, embedding, dim, model, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ",
                    params![
                        &chunk.id,
                        project_path,
                        encode_embedding(embedding),
                        embedding.len() as i64,
                        embedding_model,
                        now.to_rfc3339()
                    ],
                )?;
            }
        }

        let run = CodeIndexRun {
            id: Uuid::new_v4().to_string(),
            project_path: project_path.to_string(),
            status: "ready".to_string(),
            file_count,
            entity_count: entities.len() as i64,
            relation_count: relations.len() as i64,
            chunk_count: chunks.len() as i64,
            error: None,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            "
            INSERT INTO code_index_runs
                (id, project_path, status, file_count, entity_count, relation_count,
                 chunk_count, error, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                &run.id,
                &run.project_path,
                &run.status,
                run.file_count,
                run.entity_count,
                run.relation_count,
                run.chunk_count,
                &run.error,
                run.created_at.to_rfc3339(),
                run.updated_at.to_rfc3339()
            ],
        )?;
        Ok(run)
    }

    pub fn get_code_index_stats(&self, project_path: &str) -> AppResult<CodeIndexStats> {
        let latest_run = self
            .conn
            .query_row(
                "
                SELECT id, project_path, status, file_count, entity_count, relation_count,
                       chunk_count, error, created_at, updated_at
                FROM code_index_runs
                WHERE project_path = ?1
                ORDER BY updated_at DESC
                LIMIT 1
                ",
                params![project_path],
                row_to_code_index_run,
            )
            .optional()?;

        Ok(CodeIndexStats {
            project_path: project_path.to_string(),
            latest_run,
        })
    }

    pub fn search_code_index(
        &self,
        project_path: &str,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: i64,
    ) -> AppResult<Vec<CodeSearchResult>> {
        let terms = code_search_terms(query);
        if terms.is_empty() && query_embedding.is_none() {
            return Ok(Vec::new());
        }

        let like_patterns = terms
            .iter()
            .take(4)
            .map(|term| format!("%{}%", term))
            .collect::<Vec<_>>();
        let limit = limit.clamp(1, 20);
        let mut results = Vec::new();

        if let Some(query_embedding) = query_embedding {
            let mut stmt = self.conn.prepare(
                "
                SELECT chunks.file_path, chunks.language, chunks.start_line, chunks.end_line,
                       chunks.text, embeddings.embedding
                FROM code_chunks chunks
                JOIN code_embeddings embeddings ON embeddings.chunk_id = chunks.id
                WHERE chunks.project_path = ?1
                ",
            )?;
            let mut rows = stmt.query(params![project_path])?;
            let mut vector_matches = Vec::new();
            while let Some(row) = rows.next()? {
                let embedding_blob: Vec<u8> = row.get(5)?;
                let embedding = decode_embedding(&embedding_blob)?;
                let score = cosine_similarity(query_embedding, &embedding);
                vector_matches.push(CodeSearchResult {
                    file_path: row.get(0)?,
                    kind: "semantic_chunk".to_string(),
                    name: "语义代码片段".to_string(),
                    language: row.get(1)?,
                    start_line: row.get(2)?,
                    end_line: row.get(3)?,
                    snippet: trim_code_snippet(&row.get::<_, String>(4)?),
                    score,
                });
            }
            vector_matches.sort_by(|left, right| {
                right
                    .score
                    .partial_cmp(&left.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            results.extend(vector_matches.into_iter().take((limit / 2).max(1) as usize));
        }

        if !terms.is_empty() {
            let mut entity_stmt = self.conn.prepare(
                "
            SELECT file_path, kind, name, language, start_line, end_line, signature
            FROM code_entities
            WHERE project_path = ?1
              AND (
                name LIKE ?2 OR signature LIKE ?2 OR file_path LIKE ?2
                OR name LIKE ?3 OR signature LIKE ?3 OR file_path LIKE ?3
                OR name LIKE ?4 OR signature LIKE ?4 OR file_path LIKE ?4
                OR name LIKE ?5 OR signature LIKE ?5 OR file_path LIKE ?5
              )
            ORDER BY
              CASE WHEN name LIKE ?2 THEN 0 ELSE 1 END,
              file_path,
              start_line
            LIMIT ?6
            ",
            )?;
            let mut entity_rows = entity_stmt.query(params![
                project_path,
                like_patterns.first().map(String::as_str).unwrap_or(""),
                like_patterns.get(1).map(String::as_str).unwrap_or(""),
                like_patterns.get(2).map(String::as_str).unwrap_or(""),
                like_patterns.get(3).map(String::as_str).unwrap_or(""),
                limit
            ])?;
            while let Some(row) = entity_rows.next()? {
                let signature: String = row.get(6)?;
                results.push(CodeSearchResult {
                    file_path: row.get(0)?,
                    kind: row.get(1)?,
                    name: row.get(2)?,
                    language: row.get(3)?,
                    start_line: row.get(4)?,
                    end_line: row.get(5)?,
                    snippet: signature,
                    score: 1.0,
                });
            }

            let remaining = limit.saturating_sub(results.len() as i64);
            if remaining > 0 {
                let mut chunk_stmt = self.conn.prepare(
                    "
                SELECT file_path, language, start_line, end_line, text
                FROM code_chunks
                WHERE project_path = ?1
                  AND (
                    text LIKE ?2 OR file_path LIKE ?2
                    OR text LIKE ?3 OR file_path LIKE ?3
                    OR text LIKE ?4 OR file_path LIKE ?4
                    OR text LIKE ?5 OR file_path LIKE ?5
                  )
                ORDER BY file_path, chunk_index
                LIMIT ?6
                ",
                )?;
                let mut chunk_rows = chunk_stmt.query(params![
                    project_path,
                    like_patterns.first().map(String::as_str).unwrap_or(""),
                    like_patterns.get(1).map(String::as_str).unwrap_or(""),
                    like_patterns.get(2).map(String::as_str).unwrap_or(""),
                    like_patterns.get(3).map(String::as_str).unwrap_or(""),
                    remaining
                ])?;
                while let Some(row) = chunk_rows.next()? {
                    let text: String = row.get(4)?;
                    results.push(CodeSearchResult {
                        file_path: row.get(0)?,
                        kind: "chunk".to_string(),
                        name: "代码片段".to_string(),
                        language: row.get(1)?,
                        start_line: row.get(2)?,
                        end_line: row.get(3)?,
                        snippet: trim_code_snippet(&text),
                        score: 0.6,
                    });
                }
            }
        }

        dedupe_code_search_results(&mut results);
        results.truncate(limit as usize);
        Ok(results)
    }
}
