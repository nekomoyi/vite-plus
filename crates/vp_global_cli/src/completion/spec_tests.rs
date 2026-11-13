use std::{fs, path::PathBuf};

use tempfile::TempDir;
use vp_complete::{CompletionDirective, CompletionRequest};

use super::*;

fn complete_values(
    spec: &CmdSpec,
    vpx: Option<&CmdSpec>,
    words: &[&str],
    cwd: &Path,
) -> Vec<String> {
    let req = CompletionRequest::new(
        words.iter().map(|s| (*s).to_string()).collect(),
        words.len() - 1,
        cwd.to_path_buf(),
    );
    vp_complete::complete(spec, vpx, &req).items.into_iter().map(|it| it.value).collect()
}

fn assert_has_all(actual: &[String], expected: &[&str], context: &str) {
    for item in expected {
        assert!(actual.iter().any(|v| v == item), "{context} missing {item}: {actual:?}");
    }
}

#[test]
fn test_curated_subcommand_flags() {
    let spec = build_vp_cmd_spec();
    let vpx_spec = build_vpx_cmd_spec();
    let cwd = PathBuf::from("/workspace");

    let check_flags = complete_values(&spec, None, &["vp", "check", "--"], &cwd);
    assert_has_all(
        &check_flags,
        &["--fix", "--continue-on-error", "--deny-warnings", "--no-fmt", "--no-lint", "--quiet"],
        "check flags",
    );

    let build_flags = complete_values(&spec, None, &["vp", "build", "--"], &cwd);
    assert_has_all(
        &build_flags,
        &["--outDir", "--assetsDir", "--target", "--minify", "--watch"],
        "build flags",
    );

    let test_flags = complete_values(&spec, None, &["vp", "test", "--"], &cwd);
    assert_has_all(&test_flags, &["--coverage", "--isolate", "--reporter", "--bail"], "test flags");

    let vpx_flags = complete_values(&spec, Some(&vpx_spec), &["vpx", "--"], &cwd);
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

    let spec = build_vp_cmd_spec();
    let vpx_spec = build_vpx_cmd_spec();

    // 1. -C custom must not chdir to custom; completing ./scr should find script.ts in cwd
    let vals = complete_values(&spec, Some(&vpx_spec), &["vpx", "-C", "custom", "./scr"], cwd);
    assert_has_all(&vals, &["./script.ts"], "vpx target completion with preceding -C");

    // 2. Bare word target completion should merge binaries from node_modules/.bin and script files
    let bare_vals = complete_values(&spec, Some(&vpx_spec), &["vpx", "my"], cwd);
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
            let spec = build_vp_cmd_spec();
            assert_has_all(
                &complete_values(&spec, None, &["vp", "remove", ""], proj_dir),
                &["react", "typescript"],
                "vp remove",
            );
            assert_has_all(
                &complete_values(&spec, None, &["vp", "exec", ""], proj_dir),
                &["eslint"],
                "vp exec",
            );
            assert_has_all(
                &complete_values(&spec, None, &["vp", "env", "use", ""], proj_dir),
                &["v22.11.0", "v20.18.0"],
                "vp env use",
            );
            assert_has_all(
                &complete_values(&spec, None, &["vp", "env", "on", ""], proj_dir),
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

    let spec = build_vp_cmd_spec();
    assert_has_all(
        &complete_values(&spec, None, &["vp", "run", "--filter", ""], ws_dir),
        &["@scope/pkg-a", "pkg-b"],
        "vp run --filter",
    );
}

fn extract_cmd_spec_surface(cmd: &vp_complete::CmdSpec) -> serde_json::Value {
    let mut visible_aliases = cmd.visible_aliases.clone();
    visible_aliases.sort();

    let mut flags: Vec<serde_json::Value> = cmd
        .flags
        .iter()
        .map(|f| {
            let mut aliases = f.aliases.clone();
            aliases.sort();
            let mut short_aliases: Vec<String> =
                f.short_aliases.iter().map(|c| c.to_string()).collect();
            short_aliases.sort();
            serde_json::json!({
                "long": f.long,
                "short": f.short.map(|c| c.to_string()),
                "aliases": aliases,
                "short_aliases": short_aliases,
                "takes_value": f.takes_value,
                "optional_value": f.optional_value,
                "multiple": f.multiple,
                "hidden": f.hidden,
            })
        })
        .collect();
    flags.sort_by_key(|a| a.to_string());

    let has_last = cmd.positionals.iter().any(|p| p.last);
    let non_last: Vec<_> = cmd.positionals.iter().filter(|p| !p.last).collect();
    let max_non_last: Option<usize> =
        if non_last.iter().any(|p| p.multiple) { None } else { Some(non_last.len()) };

    let mut subcommands: Vec<serde_json::Value> =
        cmd.subcommands.iter().filter(|s| s.name != "help").map(extract_cmd_spec_surface).collect();
    subcommands.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));

    serde_json::json!({
        "name": cmd.name,
        "visible_aliases": visible_aliases,
        "hidden": cmd.hidden,
        "flags": flags,
        "has_last_positional": has_last,
        "max_non_last_positionals": max_non_last,
        "subcommands": subcommands,
    })
}

