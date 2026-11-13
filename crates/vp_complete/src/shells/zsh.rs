use std::{ffi::OsString, fmt::Write as _, path::Path};

use crate::{
    CompletionDirective, CompletionOutput, CompletionRequest, ItemKind, PathMode,
    shells::{
        ShellAdapter, ShellDialect, ShellInputMode, escape_posix_double_quoted, is_trusted_vp_exe,
        parse_indexed_words_input, sanitize_single_line, unquote_shell_token,
    },
};

pub struct ZshAdapter;

#[inline]
fn zsh_hook() -> &'static str {
    crate::assets::slice(crate::assets::RANGE_ZSH_HOOK)
}

impl ShellAdapter for ZshAdapter {
    #[inline]
    fn dialect(&self) -> ShellDialect {
        ShellDialect::Zsh
    }

    #[inline]
    fn render_hook(&self) -> &'static str {
        zsh_hook()
    }

    #[cold]
    fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String {
        let hook = zsh_hook();
        let Some(exe) = trusted_exe.filter(|p| is_trusted_vp_exe(p)) else {
            return hook.to_string();
        };
        let escaped = escape_posix_double_quoted(&exe.to_string_lossy());
        hook.replacen("  local _bin=\"\"", &format!("  local _bin=\"{escaped}\""), 1)
    }

    #[cold]
    fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode {
        parse_indexed_words_input(env_get, argv, cwd, |words, _| {
            for w in words {
                *w = unquote_shell_token(w, false, true);
            }
        })
        .map_or(ShellInputMode::RegistrationHook, ShellInputMode::Complete)
    }

    #[cold]
    fn render_output(
        &self,
        req: &CompletionRequest,
        env_get: &dyn Fn(&str) -> Option<String>,
        out: &CompletionOutput,
    ) -> String {
        if let Some(pc) = &out.path_completion {
            return render_zsh_files_directive(
                &pc.mode,
                &pc.flag_prefix,
                out.effective_cwd.as_deref(),
                out.directive,
            );
        }

        if out.items.is_empty()
            && !out.directive.contains(CompletionDirective::NO_FILE_COMP)
            && out.effective_cwd.is_some()
        {
            let prefix = req.words.get(req.current_index).map_or("", String::as_str);
            let flag_prefix = if prefix.starts_with('-') {
                prefix.find('=').map_or("", |idx| &prefix[..=idx])
            } else {
                ""
            };
            return render_zsh_files_directive(
                &PathMode::Any,
                flag_prefix,
                out.effective_cwd.as_deref(),
                out.directive,
            );
        }

        let is_quoted = env_get("_VP_COMP_QUOTE").is_some_and(|q| !q.is_empty());
        let extglob = env_get("_VP_EXTGLOB").is_some_and(|v| v == "on");

        let mut v_pad = 0usize;
        let mut d_pad = 0usize;
        for item in &out.items {
            let len = sanitize_single_line(&item.value).chars().count().min(36);
            if item.kind == ItemKind::Directory {
                d_pad = d_pad.max(len);
            } else {
                v_pad = v_pad.max(len);
            }
        }

        let mut buf = String::with_capacity(out.items.len() * 40 + 8);
        for item in &out.items {
            let raw_disp = sanitize_single_line(&item.value);
            let is_dir = item.kind == ItemKind::Directory;
            let clean_mat = if is_dir {
                sanitize_single_line(item.value.strip_suffix('/').unwrap_or(&item.value))
            } else {
                raw_disp.clone()
            };
            let mat = if is_quoted { clean_mat } else { escape_zsh_unquoted(&clean_mat, extglob) };

            if let Some(desc) = &item.description {
                buf.push(if is_dir { 'D' } else { 'V' });
                buf.push('\t');
                buf.push_str(&mat);
                buf.push('\t');
                buf.push_str(&raw_disp);
                let pad =
                    (if is_dir { d_pad } else { v_pad }).saturating_sub(raw_disp.chars().count());
                buf.extend(std::iter::repeat_n(' ', pad));
                buf.push_str(" -- ");
                buf.push_str(&sanitize_single_line(desc));
            } else {
                buf.push(if is_dir { 'd' } else { 'v' });
                buf.push('\t');
                buf.push_str(&mat);
                buf.push('\t');
                buf.push_str(&raw_disp);
            }
            buf.push('\n');
        }

        let _ = write!(buf, ":{}", out.directive.bits());
        buf
    }
}

#[cold]
fn render_zsh_files_directive(
    mode: &PathMode,
    flag_prefix: &str,
    effective_cwd: Option<&Path>,
    directive: CompletionDirective,
) -> String {
    let mut buf = String::from("__vp_files\nm:");
    let mut exts_joined = String::new();
    match mode {
        PathMode::DirsOnly => buf.push_str("dir"),
        PathMode::Any => buf.push_str("any"),
        PathMode::FilesWithExtensions(exts) => {
            for e in exts {
                let e = e.strip_prefix('.').unwrap_or(e.as_str());
                if !e.is_empty()
                    && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    if !exts_joined.is_empty() {
                        exts_joined.push('|');
                    }
                    exts_joined.push_str(e);
                }
            }
            buf.push_str(if exts_joined.is_empty() { "any" } else { "ext" });
        }
    }
    if !flag_prefix.is_empty() {
        buf.push_str("\np:");
        buf.push_str(&sanitize_single_line(flag_prefix));
    }
    if let Some(base) = effective_cwd {
        buf.push_str("\nd:");
        buf.push_str(&sanitize_single_line(&base.to_string_lossy()));
    }
    if !exts_joined.is_empty() {
        buf.push_str("\ng:");
        buf.push_str(&exts_joined);
    }
    let _ = write!(buf, "\n:{}", directive.bits());
    buf
}

/// Escape only genuine Zsh command-line metacharacters for `compadd -Q`.
/// Leaves `:`, `@`, `/`, `+`, `-`, `.`, `_`, mid-word `=`, and mid-word `#`
/// (when caller `EXTENDED_GLOB` is off) completely unescaped.
#[cold]
fn escape_zsh_unquoted(val: &str, extglob: bool) -> String {
    let mut out = String::with_capacity(val.len());
    for (i, c) in val.chars().enumerate() {
        let needs_escape = matches!(
            c,
            ' ' | '\t'
                | '\\'
                | '"'
                | '\''
                | '$'
                | '`'
                | '!'
                | '&'
                | ';'
                | '|'
                | '<'
                | '>'
                | '('
                | ')'
                | '{'
                | '}'
                | '*'
                | '?'
                | '['
                | ']'
                | '~'
        ) || (c == '#' && (i == 0 || extglob))
            || (c == '=' && i == 0)
            || (c == '^' && extglob);
        if needs_escape {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
