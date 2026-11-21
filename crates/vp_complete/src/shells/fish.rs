use std::{ffi::OsString, path::Path};

use crate::{
    CompletionOutput, CompletionRequest,
    shells::{
        ShellAdapter, ShellDialect, ShellInputMode, is_trusted_vp_exe, items_or_file_fallback,
        parse_indexed_words_input, sanitize_single_line, unquote_shell_token,
    },
};

pub struct FishAdapter;

fn fish_hook() -> &'static str {
    include_str!("hooks/fish.fish")
}

impl ShellAdapter for FishAdapter {
    fn dialect(&self) -> ShellDialect {
        ShellDialect::Fish
    }

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

    fn parse_input(
        &self,
        env_get: &dyn Fn(&str) -> Option<String>,
        argv: &[OsString],
        cwd: &Path,
    ) -> ShellInputMode {
        parse_indexed_words_input(env_get, argv, cwd).map_or(
            ShellInputMode::RegistrationHook,
            |mut req| {
                if let Some(raw) = req.words.get_mut(req.current_index) {
                    let decoded = unquote_shell_token(raw, true, false);
                    req.literal_tilde = super::protected_tilde(raw, &decoded);
                    *raw = decoded;
                }
                ShellInputMode::Complete(req)
            },
        )
    }

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
