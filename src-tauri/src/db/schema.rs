use super::*;

impl Database {
    pub(super) fn init_full_schema(conn: &Connection) -> AppResult<()> {
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                status TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
                id UNINDEXED,
                title,
                body,
                tags
            );

            CREATE TABLE IF NOT EXISTS model_configs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                base_url TEXT NOT NULL,
                model TEXT NOT NULL,
                api_key TEXT NOT NULL,
                temperature REAL NOT NULL DEFAULT 0.4,
                max_tokens INTEGER,
                context_window INTEGER NOT NULL DEFAULT 32768,
                top_p REAL,
                reasoning_effort TEXT NOT NULL DEFAULT '',
                model_kind TEXT NOT NULL DEFAULT 'chat',
                routing_group TEXT NOT NULL DEFAULT '默认组',
                routing_enabled INTEGER NOT NULL DEFAULT 1,
                routing_cost INTEGER NOT NULL DEFAULT 3,
                routing_quality INTEGER NOT NULL DEFAULT 3,
                routing_speed INTEGER NOT NULL DEFAULT 3,
                routing_tasks_json TEXT NOT NULL DEFAULT '[]',
                embedding_provider TEXT NOT NULL DEFAULT 'openai-compatible',
                embedding_base_url TEXT NOT NULL DEFAULT '',
                embedding_model TEXT NOT NULL DEFAULT '',
                embedding_api_key TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS model_suppliers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                base_url TEXT NOT NULL,
                api_key TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            INSERT INTO model_suppliers (id, name, provider, base_url, api_key, created_at, updated_at)
            SELECT lower(hex(randomblob(16))), m.name, m.provider, m.base_url, m.api_key, m.created_at, m.updated_at
            FROM model_configs m
            WHERE m.id <> 'embedding-config'
              AND NOT EXISTS (SELECT 1 FROM model_suppliers s WHERE s.provider=m.provider AND s.base_url=m.base_url AND s.api_key=m.api_key);