#[test]
fn test_cli_definitions_drift() {
    let spec = build_vp_cmd_spec();
    let raw_root_spec = vp_complete::CmdSpec::from_clap(&crate::cli::raw_cli_command());

    // 1. Root flags must match `cli::Args` 1-to-1 (no unsupported root --filter)
    assert_eq!(
        extract_cmd_spec_surface(&spec)["flags"],
        extract_cmd_spec_surface(&raw_root_spec)["flags"],
        "Root `vp` flags drifted from `cli::Args`!"
    );

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
    for gen_sub in vp_complete::bundled_local_specs() {
        let actual_sub =
            spec.subcommands.iter().find(|s| s.name == gen_sub.name).unwrap_or_else(|| {
                panic!("Generated subcommand {} missing from vp spec", gen_sub.name)
            });

        if passthrough_names.contains(&gen_sub.name.as_str()) {
            assert_eq!(actual_sub.visible_aliases, gen_sub.visible_aliases);
            assert_eq!(actual_sub.hidden, gen_sub.hidden);
        } else {
            assert_eq!(
                extract_cmd_spec_surface(actual_sub),
                extract_cmd_spec_surface(&gen_sub),
                "Completion surface for `{}` drifted from local_specs.tsv!",
                gen_sub.name
            );
        }
    }

    // 4. Verify dynamic completer wiring and Vite `-f, --filter` disambiguation
    for ws_cmd in ["run", "exec", "pack"] {
        let sub = spec.subcommands.iter().find(|s| s.name == ws_cmd).unwrap();
        let filter_flag = sub.flags.iter().find(|f| f.long.as_deref() == Some("filter")).unwrap();
        assert!(filter_flag.multiple, "{ws_cmd} --filter must be repeatable");
        assert!(
            filter_flag.completer.is_some(),
            "{ws_cmd} -F/--filter must attach workspace_pkg_completer"
        );
    }
    for vite_cmd in ["dev", "build", "preview"] {
        let sub = spec.subcommands.iter().find(|s| s.name == vite_cmd).unwrap();
        let filter_flag = sub.flags.iter().find(|f| f.long.as_deref() == Some("filter")).unwrap();
        assert!(
            filter_flag.completer.is_none(),
            "{vite_cmd} -f/--filter is Vite debug log filter, not workspace package filter"
        );
    }

    // 5. Verify vpx flags (inherited from dlx + script/watch/Node flags)
    let vpx_cmd = build_vpx_cmd_spec();
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

    let vpx_spec = build_vpx_cmd_spec();

    // 1. Single directory match (`vpx scr`) must return BOTH NO_SPACE and NO_FILE_COMP
    let req_single = CompletionRequest::new(vec!["vpx".into(), "scr".into()], 1, dir.to_path_buf());
    let out_single = vp_complete::complete(&vpx_spec, Some(&vpx_spec), &req_single);
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
    let out_mixed = vp_complete::complete(&vpx_spec, Some(&vpx_spec), &req_mixed);
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

    let spec = build_vp_cmd_spec();
    let vpx = build_vpx_cmd_spec();

    // 1. Root `vp --` must NOT fabricate unsupported `--dir`
    let root_long_flags = complete_values(&spec, Some(&vpx), &["vp", "--"], root);
    assert!(
        !root_long_flags.iter().any(|f| f == "--dir"),
        "vp must not advertise nonexistent --dir flag: {root_long_flags:?}"
    );
    let root_short_flags = complete_values(&spec, Some(&vpx), &["vp", "-"], root);
    assert!(root_short_flags.iter().any(|f| f == "-C"), "vp must advertise -C");

    // 2. Pure-Rust task extraction + real `vp -C`, `vpr -C`, `vp run -C`, and `vpr -r -C` rules
    temp_env::with_var("VP_COMPLETE_OFFLINE_ONLY_RUST", Some("1"), || {
        let vp_c =
            complete_values(&spec, Some(&vpx), &["vp", "-C", "packages/sub", "run", ""], root);
        assert_has_all(&vp_c, &["sub-task", "regex:safe"], "vp -C packages/sub run");

        let vpr_c = complete_values(&spec, Some(&vpx), &["vpr", "-C", "packages/sub", ""], root);
        assert_has_all(&vpr_c, &["sub-task", "regex:safe"], "vpr -C packages/sub");

        // `vp run` does NOT accept `-C` after `run`: must stay in `root`, not chdir to `packages/sub`
        let vp_run_c =
            complete_values(&spec, Some(&vpx), &["vp", "run", "-C", "packages/sub", ""], root);
        assert!(
            !vp_run_c.iter().any(|t| t == "sub-task"),
            "vp run -C must not chdir after subcommand: {vp_run_c:?}"
        );

        // `vpr -r -C` has `-C` after position 1: must NOT chdir
        let vpr_late_c =
            complete_values(&spec, Some(&vpx), &["vpr", "-r", "-C", "packages/sub", ""], root);
        assert!(
            !vpr_late_c.iter().any(|t| t == "sub-task"),
            "vpr -r -C must not chdir past leading token: {vpr_late_c:?}"
        );
    });

    // 3. Workspace discovery respects `!packages/ignored` negation
    let ws_names = vp_complete::get_workspace_package_names_with(root, super::read_pkg_manifest);
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
    let npm_names =
        vp_complete::get_workspace_package_names_with(npm_ws.path(), super::read_pkg_manifest);
    assert!(
        npm_names.contains(&"npm-root".to_string()) && npm_names.contains(&"@app/web".to_string())
    );
}

