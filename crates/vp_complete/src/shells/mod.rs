pub mod bash;

use std::{ffi::OsString, fmt, path::Path, str::FromStr};

pub use bash::BashAdapter;

use crate::{CompletionOutput, CompletionRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellDialect {
    Bash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDialectError(pub String);

impl fmt::Display for ParseDialectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown shell dialect: {}", self.0)
    }
}

impl std::error::Error for ParseDialectError {}

impl FromStr for ShellDialect {
    type Err = ParseDialectError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "bash" => Ok(Self::Bash),
            _ => Err(ParseDialectError(s.to_string())),
        }
    }
}

impl fmt::Display for ShellDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bash => "bash",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellInputMode {
    RegistrationHook,
    Complete(CompletionRequest),
}

pub trait ShellAdapter: Send + Sync {
    fn dialect(&self) -> ShellDialect;
    fn render_hook(&self) -> String {
        self.render_hook_with_exe(None)
    }
    fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String;
    fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode;
    fn render_output(
        &self,
        req: &CompletionRequest,
        env_get: &dyn Fn(&str) -> Option<String>,
        out: &CompletionOutput,
    ) -> String;
}

impl ShellDialect {
    #[must_use]
    pub fn adapter(&self) -> &'static dyn ShellAdapter {
        match self {
            Self::Bash => &BashAdapter,
        }
    }
}

pub(crate) fn is_trusted_vp_exe(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let norm = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    !norm.contains("/node_modules/.bin/") && !norm.ends_with("/node_modules/.bin")
}

pub(crate) fn extract_words_after_dash(argv: &[OsString]) -> Option<Vec<String>> {
    let pos = argv.iter().position(|arg| arg == "--")?;
    let rest = &argv[pos + 1..];
    let mut words = Vec::with_capacity(rest.len());
    for s in rest {
        words.push(s.to_string_lossy().into_owned());
    }
    Some(words)
}

pub(crate) fn parse_indexed_words_input(
    env_get: &dyn Fn(&str) -> Option<String>,
    argv: &[OsString],
    cwd: &Path,
) -> Option<CompletionRequest> {
    if let Some(mut words) = extract_words_after_dash(argv) {
        let idx = env_get("_VP_COMPLETE_INDEX")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(words.len().saturating_sub(1));
        if words.len() == idx {
            words.push(String::new());
        }
        Some(CompletionRequest::new(words, idx, cwd.to_path_buf()))
    } else {
        let idx = env_get("_VP_COMPLETE_INDEX")?.parse::<usize>().ok().unwrap_or(0);
        Some(CompletionRequest::new(Vec::new(), idx, cwd.to_path_buf()))
    }
}

pub(crate) fn escape_posix_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '\\' | '$' | '`' | '"') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}
