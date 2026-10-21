//! Best-effort, bounded workspace snapshots for successful command products.
use std::{collections::BTreeMap, path::{Path, PathBuf}, time::{Instant, SystemTime, Duration}};

type Snapshot = BTreeMap<String, (u64, SystemTime)>;

pub(super) async fn capture(root: PathBuf) -> Option<Snapshot> {
    tokio::task::spawn_blocking(move || scan(&root)).await.ok().flatten()
}

fn scan(root: &Path) -> Option<Snapshot> {
    fn walk(root: &Path, dir: &Path, depth: usize, count: &mut usize, start: Instant, files: &mut Snapshot) -> Option<()> {
        if depth > 16 || start.elapsed() > Duration::from_secs(2) { return None; }
        for entry in std::fs::read_dir(dir).ok()? {
            *count += 1;
            if *count > 20_000 || start.elapsed() > Duration::from_secs(2) { return None; }
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if [".git", ".codegraph", crate::brand::PROJECT_DATA_DIRECTORY, "node_modules", "target", ".venv", "venv", "__pycache__"].contains(&name.as_str()) { continue; }
            let kind = entry.file_type().ok()?;
            if kind.is_symlink() { continue; }
            let path = entry.path();
            if kind.is_dir() { walk(root, &path, depth + 1, count, start, files)?; }
            else if kind.is_file() {
                let meta = entry.metadata().ok()?;
                files.insert(path.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/"), (meta.len(), meta.modified().ok()?));
            }
        }
        Some(())
    }
    let mut files = Snapshot::new();
    walk(root, root, 0, &mut 0, Instant::now(), &mut files)?;
    Some(files)
}

pub(super) fn changed(before: &Snapshot, after: &Snapshot) -> Vec<String> {
    after.iter().filter(|(path, stamp)| before.get(*path) != Some(*stamp)).map(|(path, _)| path.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn records_script_outputs_and_modifications_but_not_old_deleted_or_internal_files() {
        let root = std::env::temp_dir().join(format!("nano-products-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("old.txt"), "old").unwrap();
        std::fs::write(root.join("deleted.txt"), "delete").unwrap();
        std::fs::write(root.join("changed.txt"), "before").unwrap();
        let before = capture(root.clone()).await.unwrap();
        let command = if cfg!(windows) { "[IO.File]::WriteAllBytes((Join-Path $PWD 'poem.docx'), [byte[]](1,2,3))" } else { "printf 'doc' > poem.docx" };
        crate::shell::run_project_command(&root, command, None).await.unwrap();
        std::fs::write(root.join("changed.txt"), "after with different size").unwrap();
        std::fs::write(root.join(".git/private"), "private").unwrap();
        std::fs::remove_file(root.join("deleted.txt")).unwrap();
        let after = capture(root.clone()).await.unwrap();
        assert_eq!(changed(&before, &after), vec!["changed.txt", "poem.docx"]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
