use crate::db::Database;
use crate::error::AppResult;
use crate::llm::create_embeddings;
use crate::models::{ModelConfig, ProjectRetrievalContext};
use crate::project_index::DOCUMENT_INDEXER;

const CODE_INDEX_LIMIT: i64 = 8;
const DOCUMENT_INDEX_LIMIT: i64 = 6;

pub(crate) async fn build_query_embedding(
    config: Option<&ModelConfig>,
    query: &str,
) -> Option<Vec<f32>> {
    let config = config?;
    if query.trim().is_empty() {
        return None;
    }
    create_embeddings(config, vec![query.to_string()])
        .await
        .ok()
        .and_then(|embeddings| embeddings.into_iter().next())
}

pub(crate) fn search_project_context(
    db: &Database,
    project_path: &str,
    query: &str,
    query_embedding: Option<&[f32]>,
) -> AppResult<ProjectRetrievalContext> {
    if query.trim().is_empty() {
        return Ok(ProjectRetrievalContext {
            code_matches: Vec::new(),
            project_index_matches: Vec::new(),
        });
    }
    let code_matches = if is_likely_code_question(query) {
        db.search_code_index(project_path, query, query_embedding, CODE_INDEX_LIMIT)?
    } else {
        Vec::new()
    };
    let project_index_matches = db.search_project_index(
        project_path,
        Some(DOCUMENT_INDEXER),
        query,
        query_embedding,
        DOCUMENT_INDEX_LIMIT,
    )?;
    Ok(ProjectRetrievalContext {
        code_matches,
        project_index_matches,
    })
}

fn is_likely_code_question(query: &str) -> bool {
    let normalized = query.to_lowercase();
    [
        "代码",
        "函数",
        "组件",
        "接口",
        "调用",
        "实现",
        "报错",
        "文件",
        "模块",
        "重构",
        "类",
        "类型",
        "方法",
        "tsx",
        "ts",
        "rust",
        "tauri",
        "api",
        "hook",
        "component",
        "function",
        "class",
        "type",
        "interface",
        "error",
        "trace",
        "call",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CodeChunk, ProjectIndexChunk};
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn detects_code_questions_consistently() {
        assert!(is_likely_code_question("这个函数在哪里实现？"));
        assert!(is_likely_code_question("trace the API call"));
        assert!(!is_likely_code_question("帮我总结产品说明"));
    }

    #[test]
    fn shared_search_combines_vector_code_and_document_results() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let project_path = "D:\\workspace\\shared-retrieval";
        let now = Utc::now();
        let code_chunks = vec![
            CodeChunk {
                id: "code-near".to_string(),
                project_path: project_path.to_string(),
                file_path: "src/semantic.rs".to_string(),
                language: "rust".to_string(),
                chunk_index: 0,
                start_line: 1,
                end_line: 2,
                text: "unrelated words chosen through vector similarity".to_string(),
                content_hash: "code-near-hash".to_string(),
                token_count: 8,
                created_at: now,
            },
            CodeChunk {
                id: "code-far".to_string(),
                project_path: project_path.to_string(),
                file_path: "src/far.rs".to_string(),
                language: "rust".to_string(),
                chunk_index: 1,
                start_line: 1,
                end_line: 2,
                text: "another unrelated chunk".to_string(),
                content_hash: "code-far-hash".to_string(),
                token_count: 4,
                created_at: now,
            },
        ];
        db.replace_code_index(
            project_path,
            2,
            &[],
            &[],
            &code_chunks,
            Some((&[vec![1.0, 0.0], vec![0.0, 1.0]], "test-embedding")),
        )
        .expect("code index should persist");

        let document_chunks = vec![
            ProjectIndexChunk {
                id: "doc-near".to_string(),
                project_path: project_path.to_string(),
                indexer: DOCUMENT_INDEXER.to_string(),
                file_path: "docs/semantic.md".to_string(),
                title: "Semantic document".to_string(),
                chunk_index: 0,
                start_line: 1,
                end_line: 2,
                text: "vector-only document match".to_string(),
                content_hash: "doc-near-hash".to_string(),
                token_count: 4,
                created_at: now,
            },
            ProjectIndexChunk {
                id: "doc-far".to_string(),
                project_path: project_path.to_string(),
                indexer: DOCUMENT_INDEXER.to_string(),
                file_path: "docs/far.md".to_string(),
                title: "Far document".to_string(),
                chunk_index: 1,
                start_line: 1,
                end_line: 2,
                text: "distant vector document".to_string(),
                content_hash: "doc-far-hash".to_string(),
                token_count: 4,
                created_at: now,
            },
        ];
        db.replace_project_index(
            project_path,
            DOCUMENT_INDEXER,
            2,
            &document_chunks,
            Some((&[vec![1.0, 0.0], vec![0.0, 1.0]], "test-embedding")),
        )
        .expect("document index should persist");

        let result = search_project_context(
            &db,
            project_path,
            "这个函数的内部机制是什么",
            Some(&[1.0, 0.0]),
        )
        .expect("shared retrieval should succeed");

        assert_eq!(result.code_matches[0].file_path, "src/semantic.rs");
        assert_eq!(
            result.project_index_matches[0].file_path,
            "docs/semantic.md"
        );
        assert_eq!(result.code_matches[0].kind, "semantic_chunk");
    }
}
