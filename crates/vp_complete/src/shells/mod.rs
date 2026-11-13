pub mod bash;
pub mod fish;
pub mod nu;
pub mod powershell;
pub mod zsh;

use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

pub use bash::BashAdapter;
pub use fish::FishAdapter;
pub use nu::NuAdapter;
pub use powershell::PwshAdapter;
pub use zsh::ZshAdapter;

use crate::{
    CmdSpec, CompletionDirective, CompletionItem, CompletionOutput, CompletionRequest,
    engine::complete,
    path::{PathMode, complete_path},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellDialect {
    Bash,
    Zsh,
    Fish,
    Nu,
    PowerShell,
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

    #[cold]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "bash" => Ok(Self::Bash),
            "zsh" => Ok(Self::Zsh),
            "fish" => Ok(Self::Fish),
            "nu" | "nushell" => Ok(Self::Nu),
            "powershell" | "pwsh" => Ok(Self::PowerShell),
            _ => Err(ParseDialectError(s.to_string())),
        }
    }
}

impl fmt::Display for ShellDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::Nu => "nu",
            Self::PowerShell => "powershell",
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
    fn render_hook(&self) -> &'static str;
    fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String {
        let _ = trusted_exe;
        self.render_hook().to_string()
    }
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
            Self::Zsh => &ZshAdapter,
            Self::Fish => &FishAdapter,
            Self::Nu => &NuAdapter,
            Self::PowerShell => &PwshAdapter,
        }
    }

    #[must_use]
    pub fn render_hook(&self) -> &'static str {
        match self {
            Self::Bash => BashAdapter.render_hook(),
            Self::Zsh => ZshAdapter.render_hook(),
            Self::Fish => FishAdapter.render_hook(),
            Self::Nu => NuAdapter.render_hook(),
            Self::PowerShell => PwshAdapter.render_hook(),
        }
    }

    #[cold]
    #[must_use]
    pub fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String {
        match self {
            Self::Bash => BashAdapter.render_hook_with_exe(trusted_exe),
            Self::Zsh => ZshAdapter.render_hook_with_exe(trusted_exe),
            Self::Fish => FishAdapter.render_hook_with_exe(trusted_exe),
            Self::Nu => NuAdapter.render_hook_with_exe(trusted_exe),
            Self::PowerShell => PwshAdapter.render_hook_with_exe(trusted_exe),
        }
    }

    #[cold]
    #[must_use]
    pub fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode {
        match self {
            Self::Bash => BashAdapter.parse_input(env_get, argv, cwd),
            Self::Zsh => ZshAdapter.parse_input(env_get, argv, cwd),
            Self::Fish => FishAdapter.parse_input(env_get, argv, cwd),
            Self::Nu => NuAdapter.parse_input(env_get, argv, cwd),
            Self::PowerShell => PwshAdapter.parse_input(env_get, argv, cwd),
        }
    }

    #[cold]
    #[must_use]
    pub fn render_output(
        &self,
        req: &CompletionRequest,
        env_get: &dyn Fn(&str) -> Option<String>,
        out: &CompletionOutput,
    ) -> String {
        match self {
            Self::Bash => BashAdapter.render_output(req, env_get, out),
            Self::Zsh => ZshAdapter.render_output(req, env_get, out),
            Self::Fish => FishAdapter.render_output(req, env_get, out),
            Self::Nu => NuAdapter.render_output(req, env_get, out),
            Self::PowerShell => PwshAdapter.render_output(req, env_get, out),
        }
    }
}

#[cold]
pub fn try_handle_env_completion<F>(
    env_get: &dyn Fn(&str) -> Option<String>,
    argv: &[OsString],
    cwd: &Path,
    build_specs: F,
) -> Option<String>
where
    F: FnOnce() -> (CmdSpec, Option<CmdSpec>),
{
    try_handle_env_completion_inner(env_get, argv, cwd, None, build_specs)
}

#[cold]
fn try_handle_env_completion_inner<F>(
    env_get: &dyn Fn(&str) -> Option<String>,
    argv: &[OsString],
    cwd: &Path,
    trusted_exe: Option<&Path>,
    build_specs: F,
) -> Option<String>
where
    F: FnOnce() -> (CmdSpec, Option<CmdSpec>),
{
    let dialect: ShellDialect = env_get("VP_COMPLETE")?.parse().ok()?;
    match dialect.parse_input(env_get, argv, cwd) {
        ShellInputMode::RegistrationHook => Some(dialect.render_hook_with_exe(trusted_exe)),
        ShellInputMode::Complete(req) => {
            let (root, vpx_spec) = build_specs();
            let out = complete(&root, vpx_spec.as_ref(), &req);
            Some(dialect.render_output(&req, env_get, &out))
        }
    }
}

#[cold]
pub fn handle_env_completion<F>(build_specs: F) -> Option<String>
where
    F: FnOnce() -> (CmdSpec, Option<CmdSpec>),
{
    let argv: Vec<OsString> = std::env::args_os().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let exe = std::env::current_exe().ok();
    try_handle_env_completion_inner(
        &|key| std::env::var(key).ok(),
        &argv,
        &cwd,
        exe.as_deref(),
        build_specs,
    )
}

