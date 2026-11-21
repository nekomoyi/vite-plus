use std::{ffi::OsString, path::Path};

use crate::{
    CompletionOutput, CompletionRequest, ItemKind,
    shells::{
        ShellAdapter, ShellDialect, ShellInputMode, effective_cwd_fallback, is_trusted_vp_exe,
        parse_indexed_words_input, sanitize_single_line,
    },
};

pub struct PwshAdapter;

fn pwsh_hook() -> &'static str {
    include_str!("hooks/pwsh.ps1")
}

impl ShellAdapter for PwshAdapter {
    fn dialect(&self) -> ShellDialect {
        ShellDialect::PowerShell
    }

    fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String {
        let hook = pwsh_hook();
        let Some(exe) = trusted_exe.filter(|p| is_trusted_vp_exe(p)) else {
            return hook.to_string();
        };
        let escaped = exe.to_string_lossy().replace('\'', "''");
        hook.replacen("    $vpExe = ''", &format!("    $vpExe = '{escaped}'"), 1)
    }

    fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode {
        let raw_cur = env_get("_VP_PWSH_RAW_CUR").is_some_and(|v| v == "1");
        let mut req = match parse_indexed_words_input(env_get, argv, cwd) {
            Some(req) => req,
            None => return ShellInputMode::RegistrationHook,
        };
        if raw_cur && let Some(cur) = req.words.get_mut(req.current_index) {
            let decoded = unquote_pwsh_prefix(cur);
            req.literal_tilde = super::protected_tilde(cur, &decoded);
            *cur = decoded;
        }
        ShellInputMode::Complete(req)
    }

    fn render_output(
        &self,
        req: &CompletionRequest,
        _env_get: &dyn Fn(&str) -> Option<String>,
        out: &CompletionOutput,
    ) -> String {
        let mut fallback = None;
        let out = effective_cwd_fallback(out, req, &mut fallback);

        let has_flag_prefix =
            out.path_completion.as_ref().is_some_and(|pc| !pc.flag_prefix.is_empty());

        let mut buf = String::with_capacity(out.items.len() * 48 + 8);
        for item in &out.items {
            if item.value.contains(['\r', '\n', '\t']) {
                continue;
            }
            let val = if !has_flag_prefix
                && (item.kind == ItemKind::File || item.kind == ItemKind::Directory)
                && item.value.starts_with('-')
            {
                format!("./{}", item.value)
            } else {
                item.value.clone()
            };

            buf.push_str(&quote_pwsh_value(&val, req.literal_tilde));
            buf.push('\t');
            let raw_list = sanitize_single_line(&item.value);
            let list_text = if raw_list.trim().is_empty() { " " } else { raw_list.as_str() };
            buf.push_str(list_text);
            buf.push('\t');
            buf.push_str(match item.kind {
                ItemKind::Subcommand => "Command",
                ItemKind::Flag => "ParameterName",
                ItemKind::Directory => "ProviderContainer",
                ItemKind::File => "ProviderItem",
                ItemKind::Value => "ParameterValue",
            });
            buf.push('\t');
            let tooltip = item
                .description
                .as_deref()
                .map(sanitize_single_line)
                .filter(|s| !s.trim().is_empty());
            buf.push_str(tooltip.as_deref().unwrap_or(list_text));
            buf.push('\n');
        }

        use std::fmt::Write as _;
        let _ = write!(buf, ":{}", out.directive.bits());
        buf
    }
}

fn is_pwsh_single_quote(c: char) -> bool {
    matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}')
}

fn is_pwsh_double_quote(c: char) -> bool {
    matches!(c, '"' | '\u{201C}' | '\u{201D}' | '\u{201E}')
}

fn quote_pwsh_value(val: &str, literal_tilde: bool) -> String {
    if (literal_tilde && val.starts_with('~'))
        || val.chars().any(|c| {
            is_pwsh_single_quote(c)
                || is_pwsh_double_quote(c)
                || matches!(
                    c,
                    ' ' | '\t'
                        | '#'
                        | '$'
                        | ';'
                        | '('
                        | ')'
                        | '{'
                        | '}'
                        | '|'
                        | '&'
                        | '`'
                        | ','
                        | '@'
                        | '['
                        | ']'
                        | '<'
                        | '>'
                )
        })
    {
        let mut out = String::with_capacity(val.len() + 4);
        out.push('\'');
        for c in val.chars() {
            if is_pwsh_single_quote(c) {
                out.push(c);
                out.push(c);
            } else {
                out.push(c);
            }
        }
        out.push('\'');
        out
    } else {
        val.to_string()
    }
}

fn unquote_pwsh_prefix(raw: &str) -> String {
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
            Mode::None => {
                if is_pwsh_single_quote(c) {
                    mode = Mode::Single;
                } else if is_pwsh_double_quote(c) {
                    mode = Mode::Double;
                } else if c == '`' {
                    if let Some(nc) = chars.next() {
                        out.push(nc);
                    }
                } else {
                    out.push(c);
                }
            }
            Mode::Single => {
                if is_pwsh_single_quote(c) {
                    if chars.peek().is_some_and(|&nc| is_pwsh_single_quote(nc)) {
                        let next_c = chars.next().unwrap_or(c);
                        out.push(next_c);
                    } else {
                        mode = Mode::None;
                    }
                } else {
                    out.push(c);
                }
            }
            Mode::Double => {
                if is_pwsh_double_quote(c) {
                    if let Some(next) = chars.next_if(|c| is_pwsh_double_quote(*c)) {
                        out.push(next);
                    } else {
                        mode = Mode::None;
                    }
                } else if c == '`' {
                    if let Some(nc) = chars.next() {
                        out.push(nc);
                    }
                } else {
                    out.push(c);
                }
            }
        }
    }
    out
}
