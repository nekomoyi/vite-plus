use std::path::Path;

use vp_complete::{
    CompletionContext, CompletionDirective, CompletionItem, CompletionOutput, ItemKind, PathMode,
};
pub(super) struct PkgManifest {
    pub(super) scripts: Vec<(String, String)>,
    deps: Vec<(String, String, &'static str)>,
}

fn read_pkg_manifest(dir: &Path) -> Option<PkgManifest> {
    let bytes = std::fs::read(dir.join("package.json")).ok()?;
    let stripped = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let val: serde_json::Value = serde_json::from_slice(stripped).ok()?;
    let obj = val.as_object()?;

    let mut scripts = Vec::new();
    if let Some(s) = obj.get("scripts").and_then(|v| v.as_object()) {
        for (k, v) in s {
            if let Some(cmd) = v.as_str() {
                scripts.push((k.clone(), cmd.to_string()));
            }
        }
    }

    let mut deps = Vec::new();
    for (field, label) in [
        ("dependencies", "dependency"),
        ("devDependencies", "devDependency"),
        ("optionalDependencies", "optionalDependency"),
        ("peerDependencies", "peerDependency"),
    ] {
        if let Some(dep_obj) = obj.get(field).and_then(|v| v.as_object()) {
            for (k, v) in dep_obj {
                deps.push((k.clone(), v.as_str().unwrap_or("").to_string(), label));
            }
        }
    }

    Some(PkgManifest { scripts, deps })
}

pub(super) fn enclosing_manifest(dir: &Path) -> Option<PkgManifest> {
    dir.ancestors().find_map(read_pkg_manifest)
}

#[must_use]
pub(super) fn workspace_package_names(cwd: &Path) -> Vec<String> {
    let Some(cwd) = vt_path::AbsolutePath::new(cwd) else { return Vec::new() };
    let Ok(graph) = vt_workspace::discover_package_graph(cwd) else { return Vec::new() };
    let mut names: Vec<_> = graph
        .node_weights()
        .map(|pkg| pkg.package_json.name.as_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

pub(super) fn complete_manifest_deps(ctx: &CompletionContext) -> CompletionOutput {
    let mut items: Vec<CompletionItem> = Vec::new();

    if let Some(pkg) = enclosing_manifest(&ctx.cwd) {
        for (pkg_name, ver_str, label) in pkg.deps {
            if pkg_name.starts_with(&ctx.prefix) && !items.iter().any(|it| it.value == pkg_name) {
                let desc = if ver_str.is_empty() {
                    label.to_string()
                } else {
                    format!("{label} ({ver_str})")
                };
                items.push(CompletionItem::value(pkg_name, Some(desc)));
            }
        }
    }

    for name in workspace_package_names(&ctx.cwd) {
        if name.starts_with(&ctx.prefix) && !items.iter().any(|it| it.value == name) {
            items.push(CompletionItem::value(name, Some("workspace package")));
        }
    }

    CompletionItem::sort_by_value(&mut items);
    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

pub(super) fn complete_workspace_pkgs(ctx: &CompletionContext) -> CompletionOutput {
    let mut items = Vec::new();
    for name in workspace_package_names(&ctx.cwd) {
        if name.starts_with(&ctx.prefix) {
            items.push(CompletionItem::value(name, Some("workspace package")));
        }
    }
    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

fn normalize_bin_filename(name: &str) -> &str {
    if cfg!(windows) {
        for ext in ".cmd,.exe,.ps1".split(',') {
            if let Some(stem) = name.strip_suffix(ext) {
                return stem;
            }
        }
    }
    name
}

fn scan_bin_dir(
    dir: &Path,
    prefix: &str,
    label: &str,
    skip_vp: bool,
    items: &mut Vec<CompletionItem>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let clean_name = normalize_bin_filename(&name_str);
        if !(skip_vp && matches!(clean_name, "vp" | "vpr" | "vpx"))
            && clean_name.starts_with(prefix)
            && !items.iter().any(|it| it.value == clean_name)
        {
            items.push(CompletionItem::value(clean_name, Some(label)));
        }
    }
}

pub(super) fn complete_bins(ctx: &CompletionContext) -> CompletionOutput {
    let mut items = Vec::new();

    for dir in ctx.cwd.ancestors() {
        let bin_dir = dir.join("node_modules/.bin");
        if bin_dir.is_dir() {
            scan_bin_dir(&bin_dir, &ctx.prefix, "local bin", false, &mut items);
        }
    }

    let gbin = super::global_bin_dir();
    if gbin.is_dir() {
        scan_bin_dir(&gbin, &ctx.prefix, "global bin", true, &mut items);
    }

    CompletionItem::sort_by_value(&mut items);
    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

pub(super) fn complete_installed_nodes(ctx: &CompletionContext) -> CompletionOutput {
    let mut items = Vec::new();
    for ver in super::installed_node_versions() {
        if ver.starts_with(&ctx.prefix) {
            items.push(CompletionItem::value(ver, Some("installed node")));
        }
    }

    for alias in "lts,latest".split(',') {
        if alias.starts_with(&ctx.prefix) {
            items.push(CompletionItem::value(alias, Some("alias")));
        }
    }

    CompletionOutput::new(
        items,
        CompletionDirective::KEEP_ORDER | CompletionDirective::NO_FILE_COMP,
    )
}

pub(super) fn complete_vpx_target(ctx: &CompletionContext) -> CompletionOutput {
    if ctx.positional_index != 0 {
        return CompletionOutput::new(vec![], CompletionDirective::DEFAULT);
    }

    let script_mode = || {
        let mut exts = Vec::with_capacity(8);
        for ext in "ts,mts,cts,tsx,js,mjs,cjs,jsx".split(',') {
            exts.push(ext.to_string());
        }
        PathMode::FilesWithExtensions(exts)
    };

    let is_path_prefix = ctx.prefix == "."
        || ctx.prefix == ".."
        || ctx.prefix.starts_with("./")
        || ctx.prefix.starts_with("../")
        || ctx.prefix.starts_with('/')
        || ctx.prefix.starts_with('~')
        || ctx.prefix.contains('/')
        || ctx.prefix.contains('\\');

    if is_path_prefix {
        return ctx.complete_path(script_mode());
    }

    let bin_out = complete_bins(ctx);
    let path_out = ctx.complete_path(script_mode());

    let mut items = bin_out.items;
    for it in path_out.items {
        if !items.iter().any(|existing| existing.value == it.value) {
            items.push(it);
        }
    }

    let mut directive = CompletionDirective::NO_FILE_COMP;
    if items.iter().any(|it| it.kind == ItemKind::Directory) {
        directive |= CompletionDirective::NO_SPACE;
    }

    CompletionOutput::new(items, directive)
}