#[test]
fn test_local_help_filtering_and_offline_node_semver() {
    let spec = build_vp_cmd_spec();
    let cwd = PathBuf::from("/workspace");
    let local_help = complete_values(&spec, None, &["./node_modules/.bin/vp", "help", ""], &cwd);
    assert!(local_help.contains(&"build".to_string()), "local help missing build: {local_help:?}");
    for global_cmd in ["env", "upgrade", "implode"] {
        assert!(
            !local_help.iter().any(|c| c == global_cmd),
            "local node_modules/.bin/vp help must hide {global_cmd}: {local_help:?}"
        );
    }

    let temp_node = TempDir::new().unwrap();
    let rel_bin = if cfg!(windows) { "node.exe" } else { "bin/node" };
    for ver in ["v9.9.0", "v22.14.0", "v20.18.0"] {
        let bin = temp_node.path().join(ver).join(rel_bin);
        fs::create_dir_all(bin.parent().unwrap()).unwrap();
        fs::write(&bin, "").unwrap();
    }
    let resolved = temp_env::with_vars([("VP_NODE_VERSION", None::<&str>)], || {
        vp_complete::resolve_offline_node_in(temp_node.path(), &[])
    })
    .expect("must resolve node binary");
    assert!(
        resolved.to_string_lossy().contains("v22.14.0"),
        "expected v22.14.0 over v9.9.0, got: {}",
        resolved.display()
    );
}
