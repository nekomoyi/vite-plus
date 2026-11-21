#[cfg(unix)]
use std::time::{Duration, Instant};
use std::{fs, path::PathBuf};

use tempfile::TempDir;

use super::*;

fn get_task_candidates(dir: &Path, prefix: &str) -> Vec<vp_complete::CompletionItem> {
    get_task_candidates_with(
        dir,
        prefix,
        || {
            std::env::var_os("VP_COMPLETE_TEST_NODE_BIN")
                .map(PathBuf::from)
                .or_else(super::super::resolve_offline_node)
        },
        |dir| {
            std::env::var_os("VP_COMPLETE_TEST_BIN_JS")
                .map(PathBuf::from)
                .or_else(|| super::super::resolve_offline_bin_js(dir))
        },
    )
}

#[test]
fn test_task_provider_scripts_and_static_tasks() {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();

    fs::write(
        dir.join("package.json"),
        r#"{
            "scripts": {
                "build": "vite build",
                "test": "vitest",
                "lint": "eslint"
            }
        }"#,
    )
    .unwrap();

    fs::write(
        dir.join("vite.config.ts"),
        r#"
        export default {
            run: {
                tasks: {
                    custom: {
                        command: "echo custom"
                    },
                    build: {
                        command: "custom build command"
                    }
                }
            }
        }
        "#,
    )
    .unwrap();

    temp_env::with_var("VP_COMPLETE_OFFLINE_ONLY_RUST", Some("1"), || {
        let tasks = get_task_candidates(dir, "");
        let names: Vec<&str> = tasks.iter().map(|t| t.value.as_str()).collect();
        assert!(names.contains(&"build"), "missing build: {names:?}");
        assert!(names.contains(&"test"), "missing test: {names:?}");
        assert!(names.contains(&"lint"), "missing lint: {names:?}");
        assert!(names.contains(&"custom"), "missing custom: {names:?}");
    });
}

#[cfg(unix)]
fn write_exec_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

#[cfg(unix)]
fn assert_pid_terminated(pid_file: &Path, label: &str) {
    let pid: u32 =
        fs::read_to_string(pid_file).expect("pid file must be written").trim().parse().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let alive = std::process::Command::new("/bin/sh")
        .args(["-c", &format!("kill -0 {pid} 2>/dev/null")])
        .status()
        .is_ok_and(|s| s.success());
    assert!(!alive, "{label} {pid} must be terminated on deadline");
}

