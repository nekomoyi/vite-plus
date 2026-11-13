#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_types,
    reason = "test suite uses standard test types and macros"
)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use tempfile::TempDir;
use vp_complete::ShellDialect;

static MOCK_VP_PATH: OnceLock<PathBuf> = OnceLock::new();

fn ensure_mock_vp() -> &'static Path {
    MOCK_VP_PATH.get_or_init(|| {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let status = Command::new("cargo")
            .current_dir(&manifest_dir)
            .args(["build", "--example", "mock_vp"])
            .status()
            .expect("Failed to build mock_vp example");
        assert!(status.success(), "Failed to build mock_vp example binary");

        let exe = format!("mock_vp{}", std::env::consts::EXE_SUFFIX);
        for rel in
            [format!("../../target/debug/examples/{exe}"), format!("target/debug/examples/{exe}")]
        {
            let candidate = manifest_dir.join(rel);
            if candidate.exists() {
                return candidate;
            }
        }
        panic!("Could not find built {exe} binary");
    })
}

fn find_bin(name: &str, fallbacks: &[&str]) -> Option<PathBuf> {
    let finder = if cfg!(windows) { "where" } else { "which" };
    if let Ok(out) = Command::new(finder).arg(name).output()
        && out.status.success()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let p = PathBuf::from(line.trim());
            if p.exists() {
                return Some(p);
            }
        }
    }
    fallbacks.iter().map(Path::new).find(|p| p.exists()).map(Path::to_path_buf)
}

fn find_nu() -> Option<PathBuf> {
    find_bin("nu", &["/opt/homebrew/bin/nu", "/usr/local/bin/nu"])
}

fn find_pwsh() -> Option<PathBuf> {
    find_bin(
        "pwsh",
        &[
            "/opt/homebrew/opt/powershell/bin/pwsh",
            "/opt/homebrew/bin/pwsh",
            "/usr/local/opt/powershell/bin/pwsh",
            "/usr/local/bin/pwsh",
        ],
    )
}

struct TestSandbox {
    _temp: TempDir,
    root: PathBuf,
    bin_dir: PathBuf,
}

impl TestSandbox {
    fn new() -> Self {
        let mock_vp = ensure_mock_vp();
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let bin_dir = root.join("bin");
        fs::create_dir(&bin_dir).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(mock_vp, bin_dir.join("vp")).unwrap();
        #[cfg(windows)]
        fs::copy(mock_vp, bin_dir.join("vp.exe")).unwrap();
        Self { _temp: temp, root, bin_dir }
    }

    fn write_hook(&self, dialect: ShellDialect, filename: &str) -> PathBuf {
        let path = self.root.join(filename);
        fs::write(&path, dialect.render_hook()).unwrap();
        path
    }

    fn install_local_vp_trap(&self) -> (PathBuf, PathBuf) {
        let local_bin_dir = self.root.join("node_modules/.bin");
        fs::create_dir_all(&local_bin_dir).unwrap();
        let marker = self.root.join("pwned_marker");
        #[cfg(unix)]
        {
            let local_vp = local_bin_dir.join("vp");
            fs::write(
                &local_vp,
                format!("#!/bin/sh\necho pwned > \"{}\"\nexit 1\n", marker.display()),
            )
            .unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&local_vp).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&local_vp, perms).unwrap();
        }
        #[cfg(windows)]
        {
            let local_vp = local_bin_dir.join("vp.cmd");
            fs::write(
                &local_vp,
                format!("@echo off\r\necho pwned > \"{}\"\r\nexit /b 1\r\n", marker.display()),
            )
            .unwrap();
        }
        (local_bin_dir, marker)
    }

    fn run_cmd(&self, program: &Path, args: &[&str], label: &str) -> String {
        let out = Command::new(program)
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("Failed to execute {label}: {e}"));
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            out.status.success(),
            "{label} failed: {}\nstdout: {stdout}",
            String::from_utf8_lossy(&out.stderr)
        );
        stdout
    }

    fn run_bash(&self, script: &str) -> Option<String> {
        let bash = Path::new("/bin/bash");
        bash.exists().then(|| self.run_cmd(bash, &["--noprofile", "--norc", "-c", script], "bash"))
    }

    fn run_zsh(&self, script: &str) -> Option<String> {
        let zsh = Path::new("/bin/zsh");
        zsh.exists().then(|| self.run_cmd(zsh, &["-f", "-c", script], "zsh"))
    }

    fn run_fish(&self, script: &str) -> Option<String> {
        let fish = find_bin("fish", &["/opt/homebrew/bin/fish", "/usr/local/bin/fish"])?;
        Some(self.run_cmd(&fish, &["-c", script], "fish"))
    }

    fn run_nu(&self, script: &str) -> Option<String> {
        let nu = find_nu()?;
        Some(self.run_cmd(&nu, &["-n", "-c", script], "nu"))
    }

    fn run_pwsh(&self, script: &str) -> Option<String> {
        let pwsh = find_pwsh()?;
        Some(self.run_cmd(
            &pwsh,
            &["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
            "pwsh",
        ))
    }
}

#[test]
fn test_headless_bash() {
    let sb = TestSandbox::new();
    fs::write(sb.root.join("build:hijack"), "hijack").unwrap();
    fs::write(sb.root.join("evil_glob_match"), "evil").unwrap();

    let script = format!(
        r#"
export PATH="{bin_dir}:$PATH"
{hook}

COMP_LINE="vp run build:wa"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp run build:wa); COMP_CWORD=2
_vp_complete; echo "RES1: ${{COMPREPLY[*]}}"

COMP_LINE="vp run build:"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp run build:); COMP_CWORD=2
_vp_complete; echo "RES2: ${{COMPREPLY[*]}}"

COMP_LINE="vp un"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp un); COMP_CWORD=1
_vp_complete; echo "RES3: ${{COMPREPLY[*]}}"
"#,
        bin_dir = sb.bin_dir.display(),
        hook = ShellDialect::Bash.render_hook(),
    );

    let Some(stdout) = sb.run_bash(&script) else { return };
    assert!(stdout.contains("RES1: watch"), "Bash prefix trimming must yield 'watch': {stdout}");
    assert!(!stdout.contains("build:hijack"), "Bash set -f must prevent glob expansion: {stdout}");
    assert!(
        stdout.contains("uninstall") && stdout.contains("unlink") && stdout.contains("un"),
        "Bash must return all 'un' aliases: {stdout}"
    );
}

