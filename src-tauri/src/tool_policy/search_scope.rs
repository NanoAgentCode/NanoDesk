//! Conservative scope recognition for common read-only search commands.
//! This is a search-cost guard, not a general shell sandbox.
use super::validate_project_relative_path;

pub(super) fn is_broad_search(command: &str, root: &str) -> bool {
    let segments = split_commands(command);
    let changes_directory = segments
        .iter()
        .filter_map(|tokens| tokens.first())
        .any(|token| {
            matches!(
                token.to_ascii_lowercase().as_str(),
                "cd" | "chdir"
                    | "set-location"
                    | "sl"
                    | "push-location"
                    | "pushd"
                    | "pop-location"
                    | "popd"
            )
        });
    segments.iter().any(|tokens| {
        let Some((position, name)) = tokens.iter().enumerate().find_map(|(i, token)| {
            let lower = token.to_ascii_lowercase();
            let name = lower.strip_suffix(".exe").unwrap_or(&lower);
            matches!(
                name,
                "rg" | "grep" | "findstr" | "get-childitem" | "gci" | "dir" | "ls" | "where"
            )
            .then(|| (i, name.to_string()))
        }) else {
            return false;
        };
        let args = &tokens[position + 1..];
        let recursive = name == "rg"
            || args.iter().any(|arg| {
                let lower = arg.to_ascii_lowercase();
                matches!(lower.as_str(), "-recurse" | "/s" | "-r" | "--recursive")
                    || (name == "grep"
                        && lower.starts_with('-')
                        && !lower.starts_with("--")
                        && lower.contains('r'))
            });
        let piped = tokens.first().map(String::as_str) == Some("|stdin|");
        recursive && (changes_directory || !has_bounded_paths(&name, args, root, piped))
    })
}

fn has_bounded_paths(name: &str, args: &[String], root: &str, piped: bool) -> bool {
    let listing = matches!(name, "get-childitem" | "gci" | "dir" | "ls" | "where")
        || args.iter().any(|arg| arg == "--files");
    let mut needs_pattern = !listing;
    let mut paths = Vec::new();
    let mut i = 0;
    let mut options = true;
    while i < args.len() {
        let arg = &args[i];
        let lower = arg.to_ascii_lowercase();
        if options && arg == "--" {
            options = false;
        } else if options && matches!(lower.as_str(), "-path" | "-literalpath") {
            i += 1;
            let Some(path) = args.get(i) else {
                return false;
            };
            paths.push(path.as_str());
        } else if options
            && matches!(
                lower.as_str(),
                "-g" | "--glob"
                    | "-t"
                    | "--type"
                    | "-filter"
                    | "-include"
                    | "-exclude"
                    | "--max-count"
                    | "-m"
                    | "--max-depth"
            )
        {
            i += 1;
            if i >= args.len() {
                return false;
            }
        } else if options && matches!(lower.as_str(), "-e" | "--regexp" | "-pattern") {
            i += 1;
            if i >= args.len() {
                return false;
            }
            needs_pattern = false;
        } else if options
            && (arg.starts_with('-')
                || (name == "findstr" && arg.starts_with('/'))
                || (name == "dir" && arg.starts_with('/')))
        {
            // Unknown options cannot establish a bounded scope accidentally.
            if !matches!(
                lower.as_str(),
                "-n" | "--line-number"
                    | "-i"
                    | "--ignore-case"
                    | "-r"
                    | "-rn"
                    | "-rin"
                    | "-rni"
                    | "--recursive"
                    | "-recurse"
                    | "-file"
                    | "-directory"
                    | "-force"
                    | "-name"
                    | "--files"
                    | "-l"
                    | "--files-with-matches"
                    | "-s"
                    | "--case-sensitive"
                    | "-f"
                    | "--fixed-strings"
                    | "--hidden"
                    | "/s"
                    | "/b"
                    | "/n"
                    | "/i"
            ) {
                return false;
            }
        } else if needs_pattern {
            needs_pattern = false;
        } else {
            paths.push(arg.as_str());
        }
        i += 1;
    }
    if paths.is_empty() {
        // A plain text filter consumes upstream output rather than scanning cwd.
        return piped
            && !listing
            && matches!(name, "rg" | "grep" | "findstr")
            && !args.iter().any(|arg| {
                matches!(
                    arg.to_ascii_lowercase().as_str(),
                    "-r" | "-rn" | "-rin" | "-rni" | "--recursive" | "/s"
                )
            });
    }
    paths.iter().all(|path| bounded_path(path, root))
}

