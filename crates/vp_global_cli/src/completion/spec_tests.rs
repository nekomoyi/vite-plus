use std::{fs, path::PathBuf};

use tempfile::TempDir;
use vp_complete::{CompletionDirective, CompletionRequest};

use super::*;

fn complete_values(words: &[&str], cwd: &Path) -> Vec<String> {
    let req = CompletionRequest::new(
        words.iter().map(|s| (*s).to_string()).collect(),
        words.len() - 1,
        cwd.to_path_buf(),
    );
    complete_entry(&req).items.into_iter().map(|it| it.value).collect()
}

fn assert_has_all(actual: &[String], expected: &[&str], context: &str) {
    for item in expected {
        assert!(actual.iter().any(|v| v == item), "{context} missing {item}: {actual:?}");
    }
}

#[test]
fn test_curated_subcommand_flags() {
    let cwd = PathBuf::from("/workspace");

    let check_flags = complete_values(&["vp", "check", "--"], &cwd);
    assert_has_all(
        &check_flags,
        &["--fix", "--continue-on-error", "--deny-warnings", "--no-fmt", "--no-lint", "--quiet"],
        "check flags",
    );

    let build_flags = complete_values(&["vp", "build", "--"], &cwd);
    assert_has_all(
        &build_flags,
        &["--outDir", "--assetsDir", "--target", "--minify", "--watch"],
        "build flags",
    );

    let test_flags = complete_values(&["vp", "test", "--"], &cwd);
    assert_has_all(&test_flags, &["--coverage", "--isolate", "--reporter", "--bail"], "test flags");

    assert_has_all(
        &complete_values(&["vp", "test", "src/", "--cov"], &cwd),
        &["--coverage"],
        "vitest options after a filter",
    );
    let vpx_flags = complete_values(&["vpx", "--"], &cwd);
    assert_has_all(
        &vpx_flags,
        &[
            "--package",
            "--shell-mode",
            "--silent",
            "--tsconfig",
            "--watch",
            "--conditions",
            "--eval",
            "--require",
            "--import",
            "--env-file",
        ],
        "vpx flags",
    );
}

#[test]
fn test_vpx_no_chdir_and_target_completion() {
    let temp_dir = TempDir::new().unwrap();
    let cwd = temp_dir.path();

    fs::write(cwd.join("script.ts"), "console.log('hi');").unwrap();
    fs::create_dir_all(cwd.join("node_modules/.bin")).unwrap();
    fs::write(cwd.join("node_modules/.bin/my-bin"), "").unwrap();

    // 1. -C custom must not chdir to custom; completing ./scr should find script.ts in cwd
    let vals = complete_values(&["vpx", "-C", "custom", "./scr"], cwd);
    assert_has_all(&vals, &["./script.ts"], "vpx target completion with preceding -C");

    // 2. Bare word target completion should merge binaries from node_modules/.bin and script files
    let bare_vals = complete_values(&["vpx", "my"], cwd);
    assert_has_all(&bare_vals, &["my-bin"], "vpx bare target completion");
}