#[test]
fn test_headless_zsh() {
    let sb = TestSandbox::new();
    let hook_file = sb.write_hook(ShellDialect::Zsh, "hook.zsh");
    fs::write(
        sb.root.join(".zshrc"),
        "PROMPT='PROMPT> '\nzstyle ':completion:*' menu no\nsetopt LIST_TYPES\n",
    )
    .unwrap();

    let script = format!(
        r#"
zmodload zsh/zpty || exit 0
export ZDOTDIR="{temp_dir}"

run_test() {{
  local cmd="$1" expected="$2"
  zpty pty /bin/zsh -di
  zpty -r pty out "*PROMPT> *" || {{ zpty -d pty; return 1; }}
  zpty -w pty "export PATH=\"{bin_dir}:\$PATH\"; rehash"
  zpty -r pty out "*PROMPT> *" || {{ zpty -d pty; return 1; }}
  zpty -w pty "source \"{hook_file}\""
  zpty -r pty out "*PROMPT> *" || {{ zpty -d pty; return 1; }}
  zpty -w pty "autoload -Uz compinit && compinit -D"
  zpty -r pty out "*PROMPT> *" || {{ zpty -d pty; return 1; }}
  zpty -w -n pty "$cmd"$'\t'$'\t'
  zpty -r pty res "*$expected*" || {{ echo "TIMEOUT: $res"; zpty -d pty; return 1; }}
  echo "RES: $res"
  zpty -d pty
}}

run_test "vp un" "uninstall"
run_test "vp run build:wa" "build:watch"
"#,
        temp_dir = sb.root.display(),
        bin_dir = sb.bin_dir.display(),
        hook_file = hook_file.display(),
    );

    let Some(stdout) = sb.run_zsh(&script) else { return };
    assert!(stdout.contains("uninstall"), "Zsh must complete un aliases: {stdout}");
    assert!(
        stdout.contains("build:watch"),
        "Zsh must complete build:watch without colon splitting: {stdout}"
    );
}

#[test]
fn test_headless_fish() {
    let sb = TestSandbox::new();
    let script = format!(
        r#"
set -gx PATH "{bin_dir}" $PATH
{hook}
echo "=== UN ==="
complete -C "vp un"
echo "=== BUILD ==="
complete -C "vp run build:"
"#,
        bin_dir = sb.bin_dir.display(),
        hook = ShellDialect::Fish.render_hook(),
    );

    let Some(stdout) = sb.run_fish(&script) else { return };
    assert!(
        stdout.contains("uninstall") && stdout.contains("unlink") && stdout.contains("un"),
        "Fish must complete 'un' aliases: {stdout}"
    );
    assert!(
        stdout.contains("build:watch") && stdout.contains("build:*"),
        "Fish must complete build tasks: {stdout}"
    );
}

#[test]
fn test_headless_nu() {
    let sb = TestSandbox::new();
    let hook_file = sb.write_hook(ShellDialect::Nu, "hook.nu");

    let script = format!(
        r#"
$env.PATH = ($env.PATH | prepend "{bin_dir}")
let child = (^$nu.current-exe -n -c 'source "{hook_file}"; ["vp un", "ls | vp un", "vp run build:"] | each {{|line| $line | commandline complete }} | to json --raw' | complete)
if $child.exit_code != 0 {{ error make {{ msg: $"nu completion failed: ($child.stderr)" }} }}
if ($child.stderr | str contains "deprecated") {{ error make {{ msg: $"nu completion emitted deprecation warning: ($child.stderr)" }} }}
print $child.stdout
"#,
        bin_dir = sb.bin_dir.display(),
        hook_file = hook_file.display(),
    );

    let Some(stdout) = sb.run_nu(&script) else { return };
    assert!(stdout.contains("uninstall") && stdout.contains("unlink"), "Nu aliases: {stdout}");
    assert!(
        stdout.contains("build:watch") && stdout.contains("build:*"),
        "Nu colon tasks: {stdout}"
    );
}

#[test]
fn test_headless_pwsh() {
    let sb = TestSandbox::new();
    let hook_file = sb.write_hook(ShellDialect::PowerShell, "hook.ps1");

    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$env:PATH = "{bin_dir}" + [IO.Path]::PathSeparator + $env:PATH
. "{hook_file}"

$global:LASTEXITCODE = 37
$cUn = (TabExpansion2 'vp un').CompletionMatches.CompletionText -join ','
$savedExit = $global:LASTEXITCODE
$cEmpty = (TabExpansion2 '').CompletionMatches.Count
$cColon = (TabExpansion2 'vp run build:').CompletionMatches.CompletionText -join ','
$cPipe = (TabExpansion2 'vp run build | Select-Object -Fir').CompletionMatches.CompletionText -join ','
$cSub = (TabExpansion2 'vp run $(Get-ChildItem -Lit').CompletionMatches.CompletionText -join ','
$cNative = ([System.Management.Automation.CommandCompletion]::CompleteInput('vp un', 5, $null)).CompletionMatches.CompletionText -join ','

Write-Output "UN=$cUn`nEXIT=$savedExit`nEMPTY=$cEmpty`nCOLON=$cColon`nPIPE=$cPipe`nSUB=$cSub`nNATIVE=$cNative"
"#,
        bin_dir = sb.bin_dir.display(),
        hook_file = hook_file.display(),
    );

    let Some(stdout) = sb.run_pwsh(&script) else { return };
    assert!(stdout.contains("uninstall") && stdout.contains("unlink"), "Pwsh aliases: {stdout}");
    assert!(stdout.contains("EXIT=37"), "Pwsh must preserve LASTEXITCODE=37: {stdout}");
    assert!(stdout.contains("build:watch") && stdout.contains("build:*"), "Pwsh colon: {stdout}");
    assert!(stdout.contains("PIPE=-First"), "Pwsh must delegate piped completion: {stdout}");
    assert!(stdout.contains("SUB=-LiteralPath"), "Pwsh must delegate $() subexpression: {stdout}");
    assert!(stdout.contains("uninstall"), "Pwsh native completer must work: {stdout}");
}

