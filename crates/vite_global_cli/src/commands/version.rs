//! Version command (Category B: JavaScript Command).

use std::process::ExitStatus;

use vite_path::AbsolutePathBuf;

use crate::{error::Error, js_executor::JsExecutor};

/// Execute the `--version` command by delegating to local or global vite-plus.
///
/// Uses the CLI's own runtime instead of the project's runtime to avoid
/// side effects (e.g., writing `.node-version` in the caller's directory).
pub async fn execute(cwd: AbsolutePathBuf) -> Result<ExitStatus, Error> {
    // Pass the Rust binary's version to JS so it can display the correct global version,
    // even when delegation resolves to a local node_modules copy.
    unsafe {
        std::env::set_var(
            vite_shared::env_vars::VITE_PLUS_GLOBAL_VERSION,
            env!("CARGO_PKG_VERSION"),
        );
    }
    let mut executor = JsExecutor::new(None);
    executor.delegate_with_cli_runtime(&cwd, &["--version".to_string()]).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_version_command_module_exists() {
        // Basic test to ensure the module compiles
        assert!(true);
    }
}