#[test]
fn test_pure_rust_dynamic_providers() {
    let temp_home = TempDir::new().unwrap();
    let temp_project = TempDir::new().unwrap();
    let proj_dir = temp_project.path();

    fs::write(
        proj_dir.join("package.json"),
        r#"{
            "name": "test-pkg",
            "dependencies": { "react": "^18.2.0" },
            "devDependencies": { "typescript": "^5.0.0" }
        }"#,
    )
    .unwrap();
    fs::create_dir_all(proj_dir.join("node_modules/.bin")).unwrap();
    fs::write(proj_dir.join("node_modules/.bin/eslint"), "").unwrap();

    let node_store = temp_home.path().join("js_runtime/node");
    fs::create_dir_all(node_store.join("v22.11.0")).unwrap();
    fs::create_dir_all(node_store.join("v20.18.0")).unwrap();

    let home_str = temp_home.path().to_str().unwrap();
    temp_env::with_vars(
        [
            ("HOME", Some(home_str)),
            ("USERPROFILE", Some(home_str)),
            ("ZDOTDIR", Some(home_str)),
            ("VP_HOME", Some(home_str)),
        ],
        || {
            assert_has_all(
                &complete_values(&["vp", "remove", ""], proj_dir),
                &["react", "typescript"],
                "vp remove",
            );
            assert_has_all(&complete_values(&["vp", "exec", ""], proj_dir), &["eslint"], "vp exec");
            assert_has_all(
                &complete_values(&["vp", "env", "use", ""], proj_dir),
                &["v22.11.0", "v20.18.0"],
                "vp env use",
            );
            for name in ["uninstall", "pin", "use", "default"] {
                assert_has_all(
                    &complete_values(&["vp", "env", name, "v20.18.0", "v22"], proj_dir),
                    &["v22.11.0"],
                    name,
                );
            }
            assert_has_all(
                &complete_values(&["vp", "env", "on", ""], proj_dir),
                &["node", "pm"],
                "vp env on",
            );
        },
    );

    let temp_ws = TempDir::new().unwrap();
    let ws_dir = temp_ws.path();
    fs::write(ws_dir.join("package.json"), r#"{"name": "root", "private": true}"#).unwrap();
    fs::write(ws_dir.join("pnpm-workspace.yaml"), "packages:\n  - 'packages/*'\n").unwrap();
    fs::create_dir_all(ws_dir.join("packages/pkg-a")).unwrap();
    fs::write(ws_dir.join("packages/pkg-a/package.json"), r#"{"name": "@scope/pkg-a"}"#).unwrap();
    fs::create_dir_all(ws_dir.join("packages/pkg-b")).unwrap();
    fs::write(ws_dir.join("packages/pkg-b/package.json"), r#"{"name": "pkg-b"}"#).unwrap();

    assert_has_all(
        &complete_values(&["vp", "run", "--filter", ""], ws_dir),
        &["@scope/pkg-a", "pkg-b"],
        "vp run --filter",
    );
}

#[test]
fn test_cli_definitions_drift() {
    let spec = catalog::build_vp_spec(false);

    let mut raw_root_spec = vp_complete::CmdSpec::from_clap(crate::cli::raw_cli_command());

    // 1. Root flags must match `cli::Args` 1-to-1 (no unsupported root --filter)
    let chdir = raw_root_spec.flags.iter_mut().find(|flag| flag.short == Some('C')).unwrap();
    chdir.chdir = true;
    chdir.value_hint = vp_complete::ValueHint::DirPath;
    assert_eq!(spec.flags, raw_root_spec.flags);

    // 2. Verify all 16 core subcommands (+ cache, hooks, toolchain) are present
    let expected_subcommands = [
        "check",
        "lint",
        "fmt",
        "dev",
        "build",
        "test",
        "preview",
        "pack",
        "doc",
        "create",
        "migrate",
        "config",
        "staged",
        "run",
        "exec",
        "dlx",
        "cache",
        "hooks",
        "toolchain",
    ];

    for name in expected_subcommands {
        assert!(
            spec.subcommands.iter().any(|s| s.name == name),
            "Subcommand {name} missing from build_vp_cmd_spec()"
        );
    }

    // 3. Verify every command in both `cli::raw_cli_command()` and `generated_local_specs`
    //    is present in `build_vp_cmd_spec()`, and non-passthrough Clap commands match 1-to-1.
    for raw_sub in &raw_root_spec.subcommands {
        assert!(
            spec.subcommands.iter().any(|s| s.name == raw_sub.name),
            "Global Clap subcommand {} missing from build_vp_cmd_spec()",
            raw_sub.name
        );
    }

    let passthrough_names = ["build", "dev", "doc", "fmt", "lint", "pack", "preview", "test"];
    for mut gen_sub in catalog::bundled_local_specs() {
        let actual_sub =
            spec.subcommands.iter().find(|s| s.name == gen_sub.name).unwrap_or_else(|| {
                panic!("Generated subcommand {} missing from vp spec", gen_sub.name)
            });

        if passthrough_names.contains(&gen_sub.name.as_str()) {
            assert_eq!(actual_sub.visible_aliases, gen_sub.visible_aliases);
            assert_eq!(actual_sub.hidden, gen_sub.hidden);
        } else {
            // The CLI accepts additional values for these suggestions-only hints.
            if let Some(long) = match gen_sub.name.as_str() {
                "create" => Some("package-manager"),
                "staged" => Some("concurrent"),
                _ => None,
            } {
                let generated =
                    gen_sub.flags.iter_mut().find(|f| f.long.as_deref() == Some(long)).unwrap();
                generated.value_hint = actual_sub
                    .flags
                    .iter()
                    .find(|f| f.long.as_deref() == Some(long))
                    .unwrap()
                    .value_hint
                    .clone();
            }
            assert_eq!(actual_sub, &gen_sub);
        }
    }

    let ws = TempDir::new().unwrap();
    fs::write(
        ws.path().join("package.json"),
        r#"{"name":"completion-workspace","workspaces":["packages/*"]}"#,
    )
    .unwrap();
    fs::create_dir_all(ws.path().join("packages/member")).unwrap();
    fs::write(ws.path().join("packages/member/package.json"), r#"{"name":"completion-member"}"#)
        .unwrap();
    for name in ["run", "exec", "pack", "dev", "build", "preview"] {
        let values = complete_values(&["vp", name, "--filter", "completion-m"], ws.path());
        assert_eq!(
            values.iter().any(|v| v == "completion-member"),
            matches!(name, "run" | "exec" | "pack"),
            "{name}"
        );
    }

    // 5. Verify vpx flags (inherited from dlx + script/watch/Node flags)
    let vpx_cmd = catalog::build_vpx_spec();
    let vpx_pkg = vpx_cmd.flags.iter().find(|f| f.long.as_deref() == Some("package")).unwrap();
    assert!(vpx_pkg.multiple, "vpx -p/--package inherited from dlx must be repeatable");
    for flag in [
        "package",
        "shell-mode",
        "silent",
        "tsconfig",
        "watch",
        "include",
        "exclude",
        "ignore",
        "kill-signal",
        "kill-timeout",
        "conditions",
        "eval",
        "require",
        "import",
        "env-file",
        "inspect",
        "inspect-brk",
        "version",
        "help",
    ] {
        assert!(
            vpx_cmd.flags.iter().any(|f| f.long.as_deref() == Some(flag)),
            "vpx missing flag --{flag}"
        );
    }
}

#[test]
fn test_vpx_target_completer_directory_directives() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    fs::create_dir_all(dir.join("scripts")).unwrap();
    fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
    fs::write(dir.join("node_modules/.bin/semver"), "").unwrap();

    // 1. Single directory match (`vpx scr`) must return BOTH NO_SPACE and NO_FILE_COMP
    let req_single = CompletionRequest::new(vec!["vpx".into(), "scr".into()], 1, dir.to_path_buf());
    let out_single = complete_entry(&req_single);
    assert_eq!(out_single.items.len(), 1);
    assert_eq!(out_single.items[0].value, "scripts/");
    assert!(
        out_single.directive.contains(CompletionDirective::NO_SPACE),
        "Single dir match must have NO_SPACE"
    );
    assert!(
        out_single.directive.contains(CompletionDirective::NO_FILE_COMP),
        "Single dir match must preserve NO_FILE_COMP"
    );

    // 2. Mixed bin + directory match (`vpx s` -> `semver` + `scripts/`) must ALSO set NO_SPACE | NO_FILE_COMP
    let req_mixed = CompletionRequest::new(vec!["vpx".into(), "s".into()], 1, dir.to_path_buf());
    let out_mixed = complete_entry(&req_mixed);
    let vals: Vec<&str> = out_mixed.items.iter().map(|i| i.value.as_str()).collect();
    assert!(vals.contains(&"semver") && vals.contains(&"scripts/"), "got: {vals:?}");
    assert!(
        out_mixed.directive.contains(CompletionDirective::NO_SPACE),
        "Mixed bin + dir match must include NO_SPACE so selecting 'scripts/' drills cleanly"
    );
    assert!(
        out_mixed.directive.contains(CompletionDirective::NO_FILE_COMP),
        "Mixed bin + dir match must include NO_FILE_COMP"
    );
}

#[test]
fn test_chdir_rules_regex_vite_config_and_workspace_discovery() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::write(root.join("package.json"), r#"{"name":"root-ws","scripts":{"root-task":"echo 1"}}"#)
        .unwrap();
    fs::write(
        root.join("pnpm-workspace.yaml"),
        "packages:\n  - 'packages/*'\n  - '!packages/ignored'\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("packages/sub")).unwrap();
    fs::write(
        root.join("packages/sub/package.json"),
        r#"{"name":"@ws/sub","scripts":{"sub-task":"echo 2"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("packages/sub/vite.config.ts"),
        r#"
import { defineConfig } from 'vite-plus';
const routePattern = /^\/api\/v[0-9]{1,2}\/items["']?$/g;
export default defineConfig({
  run: {
    tasks: {
      "regex:safe": { command: "echo ok", description: "After regex literal" },
    },
  },
});
"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("packages/ignored")).unwrap();
    fs::write(root.join("packages/ignored/package.json"), r#"{"name":"@ws/ignored"}"#).unwrap();

    // 1. Root `vp --` must NOT fabricate unsupported `--dir`
    let root_long_flags = complete_values(&["vp", "--"], root);
    assert!(
        !root_long_flags.iter().any(|f| f == "--dir"),
        "vp must not advertise nonexistent --dir flag: {root_long_flags:?}"
    );
    let root_short_flags = complete_values(&["vp", "-"], root);
    assert!(root_short_flags.iter().any(|f| f == "-C"), "vp must advertise -C");

    // 2. Pure-Rust task extraction + real `vp -C`, `vpr -C`, `vp run -C`, and `vpr -r -C` rules
    temp_env::with_var("VP_COMPLETE_OFFLINE_ONLY_RUST", Some("1"), || {
        let vp_c = complete_values(&["vp", "-C", "packages/sub", "run", ""], root);
        assert_has_all(&vp_c, &["sub-task", "regex:safe"], "vp -C packages/sub run");

        let vpr_c = complete_values(&["vpr", "-C", "packages/sub", ""], root);
        assert_has_all(&vpr_c, &["sub-task", "regex:safe"], "vpr -C packages/sub");

        // `vp run` does NOT accept `-C` after `run`: must stay in `root`, not chdir to `packages/sub`
        let vp_run_c = complete_values(&["vp", "run", "-C", "packages/sub", ""], root);
        assert!(
            !vp_run_c.iter().any(|t| t == "sub-task"),
            "vp run -C must not chdir after subcommand: {vp_run_c:?}"
        );

        // `vpr -r -C` has `-C` after position 1: must NOT chdir
        let vpr_late_c = complete_values(&["vpr", "-r", "-C", "packages/sub", ""], root);
        assert!(
            !vpr_late_c.iter().any(|t| t == "sub-task"),
            "vpr -r -C must not chdir past leading token: {vpr_late_c:?}"
        );
    });

    // 3. Workspace discovery respects `!packages/ignored` negation
    let ws_names = providers::workspace_package_names(root);
    assert!(ws_names.iter().any(|n| n == "@ws/sub"), "Must include @ws/sub: {ws_names:?}");
    assert!(
        !ws_names.iter().any(|n| n == "@ws/ignored"),
        "Must exclude !packages/ignored: {ws_names:?}"
    );

    // 4. npm/bun object-form workspaces with UTF-8 BOM
    let npm_ws = TempDir::new().unwrap();
    let mut bom_json = b"\xEF\xBB\xBF".to_vec();
    bom_json.extend_from_slice(br#"{"name":"npm-root","workspaces":{"packages":["apps/*"]}}"#);
    fs::write(npm_ws.path().join("package.json"), bom_json).unwrap();
    fs::create_dir_all(npm_ws.path().join("apps/web")).unwrap();
    fs::write(npm_ws.path().join("apps/web/package.json"), r#"{"name":"@app/web"}"#).unwrap();
    let npm_names = providers::workspace_package_names(npm_ws.path());
    assert!(
        npm_names.contains(&"npm-root".to_string()) && npm_names.contains(&"@app/web".to_string())
    );
}

#[test]
fn test_local_help_filtering_and_offline_node_semver() {
    let cwd = PathBuf::from("/workspace");
    let local_help = complete_values(&["./node_modules/.bin/vp", "help", ""], &cwd);
    assert!(local_help.contains(&"build".to_string()), "local help missing build: {local_help:?}");
    for global_cmd in ["env", "upgrade", "implode", "node", "list", "rebuild"] {
        assert!(
            !local_help.iter().any(|c| c == global_cmd),
            "local node_modules/.bin/vp help must hide {global_cmd}: {local_help:?}"
        );
    }

    let temp_node = TempDir::new().unwrap();
    let rel_bin = if cfg!(windows) { "node.exe" } else { "bin/node" };
    for ver in ["v9.9.0", "v22.14.0", "v20.18.0", "v22.14.0-rc.9", "v22.14.0-rc.10"] {
        let bin = temp_node.path().join("js_runtime/node").join(ver).join(rel_bin);
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, "").unwrap();
    }
    let resolved =
        vp_shared::EnvConfig::with_vars([("VP_HOME", temp_node.path().as_os_str())], |_| {
            assert_eq!(
                complete_values(&["vp", "env", "use", "v22"], &cwd),
                ["v22.14.0", "v22.14.0-rc.10", "v22.14.0-rc.9"]
            );
            resolve_offline_node()
        })
        .expect("must resolve node binary");
    assert!(
        resolved.to_string_lossy().contains("v22.14.0"),
        "expected v22.14.0 over v9.9.0, got: {}",
        resolved.display()
    );
}
#[test]
fn test_engine_optional_value_and_trailing_var_arg_and_lexer_fixes() {
    let temp = tempfile::TempDir::new().unwrap();
    let ws = temp.path();
    fs::create_dir_all(ws.join("packages/sub")).unwrap();
    fs::write(
        ws.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"],"scripts":{"build":"vite build"}}"#,
    )
    .unwrap();
    fs::write(ws.join("packages/sub/package.json"), r#"{"name":"@demo/sub"}"#).unwrap();

    // 1. Optional flag value (`vp staged --concurrent <Tab>`, `vp staged --concurrent --al<Tab>`, `vp staged -p f<Tab>`)
    let out_opt = complete_entry(&CompletionRequest::new(
        vec!["vp".into(), "staged".into(), "--concurrent".into(), "".into()],
        3,
        ws.to_path_buf(),
    ));
    let opt_vals: Vec<&str> = out_opt.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(opt_vals, vec!["true", "false"]);

    let out_opt_next_flag = complete_entry(&CompletionRequest::new(
        vec!["vp".into(), "staged".into(), "--concurrent".into(), "--al".into()],
        3,
        ws.to_path_buf(),
    ));
    assert!(out_opt_next_flag.items.iter().any(|i| i.value == "--allow-empty"));

    let out_short_opt = complete_entry(&CompletionRequest::new(
        vec!["vp".into(), "staged".into(), "-p".into(), "f".into()],
        3,
        ws.to_path_buf(),
    ));
    let short_opt_vals: Vec<&str> = out_short_opt.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(short_opt_vals, vec!["false"]);

    // 2. Trailing vararg stops wrapper flag parsing (`vp exec node --fi`, `vp exec node --filter `, `vp run build --fi`, while `vp lint src/ --fi` still completes)
    let out_exec_child = complete_entry(&CompletionRequest::new(
        vec!["vp".into(), "exec".into(), "node".into(), "--fi".into()],
        3,
        ws.to_path_buf(),
    ));
    assert!(
        !out_exec_child.items.iter().any(|i| i.value == "--filter"),
        "vp exec node --fi must not complete wrapper --filter: {:?}",
        out_exec_child.items
    );

    let out_exec_child_val = complete_entry(&CompletionRequest::new(
        vec!["vp".into(), "exec".into(), "node".into(), "--filter".into(), "".into()],
        4,
        ws.to_path_buf(),
    ));
    assert!(
        !out_exec_child_val.items.iter().any(|i| i.value == "@demo/sub"),
        "vp exec node --filter <Tab> must not suggest workspace packages: {:?}",
        out_exec_child_val.items
    );

    let out_run_child = complete_entry(&CompletionRequest::new(
        vec!["vp".into(), "run".into(), "build".into(), "--fi".into()],
        3,
        ws.to_path_buf(),
    ));
    assert!(!out_run_child.items.iter().any(|i| i.value == "--filter"));

    let out_lint_flag = complete_entry(&CompletionRequest::new(
        vec!["vp".into(), "lint".into(), "src/".into(), "--fi".into()],
        3,
        ws.to_path_buf(),
    ));
    assert!(out_lint_flag.items.iter().any(|i| i.value == "--fix"));

    // 3. Lexer: escaped newlines and redirections
    let lex_cont = vp_complete::lexer::parse_bash_line("vp \\\n  run \\\n  bu", 17);
    assert_eq!(lex_cont.words, vec!["vp", "run", "bu"]);

    let lex_dq_cont = vp_complete::lexer::parse_bash_line("vp \"foo\\\nbar\"", 13);
    assert_eq!(lex_dq_cont.words, vec!["vp", "foobar"]);

    let lex_redir1 = vp_complete::lexer::parse_bash_line("vp 2>/dev/null bu", 17);
    assert_eq!(lex_redir1.words, vec!["vp", "bu"]);

    let lex_redir2 = vp_complete::lexer::parse_bash_line("vp 2>&1 bu", 10);
    assert_eq!(lex_redir2.words, vec!["vp", "bu"]);

    // 4. Path: ~// resolves under home instead of root /
    let home = ws.join("fake_home");
    fs::create_dir_all(home.join("proj")).unwrap();
    let tilde_slash = vp_complete::path::complete_path_with_home(
        ws,
        Some(&home),
        "~//pr",
        vp_complete::PathMode::DirsOnly,
    );
    assert_eq!(tilde_slash.items[0].value, "~//proj/");
}
#[test]
fn test_workspace_yaml_globs_and_ancestor_fallback() {
    let temp = tempfile::TempDir::new().unwrap();
    let ws = temp.path();

    // Unindented pnpm-workspace.yaml list + {apps,packages}/* + [ab] char class + negation
    fs::write(
        ws.join("pnpm-workspace.yaml"),
        "packages:\n- '{apps,packages}/*'\n- 'tools/t[12]'\n- '!packages/ignored'\n",
    )
    .unwrap();
    fs::create_dir_all(ws.join("apps/web/src/nested")).unwrap();
    fs::create_dir_all(ws.join("packages/core")).unwrap();
    fs::create_dir_all(ws.join("packages/ignored")).unwrap();
    fs::create_dir_all(ws.join("tools/t1")).unwrap();
    fs::create_dir_all(ws.join("tools/t9")).unwrap();

    fs::write(
        ws.join("apps/web/package.json"),
        r#"{"name":"@demo/web","dependencies":{"react":"^19.0.0"},"scripts":{"web:dev":"vite"}}"#,
    )
    .unwrap();
    fs::write(ws.join("packages/core/package.json"), r#"{"name":"@demo/core"}"#).unwrap();
    fs::write(ws.join("packages/ignored/package.json"), r#"{"name":"@demo/ignored"}"#).unwrap();
    fs::write(ws.join("tools/t1/package.json"), r#"{"name":"@demo/t1"}"#).unwrap();
    fs::write(ws.join("tools/t9/package.json"), r#"{"name":"@demo/t9"}"#).unwrap();

    let pkgs = providers::workspace_package_names(ws);
    assert_eq!(pkgs, vec!["@demo/core", "@demo/t1", "@demo/web"]);

    // Subdirectory `vp remove` and pure-Rust task fallback walk ancestors
    let nested_dir = ws.join("apps/web/src/nested");
    let tasks = tasks::extract_static_tasks(&nested_dir, "web:");
    assert_eq!(tasks.iter().map(|t| t.value.as_str()).collect::<Vec<_>>(), vec!["web:dev"]);

    let remove_out = complete_entry(&CompletionRequest::new(
        vec!["vp".into(), "remove".into(), "re".into()],
        2,
        nested_dir,
    ));
    assert!(remove_out.items.iter().any(|i| i.value == "react"));
}

#[test]
fn shorthand_accepts_only_a_leading_directory_option() {
    let ws = TempDir::new().unwrap();
    fs::create_dir(ws.path().join("-tmp")).unwrap();
    for bin in ["vpr", "vpr.cmd", "VPR.EXE"] {
        for (word, expected) in [("-C-t", "-C-tmp/"), ("-C=-t", "-C=-tmp/")] {
            assert_has_all(&complete_values(&[bin, word], ws.path()), &[expected], bin);
        }
        assert_has_all(&complete_values(&[bin, "-"], ws.path()), &["-C"], bin);
        assert!(!complete_values(&[bin, "-r", "-"], ws.path()).iter().any(|v| v == "-C"));
        assert!(!complete_values(&["vp", "run", "-"], ws.path()).iter().any(|v| v == "-C"));
    }
    for bin in ["vpx.cmd", "VPX.BAT", r"C:\bin\vpx.PS1", "vpx.EXE"] {
        assert_eq!(entry::classify_vp_binary(bin), Some(entry::BinKind::Vpx));
    }
}

#[test]
fn test_bimodal_subcommand_alias_matching() {
    let cwd = PathBuf::from("/workspace");

    let req_empty = CompletionRequest::new(vec!["vp".into(), "".into()], 1, cwd.clone());
    let out_empty = complete_entry(&req_empty);
    assert!(out_empty.items.iter().any(|it| it.value == "install"));
    assert!(out_empty.items.iter().any(|it| it.value == "remove"));
    assert!(out_empty.items.iter().any(|it| it.value == "list"));
    assert!(!out_empty.items.iter().any(|it| it.value == "i"));
    assert!(!out_empty.items.iter().any(|it| it.value == "un"));
    assert!(!out_empty.items.iter().any(|it| it.value == "rm"));

    let req_un = CompletionRequest::new(vec!["vp".into(), "un".into()], 1, cwd);
    let out_un = complete_entry(&req_un);
    let values: Vec<&str> = out_un.items.iter().map(|it| it.value.as_str()).collect();
    assert!(values.contains(&"un"), "Must contain un: {values:?}");
    assert!(values.contains(&"uninstall"), "Must contain uninstall: {values:?}");
    assert!(values.contains(&"unlink"), "Must contain unlink: {values:?}");
}
