//! Complete run tasks using the project's existing runtime or static configuration.
use std::path::{Path, PathBuf};

use vp_js_runtime::{JsRuntimeProvider, NodeProvider, Platform};

mod providers;
mod tasks;

pub(crate) fn task_candidates(dir: &Path, prefix: &str) -> Vec<vp_complete::CompletionItem> {
    tasks::get_task_candidates_with(dir, prefix, resolve_offline_node, resolve_offline_bin_js)
}

fn node_store_dir() -> PathBuf {
    vp_shared::EnvConfig::get().dirs.js_runtime().join("node").as_path().to_path_buf()
}

pub(crate) fn resolve_offline_node() -> Option<PathBuf> {
    let config = vp_shared::EnvConfig::get();
    let system_node =
        || crate::shim::find_system_tool("node").map(|path| path.as_path().to_owned());
    if config.env_forbidden {
        return system_node();
    }
    let mode = vp_pm_cli::get_config_path()
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<vp_pm_cli::Config>(&bytes).ok())
        .unwrap_or_default()
        .node_shim_mode;
    if mode == vp_pm_cli::ShimMode::SystemFirst
        && let Some(node) = system_node()
    {
        return Some(node);
    }
    let store = node_store_dir();
    let mut versions = installed_node_versions();
    if let Some(pin) = config.node_version.as_deref()
        && let Some(pos) = versions.iter().position(|v| v == pin)
    {
        versions[..=pos].rotate_right(1);
    }
    let bin = NodeProvider::new().binary_relative_path(Platform::current());
    versions
        .into_iter()
        .map(|version| store.join(version).join(bin.as_str()))
        .find(|path| path.is_file())
        .or_else(|| {
            crate::shim::dispatch::find_external_tool_in("node", &vt_path::current_dir().ok()?)
                .map(|path| path.as_path().to_owned())
        })
}

fn installed_node_versions() -> Vec<String> {
    let mut versions = crate::commands::env::list_installed_versions(&node_store_dir());
    versions.retain(|version| js_semver::Version::parse(version).is_ok());
    versions.reverse();
    versions
}

pub(crate) fn resolve_offline_bin_js(dir: &Path) -> Option<PathBuf> {
    let abs_dir = vt_path::AbsolutePathBuf::new(dir.to_path_buf())?;
    if let Some(local_entry) = crate::js_executor::JsExecutor::resolve_local_vite_plus(&abs_dir) {
        return Some(local_entry.as_path().to_path_buf());
    }

    crate::js_executor::JsExecutor::new(None)
        .get_scripts_dir()
        .ok()
        .map(|d| d.join("bin.js"))
        .filter(|p| p.as_path().is_file())
        .map(|p| p.as_path().to_path_buf())
}
