//! Shell autocompletion host integration powered by `vp_complete`.
#![forbid(unsafe_code)]

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use vp_complete::{CmdSpec, EarlyCompletionOutcome, VpCompletionConfig};

use crate::cli;

/// Intercept `VP_COMPLETE` environment variable before Tokio runtime / self-setup runs.
/// Returns `Some(ExitCode)` if completion was handled, or `None` if normal CLI execution should continue.
#[inline]
pub(crate) fn try_handle_completion_early() -> Option<ExitCode> {
    let dialect_var = std::env::var_os("VP_COMPLETE")?;
    handle_completion_early_cold(&dialect_var)
}

#[cold]
#[inline(never)]
fn handle_completion_early_cold(dialect_var: &std::ffi::OsStr) -> Option<ExitCode> {
    vp_shared::ensure_blocking_stdio();
    match vp_complete::dispatch_early_completion(dialect_var, build_vp_cmd_spec, build_vpx_cmd_spec)
    {
        EarlyCompletionOutcome::ContinueCli => None,
        EarlyCompletionOutcome::InvalidDialect => Some(ExitCode::FAILURE),
        EarlyCompletionOutcome::Handled { output, trailing_newline } => {
            if trailing_newline {
                if !output.is_empty() {
                    vp_shared::output::print_stdout_line(format_args!("{output}"));
                }
            } else {
                vp_shared::output::print_stdout(format_args!("{output}"));
            }
            Some(ExitCode::SUCCESS)
        }
    }
}

#[cold]
fn global_bin_dir() -> PathBuf {
    vp_shared::EnvConfig::get().dirs.bin.as_path().to_path_buf()
}

#[cold]
fn node_store_dir() -> PathBuf {
    vp_shared::EnvConfig::get().dirs.data.join("js_runtime/node").as_path().to_path_buf()
}

#[cold]
pub(crate) fn read_pkg_manifest(dir: &Path) -> Option<vp_complete::PkgManifest> {
    let bytes = std::fs::read(dir.join("package.json")).ok()?;
    let stripped = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let val: serde_json::Value = serde_json::from_slice(stripped).ok()?;
    let obj = val.as_object()?;

    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let extract_str_array = |arr: &Vec<serde_json::Value>| -> Vec<String> {
        let mut out = Vec::with_capacity(arr.len());
        for v in arr {
            if let Some(s) = v.as_str() {
                out.push(s.to_string());
            }
        }
        out
    };

    let workspaces = match obj.get("workspaces") {
        Some(serde_json::Value::Array(arr)) => Some(extract_str_array(arr)),
        Some(serde_json::Value::Object(ws_obj)) => {
            ws_obj.get("packages").and_then(|v| v.as_array()).map(extract_str_array)
        }
        _ => None,
    };

    let mut scripts = Vec::new();
    if let Some(s) = obj.get("scripts").and_then(|v| v.as_object()) {
        for (k, v) in s {
            if let Some(cmd) = v.as_str() {
                scripts.push((k.clone(), cmd.to_string()));
            }
        }
    }

    let mut deps = Vec::new();
    for (field, label) in [
        ("dependencies", "dependency"),
        ("devDependencies", "devDependency"),
        ("optionalDependencies", "optionalDependency"),
        ("peerDependencies", "peerDependency"),
    ] {
        if let Some(dep_obj) = obj.get(field).and_then(|v| v.as_object()) {
            for (k, v) in dep_obj {
                deps.push((k.clone(), v.as_str().unwrap_or("").to_string(), label));
            }
        }
    }

    Some(vp_complete::PkgManifest { name, workspaces, scripts, deps })
}

const COMPLETION_CONFIG: VpCompletionConfig = VpCompletionConfig {
    global_bin: global_bin_dir,
    node_store: node_store_dir,
    resolve_node: resolve_offline_node,
    resolve_bin_js: resolve_offline_bin_js,
    read_manifest: read_pkg_manifest,
};

#[cold]
pub(crate) fn build_vp_cmd_spec() -> CmdSpec {
    let mut cmd = cli::raw_cli_command();
    vp_complete::build_vp_spec(&mut cmd, &COMPLETION_CONFIG)
}

#[cold]
pub(crate) fn build_vpx_cmd_spec() -> CmdSpec {
    let mut cmd = cli::raw_cli_command();
    vp_complete::build_vpx_spec(&mut cmd, &COMPLETION_CONFIG)
}

