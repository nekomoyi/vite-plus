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
    for global in ["list", "rebuild", "node"] {
        assert!(!names.contains(&global), "{global}");
    }
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

fn fish_completions(
    dir: &std::path::Path,
    args: &[&str],
    vars: &[(&str, &std::ffi::OsStr)],
) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_vp"))
        .current_dir(dir)
        .args(["--", "vp"])
        .args(args)
        .env("VP_COMPLETE", "fish")
        .env("VP_HOME", dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("VP_SELF_SETUP_NO_MODIFY_PATH", "1")
        .envs(vars.iter().copied())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.split('\t').next().unwrap().to_owned())
        .collect()
}

#[test]
fn offline_tasks_use_exported_vite_config() {
    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("vite.config.ts"), "const unused = { run: { tasks: { ghost: 'no' } } }; export default { run: { tasks: { real: 'ok' } } }").unwrap();
    assert_eq!(
        fish_completions(
            temp.path(),
            &["run", ""],
            &[("VP_COMPLETE_OFFLINE_ONLY_RUST", "1".as_ref())]
        ),
        ["real"]
    );
}

#[cfg(unix)]
#[test]
fn forbidden_runtime_and_successful_empty_task_match() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    for (bin_dir, name) in [("js_runtime/node/24.0.0/bin", "managed"), ("sysbin", "system")] {
        let bin = dir.join(bin_dir);
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("node"), format!("#!/bin/sh\necho '  {name}: from node'\n"))
            .unwrap();
        std::fs::set_permissions(bin.join("node"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let package = dir.join("node_modules/vite-plus");
    std::fs::create_dir_all(package.join("dist")).unwrap();
    std::fs::write(package.join("package.json"), r#"{"name":"vite-plus"}"#).unwrap();
    let cli = package.join("dist/bin.js");
    std::fs::write(&cli, "").unwrap();
    std::fs::write(dir.join("package.json"), r#"{"scripts":{"fallback":"true"}}"#).unwrap();
    let path = dir.join("sysbin");
    let vars = [("VP_ENV_FORBIDDEN", "1".as_ref()), ("PATH", path.as_os_str())];
    assert_eq!(fish_completions(dir, &["run", ""], &vars), ["system"]);
    assert!(fish_completions(dir, &["run", "fall"], &vars).is_empty());

    let shim = dir.join("bin");
    std::fs::create_dir(&shim).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_vp"), shim.join("node")).unwrap();
    let path = std::env::join_paths([shim, path]).unwrap();
    let vars = [("VP_ENV_FORBIDDEN", "1".as_ref()), ("PATH", path.as_os_str())];
    assert_eq!(fish_completions(dir, &["run", ""], &vars), ["fallback"]);

    let vars = [("VP_ENV_FORBIDDEN", "0".as_ref()), ("PATH", path.as_os_str())];
    assert_eq!(fish_completions(dir, &["run", ""], &vars), ["managed"]);
    std::fs::remove_file(dir.join("js_runtime/node/24.0.0/bin/node")).unwrap();
    assert_eq!(fish_completions(dir, &["run", ""], &vars), ["system"]);
}

#[test]
fn bash_uses_the_directory_the_shell_will_pass() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    let home = tmp.path().join("home");
    for (dir, marker) in
        [(cwd.join("~/work"), "marker-literal"), (home.join("work"), "marker-home")]
    {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(marker), "").unwrap();
    }
    for (word, marker) in
        [("~/work", "marker-home"), ("'~/work'", "marker-literal"), (r"\~/work", "marker-literal")]
    {
        let result = Command::new(env!("CARGO_BIN_EXE_vp"))
            .current_dir(&cwd)
            .args(["--"])
            .env("HOME", &home)
            .env("VP_COMPLETE", "bash")
            .env("_VP_COMP_LINE", format!("vp -C {word} check marker"))
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(String::from_utf8(result.stdout).unwrap().lines().next(), Some(marker));
    }
}