#[test]
fn test_local_bin_hook_prefers_global_vp() {
    let sb = TestSandbox::new();
    let (_, marker) = sb.install_local_vp_trap();
    let bin_dir = sb.bin_dir.display();

    if let Some(stdout) = sb.run_bash(&format!(
        r#"
export PATH="{bin_dir}:$PATH"
{hook}
COMP_LINE="./node_modules/.bin/vp un"; COMP_POINT=${{#COMP_LINE}}
COMP_WORDS=(./node_modules/.bin/vp un); COMP_CWORD=1
_vp_complete
echo "BASH_COMPREPLY: ${{COMPREPLY[*]}}"
"#,
        hook = ShellDialect::Bash.render_hook(),
    )) {
        assert!(!marker.exists(), "Bash hook executed untrusted local ./node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Bash must complete via global vp: {stdout}");
    }

    if let Some(stdout) = sb.run_zsh(&format!(
        r#"
export PATH="{bin_dir}:$PATH"
compadd() {{ print -l "${{(@P)${{argv[-1]}}}}"; }}
_files() {{ :; }}
{hook}
words=(./node_modules/.bin/vp un); CURRENT=2; _vp_complete
"#,
        hook = ShellDialect::Zsh.render_hook(),
    )) {
        assert!(!marker.exists(), "Zsh hook executed untrusted local ./node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Zsh must complete via global vp: {stdout}");
    }

    if sb
        .run_fish(&format!(
            r#"
set -gx PATH "{bin_dir}" $PATH
{hook}
builtin complete -c ./node_modules/.bin/vp --keep-order -f -a "(__vp_complete)"
complete -C "./node_modules/.bin/vp un"
"#,
            hook = ShellDialect::Fish.render_hook(),
        ))
        .is_some()
    {
        assert!(!marker.exists(), "Fish hook executed untrusted local ./node_modules/.bin/vp!");
    }

    let nu_hook = sb.write_hook(ShellDialect::Nu, "local_hook.nu");
    if let Some(stdout) = sb.run_nu(&format!(
        r#"
$env.PATH = ($env.PATH | prepend "{bin_dir}")
source "{hook}"
print (nu-complete vp "./node_modules/.bin/vp un" {{ command: ["./node_modules/.bin/vp", "un"] }} | get completions.value | to json --raw)
"#,
        hook = nu_hook.display(),
    )) {
        assert!(!marker.exists(), "Nushell hook executed untrusted local ./node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Nu must complete via global vp: {stdout}");
    }

    let pwsh_hook = sb.write_hook(ShellDialect::PowerShell, "local_hook.ps1");
    if let Some(stdout) = sb.run_pwsh(&format!(
        r#"
$env:PATH = "{bin_dir}" + [IO.Path]::PathSeparator + $env:PATH
. "{hook}"
$m = (TabExpansion2 './node_modules/.bin/vp un').CompletionMatches.CompletionText -join ','
Write-Output "PWSH_LOCAL=$m"
"#,
        hook = pwsh_hook.display(),
    )) {
        assert!(!marker.exists(), "Pwsh hook executed untrusted local ./node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Pwsh must complete via global vp: {stdout}");
    }
}

#[test]
fn test_local_node_modules_bin_first_on_path_and_missing_global_vp() {
    let sb = TestSandbox::new();
    let (local_bin_dir, marker) = sb.install_local_vp_trap();
    let local_bin = local_bin_dir.display();
    let bin_dir = sb.bin_dir.display();

    if let Some(stdout) = sb.run_bash(&format!(
        r#"
export PATH="{local_bin}:{bin_dir}:/usr/bin:/bin"
{hook}
COMP_LINE="vp un"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp un); COMP_CWORD=1
_vp_complete; echo "BASH_FIRST: ${{COMPREPLY[*]}}"
export PATH="{local_bin}:/usr/bin:/bin"
COMPREPLY=(); _vp_complete; echo "BASH_MISSING: ${{COMPREPLY[*]}}"
"#,
        hook = ShellDialect::Bash.render_hook(),
    )) {
        assert!(!marker.exists(), "Bash executed untrusted node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Bash must skip node_modules/.bin: {stdout}");
    }

    if let Some(stdout) = sb.run_zsh(&format!(
        r#"
export PATH="{local_bin}:{bin_dir}:/usr/bin:/bin"
compadd() {{ print -l "${{(@P)${{argv[-1]}}}}"; }}
_files() {{ :; }}
{hook}
words=(vp un); CURRENT=2; _vp_complete
export PATH="{local_bin}:/usr/bin:/bin"; _vp_complete
"#,
        hook = ShellDialect::Zsh.render_hook(),
    )) {
        assert!(!marker.exists(), "Zsh executed untrusted node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Zsh must skip node_modules/.bin: {stdout}");
    }

    if let Some(stdout) = sb.run_fish(&format!(
        r#"
set -gx PATH "{local_bin}" "{bin_dir}" /usr/bin /bin
{hook}
complete -C "vp un"
set -gx PATH "{local_bin}" /usr/bin /bin
complete -C "vp un"
"#,
        hook = ShellDialect::Fish.render_hook(),
    )) {
        assert!(!marker.exists(), "Fish executed untrusted node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Fish must skip node_modules/.bin: {stdout}");
    }

    let nu_hook = sb.write_hook(ShellDialect::Nu, "untrusted_nu.nu");
    if let Some(stdout) = sb.run_nu(&format!(
        r#"
$env.PATH = ["{local_bin}", "{bin_dir}", "/usr/bin", "/bin"]
source "{hook}"
let res1 = ("vp un" | commandline complete)
$env.PATH = ["{local_bin}", "/usr/bin", "/bin"]
let res2 = ("vp un" | commandline complete)
print ([$res1, $res2] | to json --raw)
"#,
        hook = nu_hook.display(),
    )) {
        assert!(!marker.exists(), "Nushell executed untrusted node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Nu must skip node_modules/.bin: {stdout}");
    }

    let pwsh_hook = sb.write_hook(ShellDialect::PowerShell, "untrusted_pwsh.ps1");
    if let Some(stdout) = sb.run_pwsh(&format!(
        r#"
$env:PATH = "{local_bin}" + [IO.Path]::PathSeparator + "{bin_dir}" + [IO.Path]::PathSeparator + "/usr/bin:/bin"
. "{hook}"
$m1 = (TabExpansion2 'vp un').CompletionMatches.CompletionText -join ','
$env:PATH = "{local_bin}" + [IO.Path]::PathSeparator + "/usr/bin:/bin"
$m2 = (TabExpansion2 'vp un').CompletionMatches.CompletionText -join ','
Write-Output "PWSH1=$m1`nPWSH2=$m2"
"#,
        hook = pwsh_hook.display(),
    )) {
        assert!(!marker.exists(), "PowerShell executed untrusted node_modules/.bin/vp!");
        assert!(stdout.contains("uninstall"), "Pwsh must skip node_modules/.bin: {stdout}");
    }
}

#[test]
fn test_headless_quoting_directives_and_midline_regressions() {
    let sb = TestSandbox::new();
    let bin_dir = sb.bin_dir.display();

    let spaced_dir = sb.root.join("my dir");
    fs::create_dir_all(&spaced_dir).unwrap();
    fs::write(spaced_dir.join("entry.js"), "console.log(1);").unwrap();
    fs::write(sb.root.join("no_such_task_trap_file"), "trap").unwrap();
    fs::write(sb.root.join("vp.txt"), "not vp").unwrap();

    let fake_home = sb.root.join("fake_home");
    fs::create_dir_all(fake_home.join("Library/Application Support")).unwrap();

    // 1. Bash 3.2: NO_SPACE vs trailing space, NO_FILE_COMP, and escaped space path
    if let Some(stdout) = sb.run_bash(&format!(
        r#"
export PATH="{bin_dir}:$PATH"
{hook}
COMP_LINE="vp unins"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp unins); COMP_CWORD=1
_vp_complete; printf 'BASH_SINGLE=<%s>\n' "${{COMPREPLY[0]-}}"

COMP_LINE='vp -C my\ d'; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp -C 'my\ d'); COMP_CWORD=2
_vp_complete; printf 'BASH_NOSPACE_DIR=<%s>\n' "${{COMPREPLY[0]-}}"

COMP_LINE="vp run no_such_task"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp run no_such_task); COMP_CWORD=2
_vp_complete; printf 'BASH_NOFILE_COUNT=%d\n' "${{#COMPREPLY[@]}}"
"#,
        hook = ShellDialect::Bash.render_hook(),
    )) {
        assert!(
            stdout.contains("BASH_NOSPACE_DIR=<my\\ dir/>"),
            "Bash must preserve escaped space prefix and omit trailing space on NO_SPACE dir: {stdout}"
        );
        assert!(
            stdout.contains("BASH_NOFILE_COUNT=0"),
            "Bash must respect NO_FILE_COMP without falling back to files: {stdout}"
        );
    }

    // 2. Zsh ZLE: NO_SPACE (`-S ''` after `_vp_items`) + quoted path completion
    let zsh_hook = sb.write_hook(ShellDialect::Zsh, "reg_hook.zsh");
    fs::write(
        sb.root.join(".zshrc"),
        format!(
            r#"bindkey -e
PROMPT='PROMPT> '
export PATH="{bin_dir}:$PATH"
source "{hook}"
dump_buf() {{ print -r -- "ZBUF=<$BUFFER>" >> "{temp_dir}/zbuf.txt"; print "ZDUMP_DONE"; }}
zle -N dump_buf
bindkey "^O" dump_buf
autoload -Uz compinit && compinit -u -D
"#,
            hook = zsh_hook.display(),
            temp_dir = sb.root.display(),
        ),
    )
    .unwrap();
    if let Some(stdout) = sb.run_zsh(&format!(
        r#"
zmodload zsh/zpty || exit 0
export ZDOTDIR="{temp_dir}"
for input in 'vp -C "my d' 'vp node "my dir/en'; do
  zpty p /bin/zsh -di
  zpty -r p o "*PROMPT> *" || {{ zpty -d p; exit 1; }}
  zpty -w -n p "$input"$'\t\x0f'
  zpty -r p o "*ZDUMP_DONE*" || {{ zpty -d p; exit 1; }}
  zpty -d p
done
cat "{temp_dir}/zbuf.txt"
"#,
        temp_dir = sb.root.display(),
    )) {
        assert!(
            stdout.contains("my dir/") && stdout.contains("entry.js"),
            "Zsh ZLE must complete NO_SPACE directory and quoted file path via _describe: {stdout}"
        );
    }

    // 3. Fish: quoted path completion
    if let Some(stdout) = sb.run_fish(&format!(
        "set -gx PATH \"{bin_dir}\" $PATH\n{hook}\ncomplete -C 'vp node \"my dir/en'\n",
        hook = ShellDialect::Fish.render_hook(),
    )) {
        assert!(stdout.contains("my dir/entry.js"), "Fish quoted path completion: {stdout}");
    }

    // 4. Nushell: `~/Library/Applica` with space + quoted path completion
    let nu_hook = sb.write_hook(ShellDialect::Nu, "reg_hook.nu");
    if let Some(stdout) = sb.run_nu(&format!(
        r#"
$env.HOME = "{fake_home}"
$env.PATH = ($env.PATH | prepend "{bin_dir}")
let child = (^$nu.current-exe -n -c 'source "{hook}"; ["vp -C ~/Library/Applica", "vp node \"my dir/en"] | each {{|line| $line | commandline complete }} | to json --raw' | complete)
if $child.exit_code != 0 {{ error make {{ msg: $child.stderr }} }}
print $child.stdout
"#,
        fake_home = fake_home.display(),
        hook = nu_hook.display(),
    )) {
        assert!(
            stdout.contains("Application Support/") && stdout.contains("my dir/entry.js"),
            "Nu must complete tilde path with spaces and quoted path: {stdout}"
        );
    }

    // 5. PowerShell: mid-token `TabExpansion2 'vp uninstall --global' 5` + `./vp.txt un` guard
    let pwsh_hook = sb.write_hook(ShellDialect::PowerShell, "reg_hook.ps1");
    if let Some(stdout) = sb.run_pwsh(&format!(
        r#"
$ErrorActionPreference = 'Stop'
$env:PATH = "{bin_dir}" + [IO.Path]::PathSeparator + $env:PATH
. "{hook}"
$mid = TabExpansion2 'vp uninstall --global' 5
$midMatches = $mid.CompletionMatches.CompletionText -join ','
$vpTxt = (TabExpansion2 './vp.txt un').CompletionMatches.CompletionText -join ','
Write-Output "MID_MATCHES=$midMatches`nMID_SPAN=$($mid.ReplacementIndex):$($mid.ReplacementLength)`nVPTXT=$vpTxt"
"#,
        hook = pwsh_hook.display(),
    )) {
        assert!(
            stdout.contains("uninstall")
                && stdout.contains("unlink")
                && stdout.contains("MID_SPAN=3:9"),
            "Pwsh mid-token completion must slice at cursor and replace full token (3:9): {stdout}"
        );
        assert!(
            !stdout.contains("VPTXT=un,uninstall"),
            "Pwsh must not treat ./vp.txt as vp: {stdout}"
        );
    }
}

