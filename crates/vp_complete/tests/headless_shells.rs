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
        hook = ShellDialect::Bash.adapter().render_hook(),
    );

    let Some(stdout) = sb.run_bash(&script) else { return };
    assert!(stdout.contains("RES1: watch"), "Bash prefix trimming must yield 'watch': {stdout}");
    assert!(!stdout.contains("build:hijack"), "Bash set -f must prevent glob expansion: {stdout}");
    assert!(
        stdout.contains("uninstall") && stdout.contains("unlink") && stdout.contains("un"),
        "Bash must return all 'un' aliases: {stdout}"
    );
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
            hook = ShellDialect::Bash.adapter().render_hook(),
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
