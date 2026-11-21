use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use crate::{CompletionOutput, CompletionRequest, ShellDialect, ShellInputMode};
/// Outcome of early `VP_COMPLETE` interception before runtime startup.
pub enum EarlyCompletionOutcome {
    /// `VP_COMPLETE` was empty/whitespace; proceed with normal CLI execution.
    ContinueCli,
    /// Unknown dialect; exit with failure.
    InvalidDialect,
    /// Fully framed shell output.
    Handled(String),
}

pub fn dispatch_early_completion(
    dialect_var: &OsStr,
    candidates: impl FnOnce(&CompletionRequest) -> CompletionOutput,
) -> EarlyCompletionOutcome {
    let dialect_str = dialect_var.to_string_lossy();
    if dialect_str.trim().is_empty() {
        return EarlyCompletionOutcome::ContinueCli;
    }

    let Ok(dialect) = dialect_str.parse::<ShellDialect>() else {
        return EarlyCompletionOutcome::InvalidDialect;
    };

    let adapter = dialect.adapter();
    let argv: Vec<OsString> = std::env::args_os().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env_get = |key: &str| std::env::var(key).ok();

    match adapter.parse_input(&env_get, &argv, &cwd) {
        ShellInputMode::RegistrationHook => {
            let exe = std::env::current_exe().ok();
            EarlyCompletionOutcome::Handled(adapter.render_hook_with_exe(exe.as_deref()))
        }
        ShellInputMode::Complete(req) => {
            let out = candidates(&req);
            let mut output = adapter.render_output(&req, &env_get, &out);
            if !output.is_empty() {
                output.push('\n');
            }
            EarlyCompletionOutcome::Handled(output)
        }
    }
}