#[test]
fn test_shell_regressions_braces_env_tilde_word_zero() {
    let sb = TestSandbox::new();
    let bin_dir = sb.bin_dir.display();
    let fake_home = sb.root.join("fake_home");
    let home_proj = fake_home.join("project");
    fs::create_dir_all(&home_proj).unwrap();
    fs::write(home_proj.join("entry.js"), "export default 1;\n").unwrap();
    fs::write(sb.root.join("name{one,two}.js"), "1\n").unwrap();
    fs::write(sb.root.join("hello!world.js"), "2\n").unwrap();

    let dash_dir = fake_home.join("-dashdir");
    let space_proj = sb.root.join("project space");
    let probe_marker = sb.root.join("PROBE_MARKER");
    fs::create_dir_all(&dash_dir).unwrap();
    fs::create_dir_all(&space_proj).unwrap();
    fs::write(sb.root.join("vite.config.ts"), "export default {};\n").unwrap();
    fs::write(sb.root.join("nefar$(touch PROBE_MARKER).txt"), "safe\n").unwrap();

    // 1. Bash: brace & history expansion escaping, leading env assignments, and -C ~/project
    if let Some(stdout) = sb.run_bash(&format!(
        r#"
export HOME="{fake_home}"
export PATH="{bin_dir}:$PATH"
{hook}

COMP_LINE="vp node name"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp node name); COMP_CWORD=2
_vp_complete
set +f
eval "arr_brace=(${{COMPREPLY[0]}})"
echo "BRACE_COUNT=${{#arr_brace[@]}} BRACE_VAL=${{arr_brace[0]}}"

set -H
COMP_LINE="vp node hello"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp node hello); COMP_CWORD=2
_vp_complete
echo "BANG_RAW=${{COMPREPLY[0]}}"

COMP_LINE="CI=1 vp un"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(CI=1 vp un); COMP_CWORD=2
_vp_complete
echo "ENV_CMD=${{COMPREPLY[*]}}"

COMP_LINE="CI=1 vp run build:wa"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(CI=1 vp run build:wa); COMP_CWORD=4
_vp_complete
echo "ENV_TASK=${{COMPREPLY[*]}}"

COMP_LINE='FOO=$(echo "$(printf "(")") vp run build:wa'; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(FOO=x vp run build:wa); COMP_CWORD=4
_vp_complete
echo "NESTED_ENV_TASK=${{COMPREPLY[*]}}"

COMP_LINE='FOO="`printf '\''"'\''`" vp run build:wa'; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(FOO=x vp run build:wa); COMP_CWORD=4
_vp_complete
echo "BACKTICK_ENV_TASK=${{COMPREPLY[*]}}"

COMP_LINE="vp dev --mode nefar"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp dev --mode nefar); COMP_CWORD=3
_vp_complete
eval "arr_nefar=(${{COMPREPLY[0]}})"
echo "NEFAR_VAL=${{arr_nefar[0]}}"

