//! Project scaffolding command (Category B: JavaScript Command).

use std::process::ExitStatus;

use vite_path::AbsolutePathBuf;

use crate::error::Error;

/// Execute the `create` command by delegating to local or global vite-plus.
pub async fn execute(cwd: AbsolutePathBuf, args: &[String]) -> Result<ExitStatus, Error> {
    super::delegate::execute(cwd, "create", args).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_create_command_module_exists() {
        // Basic test to ensure the module compiles
        assert!(true);
    }
}
