use std::{ffi::OsString, path::Path};

use crate::{
    CompletionOutput, CompletionRequest,
    shells::{
        ShellAdapter, ShellDialect, ShellInputMode, is_trusted_vp_exe, items_or_file_fallback,
        parse_indexed_words_input, sanitize_single_line, unquote_shell_token,
    },
};

pub struct FishAdapter;

#[inline]
fn fish_hook() -> &'static str {
    crate::assets::slice(crate::assets::RANGE_FISH_HOOK)
}

impl ShellAdapter for FishAdapter {
    #[inline]
    fn dialect(&self) -> ShellDialect {
        ShellDialect::Fish
    }

    #[inline]
    fn render_hook(&self) -> &'static str {
        fish_hook()
    }

    #[cold]
    fn render_hook_with_exe(&self, trusted_exe: Option<&Path>) -> String {
        let Some(exe) = trusted_exe.filter(|p| is_trusted_vp_exe(p)) else {
            return fish_hook().to_string();
        };
        let raw = exe.to_string_lossy();
        let mut escaped = String::with_capacity(raw.len());
        for ch in raw.chars() {
            if matches!(ch, '\\' | '$' | '"') {
                escaped.push('\\');
            }
            escaped.push(ch);
        }
        fish_hook().replacen("    set -l bin \"\"", &format!("    set -l bin \"{escaped}\""), 1)
    }

    #[cold]
    fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode {
        parse_indexed_words_input(env_get, argv, cwd, |words, idx| {
            if let Some(cur) = words.get_mut(idx) {
                *cur = unquote_shell_token(cur, true, false);
            }
        })
        .map_or(ShellInputMode::RegistrationHook, ShellInputMode::Complete)
    }

    #[cold]
    fn render_output(
        &self,
        req: &CompletionRequest,
        _env_get: &dyn Fn(&str) -> Option<String>,
        out: &CompletionOutput,
    ) -> String {
        let mut fallback = Vec::new();
        let items = items_or_file_fallback(out, req, &mut fallback);
        let mut buf = String::with_capacity(items.len() * 32);
        for item in items {
            if item.value.contains(['\r', '\n', '\t']) {
                continue;
            }
            if !buf.is_empty() {
                buf.push('\n');
            }
            buf.push_str(&item.value);
            if let Some(desc) = &item.description {
                buf.push('\t');
                buf.push_str(&sanitize_single_line(desc));
            }
        }
        buf
    }
}