COMP_LINE="vp -C ~/project node en"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp -C ~/project node en); COMP_CWORD=4
_vp_complete
echo "BASH_TILDE=${{COMPREPLY[*]}}"
        COMP_LINE='vp node "hello'; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp node '"hello'); COMP_CWORD=2
        _vp_complete
        echo "BANG_DQ=${{COMPREPLY[0]}}"

        COMP_LINE="FOO=\$(echo hi there) vp un"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(FOO=x vp un); COMP_CWORD=2
        _vp_complete
        echo "ENV_SUBST=${{COMPREPLY[*]}}"

        COMP_LINE="vp -C~/proj"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp -C~/proj); COMP_CWORD=1
        _vp_complete
        echo "BASH_ATTACHED_TILDE=${{COMPREPLY[0]}}"

        COMP_LINE="vp -C=~/proj"; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp -C = "~/proj"); COMP_CWORD=3
        _vp_complete
        echo "BASH_EQ_TILDE=${{COMPREPLY[0]}}"

        COMP_LINE='vp -C="project sp'; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp -C '="project sp'); COMP_CWORD=2
        _vp_complete
        echo "BASH_EQ_QUOTED=<${{COMPREPLY[0]}}>"

        COMP_LINE='vp -C"project sp'; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp '-C"project sp'); COMP_CWORD=1
        _vp_complete
        echo "BASH_ATTACHED_QUOTED=<${{COMPREPLY[0]}}>"

        COMP_LINE='vp dev --config="vite.con'; COMP_POINT=${{#COMP_LINE}}; COMP_WORDS=(vp dev --config '="vite.con'); COMP_CWORD=3
        _vp_complete
        echo "BASH_LONG_QUOTED=<${{COMPREPLY[0]}}>"
"#,
        fake_home = fake_home.display(),
        hook = ShellDialect::Bash.render_hook(),
    )) {
        assert!(
            !probe_marker.exists() && stdout.contains("NEFAR_VAL=nefar$(touch "),
            "Bash fallback file completion must escape command substitutions without executing them: {stdout}"
        );
        assert!(
            stdout.contains("BRACE_COUNT=1 BRACE_VAL=name{one,two}.js"),
            "Bash must escape braces to prevent brace expansion: {stdout}"
        );
        assert!(
            stdout.contains(r"BANG_RAW=hello\!world.js"),
            "Bash must escape ! to prevent history expansion: {stdout}"
        );
        assert!(
            stdout.contains("BANG_DQ=hello\"'!'\"world.js"),
            "Bash must safely quote ! inside double quotes: {stdout}"
        );
        assert!(
            stdout.contains("uninstall") && stdout.contains("ENV_SUBST=uninstall"),
            "Bash must complete subcommands after leading CI=1 and FOO=$(...): {stdout}"
        );
        assert!(
            stdout.contains("ENV_TASK=watch")
                && stdout.contains("NESTED_ENV_TASK=watch")
                && stdout.contains("BACKTICK_ENV_TASK=watch"),
            "Bash must complete tasks after leading CI=1, nested $(), and quoted backticks: {stdout}"
        );
        assert!(
            stdout.contains("BASH_TILDE=entry.js"),
            "Bash must resolve -C ~/project against HOME: {stdout}"
        );
        assert!(
            stdout.contains(&format!("BASH_ATTACHED_TILDE=-C{}/project/", fake_home.display())),
            "Bash -C~/proj must expand to executable HOME path: {stdout}"
        );
        assert!(
            stdout.contains(&format!("BASH_EQ_TILDE={}/project/", fake_home.display())),
            "Bash -C=~/proj must expand value after = to HOME path: {stdout}"
        );
        assert!(
            stdout.contains("BASH_EQ_QUOTED=<project space/>")
                && stdout.contains("BASH_ATTACHED_QUOTED=<project space/>")
                && stdout.contains("BASH_LONG_QUOTED=<vite.config.ts>"),
            "Bash quoted attached flags must trim flag prefix to match Readline replacement span: {stdout}"
        );
    }

    // 2. Fish: -C ~/project node en
    if let Some(stdout) = sb.run_fish(&format!(
        "set -gx HOME \"{fake_home}\"\nset -gx PATH \"{bin_dir}\" $PATH\n{hook}\ncomplete -C 'vp -C ~/project node en'\n",
        fake_home = fake_home.display(),
        hook = ShellDialect::Fish.render_hook(),
    )) {
        assert!(stdout.contains("entry.js"), "Fish must resolve -C ~/project: {stdout}");
    }

    // 3. Nushell: -C ~/project node en + attached -C~/proj
    let nu_hook = sb.write_hook(ShellDialect::Nu, "tilde_hook.nu");
    if let Some(stdout) = sb.run_nu(&format!(
        r#"
$env.HOME = "{fake_home}"
$env.PATH = ($env.PATH | prepend "{bin_dir}")
let child = (^$nu.current-exe -n -c 'source "{hook}"; print ("vp -C ~/project node en" | commandline complete | to json --raw); print ("vp -C~/proj" | commandline complete | to json --raw)' | complete)
if $child.exit_code != 0 {{ error make {{ msg: $child.stderr }} }}
print $child.stdout
"#,
        fake_home = fake_home.display(),
        hook = nu_hook.display(),
    )) {
        assert!(stdout.contains("entry.js"), "Nu must resolve -C ~/project: {stdout}");
        assert!(
            stdout.contains(&format!("-C{}/project/", fake_home.display())),
            "Nu must expand attached -C~/proj to HOME path: {stdout}"
        );
    }

    // 4. PowerShell: TabExpansion2 'vp' 2 (word zero) + attached -Cpro / -C=pro / -C~/proj + standalone ./-dashdir/
    let pwsh_hook = sb.write_hook(ShellDialect::PowerShell, "word0_hook.ps1");
    if let Some(stdout) = sb.run_pwsh(&format!(
        r#"
$ErrorActionPreference = 'Stop'
$env:HOME = "{fake_home}"
$env:USERPROFILE = "{fake_home}"
$env:PATH = "{bin_dir}" + [IO.Path]::PathSeparator + $env:PATH
Set-Location "{fake_home}"
. "{hook}"
$w0 = TabExpansion2 'vp' 2
$c1 = (TabExpansion2 'vp -Cpro').CompletionMatches.CompletionText -join ','
$c2 = (TabExpansion2 'vp -C=pro').CompletionMatches.CompletionText -join ','
$c3 = (TabExpansion2 'vp -C~/pro').CompletionMatches.CompletionText -join ','
$cDash = (TabExpansion2 'vp -C -dash').CompletionMatches.CompletionText -join ','
Write-Output "W0_OK=$($null -ne $w0)"
Write-Output "PWSH_C1=$c1"
Write-Output "PWSH_C2=$c2"
Write-Output "PWSH_C3=$c3"
Write-Output "PWSH_DASH=$cDash"
"#,
        fake_home = fake_home.display(),
        hook = pwsh_hook.display(),
    )) {
        assert!(stdout.contains("W0_OK=True"), "Pwsh word-zero completion must succeed: {stdout}");
        assert!(
            stdout.contains("PWSH_C1=-Cproject/"),
            "Pwsh attached -Cpro must not prepend ./: {stdout}"
        );
        assert!(
            stdout.contains("PWSH_C2=-C=project/"),
            "Pwsh attached -C=pro must not prepend ./: {stdout}"
        );
        assert!(
            stdout.contains(&format!("PWSH_C3=-C{}/project/", fake_home.display())),
            "Pwsh attached -C~/pro must expand to HOME path: {stdout}"
        );
        assert!(
            stdout.contains("PWSH_DASH=./-dashdir/"),
            "Pwsh standalone dash directory must use portable ./-dashdir/ prefix: {stdout}"
        );
    }
}

