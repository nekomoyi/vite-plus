use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::spec::{CompletionDirective, CompletionItem, CompletionOutput, ItemKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathMode {
    Any,
    DirsOnly,
    FilesWithExtensions(Vec<String>),
}

#[inline]
fn current_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

#[cold]
#[must_use]
pub fn resolve_dir_with_home(base: &Path, home: Option<&Path>, raw: &str) -> PathBuf {
    if let Some(home_dir) = home {
        if raw == "~" {
            return home_dir.to_path_buf();
        }
        if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
            return home_dir.join(rest.trim_start_matches(['/', '\\']));
        }
    }
    let path = Path::new(raw);
    if path.is_absolute() { path.to_path_buf() } else { base.join(path) }
}

#[inline]
#[must_use]
pub fn resolve_dir(base: &Path, raw: &str) -> PathBuf {
    resolve_dir_with_home(base, current_home_dir().as_deref(), raw)
}

#[inline]
#[must_use]
pub fn resolve_dir_literal(base: &Path, raw: &str) -> PathBuf {
    resolve_dir_with_home(base, None, raw)
}

#[cold]
pub(crate) fn expand_tilde_item_with_home(val: &mut String, home: Option<&Path>) {
    let Some(home_dir) = home else { return };
    let sep = if val.starts_with("~\\") {
        '\\'
    } else if val.starts_with("~/") {
        '/'
    } else {
        return;
    };
    let rest = &val[2..];
    let home_str = home_dir.to_string_lossy();
    let base = home_str.trim_end_matches(['/', '\\']);
    *val = format!("{base}{sep}{rest}");
}

#[inline]
pub(crate) fn expand_tilde_item(val: &mut String) {
    expand_tilde_item_with_home(val, current_home_dir().as_deref());
}

#[inline]
pub fn complete_path(cwd: &Path, prefix: &str, mode: PathMode) -> CompletionOutput {
    complete_path_with_home(cwd, current_home_dir().as_deref(), prefix, mode)
}

#[cold]
pub fn complete_path_with_home(
    cwd: &Path,
    home: Option<&Path>,
    prefix: &str,
    mode: PathMode,
) -> CompletionOutput {
    if prefix == "~" {
        return if home.is_some_and(Path::is_dir) {
            CompletionOutput::new(
                vec![CompletionItem::directory("~/")],
                CompletionDirective::NO_FILE_COMP | CompletionDirective::NO_SPACE,
            )
            .with_path_completion(mode, "")
        } else {
            CompletionOutput::new(Vec::new(), CompletionDirective::NO_FILE_COMP)
                .with_path_completion(mode, "")
        };
    }

    let last_slash = prefix.rfind(['/', '\\']);
    let (dir_prefix, name_prefix) = match last_slash {
        Some(idx) => (&prefix[..=idx], &prefix[idx + 1..]),
        None => ("", prefix),
    };

    let search_dir: PathBuf = if let Some(rest) =
        dir_prefix.strip_prefix("~/").or_else(|| dir_prefix.strip_prefix("~\\"))
    {
        let Some(home_dir) = home else {
            return CompletionOutput::new(Vec::new(), CompletionDirective::NO_FILE_COMP)
                .with_path_completion(mode, "");
        };
        home_dir.join(rest.trim_start_matches(['/', '\\']))
    } else if Path::new(dir_prefix).is_absolute() {
        PathBuf::from(dir_prefix)
    } else if dir_prefix.is_empty() {
        cwd.to_path_buf()
    } else {
        cwd.join(dir_prefix)
    };

    let Ok(read_dir) = fs::read_dir(&search_dir) else {
        return CompletionOutput::new(Vec::new(), CompletionDirective::NO_FILE_COMP)
            .with_path_completion(mode, "");
    };

    let mut items = Vec::new();

    if (name_prefix == "." || name_prefix == "..") && search_dir.join("..").is_dir() {
        items.push(CompletionItem::directory(format!("{dir_prefix}../")));
    }
    if name_prefix == "." && search_dir.is_dir() {
        items.push(CompletionItem::directory(format!("{dir_prefix}./")));
    }

    for entry_res in read_dir {
        let Ok(entry) = entry_res else { continue };
        let Ok(file_name) = entry.file_name().into_string() else { continue };

        if !name_prefix.starts_with('.') && file_name.starts_with('.') {
            continue;
        }
        if !file_name.starts_with(name_prefix) {
            continue;
        }

        let is_dir = entry.path().is_dir();

        match &mode {
            PathMode::DirsOnly if !is_dir => continue,
            PathMode::FilesWithExtensions(exts) if !is_dir && !exts.is_empty() => {
                let matches_ext = exts.iter().any(|ext| {
                    let clean = ext.strip_prefix('.').unwrap_or(ext);
                    file_name.strip_suffix(clean).is_some_and(|s| s.ends_with('.'))
                });
                if !matches_ext {
                    continue;
                }
            }
            _ => {}
        }

        if is_dir {
            items.push(CompletionItem::directory(format!("{dir_prefix}{file_name}/")));
        } else {
            items.push(CompletionItem::file(format!("{dir_prefix}{file_name}")));
        }
    }

    CompletionItem::sort_by_value(&mut items);

    let mut directive = CompletionDirective::NO_FILE_COMP;
    if items.iter().any(|it| it.kind == ItemKind::Directory) {
        directive |= CompletionDirective::NO_SPACE;
    }

    CompletionOutput::new(items, directive).with_path_completion(mode, "")
}
