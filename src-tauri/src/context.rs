use std::path::Path;

use tauri::State;

use crate::db::Database;
use crate::error::AppResult;
use crate::memory::{
    finish_memory_retrieval, plan_memory_retrieval, prepare_memory_retrieval, MemoryRetrievalPlan,
    PreparedMemoryRetrieval,
};
use crate::models::{BaseContextBundle, ModelConfig};
use crate::profile::load_profile_context;
use crate::project_files::{list_project_files, project_root};
use crate::project_retrieval::{build_query_embedding, search_project_context};
use crate::AppState;

const MEMORY_LIMIT: i64 = 8;

struct BaseContextPlan {
    memory: MemoryRetrievalPlan,
    profile_context: Option<String>,
    project_path: Option<String>,
    embedding_config: Option<ModelConfig>,
    query: String,
}

struct PreparedBaseContext {
    memory: PreparedMemoryRetrieval,
    profile_context: Option<String>,
    project_path: Option<String>,
    project_files: Vec<crate::models::ProjectFileEntry>,
    query: String,
    query_embedding: Option<Vec<f32>>,
}

#[tauri::command]
pub async fn load_base_context(
    state: State<'_, AppState>,
    project_path: Option<String>,
    query: String,
) -> AppResult<BaseContextBundle> {
    let canonical_project_path = project_path
        .as_deref()
        .map(project_root)
        .transpose()?
        .map(|path| path.to_string_lossy().to_string());
    let plan = {
        let db = state.db.lock().await;
        plan_base_context(&db, canonical_project_path, &query)?
    };
    let prepared = prepare_base_context(plan).await?;
    let db = state.db.lock().await;
    finish_base_context(&db, prepared)
}

pub(crate) async fn load_base_context_for_db(
    db: &Database,
    project: Option<&Path>,
    query: &str,
) -> AppResult<BaseContextBundle> {
    let project_path = project.map(|path| path.to_string_lossy().to_string());
    let plan = plan_base_context(db, project_path, query)?;
    let prepared = prepare_base_context(plan).await?;
    finish_base_context(db, prepared)
}

fn plan_base_context(
    db: &Database,
    project_path: Option<String>,
    query: &str,
) -> AppResult<BaseContextPlan> {
    Ok(BaseContextPlan {
        memory: plan_memory_retrieval(db, query, MEMORY_LIMIT)?,
        profile_context: load_profile_context(db)?,
        embedding_config: project_path
            .as_ref()
            .and_then(|_| db.get_model_config("embedding-config").ok()),
        project_path,
        query: query.to_string(),
    })
}

async fn prepare_base_context(plan: BaseContextPlan) -> AppResult<PreparedBaseContext> {
    let project_path = plan.project_path.clone();
    let files_path = project_path.clone();
    let query = plan.query.clone();
    let memory_future = prepare_memory_retrieval(plan.memory);
    let files_future = async move {
        match files_path {
            Some(path) => list_project_files(path).await,
            None => Ok(Vec::new()),
        }
    };
    let (memory, project_files_result) = tokio::join!(memory_future, files_future);
    let query_embedding = match memory.query_embedding() {
        Some(embedding) => Some(embedding.to_vec()),
        None => build_query_embedding(plan.embedding_config.as_ref(), &query).await,
    };
    let project_files = match project_files_result {
        Ok(files) => files,
        Err(error) => {
            crate::logging::warn(
                "context",
                "failed to list project files for base context",
                serde_json::json!({ "error": error.to_string() }),
            );
            Vec::new()
        }
    };
    Ok(PreparedBaseContext {
        memory,
        profile_context: plan.profile_context,
        project_path,
        project_files,
        query,
        query_embedding,
    })
}

fn finish_base_context(
    db: &Database,
    prepared: PreparedBaseContext,
) -> AppResult<BaseContextBundle> {
    let memories = finish_memory_retrieval(db, prepared.memory)?;
    let retrieval = match prepared.project_path.as_deref() {
        Some(project_path) => match search_project_context(
            db,
            project_path,
            &prepared.query,
            prepared.query_embedding.as_deref(),
        ) {
            Ok(retrieval) => retrieval,
            Err(error) => {
                crate::logging::warn(
                    "context",
                    "failed to search project indexes for base context",
                    serde_json::json!({ "error": error.to_string() }),
                );
                crate::models::ProjectRetrievalContext {
                    code_matches: Vec::new(),
                    project_index_matches: Vec::new(),
                }
            }
        },
        None => crate::models::ProjectRetrievalContext {
            code_matches: Vec::new(),
            project_index_matches: Vec::new(),
        },
    };
    Ok(BaseContextBundle {
        profile_context: prepared.profile_context,
        memories,
        project_files: prepared.project_files,
        code_matches: retrieval.code_matches,
        project_index_matches: retrieval.project_index_matches,
    })
}