#[test]
fn test_zsh_interactive_ux_and_cross_shell_chdir_fallback() {
    let sb = TestSandbox::new();
    let bin_dir = sb.bin_dir.display();
    let fake_home = sb.root.join("fake_home");
    let home_proj = fake_home.join("project");
    let dollar_home = sb.root.join("my$home [1]");
    let quote_home = sb.root.join("my'home [1]");
    let work_sub = sb.root.join("workspace/sub");
    fs::create_dir_all(&home_proj).unwrap();
    fs::create_dir_all(dollar_home.join("project")).unwrap();
    fs::create_dir_all(quote_home.join("project")).unwrap();
    fs::create_dir_all(&work_sub).unwrap();
    fs::write(home_proj.join("entry.js"), "export default 1;\n").unwrap();

    // 1. Real interactive Zsh ZLE (`zpty`) verification of exact $BUFFER results
    let zsh_hook = sb.write_hook(ShellDialect::Zsh, "ux_hook.zsh");
    fs::write(
        sb.root.join(".zshrc"),
        format!(
            r#"stty -ixon 2>/dev/null
bindkey -e
PROMPT='P> '
export HOME="{fake_home}"
export PATH="{bin_dir}:$PATH"
cd "{work_sub}"
autoload -Uz compinit && compinit -u -D
source "{hook}"
snap_and_clear() {{
  local -a eval_words
  eval "eval_words=($BUFFER)" 2>/dev/null || eval_words=("__EVAL_ERR__")
  print -r -- "SNAP=<$BUFFER> ARG1=<${{eval_words[2]-}}>" >> "{out_file}"
  BUFFER=""
  print "__SNAP_DONE__"
  zle send-break
}}
zle -N snap_and_clear
bindkey "^X^E" snap_and_clear
"#,
            fake_home = fake_home.display(),
            hook = zsh_hook.display(),
            work_sub = work_sub.display(),
            out_file = sb.root.join("zsh_ux_snaps.txt").display(),
        ),
    )
    .unwrap();

    if let Some(stdout) = sb.run_zsh(&format!(
        r#"
zmodload zsh/zpty || exit 0
export ZDOTDIR="{temp_dir}"
zpty p /bin/zsh -di
zpty -r p o "*P> *" || {{ zpty -d p; exit 1; }}

probe() {{
  zpty -w -n p "$1"$'\x18\x05'
  zpty -r p o "*__SNAP_DONE__*" || {{ zpty -d p; exit 1; }}
  zpty -r p o "*P> *" || {{ zpty -d p; exit 1; }}
}}

probe $'vp run @vit\t'
probe $'vp run build\t'
probe $'vp run build:\t\t'
probe $'vp run "@vit\t'
probe $'vp -C ..\t'
probe $'vp -C ~/proj\t'
probe $'vp -C~\t'
probe $'vp -C~/proj\t'
probe $'vp -C=~/proj\t'
probe $'vp -C ~/proj\t '
probe $'vp -C ~/project passthrough en\t'

zpty -w p 'export HOME='\''{dollar_home}'\'''
zpty -r p o "*P> *" || {{ zpty -d p; exit 1; }}
probe $'vp "-C~/proj\t"'

zpty -w p "export HOME=\"{quote_home}\""
zpty -r p o "*P> *" || {{ zpty -d p; exit 1; }}
probe $'vp \'-C~/proj\t\''

zpty -w p 'export HOME="{fake_home}"; setopt extendedglob'
zpty -r p o "*P> *" || {{ zpty -d p; exit 1; }}
probe $'vp run @vit\t'

zpty -w p "exit"
zpty -d p
cat "{temp_dir}/zsh_ux_snaps.txt"
"#,
        temp_dir = sb.root.display(),
        dollar_home = dollar_home.display(),
        quote_home = quote_home.display(),
        fake_home = fake_home.display(),
    )) {
        let home_disp = fake_home.display();
        let expected_c_home = format!("SNAP=<vp -C{home_disp}/>");
        let expected_c_proj = format!("SNAP=<vp -C{home_disp}/project/>");
        let expected_ceq_proj = format!("SNAP=<vp -C={home_disp}/project/>");
        let expected_dq_dollar = format!("ARG1=<-C{}/project/>", dollar_home.display());
        let expected_sq_quote = format!("ARG1=<-C{}/project/>", quote_home.display());
        for expected in [
            "SNAP=<vp run @vitejs/vite-monorepo#build >",
            "SNAP=<vp run build:>",
            "SNAP=<vp run build:\\*>",
            "SNAP=<vp run \"@vitejs/vite-monorepo#build\" >",
            "SNAP=<vp -C ../>",
            "SNAP=<vp -C ~/project/>",
            expected_c_home.as_str(),
            expected_c_proj.as_str(),
            expected_ceq_proj.as_str(),
            "SNAP=<vp -C ~/project >",
            "SNAP=<vp -C ~/project passthrough entry.js >",
            expected_dq_dollar.as_str(),
            expected_sq_quote.as_str(),
            "SNAP=<vp run @vitejs/vite-monorepo\\#build >",
        ] {
            assert!(
                stdout.contains(expected),
                "Zsh interactive UX mismatch: expected {expected:?} in:\n{stdout}"
            );
        }
    }

    // 2. Cross-shell `-C ~/project passthrough en<Tab>` file fallback in Bash, Fish, Nu, and Pwsh
    if let Some(stdout) = sb.run_bash(&format!(
        r#"
export HOME="{fake_home}"
export PATH="{bin_dir}:$PATH"
{hook}
COMP_LINE="vp -C ~/project passthrough en"; COMP_POINT=${{#COMP_LINE}}
COMP_WORDS=(vp -C "~/project" passthrough en); COMP_CWORD=4
_vp_complete; echo "BASH_FB=${{COMPREPLY[*]}}"
"#,
        fake_home = fake_home.display(),
        hook = ShellDialect::Bash.render_hook(),
    )) {
        assert!(stdout.contains("BASH_FB=entry.js"), "Bash -C fallback failed: {stdout}");
    }

    if let Some(stdout) = sb.run_fish(&format!(
        "set -gx HOME \"{fake_home}\"\nset -gx PATH \"{bin_dir}\" $PATH\n{hook}\ncomplete -C 'vp -C ~/project passthrough en'\n",
        fake_home = fake_home.display(),
        hook = ShellDialect::Fish.render_hook(),
    )) {
        assert!(stdout.contains("entry.js"), "Fish -C fallback failed: {stdout}");
    }

    let nu_hook = sb.write_hook(ShellDialect::Nu, "fb_hook.nu");
    if let Some(stdout) = sb.run_nu(&format!(
        r#"
$env.HOME = "{fake_home}"
$env.PATH = ($env.PATH | prepend "{bin_dir}")
let child = (^$nu.current-exe -n -c 'source "{hook}"; "vp -C ~/project passthrough en" | commandline complete | to json --raw' | complete)
if $child.exit_code != 0 {{ error make {{ msg: $child.stderr }} }}
print $child.stdout
"#,
        fake_home = fake_home.display(),
        hook = nu_hook.display(),
    )) {
        assert!(stdout.contains("entry.js"), "Nu -C fallback failed: {stdout}");
    }

    let pwsh_hook = sb.write_hook(ShellDialect::PowerShell, "fb_hook.ps1");
    if let Some(stdout) = sb.run_pwsh(&format!(
        r#"
$ErrorActionPreference = 'Stop'
$env:HOME = "{fake_home}"
$env:USERPROFILE = "{fake_home}"
$env:PATH = "{bin_dir}" + [IO.Path]::PathSeparator + $env:PATH
$pwshExe = (Get-Process -Id $PID).Path
& $pwshExe -NoLogo -NoProfile -NonInteractive -Command '. "{hook}"; $fb = (TabExpansion2 "vp -C ~/project passthrough en").CompletionMatches.CompletionText -join ","; Write-Output "PWSH_FB=$fb"'
"#,
        fake_home = fake_home.display(),
        hook = pwsh_hook.display(),
    )) {
        assert!(stdout.contains("PWSH_FB=entry.js"), "Pwsh -C fallback failed: {stdout}");
    }
}

#[cfg(unix)]
#[test]
fn test_bash_interactive_pty_trailing_quotes() {
    let sb = TestSandbox::new();
    let Some(bash_bin) = find_bin("bash", &["/bin/bash", "/usr/bin/bash"]) else {
        return;
    };
    fs::write(sb.root.join("dq_end\""), "1\n").unwrap();
    fs::write(sb.root.join("sq_end'"), "2\n").unwrap();
    let dq_dir = sb.root.join("dq_dir\"");
    let sq_dir = sb.root.join("sq_dir'");
    fs::create_dir_all(&dq_dir).unwrap();
    fs::create_dir_all(&sq_dir).unwrap();
    fs::write(dq_dir.join("child.js"), "3\n").unwrap();
    fs::write(sq_dir.join("child.js"), "4\n").unwrap();

    let rc_file = sb.root.join("bashrc_pty");
    let log_file = sb.root.join("bash_pty_argv.txt");
    fs::write(
        &rc_file,
        format!(
            r#"PS1='BP> '
export PATH="{bin_dir}:$PATH"
cd "{work_dir}"
{hook}
vp() {{
  printf 'ARGV:' >> "{log_file}"
  printf '<%s>' "$@" >> "{log_file}"
  printf '\n' >> "{log_file}"
  echo "__VP_DONE__"
}}
"#,
            bin_dir = sb.bin_dir.display(),
            work_dir = sb.root.display(),
            hook = ShellDialect::Bash.render_hook(),
            log_file = log_file.display(),
        ),
    )
    .unwrap();

    if let Some(stdout) = sb.run_zsh(&format!(
        r#"
zmodload zsh/zpty || exit 0
zpty bp "{bash}" --rcfile "{rc}" -i
zpty -r bp o "*BP> *" || {{ zpty -d bp; exit 1; }}

run_case() {{
  zpty -w -n bp "$1"
  zpty -r bp o "*__VP_DONE__*" || {{ zpty -d bp; exit 1; }}
  zpty -r bp o "*BP> *" || {{ zpty -d bp; exit 1; }}
}}

run_case $'vp node "dq_e\t\n'
run_case $'vp dev --mode="dq_e\t\n'
run_case $'vp node \'sq_e\t\n'
run_case $'vp dev --mode=\'sq_e\t\n'

run_case $'vp passthrough "dq_e"\x02\t\n'
run_case $'vp dev --config=x --mode="dq_e"\x02\t\n'
run_case $'vp passthrough \'sq_e\'\x02\t\n'
run_case $'vp dev --config=x --mode=\'sq_e\'\x02\t\n'

run_case $'vp node "dq_d\tchi\t\n'
run_case $'vp dev --mode="dq_d\tchi\t\n'
run_case $'vp node \'sq_d\tchi\t\n'
run_case $'vp dev --mode=\'sq_d\tchi\t\n'

zpty -w bp "exit"
zpty -d bp
cat "{log_file}"
"#,
        bash = bash_bin.display(),
        rc = rc_file.display(),
        log_file = log_file.display(),
    )) {
        for expected in [
            "ARGV:<node><dq_end\">",
            "ARGV:<dev><--mode=dq_end\">",
            "ARGV:<node><sq_end'>",
            "ARGV:<dev><--mode=sq_end'>",
            "ARGV:<passthrough><dq_end\">",
            "ARGV:<dev><--config=x><--mode=dq_end\">",
            "ARGV:<passthrough><sq_end'>",
            "ARGV:<dev><--config=x><--mode=sq_end'>",
            "ARGV:<node><dq_dir\"/child.js>",
            "ARGV:<dev><--mode=dq_dir\"/child.js>",
            "ARGV:<node><sq_dir'/child.js>",
            "ARGV:<dev><--mode=sq_dir'/child.js>",
        ] {
            assert!(
                stdout.contains(expected),
                "Bash interactive PTY trailing quote failed: expected {expected:?} in:\n{stdout}"
            );
        }
    }
}

#[test]
fn test_pwsh_smart_quotes_and_nu_unclosed_quoted_spaces() {
    let sb = TestSandbox::new();
    let bin_dir = sb.bin_dir.display();

    // 1. PowerShell: smart-quoted candidate must not execute `$(...)` when evaluated as command arg
    let smart_dir = sb.root.join("x\u{2019}$(Write-Output INJECTED)\u{2018}y");
    if fs::create_dir_all(&smart_dir).is_ok() {
        let pwsh_hook = sb.write_hook(ShellDialect::PowerShell, "smart_hook.ps1");
        if let Some(stdout) = sb.run_pwsh(&format!(
            r#"
$ErrorActionPreference = 'Stop'
$env:PATH = '{bin_dir}' + [IO.Path]::PathSeparator + $env:PATH
. '{hook}'
$m = @((TabExpansion2 'vp -C x').CompletionMatches.CompletionText)
$target = $m | Where-Object {{ $_ -like '*Write-Output*' }} | Select-Object -First 1
if (-not $target) {{ throw "Missing smart-quote candidate in: $($m -join ', ')" }}
$evaled = Invoke-Expression "Write-Output $target"
Write-Output "PWSH_EVAL=$evaled"
"#,
            hook = pwsh_hook.display(),
        )) {
            assert!(
                !stdout.contains("PWSH_EVAL=INJECTED"),
                "PowerShell smart quote executed injected subexpression: {stdout}"
            );
            assert!(
                stdout.contains("PWSH_EVAL=x\u{2019}$(Write-Output INJECTED)\u{2018}y"),
                "PowerShell smart quote did not preserve literal candidate: {stdout}"
            );
        }
    }

    // 2. Nushell: trailing space inside unclosed quote (`vp -C "configs `) and `--config="vit`
    let spaced_cfg = sb.root.join("configs with space");
    fs::create_dir_all(&spaced_cfg).unwrap();
    fs::write(spaced_cfg.join("vite.config.ts"), "export default {}").unwrap();
    fs::write(sb.root.join("vite.config.ts"), "export default {}").unwrap();
    let nu_hook = sb.write_hook(ShellDialect::Nu, "unclosed_nu.nu");
    if let Some(stdout) = sb.run_nu(&format!(
        r#"
$env.PATH = ($env.PATH | prepend '{bin_dir}')
let child = (^$nu.current-exe -n -c 'source "{hook}"; print ("vp -C \"configs " | commandline complete | to json --raw); print ("vp dev --config=\"vit" | commandline complete | to json --raw)' | complete)
if $child.exit_code != 0 {{ error make {{ msg: $child.stderr }} }}
print $child.stdout
"#,
        hook = nu_hook.display(),
    )) {
        assert!(
            stdout.contains("configs with space"),
            "Nushell unclosed quote with space failed: {stdout}"
        );
        assert!(
            stdout.contains("--config=vite.config.ts"),
            "Nushell quoted attached flag failed: {stdout}"
        );
    }
}
