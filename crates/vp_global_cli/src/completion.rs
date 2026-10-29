//! Dynamic shell completion registration and `-C <dir>` working-directory handling.

use std::{
    env,
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
};

use clap_complete::env::{Bash, CompleteEnv, Elvish, EnvCompleter, Fish, Powershell, Shells, Zsh};

const COMPLETE_VAR: &str = "VP_COMPLETE";
const COMPLETE_INDEX_VAR: &str = "_CLAP_COMPLETE_INDEX";

#[derive(Clone, Copy)]
enum ShellKind {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
}

struct VpShellCompleter {
    kind: ShellKind,
    inner: &'static dyn EnvCompleter,
}

impl VpShellCompleter {
    const fn new(kind: ShellKind, inner: &'static dyn EnvCompleter) -> Self {
        Self { kind, inner }
    }

    fn completion_index(&self, args: &[OsString]) -> usize {
        match self.kind {
            ShellKind::Bash | ShellKind::Elvish | ShellKind::Zsh => env::var(COMPLETE_INDEX_VAR)
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or_default(),
            ShellKind::Fish | ShellKind::Powershell => args.len().saturating_sub(1),
        }
    }
}

const VP_BASH: VpShellCompleter = VpShellCompleter::new(ShellKind::Bash, &Bash);
const VP_ELVISH: VpShellCompleter = VpShellCompleter::new(ShellKind::Elvish, &Elvish);
const VP_FISH: VpShellCompleter = VpShellCompleter::new(ShellKind::Fish, &Fish);
const VP_POWERSHELL: VpShellCompleter = VpShellCompleter::new(ShellKind::Powershell, &Powershell);
const VP_ZSH: VpShellCompleter = VpShellCompleter::new(ShellKind::Zsh, &Zsh);

const VP_SHELLS: Shells<'static> =
    Shells(&[&VP_BASH, &VP_ELVISH, &VP_FISH, &VP_POWERSHELL, &VP_ZSH]);

impl EnvCompleter for VpShellCompleter {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn is(&self, name: &str) -> bool {
        self.inner.is(name)
    }

    fn write_registration(
        &self,
        var: &str,
        name: &str,
        bin: &str,
        completer: &str,
        buf: &mut dyn Write,
    ) -> Result<(), std::io::Error> {
        match self.kind {
            ShellKind::Bash => buf.write_all(include_bytes!("../completion-register.bash")),
            ShellKind::Zsh => write_zsh_registration(var, name, bin, completer, buf),
            ShellKind::Fish => write_fish_registration(var, bin, completer, buf),
            ShellKind::Elvish | ShellKind::Powershell => {
                self.inner.write_registration(var, name, bin, completer, buf)
            }
        }
    }

    fn write_complete(
        &self,
        cmd: &mut clap::Command,
        args: Vec<OsString>,
        current_dir: Option<&Path>,
        buf: &mut dyn Write,
    ) -> Result<(), std::io::Error> {
        let mut resolved_dir: Option<PathBuf> = current_dir.map(Path::to_path_buf);
        let index = self.completion_index(&args);
        if let Some(dir) = completion_chdir_from_command_args(&args, index)
            && let Some(base_dir) = current_dir
                .and_then(|path| vt_path::AbsolutePathBuf::new(path.to_path_buf()))
                .or_else(|| vt_path::current_dir().ok())
            && let Ok(target) = crate::apply_chdir(&base_dir, &dir)
        {
            resolved_dir = Some(target.as_path().to_path_buf());
        }
        self.inner.write_complete(cmd, args, resolved_dir.as_deref(), buf)
    }
}

/// Extract a completed leading `-C <dir>` from the post-separator command words
/// (`["vp", ...]`) when the cursor index has moved past the `-C` option.
pub(crate) fn completion_chdir_from_command_args(
    command_args: &[OsString],
    complete_index: usize,
) -> Option<String> {
    let [_command, user_args @ ..] = command_args else {
        return None;
    };
    let current_user_arg_index = complete_index.checked_sub(1)?;
    let prefix: Vec<String> =
        user_args.iter().take(2).map(|arg| arg.to_string_lossy().into_owned()).collect();
    let (dir, consumed) = crate::parse_leading_chdir(&prefix)?;
    if current_user_arg_index < consumed {
        return None;
    }
    Some(dir)
}

/// Process a shell completion request if `VP_COMPLETE` is set, exiting on completion.
pub(crate) fn complete(factory: fn() -> clap::Command) {
    CompleteEnv::with_factory(factory).var(COMPLETE_VAR).shells(VP_SHELLS).complete();
}

