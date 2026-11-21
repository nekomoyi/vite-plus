//! Shell autocompletion host integration powered by `vp_complete`.
#![forbid(unsafe_code)]

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

mod catalog;
mod entry;
mod providers;
mod tasks;

use vp_complete::{
    Argument, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput,
    CompletionRequest, EarlyCompletionOutcome, PathMode,
};
use vp_js_runtime::{JsRuntimeProvider, NodeProvider, Platform};

/// Intercept `VP_COMPLETE` environment variable before Tokio runtime / self-setup runs.
/// Returns `Some(ExitCode)` if completion was handled, or `None` if normal CLI execution should continue.
pub(crate) fn try_handle_completion_early() -> Option<ExitCode> {
    let dialect_var = std::env::var_os("VP_COMPLETE")?;
    handle_completion_early_cold(&dialect_var)
}

#[cold]
fn handle_completion_early_cold(dialect_var: &std::ffi::OsStr) -> Option<ExitCode> {
    vp_shared::ensure_blocking_stdio();
    match vp_complete::dispatch_early_completion(dialect_var, complete_entry) {
        EarlyCompletionOutcome::ContinueCli => None,
        EarlyCompletionOutcome::InvalidDialect => Some(ExitCode::FAILURE),
        EarlyCompletionOutcome::Handled(output) => {
            vp_shared::output::print_stdout(format_args!("{output}"));
            Some(ExitCode::SUCCESS)
        }
    }
}

fn node_store_dir() -> PathBuf {
    vp_shared::EnvConfig::get().dirs.js_runtime().join("node").as_path().to_path_buf()
}

fn complete_entry(request: &CompletionRequest) -> CompletionOutput {
    let root = entry::select_entry(request.words.first().map_or("", String::as_str));
    vp_complete::complete(&root, request, &|command, arg, ctx| {
        let name = command.name.as_str();
        let top_level = ctx.parent == Some("vp") || ctx.parent.is_none();
        let out = match arg {
            Argument::Positional(_) if top_level && name == "run" && ctx.positional_index == 0 => {
                complete_task(ctx)
            }
            Argument::Positional(_) if top_level && matches!(name, "run" | "exec") => {
                ctx.complete_path(PathMode::Any)
            }
            Argument::Positional(_) if top_level && name == "help" && ctx.positional_index == 0 => {
                CompletionOutput::new(
                    root.subcommands
                        .iter()
                        .filter(|sub| !sub.hidden && sub.name.starts_with(&ctx.prefix))
                        .map(|sub| CompletionItem::value(sub.name.clone(), sub.about.clone()))
                        .collect(),
                    CompletionDirective::NO_FILE_COMP,
                )
            }
            _ => return None,
        };
        Some(out)
    })
}

fn complete_task(ctx: &CompletionContext) -> CompletionOutput {
    CompletionOutput::new(
        tasks::get_task_candidates_with(
            &ctx.cwd,
            &ctx.prefix,
            resolve_offline_node,
            resolve_offline_bin_js,
        ),
        CompletionDirective::NO_FILE_COMP,
    )
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

#[cfg(test)]
mod spec_tests;
