use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{backup::Backup, params, Connection, OpenFlags, OptionalExtension};

use crate::brand;
use crate::error::AppResult;

const DOMAIN_SUFFIXES: &[&str] = &["config", "conversations", "knowledge", "project-index"];
const STANDALONE_SUFFIXES: &[&str] = &["runtime", "observability"];
pub(crate) const MIGRATION_KEY: &str = "legacy-brand-import-v1";
pub(crate) const SEARCH_INDEX_REBUILD_KEY: &str = "legacy-brand-fts-rebuild-v1";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct LegacyMigrationReport {
    pub databases: usize,
    pub files: usize,
}

/// Imports legacy NanoAgent data without overwriting any NanoDesk data.
///
/// Split databases are preferred. A pre-split monolithic database is copied to
/// each domain target; `Database::open` then keeps only the tables owned by that
/// domain. SQLite's online backup API makes the snapshot safe even when WAL is
/// present, and a temporary target keeps partial migrations invisible.
pub fn migrate_legacy_app_data(current_dir: &Path) -> AppResult<LegacyMigrationReport> {
    crate::db::register_sqlite_vec_extension();
    fs::create_dir_all(current_dir)?;
    let sibling_legacy_dir = legacy_sibling_dir(current_dir);
    let mut report = LegacyMigrationReport::default();

    for suffix in DOMAIN_SUFFIXES {
        let target = database_path(current_dir, brand::STORAGE_PREFIX, Some(suffix));
        let source = find_legacy_database(current_dir, sibling_legacy_dir.as_deref(), Some(suffix))
            .or_else(|| find_legacy_database(current_dir, sibling_legacy_dir.as_deref(), None));
        if let Some(source) = source {
            let mut changed = false;
            if !target.exists() {
                backup_database(&source, &target)?;
                mark_database_imported(&target)?;
                changed = true;
            } else if merge_database_without_overwrite(&source, &target)? {
                changed = true;
            }
            if changed {
                report.databases += 1;
            }
        }
    }

    for suffix in STANDALONE_SUFFIXES {
        let target = database_path(current_dir, brand::STORAGE_PREFIX, Some(suffix));
        if let Some(source) =
            find_legacy_database(current_dir, sibling_legacy_dir.as_deref(), Some(suffix))
        {
            let mut changed = false;
            if !target.exists() {
                backup_database(&source, &target)?;
                mark_database_imported(&target)?;
                changed = true;
            } else if merge_database_without_overwrite(&source, &target)? {
                changed = true;
            }
            if changed {
                report.databases += 1;
            }
        }
    }

    if let Some(legacy_dir) = sibling_legacy_dir.as_deref() {
        let settings_source = legacy_dir.join("settings.json");
        let settings_target = current_dir.join("settings.json");
        if settings_source.is_file() && !settings_target.exists() {
            fs::copy(settings_source, settings_target)?;
            report.files += 1;
        }
        let legacy_temp = legacy_dir.join("temp");
        if legacy_temp.is_dir() {
            report.files += copy_tree_without_overwrite(&legacy_temp, &current_dir.join("temp"))?;
        }
    }

    Ok(report)
}

fn legacy_sibling_dir(current_dir: &Path) -> Option<PathBuf> {
    let is_default_dir = current_dir
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(brand::IDENTIFIER));
    if !is_default_dir {
        return None;
    }
    current_dir
        .parent()
        .map(|parent| parent.join(brand::LEGACY_IDENTIFIER))
}

fn find_legacy_database(
    current_dir: &Path,
    sibling_legacy_dir: Option<&Path>,
    suffix: Option<&str>,
) -> Option<PathBuf> {
    let name = database_name(brand::LEGACY_STORAGE_PREFIX, suffix);
    [Some(current_dir), sibling_legacy_dir]
        .into_iter()
        .flatten()
        .map(|dir| dir.join(&name))
        .find(|path| path.is_file())
}

fn database_path(dir: &Path, prefix: &str, suffix: Option<&str>) -> PathBuf {
    dir.join(database_name(prefix, suffix))
}

fn database_name(prefix: &str, suffix: Option<&str>) -> String {
    match suffix {
        Some(suffix) => format!("{prefix}-{suffix}.sqlite3"),
        None => format!("{prefix}.sqlite3"),
    }
}