#[cfg(test)]
pub(crate) fn try_complete(
    factory: fn() -> clap::Command,
    args: impl IntoIterator<Item = impl Into<OsString>>,
    current_dir: Option<&Path>,
) -> clap::error::Result<bool> {
    CompleteEnv::with_factory(factory)
        .var(COMPLETE_VAR)
        .shells(VP_SHELLS)
        .try_complete(args, current_dir)
}

fn is_safe_unquoted_shell_word(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '/' | '_' | '-' | '.' | ',' | '+' | '=' | ':' | '@')
        })
}

fn posix_quote(value: &str) -> String {
    if is_safe_unquoted_shell_word(value) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn fish_quote(value: &str) -> String {
    if is_safe_unquoted_shell_word(value) {
        value.to_string()
    } else {
        let mut out = String::with_capacity(value.len() + 2);
        out.push('\'');
        for c in value.chars() {
            match c {
                '\\' => out.push_str(r"\\"),
                '\'' => out.push_str(r"\'"),
                _ => out.push(c),
            }
        }
        out.push('\'');
        out
    }
}

fn write_zsh_registration(
    var: &str,
    name: &str,
    bin: &str,
    completer: &str,
    buf: &mut dyn Write,
) -> Result<(), std::io::Error> {
    let escaped_name = name.replace('-', "_");
    let bin = posix_quote(bin);
    let completer = posix_quote(completer);

    let script = r#"#compdef BIN
function _clap_expand_chdir_word_NAME() {
    local raw=$1 expanded=$1
    if [[ "$raw" != *[\`\;\&\|\<\>\(\)$'\n']* ]]; then
        eval "expanded=$raw" 2>/dev/null || expanded=$1
    fi
    print -rn -- "$expanded"
}

function _clap_dynamic_completer_NAME() {
    local _CLAP_COMPLETE_INDEX=$(expr $CURRENT - 1)
    local _CLAP_IFS=$'\n'
    local -a completion_words=("${words[@]}")

    if [[ "${completion_words[2]-}" == "-C" || "${completion_words[2]-}" == "\"-C\"" || "${completion_words[2]-}" == "'-C'" ]]; then
        if (( _CLAP_COMPLETE_INDEX > 2 )) && (( ${#completion_words[@]} >= 3 )); then
            completion_words[2]="-C"
            completion_words[3]="$(_clap_expand_chdir_word_NAME "${completion_words[3]}")"
        fi
    elif [[ "${completion_words[2]-}" == -C?* ]]; then
        if (( _CLAP_COMPLETE_INDEX > 1 )); then
            local __vp_rest=${completion_words[2]#-C}
            __vp_rest=${__vp_rest#=}
            completion_words[2]="-C=$(_clap_expand_chdir_word_NAME "$__vp_rest")"
        fi
    elif [[ "${completion_words[2]-}" == [\"\']-C?* ]]; then
        if (( _CLAP_COMPLETE_INDEX > 1 )); then
            local __vp_expanded="$(_clap_expand_chdir_word_NAME "${completion_words[2]}")"
            if [[ "$__vp_expanded" == "-C" ]]; then
                if (( _CLAP_COMPLETE_INDEX > 2 )) && (( ${#completion_words[@]} >= 3 )); then
                    completion_words[2]="-C"
                    completion_words[3]="$(_clap_expand_chdir_word_NAME "${completion_words[3]}")"
                fi
            elif [[ "$__vp_expanded" == -C?* ]]; then
                completion_words[2]="$__vp_expanded"
            fi
        fi
    fi

    local completions=("${(@f)$( \
        _CLAP_IFS="$_CLAP_IFS" \
        _CLAP_COMPLETE_INDEX="$_CLAP_COMPLETE_INDEX" \
        VAR="zsh" \
        COMPLETER -- "${completion_words[@]}" 2>/dev/null \
    )}")

    if [[ -n $completions ]]; then
        local -a dirs=()
        local -a other=()
        local completion
        for completion in $completions; do
            local value="${completion%%:*}"
            if [[ "$value" == */ ]]; then
                local dir_no_slash="${value%/}"
                if [[ "$completion" == *:* ]]; then
                    local desc="${completion#*:}"
                    dirs+=("$dir_no_slash:$desc")
                else
                    dirs+=("$dir_no_slash")
                fi
            else
                other+=("$completion")
            fi
        done
        [[ -n $dirs ]] && _describe -V 'values' dirs -S '/' -r '/'
        [[ -n $other ]] && _describe -V 'values' other
    fi
}

compdef _clap_dynamic_completer_NAME BIN"#
        .replace("NAME", &escaped_name)
        .replace("COMPLETER", &completer)
        .replace("BIN", &bin)
        .replace("VAR", var);

    writeln!(buf, "{script}")
}

fn write_fish_registration(
    var: &str,
    bin: &str,
    completer: &str,
    buf: &mut dyn Write,
) -> Result<(), std::io::Error> {
    let bin = fish_quote(bin);
    let completer = fish_quote(completer);

    let script = r#"function __vp_complete_chdir --no-scope-shadowing
    set -l line (commandline --current-process --cut-at-cursor)
    set -l raw
    set -l strip_prefix 0
    if set -l m (string match -r -- '^\s*\S+\s+(?:-C|"-C"|\'-C\')\s+((?:"[^"]*"|\'[^\']*\'|\\.|[^\s])+)\s' "$line")
        set raw $m[2]
    else if set -l m (string match -r -- '^\s*\S+\s+-C=((?:"[^"]*"|\'[^\']*\'|\\.|[^\s])+)\s' "$line")
        set raw $m[2]
    else if set -l m (string match -r -- '^\s*\S+\s+-C((?:"[^"]*"|\'[^\']*\'|\\.|[^\s=])(?:"[^"]*"|\'[^\']*\'|\\.|[^\s])*)\s' "$line")
        set raw $m[2]
    else if set -l m (string match -r -- '^\s*\S+\s+((?:"[^"]*"|\'[^\']*\'|\\.|[^\s])+)\s' "$line")
        set raw $m[2]
        set strip_prefix 1
    else
        return 1
    end
    if not string match -rq -- '[\`;&|<>()\n]' "$raw"
        if eval "set -l expanded $raw" 2>/dev/null
            if test $strip_prefix -eq 1
                set expanded (string replace -r -- '^-C=?' '' "$expanded")
            end
            printf '%s\n' "$expanded"
            return 0
        end
    end
    return 1
end

function __vp_complete --no-scope-shadowing
    set -l tokens (commandline --current-process --tokenize --cut-at-cursor)
    set -l current (commandline --current-token)
    if test (count $tokens) -ge 3; and test "$tokens[2]" = "-C"
        set -l dir (__vp_complete_chdir); and set tokens[3] "$dir"
    else if test (count $tokens) -ge 2; and string match -qr '^-C.+' -- "$tokens[2]"
        set -l dir (__vp_complete_chdir); and set tokens[2] "-C=$dir"
    end
    VAR=fish COMPLETER -- $tokens $current
end

function __vpr_complete --no-scope-shadowing
    set -l tokens (commandline --current-process --tokenize --cut-at-cursor)
    set -l current (commandline --current-token)
    set -l args $tokens[2..]
    set -l translated vp
    if test (count $args) -eq 0; and string match -qr '^-C' -- "$current"
        # Keep completing the global -C option until its value is finished.
    else if test (count $args) -ge 1; and test "$args[1]" = "-C"
        set -a translated -C
        if test (count $args) -ge 2
            set -l dir (__vp_complete_chdir; or printf '%s\n' "$args[2]")
            set -a translated "$dir" run $args[3..]
        end
    else if test (count $args) -ge 1; and string match -qr '^-C.+' -- "$args[1]"
        set -l dir (__vp_complete_chdir); and set args[1] "-C=$dir"
        set -a translated "$args[1]" run $args[2..]
    else
        set -a translated run $args
    end
    VAR=fish COMPLETER -- $translated $current
end

complete --keep-order --exclusive --command BIN --arguments "(__vp_complete)"
complete -c vpr --keep-order --exclusive --arguments "(__vpr_complete)""#
        .replace("COMPLETER", &completer)
        .replace("BIN", &bin)
        .replace("VAR", var);

    writeln!(buf, "{script}")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use clap_complete::env::EnvCompleter;

    use super::{completion_chdir_from_command_args, try_complete};

    fn os_args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn completion_chdir_extracts_target_when_cursor_is_past_chdir() {
        assert_eq!(
            completion_chdir_from_command_args(&os_args(&["vp", "-C", "apps/web", "run", ""]), 4,),
            Some("apps/web".to_string())
        );
        assert_eq!(
            completion_chdir_from_command_args(&os_args(&["vp", "-Capps/web", ""]), 2),
            Some("apps/web".to_string())
        );
        assert_eq!(
            completion_chdir_from_command_args(&os_args(&["vp", "-C=apps/web", ""]), 2),
            Some("apps/web".to_string())
        );
    }

    #[test]
    fn completion_chdir_skips_when_cursor_is_still_on_chdir_argument() {
        assert_eq!(
            completion_chdir_from_command_args(&os_args(&["vp", "-C", "apps/web"]), 2),
            None
        );
        assert_eq!(
            completion_chdir_from_command_args(&os_args(&["vp", "-C", "apps/web", "run"]), 2),
            None
        );
        assert_eq!(completion_chdir_from_command_args(&os_args(&["vp", "-Capps/web"]), 1), None);
        assert_eq!(
            completion_chdir_from_command_args(&os_args(&["vp", "-Capps/web", "run"]), 1),
            None
        );
    }

    #[test]
    #[serial_test::serial]
    fn complete_env_applies_leading_chdir_before_invoking_completer() {
        let temp_dir = tempfile::tempdir().unwrap();
        let target_dir = temp_dir.path().join("apps").join("web");
        std::fs::create_dir_all(&target_dir).unwrap();
        let original_cwd = std::env::current_dir().unwrap();

        let observed_cwd = std::sync::Arc::new(std::sync::Mutex::new(None));
        let observed_clone = std::sync::Arc::clone(&observed_cwd);

        fn command_with_cwd_probe() -> clap::Command {
            clap::Command::new("vp")
                .arg(clap::Arg::new("chdir").short('C').value_hint(clap::ValueHint::DirPath))
                .subcommand(clap::Command::new("probe").arg(
                    clap::Arg::new("file").value_hint(clap::ValueHint::AnyPath).add(
                        clap_complete::ArgValueCompleter::new(|_: &std::ffi::OsStr| {
                            let cwd = std::env::current_dir().ok();
                            if let Some(path) = cwd {
                                // Record the process cwd observed during dynamic completion
                                let _ = std::env::set_current_dir(&path);
                            }
                            vec![]
                        }),
                    ),
                ))
        }

        // Put a marker file only inside `apps/web`
        std::fs::write(target_dir.join("inside-web.js"), "").unwrap();
        std::fs::write(temp_dir.path().join("root-only.js"), "").unwrap();

        unsafe {
            std::env::set_var("VP_COMPLETE", "fish");
        }
        let completed = try_complete(
            command_with_cwd_probe,
            ["vp", "--", "vp", "-C", "apps/web", "probe", ""],
            Some(temp_dir.path()),
        )
        .unwrap();
        let after_cwd = std::env::current_dir().unwrap();
        *observed_clone.lock().unwrap() = Some(after_cwd);
        let _ = std::env::set_current_dir(&original_cwd);

        assert!(completed, "CompleteEnv should handle the completion request");
        assert_eq!(
            observed_cwd.lock().unwrap().as_ref().unwrap().canonicalize().unwrap(),
            target_dir.canonicalize().unwrap()
        );
    }

    #[test]
    #[cfg(unix)]
    fn shell_adapters_expand_local_vars_and_preserve_single_quoted_paths() {
        use std::{os::unix::fs::PermissionsExt, process::Command};

        let temp_dir = tempfile::tempdir().unwrap();
        let bin_dir = temp_dir.path().join("bin");
        let capture_file = temp_dir.path().join("captured.txt");
        std::fs::create_dir_all(&bin_dir).unwrap();

        let fake_vp = bin_dir.join("vp");
        std::fs::write(
            &fake_vp,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\n", capture_file.display()),
        )
        .unwrap();
        let mut perms = std::fs::metadata(&fake_vp).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_vp, perms).unwrap();

        // 1. Bash adapter (`completion-register.bash`): local $PROJECT, tilde, single-quoted literal, and `vpr -C`
        if Command::new("bash").arg("-c").arg("exit 0").status().is_ok() {
            let mut reg = Vec::new();
            super::VP_BASH
                .write_registration("VP_COMPLETE", "vp", "vp", fake_vp.to_str().unwrap(), &mut reg)
                .unwrap();
            let reg_file = temp_dir.path().join("reg.bash");
            std::fs::write(&reg_file, reg).unwrap();

            let script = r#"
                set -eu
                source "$1"
                HOME="/Users/tester"
                PROJECT="apps/my app"

                COMP_WORDS=(vp -C '"$PROJECT"' run "")
                COMP_CWORD=4
                COMP_LINE='vp -C "$PROJECT" run '
                COMP_TYPE=9
                _clap_complete_vp
                grep -Fx 'apps/my app' "$2" >/dev/null

                COMP_WORDS=(vp -C "'~/literal'" run "")
                COMP_CWORD=4
                COMP_LINE="vp -C '~/literal' run "
                _clap_complete_vp
                grep -Fx '~/literal' "$2" >/dev/null

                COMP_WORDS=(vp -C "~/expanded" run "")
                COMP_CWORD=4
                COMP_LINE="vp -C ~/expanded run "
                _clap_complete_vp
                grep -Fx '/Users/tester/expanded' "$2" >/dev/null

                COMP_WORDS=(vpr -C '"$PROJECT"' build:prod)
                COMP_CWORD=3
                COMP_LINE='vpr -C "$PROJECT" build:prod'
                _clap_complete_vpr
                grep -Fx 'apps/my app' "$2" >/dev/null
                grep -Fx 'run' "$2" >/dev/null
                grep -Fx 'build:prod' "$2" >/dev/null
            "#;
            let output = Command::new("bash")
                .arg("-c")
                .arg(script)
                .arg("test-bash-adapter")
                .arg(&reg_file)
                .arg(&capture_file)
                .env(
                    "PATH",
                    format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap_or_default()),
                )
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "bash adapter check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // 2. Zsh adapter: local $PROJECT, tilde, and single-quoted literal
        if Command::new("zsh").arg("-c").arg("exit 0").status().is_ok() {
            let mut reg = Vec::new();
            super::VP_ZSH
                .write_registration("VP_COMPLETE", "vp", "vp", fake_vp.to_str().unwrap(), &mut reg)
                .unwrap();
            let reg_file = temp_dir.path().join("reg.zsh");
            std::fs::write(&reg_file, reg).unwrap();

            let script = r#"
                set -eu
                compdef() { : }
                _describe() { : }
                source "$1"
                HOME="/Users/tester"
                PROJECT="apps/my app"

                words=(vp -C '"$PROJECT"' run "")
                CURRENT=5
                _clap_dynamic_completer_vp
                grep -Fx 'apps/my app' "$2" >/dev/null

                words=(vp -C "'~/literal'" run "")
                CURRENT=5
                _clap_dynamic_completer_vp
                grep -Fx '~/literal' "$2" >/dev/null

                words=(vp -C "~/expanded" run "")
                CURRENT=5
                _clap_dynamic_completer_vp
                grep -Fx '/Users/tester/expanded' "$2" >/dev/null
            "#;
            let output = Command::new("zsh")
                .arg("-c")
                .arg(script)
                .arg("test-zsh-adapter")
                .arg(&reg_file)
                .arg(&capture_file)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "zsh adapter check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // 3. Fish adapter: function-local `set -l PROJECT`, tilde, and single-quoted `'~/literal'` / `'$PROJECT'`
        if Command::new("fish").arg("--no-config").arg("-c").arg("exit 0").status().is_ok() {
            let mut reg = Vec::new();
            super::VP_FISH
                .write_registration("VP_COMPLETE", "vp", "vp", fake_vp.to_str().unwrap(), &mut reg)
                .unwrap();
            let reg_file = temp_dir.path().join("reg.fish");
            std::fs::write(&reg_file, reg).unwrap();

            let script = r#"
                source $argv[1]
                set -lx HOME /Users/tester
                set -l PROJECT "apps/my app"

                complete --do-complete='vp -C "$PROJECT" run ' >/dev/null
                grep -Fx 'apps/my app' $argv[2] >/dev/null; or exit 1

                complete --do-complete="vp -C '~/literal' run " >/dev/null
                grep -Fx '~/literal' $argv[2] >/dev/null; or exit 1

                complete --do-complete="vp -C '\$PROJECT' run " >/dev/null
                grep -Fx '$PROJECT' $argv[2] >/dev/null; or exit 1

                complete --do-complete="vp -C ~/expanded run " >/dev/null
                grep -Fx '/Users/tester/expanded' $argv[2] >/dev/null; or exit 1

                complete --do-complete='vpr -C "$PROJECT" ' >/dev/null
                grep -Fx 'apps/my app' $argv[2] >/dev/null; or exit 1
            "#;
            let output = Command::new("fish")
                .arg("--no-config")
                .arg("-c")
                .arg(script)
                .arg(&reg_file)
                .arg(&capture_file)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "fish adapter check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