#[cfg(unix)]
#[test]
fn test_task_provider_offline_node_timeout() {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();

    fs::write(dir.join("package.json"), r#"{"scripts":{"fallback-task":"echo ok"}}"#).unwrap();
    let fake_node = write_exec_script(dir, "fake_node.sh", "#!/bin/sh\nsleep 10\n");
    let fake_bin_js = dir.join("bin.js");
    fs::write(&fake_bin_js, "console.log('hi');").unwrap();

    temp_env::with_vars(
        [
            ("VP_COMPLETE_OFFLINE_ONLY_RUST", None::<&str>),
            ("VP_COMPLETE_TEST_NODE_BIN", Some(fake_node.to_str().unwrap())),
            ("VP_COMPLETE_TEST_BIN_JS", Some(fake_bin_js.to_str().unwrap())),
        ],
        || {
            let start = Instant::now();
            let tasks = get_task_candidates(dir, "");
            let elapsed = start.elapsed();

            assert!(
                elapsed < Duration::from_millis(2500),
                "Timeout must trigger within 2.5s, took {elapsed:?}"
            );
            assert!(
                elapsed >= Duration::from_millis(1400),
                "Timeout should wait ~1.5s, took {elapsed:?}"
            );
            let names: Vec<&str> = tasks.iter().map(|t| t.value.as_str()).collect();
            assert_eq!(names, vec!["fallback-task"]);
        },
    );
}

#[test]
fn test_sanitize_task_current_prefix() {
    for (input, expected) in [
        ("test\\:u", "test:u"),
        ("pkg\\#test\\:u", "pkg#test:u"),
        ("'test:unit'", "test:unit"),
        ("\"pkg#build\"", "pkg#build"),
    ] {
        assert_eq!(sanitize_task_current_prefix(input), expected);
    }
}

#[test]
fn test_parse_task_completion_lines() {
    let stdout = concat!(
        "  build: Run project build: vite build --mode:prod\n",
        "  test:unit: Run unit tests\n",
        "  pkg#test:unit: Workspace unit tests\n",
        "  //: Root comment\n",
        "  pkg#//: Workspace comment\n",
        "  rolldown## Scrips for docs #: echo 1\n",
        "# Global comment: echo 2\n",
        "  invalid task with spaces: echo 3\n",
        "  empty_desc: \n",
    );

    let complete = |cur| {
        parse_task_completion_lines(stdout, cur).into_iter().map(|c| c.value).collect::<Vec<_>>()
    };

    assert_eq!(complete(""), vec!["build", "test:unit", "pkg#test:unit", "empty_desc"]);
    assert_eq!(complete("test:"), vec!["test:unit"]);
    assert!(complete("nonexistent").is_empty());

    let all = parse_task_completion_lines(stdout, "");
    assert_eq!(all[0].description.as_deref(), Some("Run project build: vite build --mode:prod"));
    assert_eq!(all[3].description, None);
}

#[cfg(unix)]
#[test]
fn test_task_provider_sets_vp_env_forbidden_and_kills_process_group() {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();

    let env_report_node = write_exec_script(
        dir,
        "env_report_node.sh",
        "#!/bin/sh\necho \"  forbidden_${VP_ENV_FORBIDDEN:-unset}: ${VP_COMPLETE:-stripped}\"\n",
    );
    let fake_bin_js = dir.join("bin.js");
    fs::write(&fake_bin_js, "// stub").unwrap();

    temp_env::with_vars(
        [
            ("VP_COMPLETE", Some("zsh")),
            ("VP_COMPLETE_OFFLINE_ONLY_RUST", None::<&str>),
            ("VP_COMPLETE_TEST_NODE_BIN", Some(env_report_node.to_str().unwrap())),
            ("VP_COMPLETE_TEST_BIN_JS", Some(fake_bin_js.to_str().unwrap())),
        ],
        || {
            let items = get_task_candidates(dir, "");
            assert_eq!(items.len(), 1, "Expected 1 task candidate, got: {items:?}");
            assert_eq!(items[0].value, "forbidden_1", "Must set VP_ENV_FORBIDDEN=1");
            assert_eq!(items[0].description.as_deref(), Some("stripped"));
        },
    );

    let pid_file = dir.join("grandchild.pid");
    let hang_tree_node = write_exec_script(
        dir,
        "hang_tree_node.sh",
        &format!(
            "#!/bin/sh\nsleep 30 </dev/null >/dev/null 2>&1 &\necho $! > \"{}\"\nsleep 30\n",
            pid_file.display()
        ),
    );

    temp_env::with_vars(
        [
            ("VP_COMPLETE_OFFLINE_ONLY_RUST", None::<&str>),
            ("VP_COMPLETE_TEST_NODE_BIN", Some(hang_tree_node.to_str().unwrap())),
            ("VP_COMPLETE_TEST_BIN_JS", Some(fake_bin_js.to_str().unwrap())),
        ],
        || {
            let _ = get_task_candidates(dir, "");
        },
    );

    assert_pid_terminated(&pid_file, "Grandchild process");
}

#[cfg(unix)]
#[test]
fn test_task_provider_early_stdout_close_respects_deadline() {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();
    fs::write(
        dir.join("package.json"),
        r#"{"name":"demo","scripts":{"fallback:build":"vite build"}}"#,
    )
    .unwrap();
    let fake_bin_js = dir.join("bin.js");
    fs::write(&fake_bin_js, "// stub").unwrap();

    let pid_file = dir.join("early_close.pid");
    let early_close_node = write_exec_script(
        dir,
        "early_close_node.sh",
        &format!(
            "#!/bin/sh\necho $$ > \"{}\"\nexec >/dev/null 2>&1\nsleep 8\n",
            pid_file.display()
        ),
    );

    let (items, elapsed) = temp_env::with_vars(
        [
            ("VP_COMPLETE_OFFLINE_ONLY_RUST", None::<&str>),
            ("VP_COMPLETE_TEST_NODE_BIN", Some(early_close_node.to_str().unwrap())),
            ("VP_COMPLETE_TEST_BIN_JS", Some(fake_bin_js.to_str().unwrap())),
        ],
        || {
            let start = std::time::Instant::now();
            let items = get_task_candidates(dir, "");
            (items, start.elapsed())
        },
    );

    assert!(
        elapsed < Duration::from_millis(2500),
        "Early stdout close must enforce 1500ms total deadline, took {elapsed:?}"
    );
    assert!(items.iter().any(|i| i.value == "fallback:build"));
    assert_pid_terminated(&pid_file, "Early-stdout-close child");
}