#[cold]
pub(crate) fn resolve_offline_node() -> Option<PathBuf> {
    let config = vp_shared::EnvConfig::get();
    let node_store = config.dirs.data.join("js_runtime/node");
    let fallback_bin = config.dirs.fallback_bin();
    vp_complete::resolve_offline_node_in(
        node_store.as_path(),
        &[config.dirs.bin.as_path(), fallback_bin.as_path()],
    )
}

#[cold]
pub(crate) fn resolve_offline_bin_js(dir: &Path) -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os("VP_COMPLETE_TEST_BIN_JS") {
        let p = PathBuf::from(custom);
        if p.is_file() {
            return Some(p);
        }
    }

    let abs_dir = vt_path::AbsolutePathBuf::new(dir.to_path_buf())?;
    if let Some(local_entry) = crate::js_executor::JsExecutor::resolve_local_vite_plus(&abs_dir) {
        return Some(local_entry.as_path().to_path_buf());
    }

    crate::js_executor::JsExecutor::new(None)
        .get_scripts_dir()
        .ok()
        .map(|d| d.join("bin.js"))
        .filter(|p| p.as_path().is_file())
        .map(|p| p.as_path().to_path_buf())
}

#[cfg(test)]
pub(crate) fn get_task_candidates(dir: &Path, prefix: &str) -> Vec<vp_complete::CompletionItem> {
    vp_complete::get_task_candidates_with(
        dir,
        prefix,
        resolve_offline_node,
        resolve_offline_bin_js,
        read_pkg_manifest,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, Instant},
    };

    use tempfile::TempDir;
    use vp_complete::{
        CompletionContext, CompletionRequest, parse_task_completion_lines,
        sanitize_task_current_prefix, task_completer_with,
    };

    use super::*;

    #[test]
    fn test_bimodal_subcommand_alias_matching() {
        let spec = build_vp_cmd_spec();
        let cwd = PathBuf::from("/workspace");

        let req_empty = CompletionRequest::new(vec!["vp".into(), "".into()], 1, cwd.clone());
        let out_empty = vp_complete::complete(&spec, None, &req_empty);
        assert!(out_empty.items.iter().any(|it| it.value == "install"));
        assert!(out_empty.items.iter().any(|it| it.value == "remove"));
        assert!(out_empty.items.iter().any(|it| it.value == "list"));
        assert!(!out_empty.items.iter().any(|it| it.value == "i"));
        assert!(!out_empty.items.iter().any(|it| it.value == "un"));
        assert!(!out_empty.items.iter().any(|it| it.value == "rm"));

        let req_un = CompletionRequest::new(vec!["vp".into(), "un".into()], 1, cwd);
        let out_un = vp_complete::complete(&spec, None, &req_un);
        let values: Vec<&str> = out_un.items.iter().map(|it| it.value.as_str()).collect();
        assert!(values.contains(&"un"), "Must contain un: {values:?}");
        assert!(values.contains(&"uninstall"), "Must contain uninstall: {values:?}");
        assert!(values.contains(&"unlink"), "Must contain unlink: {values:?}");
    }

    #[test]
    fn test_task_provider_positional_guard() {
        let completer =
            task_completer_with(resolve_offline_node, resolve_offline_bin_js, read_pkg_manifest);
        let ctx_pos1 = CompletionContext {
            cwd: PathBuf::from("/workspace"),
            prefix: String::new(),
            words: vec!["vp".into(), "run".into(), "build".into(), "".into()],
            current_index: 3,
            positional_index: 1,
            is_local_bin: false,
        };
        let out = completer(&ctx_pos1);
        assert!(out.items.is_empty(), "Must return empty items for positional_index > 0");
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

    fn write_exec_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
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
            parse_task_completion_lines(stdout, cur)
                .into_iter()
                .map(|c| c.value)
                .collect::<Vec<_>>()
        };

        assert_eq!(complete(""), vec!["build", "test:unit", "pkg#test:unit", "empty_desc"]);
        assert_eq!(complete("test:"), vec!["test:unit"]);
        assert!(complete("nonexistent").is_empty());

        let all = parse_task_completion_lines(stdout, "");
        assert_eq!(
            all[0].description.as_deref(),
            Some("Run project build: vite build --mode:prod")
        );
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

        #[cfg(unix)]
        {
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
    }

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

        #[cfg(unix)]
        {
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
    }
}

#[cfg(test)]
mod spec_tests;