fn backup_database(source: &Path, target: &Path) -> AppResult<()> {
    let temp = target.with_extension(format!("sqlite3.migrating-{}", uuid::Uuid::new_v4()));
    let result = (|| -> AppResult<()> {
        let source = Connection::open_with_flags(
            source,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut destination = Connection::open(&temp)?;
        let backup = Backup::new(&source, &mut destination)?;
        backup.run_to_completion(64, Duration::from_millis(5), None)?;
        drop(backup);
        drop(destination);
        fs::rename(&temp, target)?;
        Ok(())
    })();
    if result.is_err() && temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn mark_database_imported(target: &Path) -> AppResult<()> {
    let connection = Connection::open(target)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS storage_migrations (
            key TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO storage_migrations (key, applied_at) VALUES (?1, ?2)",
        params![MIGRATION_KEY, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn merge_database_without_overwrite(source: &Path, target: &Path) -> AppResult<bool> {
    let connection = Connection::open(target)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS storage_migrations (
            key TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;
    let imported = connection
        .query_row(
            "SELECT 1 FROM storage_migrations WHERE key = ?1",
            params![MIGRATION_KEY],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if imported {
        return Ok(false);
    }

    connection.execute(
        "ATTACH DATABASE ?1 AS legacy",
        params![source.to_string_lossy()],
    )?;
    connection.execute_batch("PRAGMA foreign_keys = OFF; BEGIN IMMEDIATE;")?;
    let result = (|| -> AppResult<()> {
        let virtual_tables = virtual_table_names(&connection)?;
        let tables = ordinary_table_names(&connection)?;
        for table in tables {
            if table == "storage_migrations"
                || table == "memory_embeddings"
                || is_virtual_table_or_shadow(&table, &virtual_tables)
                || !table_exists(&connection, "legacy", &table)?
            {
                continue;
            }
            copy_common_columns(&connection, &table)?;
        }
        connection.execute(
            "INSERT INTO storage_migrations (key, applied_at) VALUES (?1, ?2)",
            params![MIGRATION_KEY, chrono::Utc::now().to_rfc3339()],
        )?;
        connection.execute_batch("COMMIT;")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    let detach_result =
        connection.execute_batch("DETACH DATABASE legacy; PRAGMA foreign_keys = ON;");
    result?;
    detach_result?;
    Ok(true)
}

fn ordinary_table_names(connection: &Connection) -> AppResult<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT name FROM main.sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

fn virtual_table_names(connection: &Connection) -> AppResult<Vec<String>> {
    let mut names = Vec::new();
    for schema in ["main", "legacy"] {
        let mut statement = connection.prepare(&format!(
            "SELECT name FROM {schema}.sqlite_master WHERE sql LIKE 'CREATE VIRTUAL TABLE%'"
        ))?;
        names.extend(
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn is_virtual_table_or_shadow(table: &str, virtual_tables: &[String]) -> bool {
    virtual_tables.iter().any(|virtual_table| {
        table == virtual_table || table.starts_with(&format!("{virtual_table}_"))
    })
}

fn table_exists(connection: &Connection, schema: &str, table: &str) -> AppResult<bool> {
    connection
        .query_row(
            &format!("SELECT 1 FROM {schema}.sqlite_master WHERE type = 'table' AND name = ?1"),
            params![table],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(Into::into)
}

fn copy_common_columns(connection: &Connection, table: &str) -> AppResult<()> {
    let target_columns = table_columns(connection, "main", table)?;
    let source_columns = table_columns(connection, "legacy", table)?;
    let source_columns = source_columns
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let columns = target_columns
        .into_iter()
        .filter(|column| source_columns.contains(column))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Ok(());
    }
    let columns = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>()
        .join(", ");
    connection.execute_batch(&format!(
        "INSERT OR IGNORE INTO main.{} ({columns}) SELECT {columns} FROM legacy.{};",
        quote_identifier(table),
        quote_identifier(table)
    ))?;
    Ok(())
}

fn table_columns(connection: &Connection, schema: &str, table: &str) -> AppResult<Vec<String>> {
    let mut statement = connection.prepare(&format!(
        "PRAGMA {}.table_info({})",
        quote_identifier(schema),
        quote_identifier(table)
    ))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns)
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn copy_tree_without_overwrite(source: &Path, target: &Path) -> AppResult<usize> {
    fs::create_dir_all(target)?;
    let mut copied = 0;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            copied += copy_tree_without_overwrite(&entry.path(), &destination)?;
        } else if file_type.is_file() && !destination.exists() {
            fs::copy(entry.path(), destination)?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nanodesk-legacy-{label}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn imports_split_databases_without_overwriting_current_data() {
        let root = temp_root("split");
        let current = root.join(brand::IDENTIFIER);
        let legacy = root.join(brand::LEGACY_IDENTIFIER);
        fs::create_dir_all(&legacy).unwrap();
        let source = database_path(&legacy, brand::LEGACY_STORAGE_PREFIX, Some("config"));
        Connection::open(&source)
            .unwrap()
            .execute_batch(
                "CREATE TABLE marker (value TEXT NOT NULL);
                 INSERT INTO marker VALUES ('legacy');",
            )
            .unwrap();
        fs::write(legacy.join("settings.json"), "{\"legacy\":true}").unwrap();

        let first = migrate_legacy_app_data(&current).unwrap();
        assert_eq!(first.databases, 1);
        assert_eq!(first.files, 1);
        let target = database_path(&current, brand::STORAGE_PREFIX, Some("config"));
        assert_eq!(
            Connection::open(&target)
                .unwrap()
                .query_row("SELECT COUNT(*) FROM marker", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );

        Connection::open(&target)
            .unwrap()
            .execute("UPDATE marker SET value = 'current'", [])
            .unwrap();
        Connection::open(&source)
            .unwrap()
            .execute("UPDATE marker SET value = 'changed legacy'", [])
            .unwrap();
        assert_eq!(migrate_legacy_app_data(&current).unwrap().databases, 0);
        assert_eq!(
            Connection::open(&target)
                .unwrap()
                .query_row("SELECT value FROM marker", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "current"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn merges_legacy_rows_into_an_existing_target_without_overwriting_conflicts() {
        let root = temp_root("merge");
        let current = root.join(brand::IDENTIFIER);
        let legacy = root.join(brand::LEGACY_IDENTIFIER);
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();
        let source = database_path(&legacy, brand::LEGACY_STORAGE_PREFIX, Some("config"));
        let target = database_path(&current, brand::STORAGE_PREFIX, Some("config"));
        for (path, values) in [
            (&source, "('shared', 'legacy'), ('legacy-only', 'legacy')"),
            (
                &target,
                "('shared', 'current'), ('current-only', 'current')",
            ),
        ] {
            Connection::open(path)
                .unwrap()
                .execute_batch(&format!(
                    "CREATE TABLE records (id TEXT PRIMARY KEY, value TEXT NOT NULL);
                     INSERT INTO records VALUES {values};"
                ))
                .unwrap();
        }

        assert_eq!(migrate_legacy_app_data(&current).unwrap().databases, 1);
        let connection = Connection::open(target).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT value FROM records WHERE id = 'shared'", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "current"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM records", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(migrate_legacy_app_data(&current).unwrap().databases, 0);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fans_out_a_monolithic_database_for_domain_initialization() {
        let root = temp_root("monolith");
        let current = root.join(brand::IDENTIFIER);
        let legacy = root.join(brand::LEGACY_IDENTIFIER);
        fs::create_dir_all(&legacy).unwrap();
        Connection::open(database_path(&legacy, brand::LEGACY_STORAGE_PREFIX, None))
            .unwrap()
            .execute("CREATE TABLE marker (value TEXT NOT NULL)", [])
            .unwrap();

        let report = migrate_legacy_app_data(&current).unwrap();
        assert_eq!(report.databases, DOMAIN_SUFFIXES.len());
        for suffix in DOMAIN_SUFFIXES {
            assert!(database_path(&current, brand::STORAGE_PREFIX, Some(suffix)).exists());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn monolithic_model_configuration_survives_domain_initialization() {
        let root = temp_root("monolith-open");
        let current = root.join(brand::IDENTIFIER);
        let legacy = root.join(brand::LEGACY_IDENTIFIER);
        fs::create_dir_all(&legacy).unwrap();
        let source = database_path(&legacy, brand::LEGACY_STORAGE_PREFIX, None);
        Connection::open(&source)
            .unwrap()
            .execute_batch(
                "CREATE TABLE model_configs (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, provider TEXT NOT NULL,
                    base_url TEXT NOT NULL, model TEXT NOT NULL, api_key TEXT NOT NULL,
                    temperature REAL NOT NULL DEFAULT 0.4, max_tokens INTEGER,
                    context_window INTEGER NOT NULL DEFAULT 32768, top_p REAL,
                    reasoning_effort TEXT NOT NULL DEFAULT '',
                    embedding_provider TEXT NOT NULL DEFAULT 'openai-compatible',
                    embedding_base_url TEXT NOT NULL DEFAULT '',
                    embedding_model TEXT NOT NULL DEFAULT '',
                    embedding_api_key TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
                );
                INSERT INTO model_configs
                    (id, name, provider, base_url, model, api_key, created_at, updated_at)
                VALUES ('legacy-model', 'Legacy', 'openai-compatible', 'http://localhost',
                        'legacy', 'secret', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
            )
            .unwrap();

        migrate_legacy_app_data(&current).unwrap();
        let database =
            crate::db::Database::open(database_path(&current, brand::STORAGE_PREFIX, None))
                .expect("migrated domain databases should initialize");
        assert_eq!(database.list_model_configs().unwrap().len(), 1);
        assert_eq!(
            database.get_model_config("legacy-model").unwrap().api_key,
            "secret"
        );
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }
}