fn bounded_path(path: &str, root: &str) -> bool {
    // Do not infer a safe directory from shell expansion or traversal.
    if path.contains(['$', '`', '(', ')', '{', '}', ',', ';', '~']) {
        return false;
    }
    let normalized = path.replace('\\', "/");
    if normalized.split('/').any(|part| part == "..") {
        return false;
    }
    // A wildcard is bounded only when its fixed directory is a proper child.
    let fixed = if let Some(index) = normalized.find(['*', '?', '[']) {
        let Some(slash) = normalized[..index].rfind('/') else {
            return false;
        };
        &normalized[..slash]
    } else {
        normalized.as_str()
    };
    let Ok(relative) = validate_project_relative_path(root, fixed) else {
        return false;
    };
    let Ok(project) = super::project_root(root) else {
        return false;
    };
    let Ok(target) = project.join(relative).canonicalize() else {
        return false;
    };
    target != project && target.starts_with(&project)
}

fn split_commands(command: &str) -> Vec<Vec<String>> {
    let mut segments = vec![Vec::new()];
    let mut token = String::new();
    let mut quote = None;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(delimiter) = quote {
            if ch == delimiter {
                quote = None;
            } else {
                token.push(ch);
            }
        } else if matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if ch.is_whitespace() || matches!(ch, ';' | '&' | '|') {
            if !token.is_empty() {
                segments
                    .last_mut()
                    .unwrap()
                    .push(std::mem::take(&mut token));
            }
            if matches!(ch, ';' | '&' | '|' | '\n' | '\r') {
                segments.push(Vec::new());
                if ch == '|' && chars.peek() == Some(&'|') {
                    chars.next(); // Conditional OR does not provide stdin.
                } else if ch == '|' {
                    segments.last_mut().unwrap().push("|stdin|".to_string());
                }
            }
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        segments.last_mut().unwrap().push(token);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_scope_uses_real_paths_instead_of_code_keywords() {
        let root = std::env::temp_dir().join(format!("search-scope-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("src/components")).unwrap();
        std::fs::create_dir_all(root.join("source files")).unwrap();
        std::fs::write(root.join("src/app.rs"), "").unwrap();
        let project = root.to_str().unwrap();
        for command in [
            "Get-ChildItem -Path src/components -Recurse -Filter *.tsx",
            "gci './source files' -Recurse -Include *.ts",
            "rg -n service src/components",
            "rg -n -g '*.rs' service src",
            "rg --files src",
            "rg -e service src/components",
            "grep -rn service src/components",
            "dir /s src/*.rs",
            "findstr /s /n service src/*.rs",
            "rg service src/app.rs",
            "rg cd src",
            "rg service src | rg model",
            "Get-ChildItem src -Recurse | Select-String service",
            "git status --short",
        ] {
            assert!(
                !is_broad_search(command, project),
                "should allow: {command}"
            );
        }
        assert!(is_broad_search(
            &format!("rg service '{}'", root.display()),
            project
        ));
        for command in [
            "rg service",
            "rg --files",
            "rg service .",
            "rg service ./",
            "Get-ChildItem -Recurse -Filter *.tsx",
            "dir /s *.rs",
            "grep -r service .",
            "rg service src .",
            "rg service ../outside",
            "rg service src/../",
            "rg service $env:HOME",
            "rg service missing",
            "rg -g src service",
            "rg service src; rg service .",
            "rg service src\nrg service .",
            "cd ..; rg service src",
            "rg service src | rg model .",
            "rg service src | rg --files",
            "rg service src || rg service",
            "rg service .git",
        ] {
            assert!(is_broad_search(command, project), "should block: {command}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
