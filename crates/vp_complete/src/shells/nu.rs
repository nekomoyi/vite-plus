use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{
    CompletionOutput, CompletionRequest,
    shells::{
        ShellAdapter, ShellDialect, ShellInputMode, is_trusted_vp_exe, items_or_file_fallback,
        parse_indexed_words_input,
    },
};

pub struct NuAdapter;

/// The installed env script already declares the wrapped `vp` command.
pub fn render_installed_hook(builtin_prefix: &str, escaped_bin_dir: &str) -> String {
    let preferred = format!(
        r#"("{escaped_bin_dir}" | {builtin_prefix}path expand --no-symlink | {builtin_prefix}path join $exe)"#
    );
    render_nu_hook(builtin_prefix, &preferred)
}

fn render_nu_hook(builtin_prefix: &str, preferred: &str) -> String {
    include_str!("hooks/nu.nu").replace('%', builtin_prefix).replace("__PREFERRED__", preferred)
}

#[derive(serde::Serialize)]
struct NuCompletion<'a> {
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

impl ShellAdapter for NuAdapter {
    fn dialect(&self) -> ShellDialect {
        ShellDialect::Nu
    }

    fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String {
        let exe = trusted_exe
            .filter(|p| is_trusted_vp_exe(p))
            .map(|p| p.to_string_lossy())
            .unwrap_or_default();
        let preferred = serde_json::to_string(&exe).expect("a string serializes to JSON");
        let mut hook = render_nu_hook("", &preferred);
        hook.push_str("export extern \"vp\" [...args: string@\"nu-complete vp\"]\n");
        hook
    }

    fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode {
        if let Some(mut req) = parse_indexed_words_input(env_get, argv, cwd) {
            super::decode_words(&mut req, unquote_nu_span);
            ShellInputMode::Complete(req)
        } else if let Some(comp_line) = env_get("COMP_LINE") {
            let point = env_get("COMP_POINT")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(comp_line.len());
            let lex = crate::lexer::parse_bash_line(&comp_line, point);
            let mut req = CompletionRequest::new(lex.words, lex.current_index, cwd.to_path_buf());
            req.literal_tilde = lex.cursor.literal_tilde;
            ShellInputMode::Complete(req)
        } else {
            ShellInputMode::RegistrationHook
        }
    }

    fn render_output(
        &self,
        req: &CompletionRequest,
        env_get: &dyn Fn(&str) -> Option<String>,
        out: &CompletionOutput,
    ) -> String {
        let mut fallback = Vec::new();
        let items = items_or_file_fallback(out, req, &mut fallback);
        let home_dir = resolve_home_dir(env_get);

        let completions: Vec<_> = items
            .iter()
            .map(|item| NuCompletion {
                value: quote_nu_value(&item.value, home_dir.as_deref(), req.literal_tilde),
                description: item.description.as_deref(),
            })
            .collect();
        serde_json::to_string(&completions).expect("completion strings serialize to JSON")
    }
}

fn resolve_home_dir(env_get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(h) = env_get("HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(h));
    }
    if let Some(up) = env_get("USERPROFILE").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(up));
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn unquote_nu_span(raw: &str) -> String {
    if raw.starts_with('-')
        && let Some((flag, val)) = raw.split_once('=')
        && !flag.contains(['"', '\'', '`'])
    {
        return format!("{flag}={}", unquote_nu_span(val));
    }

    // Raw string: r'...' or r#'...'#
    if let Some(after_r) = raw.strip_prefix('r') {
        let hashes = after_r.chars().take_while(|&c| c == '#').count();
        let rest = &after_r[hashes..];
        if let Some(body) = rest.strip_prefix('\'') {
            let closer = format!("'{}", "#".repeat(hashes));
            if let Some(stripped) = body.strip_suffix(&closer) {
                return stripped.to_string();
            }
            return body.to_string();
        }
    }

    // Single-quoted string (no backslash escapes in Nushell)
    if let Some(body) = raw.strip_prefix('\'') {
        return body.strip_suffix('\'').unwrap_or(body).to_string();
    }

    // Backtick bareword quote (no backslash escapes in Nushell)
    if let Some(body) = raw.strip_prefix('`') {
        return body.strip_suffix('`').unwrap_or(body).to_string();
    }

    // Double-quoted string
    if let Some(body) = raw.strip_prefix('"') {
        let mut out = String::with_capacity(body.len());
        let mut chars = body.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '"' && chars.peek().is_none() {
                break;
            } else if c == '\\'
                && let Some(next) = chars.next()
            {
                match next {
                    '"' | '\\' | '/' => out.push(next),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }

    raw.to_string()
}

fn quote_nu_value(val: &str, home_dir: Option<&Path>, literal_tilde: bool) -> String {
    let needs_quote = (literal_tilde && val.starts_with('~'))
        || val.is_empty()
        || val.chars().any(|c| {
            matches!(
                c,
                ' ' | '\t'
                    | '\n'
                    | '\r'
                    | '"'
                    | '\''
                    | '#'
                    | '`'
                    | ';'
                    | '{'
                    | '}'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '|'
                    | '$'
                    | '&'
                    | '<'
                    | '>'
                    | '*'
                    | '?'
                    | '='
            )
        });

    if !needs_quote {
        return val.to_string();
    }

    let mut target = val.to_owned();
    if !literal_tilde {
        crate::path::expand_tilde_item_with_home(&mut target, home_dir);
    }

    let mut out = String::with_capacity(target.len() + 4);
    out.push('"');
    for c in target.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
