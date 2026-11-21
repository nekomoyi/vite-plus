use std::{ffi::OsString, path::Path};

use crate::{
    CompletionDirective, CompletionOutput, CompletionRequest,
    lexer::{BashLexResult, BashQuoteMode, parse_bash_line_with_wordbreaks},
    path::PathMode,
    shells::{
        ShellAdapter, ShellDialect, ShellInputMode, escape_posix_double_quoted, is_trusted_vp_exe,
        parse_indexed_words_input,
    },
};

pub struct BashAdapter;

fn bash_hook() -> &'static str {
    include_str!("hooks/bash.sh")
}

fn bash_lex_from_env(env_get: &dyn Fn(&str) -> Option<String>) -> Option<BashLexResult> {
    let wordbreaks = bash_wordbreaks(env_get);
    env_get("_VP_COMP_LINE").or_else(|| env_get("COMP_LINE")).map(|line| {
        let point = env_get("_VP_COMP_POINT")
            .or_else(|| env_get("COMP_POINT"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(line.len());
        parse_bash_line_with_wordbreaks(&line, point, &wordbreaks)
    })
}

fn bash_wordbreaks(env_get: &dyn Fn(&str) -> Option<String>) -> String {
    env_get("_VP_COMP_WORDBREAKS")
        .or_else(|| env_get("COMP_WORDBREAKS"))
        .unwrap_or_else(|| " \t\n\"'><=;|&(:".to_string())
}

impl ShellAdapter for BashAdapter {
    fn dialect(&self) -> ShellDialect {
        ShellDialect::Bash
    }

    fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String {
        let hook = bash_hook();
        let Some(exe) = trusted_exe.filter(|p| is_trusted_vp_exe(p)) else {
            return hook.to_string();
        };
        let escaped = escape_posix_double_quoted(&exe.to_string_lossy());
        hook.replacen("_bin=\"\"", &format!("_bin=\"{escaped}\""), 1)
    }

    fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode {
        if let Some(res) = bash_lex_from_env(env_get) {
            let mut req = CompletionRequest::new(res.words, res.current_index, cwd.to_path_buf());
            req.literal_tilde = res.cursor.literal_tilde;
            req.bash_cursor = Some(res.cursor);
            ShellInputMode::Complete(req)
        } else {
            parse_indexed_words_input(env_get, argv, cwd)
                .map_or(ShellInputMode::RegistrationHook, ShellInputMode::Complete)
        }
    }

    fn render_output(
        &self,
        req: &CompletionRequest,
        env_get: &dyn Fn(&str) -> Option<String>,
        out: &CompletionOutput,
    ) -> String {
        let prefix = req.words.get(req.current_index).map(String::as_str).unwrap_or("");

        let (quote_mode, has_closing_quote, to_trim) = if let Some(lex) = req.bash_cursor {
            (
                lex.quote_mode,
                lex.has_closing_quote,
                lex.last_unescaped_wordbreak_end.and_then(|end| prefix.get(..end)),
            )
        } else {
            let wordbreaks = bash_wordbreaks(env_get);
            let trim = prefix.rfind(|c| wordbreaks.contains(c)).map(|idx| {
                let next_len = prefix[idx..].chars().next().map_or(1, char::len_utf8);
                &prefix[..idx + next_len]
            });
            (BashQuoteMode::None, false, trim)
        };
        let keep_close = has_closing_quote
            && env_get("_VP_BASH_V").and_then(|s| s.parse::<u32>().ok()).is_some_and(|v| v < 4);

        let fallback;
        let out =
            if out.items.is_empty() && !out.directive.contains(CompletionDirective::NO_FILE_COMP) {
                let path_pfx = to_trim
                    .filter(|p| p.starts_with('-') && p.ends_with('='))
                    .and_then(|p| prefix.strip_prefix(p))
                    .unwrap_or(prefix);
                fallback = crate::path::complete_paths(
                    out.cwd_or(&req.cwd),
                    crate::path::current_home_dir().as_deref(),
                    path_pfx,
                    PathMode::Any,
                    !req.literal_tilde,
                );
                if fallback.items.is_empty() { out } else { &fallback }
            } else {
                out
            };

        let mut buf = String::with_capacity(out.items.len() * 24 + 8);
        for item in &out.items {
            let val = if let Some(trim_prefix) = to_trim {
                item.value.strip_prefix(trim_prefix).unwrap_or(&item.value)
            } else {
                &item.value
            };
            if val.contains(['\r', '\n']) {
                continue;
            }

            if req.literal_tilde && quote_mode == BashQuoteMode::None && val.starts_with('~') {
                buf.push('\\');
            }
            push_escaped_bash(&mut buf, val, quote_mode, keep_close);
            buf.push('\n');
        }

        let mut bits = out.directive.bits();
        if quote_mode != BashQuoteMode::None {
            bits |= 64;
        }
        use std::fmt::Write as _;
        let _ = write!(buf, ":{bits}");
        buf
    }
}

fn push_escaped_bash(out: &mut String, val: &str, quote_mode: BashQuoteMode, keep_close: bool) {
    match quote_mode {
        BashQuoteMode::None => {
            for (i, c) in val.chars().enumerate() {
                let needs_escape = matches!(
                    c,
                    ' ' | '\t'
                        | '&'
                        | ';'
                        | '|'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                        | '{'
                        | '}'
                        | '!'
                        | '$'
                        | '`'
                        | '"'
                        | '\''
                        | '\\'
                        | '*'
                        | '?'
                        | '['
                        | ']'
                ) || (c == '#' && i == 0);
                if needs_escape {
                    out.push('\\');
                }
                out.push(c);
            }
        }
        BashQuoteMode::Double => {
            if val.starts_with('!') {
                out.push('"');
            }
            for c in val.chars() {
                if matches!(c, '\\' | '"' | '$' | '`') {
                    out.push('\\');
                    out.push(c);
                } else if c == '!' {
                    out.push_str("\"'!'\"");
                } else {
                    out.push(c);
                }
            }
            if !keep_close && val.ends_with(['!', '"']) {
                out.push('"');
            }
        }
        BashQuoteMode::Single => {
            for c in val.chars() {
                if c == '\'' {
                    out.push_str("'\\''");
                } else {
                    out.push(c);
                }
            }
            if !keep_close && val.ends_with('\'') {
                out.push('\'');
            }
        }
    }
}
