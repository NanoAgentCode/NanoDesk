use super::types::FileStamp;
use crate::error::{AppError, AppResult};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::UNIX_EPOCH;

pub(super) fn changed_paths(
    before: &BTreeMap<String, FileStamp>,
    after: &BTreeMap<String, FileStamp>,
) -> BTreeSet<String> {
    before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .cloned()
        .collect()
}

pub(super) fn scan_files(root: &Path, recursive: bool) -> AppResult<BTreeMap<String, FileStamp>> {
    fn walk(
        root: &Path,
        dir: &Path,
        recursive: bool,
        files: &mut BTreeMap<String, FileStamp>,
    ) -> AppResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if [
                ".git",
                ".codegraph",
                crate::brand::PROJECT_DATA_DIRECTORY,
                "node_modules",
                "target",
            ]
            .contains(&name.as_str())
            {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() && recursive {
                walk(root, &path, recursive, files)?;
            } else if kind.is_file() {
                let meta = entry.metadata()?;
                let modified_ns = meta
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                files.insert(
                    path.strip_prefix(root)
                        .map_err(|_| AppError::from("目录边界错误"))?
                        .to_string_lossy()
                        .replace('\\', "/"),
                    FileStamp {
                        modified_ns,
                        size: meta.len(),
                    },
                );
                if files.len() > 20000 {
                    return Err("监听目录超过 20000 个文件，请缩小目录范围。".into());
                }
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, recursive, &mut files)?;
    Ok(files)
}
