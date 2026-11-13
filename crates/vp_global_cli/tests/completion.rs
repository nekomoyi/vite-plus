#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "test binary harness uses standard process and timing APIs"
)]

use std::{
    process::Command,
    time::{Duration, Instant},
};

use tempfile::TempDir;

#[test]
fn test_early_completion_intercept_uninitialized_home() {
    let temp_home = TempDir::new().expect("create temp dir");
    let vp_bin = env!("CARGO_BIN_EXE_vp");

    let start = Instant::now();
    let output = Command::new(vp_bin)
        .args(["--", "vp", "un"])
        .env("VP_COMPLETE", "zsh")
        .env("_VP_COMPLETE_INDEX", "1")
        .env("VP_HOME", temp_home.path())
        .env("HOME", temp_home.path())
        .env("USERPROFILE", temp_home.path())
        .env("ZDOTDIR", temp_home.path())
        .output()
        .expect("spawn vp binary");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(3),
        "Early completion must exit without hang (<3s in debug build), took {elapsed:?}"
    );
    assert!(
        output.status.success(),
        "Completion exit code was not 0: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("V\tun\t"), "Stdout must contain alias 'un' candidate, got: {stdout}");
    assert!(
        stdout.contains("V\tuninstall\t"),
        "Stdout must contain 'uninstall' candidate, got: {stdout}"
    );
    assert!(
        stdout.contains("V\tunlink\t"),
        "Stdout must contain 'unlink' candidate, got: {stdout}"
    );
}

#[test]
fn test_local_node_modules_bin_hides_global_only_commands() {
    let temp_home = TempDir::new().expect("create temp dir");
    let vp_bin = env!("CARGO_BIN_EXE_vp");

    let output = Command::new(vp_bin)
        .args(["--", "./node_modules/.bin/vp", ""])
        .env("VP_COMPLETE", "fish")
        .env("_VP_COMPLETE_INDEX", "1")
        .env("VP_HOME", temp_home.path())
        .env("HOME", temp_home.path())
        .env("USERPROFILE", temp_home.path())
        .env("ZDOTDIR", temp_home.path())
        .output()
        .expect("spawn vp binary");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let names: Vec<&str> = stdout.lines().filter_map(|l| l.split('\t').next()).collect();

    assert!(names.contains(&"install"), "Must include install: {names:?}");
    assert!(names.contains(&"run"), "Must include run: {names:?}");
    assert!(names.contains(&"check"), "Must include check: {names:?}");
    assert!(
        !names.contains(&"env"),
        "Local ./node_modules/.bin/vp must hide global-only 'env': {names:?}"
    );
    assert!(
        !names.contains(&"upgrade"),
        "Local ./node_modules/.bin/vp must hide global-only 'upgrade': {names:?}"
    );
    assert!(
        !names.contains(&"implode"),
        "Local ./node_modules/.bin/vp must hide global-only 'implode': {names:?}"
    );
}
