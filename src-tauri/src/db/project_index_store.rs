use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use super::{
    code_search_terms, cosine_similarity, decode_embedding, encode_embedding, parse_time_for_row,
    trim_code_snippet, Database,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    ProjectIndexChunk, ProjectIndexRun, ProjectIndexSearchResult, ProjectIndexStats,
};

impl Database {
    pub fn replace_project_index(
        &self,
        project_path: &str,
        indexer: &str,
        file_count: i64,
        chunks: &[ProjectIndexChunk],
        chunk_embeddings: Option<(&[Vec<f32>], &str)>,
    ) -> AppResult<ProjectIndexRun> {
        let now = Utc::now();
        if let Some((embeddings, _)) = chunk_embeddings {
            if embeddings.len() != chunks.len() {
                return Err(AppError::Message(
                    "项目索引 chunk 与 embedding 数量不一致".to_string(),
                ));
            }
        }

        self.project_conn.execute(
            "DELETE FROM project_index_chunks_fts WHERE project_path = ?1 AND indexer = ?2",
            params![project_path, indexer],
        )?;
        self.project_conn.execute(
            "DELETE FROM project_index_embeddings WHERE project_path = ?1 AND indexer = ?2",
            params![project_path, indexer],
        )?;
        self.project_conn.execute(
            "DELETE FROM project_index_chunks WHERE project_path = ?1 AND indexer = ?2",
            params![project_path, indexer],
        )?;

        for (chunk_index, chunk) in chunks.iter().enumerate() {
            self.project_conn.execute(
                "
                INSERT INTO project_index_chunks
                    (id, project_path, indexer, file_path, title, chunk_index,
                     start_line, end_line, text, content_hash, token_count, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ",
                params![
                    &chunk.id,
                    project_path,
                    indexer,
                    &chunk.file_path,
                    &chunk.title,
                    chunk.chunk_index,
                    chunk.start_line,
                    chunk.end_line,
                    &chunk.text,
                    &chunk.content_hash,
                    chunk.token_count,
                    now.to_rfc3339()
                ],
            )?;
            self.project_conn.execute(
                "
                INSERT INTO project_index_chunks_fts
                    (chunk_id, project_path, indexer, file_path, title, text)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    &chunk.id,
                    project_path,
                    indexer,
                    &chunk.file_path,
                    &chunk.title,
                    &chunk.text
                ],
            )?;
            if let Some((embeddings, embedding_model)) = chunk_embeddings {
                let embedding = &embeddings[chunk_index];
                self.project_conn.execute(
                    "
                    INSERT INTO project_index_embeddings
                        (chunk_id, project_path, indexer, embedding, dim, model, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    ",
                    params![
                        &chunk.id,
                        project_path,
                        indexer,
                        encode_embedding(embedding),
                        embedding.len() as i64,
                        embedding_model,
                        now.to_rfc3339()
                    ],
                )?;
            }
        }

        let run = ProjectIndexRun {
            id: Uuid::new_v4().to_string(),
            project_path: project_path.to_string(),
            indexer: indexer.to_string(),
            status: "ready".to_string(),
            file_count,
            chunk_count: chunks.len() as i64,
            error: None,
            created_at: now,
            updated_at: now,
        };
        self.project_conn.execute(
            "
            INSERT INTO project_index_runs
                (id, project_path, indexer, status, file_count, chunk_count,
                 error, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                &run.id,
                &run.project_path,
                &run.indexer,
                &run.status,
                run.file_count,
                run.chunk_count,
                &run.error,
                run.created_at.to_rfc3339(),
                run.updated_at.to_rfc3339()
            ],
        )?;
        Ok(run)
    }

    pub fn get_project_index_stats(&self, project_path: &str) -> AppResult<ProjectIndexStats> {
        let mut stmt = self.project_conn.prepare(
            "
            SELECT id, project_path, indexer, status, file_count, chunk_count,
                   error, created_at, updated_at
            FROM project_index_runs
            WHERE project_path = ?1
            ORDER BY indexer ASC, updated_at DESC
            ",
        )?;
        let mut latest_by_indexer = std::collections::BTreeMap::new();
        let rows = stmt.query_map(params![project_path], row_to_project_index_run)?;
        for row in rows {
            let run = row?;
            latest_by_indexer.entry(run.indexer.clone()).or_insert(run);
        }

        Ok(ProjectIndexStats {
            project_path: project_path.to_string(),
            runs: latest_by_indexer.into_values().collect(),
        })
    }

    pub fn search_project_index(
        &self,
        project_path: &str,
        indexer: Option<&str>,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: i64,
    ) -> AppResult<Vec<ProjectIndexSearchResult>> {
        let terms = code_search_terms(query);
        if terms.is_empty() && query_embedding.is_none() {
            return Ok(Vec::new());
        }

        let limit = limit.clamp(1, 20);
        let mut results = Vec::new();

        if let Some(query_embedding) = query_embedding {
            let mut stmt = self.project_conn.prepare(
                "
                SELECT chunks.indexer, chunks.file_path, chunks.title, chunks.chunk_index,
                       chunks.start_line, chunks.end_line, chunks.text, embeddings.embedding
                FROM project_index_chunks chunks
                JOIN project_index_embeddings embeddings ON embeddings.chunk_id = chunks.id
                WHERE chunks.project_path = ?1 AND (?2 IS NULL OR chunks.indexer = ?2)
                ",
            )?;
            let mut rows = stmt.query(params![project_path, indexer])?;
            let mut vector_matches = Vec::new();
            while let Some(row) = rows.next()? {
                let embedding_blob: Vec<u8> = row.get(7)?;
                let embedding = decode_embedding(&embedding_blob)?;
                let score = cosine_similarity(query_embedding, &embedding);
                vector_matches.push(ProjectIndexSearchResult {
                    indexer: row.get(0)?,
                    file_path: row.get(1)?,
                    title: row.get(2)?,
                    chunk_index: row.get(3)?,
                    start_line: row.get(4)?,
                    end_line: row.get(5)?,
                    snippet: trim_code_snippet(&row.get::<_, String>(6)?),
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
            let like_patterns = terms
                .iter()
                .take(4)
                .map(|term| format!("%{}%", term))
                .collect::<Vec<_>>();
            let remaining = limit.saturating_sub(results.len() as i64);
            if remaining > 0 {
                let mut stmt = self.project_conn.prepare(
                    "
                    SELECT indexer, file_path, title, chunk_index, start_line, end_line, text
                    FROM project_index_chunks
                    WHERE project_path = ?1
                      AND (?2 IS NULL OR indexer = ?2)
                      AND (
                        text LIKE ?3 OR title LIKE ?3 OR file_path LIKE ?3
                        OR text LIKE ?4 OR title LIKE ?4 OR file_path LIKE ?4
                        OR text LIKE ?5 OR title LIKE ?5 OR file_path LIKE ?5
                        OR text LIKE ?6 OR title LIKE ?6 OR file_path LIKE ?6
                      )
                    ORDER BY file_path, chunk_index
                    LIMIT ?7
                    ",
                )?;
                let mut rows = stmt.query(params![
                    project_path,
                    indexer,
                    like_patterns.first().map(String::as_str).unwrap_or(""),
                    like_patterns.get(1).map(String::as_str).unwrap_or(""),
                    like_patterns.get(2).map(String::as_str).unwrap_or(""),
                    like_patterns.get(3).map(String::as_str).unwrap_or(""),
                    remaining
                ])?;
                while let Some(row) = rows.next()? {
                    results.push(ProjectIndexSearchResult {
                        indexer: row.get(0)?,
                        file_path: row.get(1)?,
                        title: row.get(2)?,
                        chunk_index: row.get(3)?,
                        start_line: row.get(4)?,
                        end_line: row.get(5)?,
                        snippet: trim_code_snippet(&row.get::<_, String>(6)?),
                        score: 0.55,
                    });
                }
            }
        }

        dedupe_project_index_search_results(&mut results);
        results.truncate(limit as usize);
        Ok(results)
    }
}

fn row_to_project_index_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectIndexRun> {
    let created_at: String = row.get(7)?;
    let updated_at: String = row.get(8)?;
    Ok(ProjectIndexRun {
        id: row.get(0)?,
        project_path: row.get(1)?,
        indexer: row.get(2)?,
        status: row.get(3)?,
        file_count: row.get(4)?,
        chunk_count: row.get(5)?,
        error: row.get(6)?,
        created_at: parse_time_for_row(&created_at)?,
        updated_at: parse_time_for_row(&updated_at)?,
    })
}

fn dedupe_project_index_search_results(results: &mut Vec<ProjectIndexSearchResult>) {
    let mut seen = std::collections::HashSet::new();
    results.retain(|result| {
        seen.insert(format!(
            "{}:{}:{}:{}",
            result.indexer, result.file_path, result.start_line, result.end_line
        ))
    });
}