            CREATE TABLE IF NOT EXISTS mcp_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                transport TEXT NOT NULL DEFAULT 'stdio',
                command TEXT NOT NULL,
                args_json TEXT NOT NULL DEFAULT '[]',
                env_json TEXT NOT NULL DEFAULT '{}',
                url TEXT NOT NULL DEFAULT '',
                headers_json TEXT NOT NULL DEFAULT '{}',
                working_dir TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ops_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL,
                auth_method TEXT NOT NULL DEFAULT 'key',
                key_path TEXT NOT NULL DEFAULT '',
                password TEXT NOT NULL DEFAULT '',
                remote_dir TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model_config_id TEXT,
                project_path TEXT,
                archived INTEGER NOT NULL DEFAULT 0,
                archived_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (model_config_id) REFERENCES model_configs(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata_json TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conversation_created
                ON messages(conversation_id, created_at);

            CREATE TABLE IF NOT EXISTS rag_files (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                name TEXT NOT NULL,
                mime TEXT NOT NULL,
                size INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                chunk_count INTEGER NOT NULL,
                status TEXT NOT NULL,
                error TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS rag_chunks (
                id TEXT PRIMARY KEY,
                file_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                text TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                metadata_json TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (file_id) REFERENCES rag_files(id) ON DELETE CASCADE,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS rag_embeddings (
                chunk_id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                embedding BLOB NOT NULL,
                dim INTEGER NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (chunk_id) REFERENCES rag_chunks(id) ON DELETE CASCADE,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS rag_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                conversation_id UNINDEXED,
                file_id UNINDEXED,
                file_name,
                text
            );

            CREATE TABLE IF NOT EXISTS code_index_runs (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                status TEXT NOT NULL,
                file_count INTEGER NOT NULL,
                entity_count INTEGER NOT NULL,
                relation_count INTEGER NOT NULL,
                chunk_count INTEGER NOT NULL,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS code_entities (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                language TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                signature TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS code_relations (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                source_entity_id TEXT,
                source_name TEXT NOT NULL,
                target_entity_id TEXT,
                target_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS code_chunks (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                language TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                text TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS code_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                project_path UNINDEXED,
                file_path,
                language,
                text
            );

            CREATE TABLE IF NOT EXISTS code_embeddings (
                chunk_id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                embedding BLOB NOT NULL,
                dim INTEGER NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (chunk_id) REFERENCES code_chunks(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS project_index_runs (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                indexer TEXT NOT NULL,
                status TEXT NOT NULL,
                file_count INTEGER NOT NULL,
                chunk_count INTEGER NOT NULL,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS project_index_chunks (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                indexer TEXT NOT NULL,
                file_path TEXT NOT NULL,
                title TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                text TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS project_index_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                project_path UNINDEXED,
                indexer UNINDEXED,
                file_path,
                title,
                text
            );

            CREATE TABLE IF NOT EXISTS project_index_embeddings (
                chunk_id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                indexer TEXT NOT NULL,
                embedding BLOB NOT NULL,
                dim INTEGER NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (chunk_id) REFERENCES project_index_chunks(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_rag_files_conversation
                ON rag_files(conversation_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_rag_chunks_conversation
                ON rag_chunks(conversation_id, chunk_index);
            CREATE INDEX IF NOT EXISTS idx_rag_embeddings_conversation
                ON rag_embeddings(conversation_id);

            CREATE INDEX IF NOT EXISTS idx_code_index_runs_project
                ON code_index_runs(project_path, updated_at);
            CREATE INDEX IF NOT EXISTS idx_code_entities_project_name
                ON code_entities(project_path, name);
            CREATE INDEX IF NOT EXISTS idx_code_relations_project_source
                ON code_relations(project_path, source_name);
            CREATE INDEX IF NOT EXISTS idx_code_relations_project_target
                ON code_relations(project_path, target_name);
            CREATE INDEX IF NOT EXISTS idx_code_chunks_project_file
                ON code_chunks(project_path, file_path, chunk_index);
            CREATE INDEX IF NOT EXISTS idx_code_embeddings_project
                ON code_embeddings(project_path);
            CREATE INDEX IF NOT EXISTS idx_project_index_runs_project
                ON project_index_runs(project_path, indexer, updated_at);
            CREATE INDEX IF NOT EXISTS idx_project_index_chunks_project
                ON project_index_chunks(project_path, indexer, file_path, chunk_index);
            CREATE INDEX IF NOT EXISTS idx_project_index_embeddings_project
                ON project_index_embeddings(project_path, indexer);

            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id UNINDEXED,
                title,
                content,
                tags
            );

            CREATE TABLE IF NOT EXISTS memory_embeddings (
                vector_rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                memory_id TEXT NOT NULL UNIQUE,
                model TEXT NOT NULL,
                dimensions INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                indexed_at TEXT NOT NULL,
                FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS memory_entities (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                normalized_name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(kind, normalized_name)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memory_entities_fts USING fts5(
                id UNINDEXED,
                name
            );

            CREATE TABLE IF NOT EXISTS memory_entity_links (
                memory_id TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                weight REAL NOT NULL DEFAULT 1,
                PRIMARY KEY (memory_id, entity_id),
                FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE,
                FOREIGN KEY (entity_id) REFERENCES memory_entities(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS memory_relations (
                source_entity_id TEXT NOT NULL,
                target_entity_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                weight REAL NOT NULL DEFAULT 1,
                evidence_memory_id TEXT NOT NULL,
                PRIMARY KEY (source_entity_id, target_entity_id, kind, evidence_memory_id),
                FOREIGN KEY (source_entity_id) REFERENCES memory_entities(id) ON DELETE CASCADE,
                FOREIGN KEY (target_entity_id) REFERENCES memory_entities(id) ON DELETE CASCADE,
                FOREIGN KEY (evidence_memory_id) REFERENCES memories(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS profile_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                profile_generation INTEGER NOT NULL DEFAULT 1,
                next_event_revision INTEGER NOT NULL DEFAULT 0,
                skipped_observation_count INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profile_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                enabled INTEGER NOT NULL DEFAULT 0,
                model_config_id TEXT,
                character_threshold INTEGER NOT NULL DEFAULT 3000,
                idle_seconds INTEGER NOT NULL DEFAULT 1800,
                max_wait_seconds INTEGER NOT NULL DEFAULT 86400,
                long_input_threshold INTEGER NOT NULL DEFAULT 8000,
                rolling_hour_attempt_limit INTEGER NOT NULL DEFAULT 2,
                rolling_day_attempt_limit INTEGER NOT NULL DEFAULT 8,
                rolling_day_candidate_character_limit INTEGER NOT NULL DEFAULT 30000,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (model_config_id) REFERENCES model_configs(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS profile_observations (
                id TEXT PRIMARY KEY,
                source_message_id TEXT NOT NULL UNIQUE,
                conversation_id TEXT NOT NULL,
                raw_character_count INTEGER NOT NULL,
                candidate_character_count INTEGER NOT NULL DEFAULT 0,
                candidate_hash TEXT NOT NULL DEFAULT '',
                candidate_kind TEXT NOT NULL DEFAULT 'implicit',
                input_kind TEXT NOT NULL DEFAULT 'normal',
                cleaner_version TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                skip_reason TEXT,
                profile_generation INTEGER NOT NULL,
                observation_revision INTEGER NOT NULL UNIQUE,
                preprocess_lease_owner TEXT,
                preprocess_lease_expires_at TEXT,
                preprocess_lease_epoch INTEGER NOT NULL DEFAULT 0,
                observed_at TEXT NOT NULL,
                processed_at TEXT,
                FOREIGN KEY (source_message_id) REFERENCES messages(id) ON DELETE CASCADE,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS profile_extraction_batches (
                id TEXT PRIMARY KEY,
                trigger_kind TEXT NOT NULL,
                model_config_id TEXT,
                model_provider_snapshot TEXT,
                model_base_url_snapshot TEXT,
                model_name_snapshot TEXT,
                model_config_hash TEXT,
                profile_generation INTEGER NOT NULL,
                status TEXT NOT NULL,
                observation_count INTEGER NOT NULL,
                input_character_count INTEGER NOT NULL,
                estimated_input_tokens INTEGER NOT NULL DEFAULT 0,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                available_at TEXT NOT NULL,
                lease_owner TEXT,
                lease_expires_at TEXT,
                lease_epoch INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                FOREIGN KEY (model_config_id) REFERENCES model_configs(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS profile_batch_observations (
                batch_id TEXT NOT NULL,
                observation_id TEXT NOT NULL,
                batch_index INTEGER NOT NULL,
                PRIMARY KEY (batch_id, observation_id),
                FOREIGN KEY (batch_id) REFERENCES profile_extraction_batches(id) ON DELETE CASCADE,
                FOREIGN KEY (observation_id) REFERENCES profile_observations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS user_profile_facts (
                id TEXT PRIMARY KEY,
                dimension TEXT NOT NULL,
                normalized_value TEXT NOT NULL,
                display_value TEXT NOT NULL,
                category TEXT NOT NULL,
                global INTEGER NOT NULL,
                confidence REAL NOT NULL,
                extractor_model_config_id TEXT NOT NULL,
                last_observation_revision INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE (dimension, normalized_value)
            );

            CREATE TABLE IF NOT EXISTS user_profile_fact_sources (
                fact_id TEXT NOT NULL,
                observation_id TEXT,
                source_message_id TEXT,
                created_at TEXT NOT NULL,
                PRIMARY KEY (fact_id, observation_id),
                FOREIGN KEY (fact_id) REFERENCES user_profile_facts(id) ON DELETE CASCADE,
                FOREIGN KEY (observation_id) REFERENCES profile_observations(id) ON DELETE SET NULL,
                FOREIGN KEY (source_message_id) REFERENCES messages(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS profile_fact_tombstones (
                dimension TEXT NOT NULL,
                normalized_value TEXT NOT NULL,
                delete_revision INTEGER NOT NULL,
                deleted_at TEXT NOT NULL,
                PRIMARY KEY (dimension, normalized_value)
            );

            CREATE TABLE IF NOT EXISTS profile_batch_commits (
                batch_id TEXT PRIMARY KEY,
                lease_epoch INTEGER NOT NULL,
                applied_at TEXT NOT NULL,
                FOREIGN KEY (batch_id) REFERENCES profile_extraction_batches(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS profile_foreground_leases (
                owner TEXT PRIMARY KEY,
                expires_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profile_usage_attempts (
                id TEXT PRIMARY KEY,
                batch_id TEXT,
                started_at_utc TEXT NOT NULL,
                candidate_character_count INTEGER NOT NULL,
                estimated_input_tokens INTEGER NOT NULL,
                actual_input_tokens INTEGER NOT NULL DEFAULT 0,
                actual_output_tokens INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (batch_id) REFERENCES profile_extraction_batches(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_memory_embeddings_model
                ON memory_embeddings(model, dimensions);
            CREATE INDEX IF NOT EXISTS idx_memory_entity_links_entity
                ON memory_entity_links(entity_id, memory_id);
            CREATE INDEX IF NOT EXISTS idx_memory_relations_source
                ON memory_relations(source_entity_id, target_entity_id);
            CREATE INDEX IF NOT EXISTS idx_memory_relations_target
                ON memory_relations(target_entity_id, source_entity_id);
            CREATE INDEX IF NOT EXISTS idx_profile_observations_status_revision
                ON profile_observations(status, profile_generation, observation_revision);
            CREATE INDEX IF NOT EXISTS idx_profile_batches_status_available
                ON profile_extraction_batches(status, available_at);
            CREATE INDEX IF NOT EXISTS idx_profile_usage_started
                ON profile_usage_attempts(started_at_utc);
            CREATE INDEX IF NOT EXISTS idx_profile_fact_dimension
                ON user_profile_facts(dimension, updated_at);
            ",
        )?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO profile_state (id, profile_generation, next_event_revision, updated_at) VALUES (1, 1, 0, ?1)",
            params![now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO profile_settings
                (id, enabled, model_config_id, character_threshold, idle_seconds,
                 max_wait_seconds, long_input_threshold, rolling_hour_attempt_limit,
                 rolling_day_attempt_limit, rolling_day_candidate_character_limit, updated_at)
             VALUES (1, 0, NULL, 3000, 1800, 86400, 8000, 2, 8, 30000, ?1)",
            params![now],
        )?;
        Self::ensure_column(conn, "conversations", "project_path", "TEXT")?;
        Self::ensure_column(
            conn,
            "conversations",
            "archived",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        Self::ensure_column(conn, "conversations", "archived_at", "TEXT")?;
        Self::ensure_column(conn, "messages", "metadata_json", "TEXT")?;
        Self::ensure_column(
            conn,
            "profile_state",
            "skipped_observation_count",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "embedding_provider",
            "TEXT NOT NULL DEFAULT 'openai-compatible'",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "embedding_base_url",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "embedding_model",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "embedding_api_key",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "temperature",
            "REAL NOT NULL DEFAULT 0.4",
        )?;
        Self::ensure_column(conn, "model_configs", "max_tokens", "INTEGER")?;
        Self::ensure_column(
            conn,
            "model_configs",
            "context_window",
            "INTEGER NOT NULL DEFAULT 32768",
        )?;
        Self::ensure_column(conn, "model_configs", "top_p", "REAL")?;
        Self::ensure_column(
            conn,
            "model_configs",
            "reasoning_effort",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "model_kind",
            "TEXT NOT NULL DEFAULT 'chat'",
        )?;
        conn.execute(
            "UPDATE model_configs SET model_kind = 'embedding' WHERE id = 'embedding-config'",
            [],
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "routing_group",
            "TEXT NOT NULL DEFAULT '默认组'",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "routing_enabled",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "routing_cost",
            "INTEGER NOT NULL DEFAULT 3",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "routing_quality",
            "INTEGER NOT NULL DEFAULT 3",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "routing_speed",
            "INTEGER NOT NULL DEFAULT 3",
        )?;
        Self::ensure_column(
            conn,
            "model_configs",
            "routing_tasks_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        Self::ensure_column(
            conn,
            "mcp_servers",
            "transport",
            "TEXT NOT NULL DEFAULT 'stdio'",
        )?;
        Self::ensure_column(
            conn,
            "mcp_servers",
            "args_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        Self::ensure_column(
            conn,
            "mcp_servers",
            "env_json",
            "TEXT NOT NULL DEFAULT '{}'",
        )?;
        Self::ensure_column(conn, "mcp_servers", "url", "TEXT NOT NULL DEFAULT ''")?;
        Self::ensure_column(
            conn,
            "mcp_servers",
            "headers_json",
            "TEXT NOT NULL DEFAULT '{}'",
        )?;
        Self::ensure_column(
            conn,
            "mcp_servers",
            "working_dir",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        Self::ensure_column(conn, "mcp_servers", "enabled", "INTEGER NOT NULL DEFAULT 1")?;
        Self::ensure_column(
            conn,
            "ops_servers",
            "auth_method",
            "TEXT NOT NULL DEFAULT 'key'",
        )?;
        Self::ensure_column(conn, "ops_servers", "key_path", "TEXT NOT NULL DEFAULT ''")?;
        Self::ensure_column(conn, "ops_servers", "password", "TEXT NOT NULL DEFAULT ''")?;
        Self::ensure_column(
            conn,
            "ops_servers",
            "remote_dir",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        Ok(())
    }

    pub(super) fn ensure_column(
        conn: &Connection,
        table: &str,
        column: &str,
        definition: &str,
    ) -> AppResult<()> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;

        if !columns.iter().any(|name| name == column) {
            conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )?;
        }

        Ok(())
    }
}
