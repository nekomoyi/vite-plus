use std::{
    ffi::OsString,
    fmt::Write as _,
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

#[inline]
fn nu_hook() -> &'static str {
    crate::assets::slice(crate::assets::RANGE_NU_HOOK)
}

#[cold]
fn push_json_str(buf: &mut String, s: &str) {
    buf.push('"');
    for c in s.chars() {
        match c {
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            '\x08' => buf.push_str("\\b"),
            '\x0c' => buf.push_str("\\f"),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(buf, "\\u{:04x}", c as u32);
            }
            c => buf.push(c),
        }
    }
    buf.push('"');
}

impl ShellAdapter for NuAdapter {
    #[inline]
    fn dialect(&self) -> ShellDialect {
        ShellDialect::Nu
    }

    #[inline]
    fn render_hook(&self) -> &'static str {
        nu_hook()
    }

    #[cold]
    fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String {
        let hook = nu_hook();
        let Some(exe) = trusted_exe.filter(|p| is_trusted_vp_exe(p)) else {
            return hook.to_string();
        };
        let mut quoted = String::new();
        push_json_str(&mut quoted, &exe.to_string_lossy());
        hook.replacen("    let preferred = \"\"\n", &format!("    let preferred = {quoted}\n"), 1)
    }

    #[cold]
    fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode {
        if let Some(req) = parse_indexed_words_input(env_get, argv, cwd, |words, _| {
            for w in words {
                *w = unquote_nu_span(w);
            }
        }) {
            ShellInputMode::Complete(req)
        } else if let Some(comp_line) = env_get("COMP_LINE") {
            let point = env_get("COMP_POINT")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(comp_line.len());
            let lex = crate::lexer::parse_bash_line(&comp_line, point);
            ShellInputMode::Complete(CompletionRequest::new(
                lex.words,
                lex.current_index,
                cwd.to_path_buf(),
            ))
        } else {
            ShellInputMode::RegistrationHook
        }
    }

    #[cold]
    fn render_output(
        &self,
        req: &CompletionRequest,
        env_get: &dyn Fn(&str) -> Option<String>,
        out: &CompletionOutput,
    ) -> String {
        let mut fallback = Vec::new();
        let items = items_or_file_fallback(out, req, &mut fallback);
        let home_dir = resolve_home_dir(env_get);

        let mut json = String::with_capacity(items.len() * 32 + 2);
        json.push('[');
        for (idx, item) in items.iter().enumerate() {
            if idx > 0 {
                json.push(',');
            }
            json.push_str("{\"value\":");
            let quoted = quote_nu_value(&item.value, home_dir.as_deref());
            push_json_str(&mut json, &quoted);
            if let Some(desc) = &item.description {
                json.push_str(",\"description\":");
                push_json_str(&mut json, desc);
            }
            json.push('}');
        }
        json.push(']');
        json
    }
}

#[cold]
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

#[cold]
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

#[cold]
fn quote_nu_value(val: &str, home_dir: Option<&Path>) -> String {
    let needs_quote = val.is_empty()
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

    let expanded;
    let target = if let Some(home) = home_dir {
        let home_str = home.to_string_lossy();
        let home_trimmed = home_str.trim_end_matches(['/', '\\']);
        if val == "~" {
            expanded = home_trimmed.to_string();
            expanded.as_str()
        } else if let Some(rest) = val.strip_prefix("~/") {
            expanded = format!("{home_trimmed}/{rest}");
            expanded.as_str()
        } else if let Some(rest) = val.strip_prefix("~\\") {
            expanded = format!("{home_trimmed}\\{rest}");
            expanded.as_str()
        } else {
            val
        }
    } else {
        val
    };

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
