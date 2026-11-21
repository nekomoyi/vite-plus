pub mod bash;
pub mod zsh;

use std::{ffi::OsString, fmt, path::Path, str::FromStr};

pub use bash::BashAdapter;
pub use zsh::ZshAdapter;

use crate::{CompletionOutput, CompletionRequest, path::PathMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellDialect {
    Bash,
    Zsh,
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
            "zsh" => Ok(Self::Zsh),
            _ => Err(ParseDialectError(s.to_string())),
        }
    }
}

impl fmt::Display for ShellDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
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
            Self::Zsh => &ZshAdapter,
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

fn complete_fallback_paths(cwd: &Path, prefix: &str, literal: bool) -> CompletionOutput {
    if prefix.starts_with('-')
        && let Some(eq_idx) = prefix.find('=')
    {
        let flag_prefix = &prefix[..=eq_idx];
        let path_prefix = &prefix[eq_idx + 1..];
        let mut out = crate::path::complete_paths(
            cwd,
            crate::path::current_home_dir().as_deref(),
            path_prefix,
            PathMode::Any,
            !literal,
        );
        for item in &mut out.items {
            item.value.insert_str(0, flag_prefix);
        }
        if let Some(pc) = &mut out.path_completion {
            pc.flag_prefix.insert_str(0, flag_prefix);
        }
        out
    } else {
        crate::path::complete_paths(
            cwd,
            crate::path::current_home_dir().as_deref(),
            prefix,
            PathMode::Any,
            !literal,
        )
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

pub(super) fn decode_words(req: &mut CompletionRequest, decode: impl Fn(&str) -> String) {
    for (index, word) in req.words.iter_mut().enumerate() {
        let mut decoded = decode(word);
        if index < req.current_index && word.starts_with('~') {
            crate::path::expand_tilde_item(&mut decoded);
        } else if index == req.current_index {
            req.literal_tilde = protected_tilde(word, &decoded);
        }
        *word = decoded;
    }
}

pub(super) fn protected_tilde(raw: &str, decoded: &str) -> bool {
    // Completing "-C~/src" offers an absolute path that also works when executed.
    // Starting the quote at the value (-C'~/src') explicitly requests a literal tilde.
    let raw = if decoded.starts_with('-') { raw.trim_start_matches(['\'', '"', '`']) } else { raw };
    decoded.find('~').is_some_and(|offset| raw.get(..=offset) != decoded.get(..=offset))
}