#[cold]
pub(crate) fn is_trusted_vp_exe(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let norm = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    !norm.contains("/node_modules/.bin/") && !norm.ends_with("/node_modules/.bin")
}

#[cold]
pub(crate) fn extract_words_after_dash(argv: &[OsString]) -> Option<Vec<String>> {
    let pos = argv.iter().position(|arg| arg == "--")?;
    let rest = &argv[pos + 1..];
    let mut words = Vec::with_capacity(rest.len());
    for s in rest {
        words.push(s.to_string_lossy().into_owned());
    }
    Some(words)
}

#[cold]
pub(crate) fn parse_indexed_words_input(
    env_get: &dyn Fn(&str) -> Option<String>,
    argv: &[OsString],
    cwd: &Path,
    transform: fn(&mut Vec<String>, usize),
) -> Option<CompletionRequest> {
    if let Some(mut words) = extract_words_after_dash(argv) {
        let idx = env_get("_VP_COMPLETE_INDEX")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(words.len().saturating_sub(1));
        transform(&mut words, idx);
        if words.len() <= idx {
            words.resize(idx + 1, String::new());
        }
        Some(CompletionRequest::new(words, idx, cwd.to_path_buf()))
    } else {
        let idx = env_get("_VP_COMPLETE_INDEX")?.parse::<usize>().ok().unwrap_or(0);
        Some(CompletionRequest::new(vec![String::new(); idx + 1], idx, cwd.to_path_buf()))
    }
}

#[cold]
fn complete_fallback_paths(cwd: &Path, prefix: &str) -> CompletionOutput {
    if prefix.starts_with('-')
        && let Some(eq_idx) = prefix.find('=')
    {
        let flag_prefix = &prefix[..=eq_idx];
        let path_prefix = &prefix[eq_idx + 1..];
        let mut out = complete_path(cwd, path_prefix, PathMode::Any);
        for item in &mut out.items {
            item.value.insert_str(0, flag_prefix);
        }
        if let Some(pc) = &mut out.path_completion {
            pc.flag_prefix.insert_str(0, flag_prefix);
        }
        out
    } else {
        complete_path(cwd, prefix, PathMode::Any)
    }
}

#[cold]
pub(crate) fn effective_cwd_fallback<'a>(
    out: &'a CompletionOutput,
    req: &CompletionRequest,
    slot: &'a mut Option<CompletionOutput>,
) -> &'a CompletionOutput {
    if out.effective_cwd.is_some()
        && out.items.is_empty()
        && !out.directive.contains(CompletionDirective::NO_FILE_COMP)
    {
        let prefix = req.words.get(req.current_index).map_or("", String::as_str);
        slot.insert(complete_fallback_paths(out.cwd_or(&req.cwd), prefix))
    } else {
        out
    }
}

#[cold]
pub(crate) fn items_or_file_fallback<'a>(
    out: &'a CompletionOutput,
    req: &CompletionRequest,
    fallback_buf: &'a mut Vec<CompletionItem>,
) -> &'a [CompletionItem] {
    if out.items.is_empty() && !out.directive.contains(CompletionDirective::NO_FILE_COMP) {
        let prefix = req.words.get(req.current_index).map_or("", String::as_str);
        *fallback_buf = complete_fallback_paths(out.cwd_or(&req.cwd), prefix).items;
        fallback_buf
    } else {
        &out.items
    }
}

#[cold]
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

#[cold]
pub(crate) fn sanitize_single_line(s: &str) -> String {
    if !s.bytes().any(|b| matches!(b, b'\r' | b'\n' | b'\t')) {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        out.push(if matches!(ch, '\r' | '\n' | '\t') { ' ' } else { ch });
    }
    out
}

#[cold]
pub(crate) fn unquote_shell_token(
    raw: &str,
    single_quote_escapes: bool,
    backtick_in_double: bool,
) -> String {
    #[derive(Copy, Clone, PartialEq, Eq)]
    enum Mode {
        None,
        Single,
        Double,
    }
    let mut mode = Mode::None;
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match mode {
            Mode::None => match c {
                '\'' => mode = Mode::Single,
                '"' => mode = Mode::Double,
                '\\' => {
                    if let Some(nc) = chars.next() {
                        out.push(nc);
                    }
                }
                _ => out.push(c),
            },
            Mode::Single => {
                if c == '\'' {
                    mode = Mode::None;
                } else if single_quote_escapes
                    && c == '\\'
                    && let Some(&nc @ ('\'' | '\\')) = chars.peek()
                {
                    let _ = chars.next();
                    out.push(nc);
                } else {
                    out.push(c);
                }
            }
            Mode::Double => {
                if c == '"' {
                    mode = Mode::None;
                } else if c == '\\'
                    && let Some(&nc) = chars.peek()
                    && (matches!(nc, '"' | '\\' | '$') || (backtick_in_double && nc == '`'))
                {
                    let _ = chars.next();
                    out.push(nc);
                } else {
                    out.push(c);
                }
            }
        }
    }
    out
}
