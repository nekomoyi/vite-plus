#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_types,
    reason = "test suite uses standard test types and macros"
)]

use std::{fs, path::PathBuf, sync::Arc};

use tempfile::TempDir;
use vp_complete::{
    CmdSpec, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput,
    CompletionRequest, FlagSpec, ItemKind, PathMode, PositionalSpec, ValueHint, complete,
    complete_path, parse_bash_line,
};

fn create_sample_root_spec() -> CmdSpec {
    CmdSpec::new("vp")
        .about("Vite+ command line interface")
        .flag(
            FlagSpec::new()
                .short('C')
                .long("dir")
                .about("Run as if vp was started in <dir>")
                .takes_value(true)
                .chdir(true)
                .global(true)
                .value_hint(ValueHint::DirPath),
        )
        .flag(
            FlagSpec::new()
                .long("log-level")
                .about("Set logging level")
                .takes_value(true)
                .global(true)
                .value_hint(ValueHint::StaticValues(vec![
                    ("silent".into(), Some("No output".into())),
                    ("info".into(), Some("Default output".into())),
                    ("debug".into(), Some("Verbose debug output".into())),
                ])),
        )
        .flag(FlagSpec::new().short('v').long("verbose").about("Enable verbose output"))
        .subcommand(CmdSpec::new("run").about("Run a project script or task").positional(
            PositionalSpec::new("script").about("Script name to execute").completer(Arc::new(
                |_ctx: &CompletionContext| {
                    CompletionOutput::from_items(vec![
                        CompletionItem::value("build", Some("Build the project")),
                        CompletionItem::value("build:watch", Some("Build and watch")),
                        CompletionItem::value("test", Some("Run unit tests")),
                    ])
                },
            )),
        ))
        .subcommand(
            CmdSpec::new("install")
                .about("Install project dependencies")
                .visible_aliases(vec!["i", "add"]),
        )
        .subcommand(
            CmdSpec::new("uninstall")
                .about("Remove dependencies")
                .visible_aliases(vec!["unlink", "un"]),
        )
        .subcommand(CmdSpec::new("remove").about("Remove a dependency").hidden_alias("rm"))
        .subcommand(CmdSpec::new("env").about("Manage Vite+ environments").global_only(true))
        .subcommand(CmdSpec::new("upgrade").about("Upgrade Vite+").global_only(true))
        .subcommand(CmdSpec::new("implode").about("Uninstall Vite+ completely").global_only(true))
}

fn create_sample_vpx_spec() -> CmdSpec {
    CmdSpec::new("vpx")
        .about("Execute an npm package or script")
        .flag(
            FlagSpec::new()
                .short('C')
                .long("conditions")
                .about("Pass conditions to runtime")
                .takes_value(true),
        )
        .flag(FlagSpec::new().long("package").about("Package to install and run").takes_value(true))
        .positional(PositionalSpec::new("command").about("Command or package to run").completer(
            Arc::new(|ctx: &CompletionContext| {
                CompletionOutput::from_items(vec![CompletionItem::value(
                    format!("cwd:{}", ctx.cwd.display()),
                    None::<String>,
                )])
            }),
        ))
}

#[test]
fn test_bimodal_subcommand_alias_matching() {
    let root = create_sample_root_spec();
    let cwd = PathBuf::from("/test");

    // (1) Empty prefix lists visible canonical commands only, with alias hints
    let req_empty = CompletionRequest::new(vec!["vp".into(), "".into()], 1, cwd.clone());
    let out_empty = complete(&root, None, &req_empty);

    let values: Vec<&str> = out_empty.items.iter().map(|it| it.value.as_str()).collect();
    assert!(values.contains(&"run"), "run should be listed");
    assert!(values.contains(&"install"), "install should be listed");
    assert!(values.contains(&"uninstall"), "uninstall should be listed");
    assert!(!values.contains(&"unlink"), "alias 'unlink' should NOT be listed with empty prefix");
    assert!(!values.contains(&"un"), "alias 'un' should NOT be listed with empty prefix");
    assert!(!values.contains(&"rm"), "hidden alias 'rm' should NOT be listed with empty prefix");

    // Verify alias hint in description for uninstall
    let uninst_item = out_empty.items.iter().find(|it| it.value == "uninstall").unwrap();
    let desc = uninst_item.description.as_deref().unwrap_or("");
    assert!(
        desc.contains("aliases:") && desc.contains("unlink") && desc.contains("un"),
        "Description should mention aliases: got '{desc}'"
    );

    // (2) Non-empty prefix "un" returns canonical name AND visible aliases as equal peers:
    // "un", "uninstall", and "unlink" together!
    let req_un = CompletionRequest::new(vec!["vp".into(), "un".into()], 1, cwd.clone());
    let out_un = complete(&root, None, &req_un);
    let values_un: Vec<&str> = out_un.items.iter().map(|it| it.value.as_str()).collect();
    assert!(values_un.contains(&"un"), "visible alias 'un' should be suggested");
    assert!(values_un.contains(&"uninstall"), "canonical 'uninstall' should be suggested");
    assert!(values_un.contains(&"unlink"), "visible alias 'unlink' should be suggested");

    // (3) Hidden alias matching when non-empty prefix matches
    let req_rm = CompletionRequest::new(vec!["vp".into(), "rm".into()], 1, cwd.clone());
    let out_rm = complete(&root, None, &req_rm);
    let values_rm: Vec<&str> = out_rm.items.iter().map(|it| it.value.as_str()).collect();
    assert!(values_rm.contains(&"rm"), "hidden alias 'rm' should be suggested when prefix matches");
}

#[test]
fn test_chdir_flag_effective_cwd() {
    let root = create_sample_root_spec();
    let vpx = create_sample_vpx_spec();
    let base_cwd = PathBuf::from("/workspace/root");

    // (1) vp -C subpkg run <Tab> -> ctx.cwd updated to /workspace/root/subpkg
    let captured_cwd = Arc::new(std::sync::Mutex::new(PathBuf::new()));
    let captured_clone = captured_cwd.clone();
    let mut custom_root = root.clone();
    custom_root.subcommands.iter_mut().find(|s| s.name == "run").unwrap().positionals[0]
        .completer = Some(Arc::new(move |ctx: &CompletionContext| {
        *captured_clone.lock().unwrap() = ctx.cwd.clone();
        CompletionOutput::empty()
    }));

    let req_vp = CompletionRequest::new(
        vec!["vp".into(), "-C".into(), "subpkg".into(), "run".into(), "".into()],
        4,
        base_cwd.clone(),
    );
    complete(&custom_root, None, &req_vp);
    assert_eq!(*captured_cwd.lock().unwrap(), base_cwd.join("subpkg"));

    // (2) vpr --dir=subpkg <Tab> -> vpr routes directly to run and updates cwd
    let captured_clone2 = captured_cwd.clone();
    custom_root.subcommands.iter_mut().find(|s| s.name == "run").unwrap().positionals[0]
        .completer = Some(Arc::new(move |ctx: &CompletionContext| {
        *captured_clone2.lock().unwrap() = ctx.cwd.clone();
        CompletionOutput::empty()
    }));

    let req_vpr = CompletionRequest::new(
        vec!["vpr".into(), "--dir=subpkg".into(), "".into()],
        2,
        base_cwd.clone(),
    );
    complete(&custom_root, None, &req_vpr);
    assert_eq!(*captured_cwd.lock().unwrap(), base_cwd.join("subpkg"));

    // (3) vpx -C subpkg <Tab> -> chdir_flag is false on vpx, so cwd remains original base_cwd!
    let req_vpx = CompletionRequest::new(
        vec!["vpx".into(), "-C".into(), "subpkg".into(), "".into()],
        3,
        base_cwd.clone(),
    );
    let out_vpx = complete(&root, Some(&vpx), &req_vpx);
    assert_eq!(
        out_vpx.items[0].value,
        format!("cwd:{}", base_cwd.display()),
        "vpx must not change cwd because chdir_flag is false"
    );
}

#[test]
fn test_bash_lexer() {
    // (1) POSIX shlex segment reset on &&
    let res = parse_bash_line("echo hi && vp run build:wa", 26);
    assert_eq!(res.words, vec!["vp", "run", "build:wa"]);
    assert_eq!(res.current_index, 2);

    // (2) Trailing whitespace creates an empty candidate at current_index
    let res_space = parse_bash_line("vp run ", 7);
    assert_eq!(res_space.words, vec!["vp", "run", ""]);
    assert_eq!(res_space.current_index, 2);

    // (3) Quotes and escape sequences
    let res_quotes = parse_bash_line("vp run \"build with\\\"quote\" 'single quote'", 41);
    assert_eq!(res_quotes.words, vec!["vp", "run", "build with\"quote", "single quote"]);

    // (4) Other pipeline and command separators: |, ;, ||
    let res_pipe = parse_bash_line("cat file | vp install", 21);
    assert_eq!(res_pipe.words, vec!["vp", "install"]);
    assert_eq!(res_pipe.current_index, 1);

    // (5) Multi-line input and newlines resetting words
    let res_nl = parse_bash_line("echo hi\nvp un", 13);
    assert_eq!(res_nl.words, vec!["vp", "un"]);
    assert_eq!(res_nl.current_index, 1);

    let res_nu_block = parse_bash_line("do {\n  let x = 1\n  vpr ", 23);
    assert_eq!(res_nu_block.words, vec!["vpr", ""]);
    assert_eq!(res_nu_block.current_index, 1);
}

#[test]
fn test_path_provider() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Create file structure:
    // root/
    //   src/
    //     index.ts
    //     index.js
    //     readme.md
    //   linked_dir -> src
    //   .hidden_file
    //   package.json

    let src = root.join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("index.ts"), "").unwrap();
    fs::write(src.join("index.js"), "").unwrap();
    fs::write(src.join("readme.md"), "").unwrap();
    fs::write(root.join(".hidden_file"), "").unwrap();
    fs::write(root.join("package.json"), "").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&src, root.join("linked_dir")).unwrap();

    // 1. Preserve ./ prefix
    let out = complete_path(root, "./s", PathMode::Any);
    let values: Vec<&str> = out.items.iter().map(|it| it.value.as_str()).collect();
    assert!(values.contains(&"./src/"), "Must preserve ./ and append / to dir");
    assert!(out.directive.contains(CompletionDirective::NO_SPACE));
    assert!(out.directive.contains(CompletionDirective::NO_FILE_COMP));

    // 2. Follow symlinked directory
    #[cfg(unix)]
    {
        let out_sym = complete_path(root, "link", PathMode::Any);
        let items: Vec<_> = out_sym.items.iter().filter(|i| i.value == "linked_dir/").collect();
        assert_eq!(
            items.len(),
            1,
            "Symlink to directory must be recognized as directory with trailing /"
        );
        assert_eq!(items[0].kind, ItemKind::Directory);
    }

    // 3. Extension filtering: FilesWithExtensions(["ts".into()]) includes directories + .ts files
    let out_ext = complete_path(root, "src/", PathMode::FilesWithExtensions(vec!["ts".into()]));
    let ext_values: Vec<&str> = out_ext.items.iter().map(|it| it.value.as_str()).collect();
    assert!(ext_values.contains(&"src/index.ts"));
    assert!(!ext_values.contains(&"src/index.js"));
    assert!(!ext_values.contains(&"src/readme.md"));

    // 4. Dotfiles are hidden unless prefix starts with .
    let out_no_dot = complete_path(root, "", PathMode::Any);
    let no_dot_vals: Vec<&str> = out_no_dot.items.iter().map(|it| it.value.as_str()).collect();
    assert!(!no_dot_vals.contains(&".hidden_file"));

    let out_dot = complete_path(root, ".", PathMode::Any);
    let dot_vals: Vec<&str> = out_dot.items.iter().map(|it| it.value.as_str()).collect();
    assert!(dot_vals.contains(&".hidden_file"));
}

#[test]
fn test_flag_completion_and_deduplication() {
    let root = create_sample_root_spec();
    let cwd = PathBuf::from("/test");

    // (1) Suggest flags when prefix starts with '-'
    let req = CompletionRequest::new(vec!["vp".into(), "-".into()], 1, cwd.clone());
    let out = complete(&root, None, &req);
    let flags: Vec<&str> = out.items.iter().map(|i| i.value.as_str()).collect();
    assert!(flags.contains(&"--log-level"));
    assert!(flags.contains(&"--verbose"));
    assert!(flags.contains(&"-v"));

    // (2) Flag deduplication: --verbose is non-multiple, once used it is omitted
    let req_dedup =
        CompletionRequest::new(vec!["vp".into(), "--verbose".into(), "-".into()], 2, cwd.clone());
    let out_dedup = complete(&root, None, &req_dedup);
    let dedup_flags: Vec<&str> = out_dedup.items.iter().map(|i| i.value.as_str()).collect();
    assert!(!dedup_flags.contains(&"--verbose"));
    assert!(!dedup_flags.contains(&"-v"), "Short alias must also be omitted");

    // (3) '--' barrier: after bare '--', tokens starting with '-' are treated as positionals
    let req_barrier = CompletionRequest::new(
        vec!["vp".into(), "run".into(), "--".into(), "-".into()],
        3,
        cwd.clone(),
    );
    let out_barrier = complete(&root, None, &req_barrier);
    // After '--', positional completer of 'run' should be called, flags should NOT be suggested
    let kinds: Vec<ItemKind> = out_barrier.items.iter().map(|i| i.kind).collect();
    assert!(!kinds.contains(&ItemKind::Flag), "No flags suggested after '--'");

    // (4) Inline flag value completion: --log-level=d<Tab> -> --log-level=debug
    let req_inline =
        CompletionRequest::new(vec!["vp".into(), "--log-level=d".into()], 1, cwd.clone());
    let out_inline = complete(&root, None, &req_inline);
    let inline_vals: Vec<&str> = out_inline.items.iter().map(|i| i.value.as_str()).collect();
    assert!(inline_vals.contains(&"--log-level=debug"));
}

#[test]
fn test_local_bin_filtering() {
    let root = create_sample_root_spec();
    let cwd = PathBuf::from("/test");

    // (1) Normal vp has env, upgrade, implode
    let req_global = CompletionRequest::new(vec!["vp".into(), "".into()], 1, cwd.clone());
    let out_global = complete(&root, None, &req_global);
    let global_vals: Vec<&str> = out_global.items.iter().map(|i| i.value.as_str()).collect();
    assert!(global_vals.contains(&"env"));
    assert!(global_vals.contains(&"upgrade"));
    assert!(global_vals.contains(&"implode"));

    // (2) node_modules/.bin/vp hides global_only subcommands
    let req_local =
        CompletionRequest::new(vec!["./node_modules/.bin/vp".into(), "".into()], 1, cwd.clone());
    let out_local = complete(&root, None, &req_local);
    let local_vals: Vec<&str> = out_local.items.iter().map(|i| i.value.as_str()).collect();
    assert!(!local_vals.contains(&"env"));
    assert!(!local_vals.contains(&"upgrade"));
    assert!(!local_vals.contains(&"implode"));
    assert!(local_vals.contains(&"run"), "Non-global subcommands remain visible");
}

#[test]
fn test_clap_conversion() {
    let cmd = clap::Command::new("vp_test")
        .about("Test command")
        .subcommand(
            clap::Command::new("build")
                .about("Build command")
                .visible_alias("b")
                .alias("hidden_b")
                .arg(
                    clap::Arg::new("target")
                        .long("target")
                        .value_parser(["node", "browser"])
                        .help("Target environment"),
                ),
        )
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_hint(clap::ValueHint::FilePath)
                .help("Config file"),
        );

    let spec = CmdSpec::from_clap(&cmd);
    assert_eq!(spec.name, "vp_test");
    assert_eq!(spec.about.as_deref(), Some("Test command"));
    let build_sub = spec.subcommands.iter().find(|s| s.name == "build").unwrap();
    assert!(spec.subcommands.iter().any(|s| s.name == "help"));
    assert!(build_sub.visible_aliases.contains(&"b".to_string()));
    assert!(build_sub.hidden_aliases.contains(&"hidden_b".to_string()));

    let config_flag = spec.flags.iter().find(|f| f.long.as_deref() == Some("config")).unwrap();
    assert_eq!(config_flag.short, Some('c'));
    assert!(matches!(config_flag.value_hint, ValueHint::FilePath { .. }));
    assert!(spec.flags.iter().any(|f| f.long.as_deref() == Some("help") && f.short == Some('h')));

    let target_flag = build_sub.flags.iter().find(|f| f.long.as_deref() == Some("target")).unwrap();
    match &target_flag.value_hint {
        ValueHint::StaticValues(vals) => {
            let names: Vec<&str> = vals.iter().map(|(v, _)| v.as_str()).collect();
            assert!(names.contains(&"node"));
            assert!(names.contains(&"browser"));
        }
        other => panic!("Expected StaticValues, got {other:?}"),
    }
}

#[test]
fn test_engine_snapshots() {
    let root = create_sample_root_spec();
    let cwd = PathBuf::from("/test");

    // Empty prefix snapshot
    let req_empty = CompletionRequest::new(vec!["vp".into(), "".into()], 1, cwd.clone());
    let out_empty = complete(&root, None, &req_empty);
    insta::assert_debug_snapshot!("empty_prefix_completions", out_empty);

    // "un" prefix snapshot
    let req_un = CompletionRequest::new(vec!["vp".into(), "un".into()], 1, cwd.clone());
    let out_un = complete(&root, None, &req_un);
    insta::assert_debug_snapshot!("un_prefix_completions", out_un);

    // Flag suggestions snapshot
    let req_flags = CompletionRequest::new(vec!["vp".into(), "-".into()], 1, cwd.clone());
    let out_flags = complete(&root, None, &req_flags);
    insta::assert_debug_snapshot!("flag_suggestions", out_flags);

    // Bash lexer snapshot
    let lex_result = parse_bash_line("echo hi && vp run build:wa", 26);
    insta::assert_debug_snapshot!("bash_lexer_compound_line", lex_result);
}

#[test]
fn test_file_path_empty_extensions_matches_all_files() {
    let temp = TempDir::new().unwrap();
    let root_dir = temp.path();
    fs::write(root_dir.join(".env.local"), "A=1").unwrap();
    fs::write(root_dir.join("notes.txt"), "hello").unwrap();
    fs::create_dir(root_dir.join("configs")).unwrap();

    // Direct complete_path with empty extensions vector
    let out = complete_path(root_dir, "", PathMode::FilesWithExtensions(vec![]));
    let vals: Vec<&str> = out.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        vals.contains(&"notes.txt"),
        "Empty extensions must match regular files, got: {vals:?}"
    );
    assert!(
        vals.contains(&"configs/"),
        "Empty extensions must still include directories, got: {vals:?}"
    );

    // Also via clap::ValueHint::FilePath conversion
    let cmd = clap::Command::new("vp")
        .arg(clap::Arg::new("env-file").long("env-file").value_hint(clap::ValueHint::FilePath));
    let spec = CmdSpec::from_clap(&cmd);
    let req = CompletionRequest::new(
        vec!["vp".into(), "--env-file".into(), ".".into()],
        2,
        root_dir.to_path_buf(),
    );
    let flag_out = complete(&spec, None, &req);
    let flag_vals: Vec<&str> = flag_out.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        flag_vals.contains(&".env.local"),
        "clap::ValueHint::FilePath must complete .env.local, got: {flag_vals:?}"
    );
}

#[test]
fn test_chdir_respects_double_dash_and_chained_chdir() {
    let temp = TempDir::new().unwrap();
    let root_dir = temp.path();
    fs::write(root_dir.join("file_in_root.txt"), "hi").unwrap();
    fs::create_dir_all(root_dir.join("packages/app")).unwrap();

    let spec =
        CmdSpec::new("vp")
            .flag(
                FlagSpec::new()
                    .short('C')
                    .long("dir")
                    .takes_value(true)
                    .multiple(true)
                    .chdir(true)
                    .global(true)
                    .value_hint(ValueHint::DirPath),
            )
            .subcommand(CmdSpec::new("run").positional(PositionalSpec::new("task")).positional(
                PositionalSpec::new("args").multiple(true).value_hint(ValueHint::AnyPath),
            ))
            .subcommand(CmdSpec::new("env").flag(FlagSpec::new().long("env-only-flag")));

    // 1. After `--`, `-C` is a passthrough arg, NOT vp's chdir flag (so files in root_dir must appear)
    let req_after_dd = CompletionRequest::new(
        vec!["vp".into(), "run".into(), "build".into(), "--".into(), "-C".into(), "".into()],
        5,
        root_dir.to_path_buf(),
    );
    let out_dd = complete(&spec, None, &req_after_dd);
    let dd_vals: Vec<&str> = out_dd.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        dd_vals.contains(&"file_in_root.txt"),
        "After '--', '-C <Tab>' must complete AnyPath positionals including files, got: {dd_vals:?}"
    );

    // 2. Chained `-C packages -C <Tab>` resolves relative to `packages/`
    let req_chained = CompletionRequest::new(
        vec!["vp".into(), "-C".into(), "packages".into(), "-C".into(), "".into()],
        4,
        root_dir.to_path_buf(),
    );
    let out_chained = complete(&spec, None, &req_chained);
    let chained_vals: Vec<&str> = out_chained.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        chained_vals.contains(&"app/"),
        "Chained '-C packages -C <Tab>' must resolve against effective_cwd, got: {chained_vals:?}"
    );

    // 3. Subcommand transition must NOT trigger at positional_index > 0
    let mixed_spec = CmdSpec::new("vp")
        .positional(PositionalSpec::new("first"))
        .positional(PositionalSpec::new("second"))
        .subcommand(CmdSpec::new("env").flag(FlagSpec::new().long("secret-env-flag")));
    let req_pos1 = CompletionRequest::new(
        vec!["vp".into(), "not_a_subcmd".into(), "env".into(), "-".into()],
        3,
        root_dir.to_path_buf(),
    );
    let out_pos1 = complete(&mixed_spec, None, &req_pos1);
    let pos1_vals: Vec<&str> = out_pos1.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        !pos1_vals.contains(&"--secret-env-flag"),
        "Token matching subcommand name at positional_index > 0 must not enter subcommand: {pos1_vals:?}"
    );
}

#[test]
fn test_tilde_path_completion() {
    let cwd_tmp = TempDir::new().unwrap();
    let home_tmp = TempDir::new().unwrap();
    fs::create_dir(home_tmp.path().join("projects")).unwrap();
    fs::write(home_tmp.path().join("notes.ts"), "").unwrap();

    // 1. Bare "~" must complete to "~/" with Directory kind and NO_SPACE | NO_FILE_COMP
    let out_bare = complete_path(cwd_tmp.path(), "~", PathMode::Any);
    let bare_vals: Vec<&str> = out_bare.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(bare_vals, vec!["~/"]);
    assert_eq!(out_bare.items[0].kind, ItemKind::Directory);
    assert!(out_bare.directive.contains(CompletionDirective::NO_SPACE));
    assert!(out_bare.directive.contains(CompletionDirective::NO_FILE_COMP));

    // 2. "~/p" and "~/" resolve against home directory cleanly without dead branches
    let out_home = vp_complete::path::complete_path_with_home(
        cwd_tmp.path(),
        Some(home_tmp.path()),
        "~/p",
        PathMode::Any,
    );
    let home_vals: Vec<&str> = out_home.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(home_vals, vec!["~/projects/"]);

    // 3. When home is None, bare "~" does not fabricate "~/" and "~/" returns empty NO_FILE_COMP
    let out_no_home =
        vp_complete::path::complete_path_with_home(cwd_tmp.path(), None, "~", PathMode::Any);
    assert!(out_no_home.items.is_empty());
    let out_slash_no_home =
        vp_complete::path::complete_path_with_home(cwd_tmp.path(), None, "~/", PathMode::Any);
    assert!(out_slash_no_home.items.is_empty());
}

#[test]
fn test_clap_from_unbuilt_includes_generated_help_and_last_positionals() {
    // Finding #10 & #8: Unbuilt Clap command must materialize -h/--help and `last = true` positionals
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("filesystem_leak.txt"), "nope").unwrap();

    let cmd = clap::Command::new("vp")
        .subcommand(clap::Command::new("env").about("Manage env"))
        .subcommand(
            clap::Command::new("remove").arg(clap::Arg::new("packages").num_args(1..)).arg(
                clap::Arg::new("passthrough")
                    .num_args(0..)
                    .last(true)
                    .value_hint(clap::ValueHint::AnyPath),
            ),
        );

    let mut spec = CmdSpec::from_clap(&cmd);
    let rm = spec.subcommands.iter_mut().find(|s| s.name == "remove").unwrap();
    rm.positionals[0].completer = Some(Arc::new(|_ctx| {
        CompletionOutput::new(
            vec![
                CompletionItem::value("react", None::<String>),
                CompletionItem::value("vitest", None::<String>),
            ],
            CompletionDirective::NO_FILE_COMP,
        )
    }));

    // 1. Root `vp --h` must complete `--help`
    let out_root_help = complete(
        &spec,
        None,
        &CompletionRequest::new(vec!["vp".into(), "--h".into()], 1, temp.path().to_path_buf()),
    );
    let root_flags: Vec<&str> = out_root_help.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        root_flags.contains(&"--help"),
        "Root must offer generated --help, got: {root_flags:?}"
    );

    // 2. Subcommand `vp env -` must offer `-h` and `--help`
    let out_env_help = complete(
        &spec,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "env".into(), "-".into()],
            2,
            temp.path().to_path_buf(),
        ),
    );
    let env_flags: Vec<&str> = out_env_help.items.iter().map(|i| i.value.as_str()).collect();
    assert!(env_flags.contains(&"--help") && env_flags.contains(&"-h"), "got: {env_flags:?}");

    // 3. Variadic positional `vp remove react <Tab>` BEFORE `--` must stay on `packages`, NOT leak to `last=true` passthrough!
    let out_rm_second = complete(
        &spec,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "remove".into(), "react".into(), "".into()],
            3,
            temp.path().to_path_buf(),
        ),
    );
    let rm_vals: Vec<&str> = out_rm_second.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        rm_vals.contains(&"vitest") && !rm_vals.contains(&"filesystem_leak.txt"),
        "Second package argument before '--' must stay on variadic packages positional, got: {rm_vals:?}"
    );

    // 4. After `--`, `vp remove react -- <Tab>` routes to `last=true` passthrough (`AnyPath`)
    let out_rm_after_dd = complete(
        &spec,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "remove".into(), "react".into(), "--".into(), "".into()],
            4,
            temp.path().to_path_buf(),
        ),
    );
    let dd_vals: Vec<&str> = out_rm_after_dd.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        dd_vals.contains(&"filesystem_leak.txt"),
        "After '--', completion must route to last=true passthrough positional, got: {dd_vals:?}"
    );
}

#[test]
fn test_vpr_non_global_chdir_short_clusters_and_windows_exts() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("packages/subpkg")).unwrap();
    fs::write(temp.path().join("packages/subpkg/package.json"), "{}").unwrap();

    // Real cli::Args has non-global `-C` on root!
    let root = CmdSpec::new("vp")
        .flag(
            FlagSpec::new().short('C').takes_value(true).chdir(true).value_hint(ValueHint::DirPath),
        )
        .subcommand(
            CmdSpec::new("run")
                .flag(FlagSpec::new().short('r').long("recursive"))
                .flag(
                    FlagSpec::new()
                        .short('F')
                        .long("filter")
                        .takes_value(true)
                        .multiple(true)
                        .completer(Arc::new(|ctx| {
                            let mut items =
                                vec![CompletionItem::value("@demo/subpkg", None::<String>)];
                            items.retain(|i| i.value.starts_with(&ctx.prefix));
                            CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
                        })),
                )
                .positional(PositionalSpec::new("task").completer(Arc::new(|ctx| {
                    if ctx.positional_index != 0 {
                        return CompletionOutput::empty();
                    }
                    CompletionOutput::new(
                        vec![CompletionItem::value(
                            format!("task_in_{}", ctx.cwd.file_name().unwrap().to_string_lossy()),
                            None::<String>,
                        )],
                        CompletionDirective::NO_FILE_COMP,
                    )
                })))
                .positional(
                    PositionalSpec::new("args").multiple(true).value_hint(ValueHint::AnyPath),
                ),
        );
    let vpx = create_sample_vpx_spec();

    // Finding #7: `vpr -C packages/subpkg <Tab>` must keep positional_index == 0 and complete tasks!
    let out_vpr_chdir = complete(
        &root,
        Some(&vpx),
        &CompletionRequest::new(
            vec!["vpr".into(), "-C".into(), "packages/subpkg".into(), "".into()],
            3,
            temp.path().to_path_buf(),
        ),
    );
    let vpr_vals: Vec<&str> = out_vpr_chdir.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(
        vpr_vals,
        vec!["task_in_subpkg"],
        "vpr -C <dir> <Tab> must complete tasks at positional_index 0, got: {vpr_vals:?}"
    );
    assert_eq!(
        out_vpr_chdir.effective_cwd,
        Some(temp.path().join("packages/subpkg")),
        "effective_cwd must reflect resolved -C directory"
    );

    // `vpr` only accepts non-global `-C` at position 1 (leading token), matching runtime cli_args.rs:
    let out_vpr_lead_flag = complete(
        &root,
        Some(&vpx),
        &CompletionRequest::new(vec!["vpr".into(), "-".into()], 1, temp.path().to_path_buf()),
    );
    assert!(
        out_vpr_lead_flag.items.iter().any(|i| i.value == "-C"),
        "vpr -<Tab> at index 1 must suggest -C"
    );
    let out_vpr_later_flag = complete(
        &root,
        Some(&vpx),
        &CompletionRequest::new(
            vec!["vpr".into(), "-r".into(), "-".into()],
            2,
            temp.path().to_path_buf(),
        ),
    );
    assert!(
        !out_vpr_later_flag.items.iter().any(|i| i.value == "-C"),
        "vpr -r -<Tab> at index > 1 must NOT suggest non-global -C"
    );
    let out_vp_run_after = complete(
        &root,
        Some(&vpx),
        &CompletionRequest::new(
            vec!["vp".into(), "run".into(), "-".into()],
            2,
            temp.path().to_path_buf(),
        ),
    );
    assert!(
        !out_vp_run_after.items.iter().any(|i| i.value == "-C"),
        "vp run -<Tab> must NOT suggest non-global root -C"
    );

    // Finding #9a: Short-option cluster `-rF @demo/subpkg <Tab>` consumes `@demo/subpkg` as -F value!
    let out_cluster = complete(
        &root,
        Some(&vpx),
        &CompletionRequest::new(
            vec!["vp".into(), "run".into(), "-rF".into(), "@demo/subpkg".into(), "".into()],
            4,
            temp.path().join("packages/subpkg"),
        ),
    );
    let cluster_vals: Vec<&str> = out_cluster.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(
        cluster_vals,
        vec!["task_in_subpkg"],
        "-rF <pkg> <Tab> must consume <pkg> as -F value and complete tasks, got: {cluster_vals:?}"
    );

    // Finding #9b: Attached short-option value `-F@demo/` and clustered `-rF@demo/`
    for attached in ["-F@demo/", "-rF@demo/"] {
        let prefix_part = attached.strip_suffix("@demo/").unwrap();
        let out_att = complete(
            &root,
            Some(&vpx),
            &CompletionRequest::new(
                vec!["vp".into(), "run".into(), attached.into()],
                2,
                temp.path().to_path_buf(),
            ),
        );
        let att_vals: Vec<&str> = out_att.items.iter().map(|i| i.value.as_str()).collect();
        assert_eq!(
            att_vals,
            vec![format!("{prefix_part}@demo/subpkg")],
            "Attached short option '{attached}' must complete value with prefix, got: {att_vals:?}"
        );
    }

    // Finding #14: Windows executable extensions `.cmd`, `.bat`, `.EXE`, `.PS1` (including backslash paths)
    for vpx_bin in ["vpx.cmd", "VPX.BAT", r"C:\Users\dev\.vite-plus\bin\vpx.CMD", "vpx.EXE"] {
        let out_win_vpx = complete(
            &root,
            Some(&vpx),
            &CompletionRequest::new(
                vec![vpx_bin.into(), "-C".into(), "packages/subpkg".into(), "".into()],
                3,
                temp.path().to_path_buf(),
            ),
        );
        assert_eq!(
            out_win_vpx.items[0].value,
            format!("cwd:{}", temp.path().display()),
            "{vpx_bin} must route to vpx spec without chdir"
        );
    }
}

#[test]
fn test_word_zero_does_not_panic() {
    let root = create_sample_root_spec();
    let cwd = PathBuf::from("/workspace/root");

    for words in [vec!["vp".to_string()], vec!["vp".to_string(), "uninstall".to_string()]] {
        let out = complete(&root, None, &CompletionRequest::new(words, 0, cwd.clone()));
        assert!(out.items.is_empty(), "Completing word 0 must not panic or emit subcommands");
        assert_eq!(out.directive, CompletionDirective::DEFAULT);
    }
}

#[test]
fn test_tilde_chdir_resolution() {
    if std::env::var_os("_VP_TEST_TILDE_SUBPROC").is_none() {
        let temp = TempDir::new().unwrap();
        let fake_home = temp.path().join("home");
        fs::create_dir_all(fake_home.join("project")).unwrap();
        fs::write(fake_home.join("project/entry.js"), "console.log(1);\n").unwrap();
        let other_cwd = temp.path().join("other");
        fs::create_dir_all(&other_cwd).unwrap();

        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "test_tilde_chdir_resolution", "--nocapture"])
            .env("_VP_TEST_TILDE_SUBPROC", "1")
            .env("_VP_TEST_OTHER_CWD", &other_cwd)
            .env("HOME", &fake_home)
            .env("USERPROFILE", &fake_home)
            .status()
            .unwrap();
        assert!(status.success(), "Subprocess tilde chdir test failed");
        return;
    }

    let other_cwd = PathBuf::from(std::env::var_os("_VP_TEST_OTHER_CWD").unwrap());
    let root = CmdSpec::new("vp")
        .flag(
            FlagSpec::new()
                .short('C')
                .long("dir")
                .takes_value(true)
                .chdir(true)
                .global(true)
                .value_hint(ValueHint::DirPath),
        )
        .subcommand(
            CmdSpec::new("node")
                .positional(PositionalSpec::new("script").value_hint(ValueHint::AnyPath)),
        );

    let fake_home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let home_proj_slash = format!("{}/project/", fake_home.display());

    // Standalone `-C ~/project` expands against HOME (matching shell execution)
    for args in
        [vec!["vp", "-C", "~/project", "node", "en"], vec!["vp", "-C", r"~\project", "node", "en"]]
    {
        let words: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        let idx = words.len() - 1;
        let out = complete(&root, None, &CompletionRequest::new(words, idx, other_cwd.clone()));
        let vals: Vec<&str> = out.items.iter().map(|i| i.value.as_str()).collect();
        assert_eq!(vals, vec!["entry.js"], "Standalone tilde chdir failed for {args:?}");
    }

    // Attached `-C~/proj`, `-C=~/proj`, `--dir=~/proj` emit expanded executable paths
    for (token, expected) in [
        ("-C~", format!("-C{}/", fake_home.display())),
        ("-C~/proj", format!("-C{home_proj_slash}")),
        ("-C=~/proj", format!("-C={home_proj_slash}")),
        ("--dir=~/proj", format!("--dir={home_proj_slash}")),
    ] {
        let out = complete(
            &root,
            None,
            &CompletionRequest::new(vec!["vp".into(), token.into()], 1, other_cwd.clone()),
        );
        let vals: Vec<&str> = out.items.iter().map(|i| i.value.as_str()).collect();
        assert_eq!(vals, vec![expected.as_str()], "Attached tilde completion failed for {token}");
    }

    // Unexpanded attached `-C~/project` resolves literally against cwd (matching apply_chdir)
    let out_literal_attached = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "-C~/project".into(), "node".into(), "en".into()],
            3,
            other_cwd.clone(),
        ),
    );
    assert!(out_literal_attached.items.is_empty());

    let out_home_root = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "-C".into(), "~".into(), "node".into(), "proj".into()],
            4,
            other_cwd,
        ),
    );
    let home_vals: Vec<&str> = out_home_root.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(home_vals, vec!["project/"]);
}

#[test]
fn test_dotdot_navigation_missing_dir_and_path_completion_metadata() {
    let temp = TempDir::new().unwrap();
    let ws = temp.path();
    fs::create_dir_all(ws.join("packages/alpha")).unwrap();
    fs::create_dir_all(ws.join("packages/beta")).unwrap();
    fs::create_dir_all(ws.join("packages/.hidden_pkg")).unwrap();
    fs::write(ws.join("packages/alpha/file.txt"), "x").unwrap();

    let alpha_cwd = ws.join("packages/alpha");

    // 1. `..` completes to `../` as Directory with NO_SPACE | NO_FILE_COMP
    let out_dotdot = complete_path(&alpha_cwd, "..", PathMode::DirsOnly);
    let dd_vals: Vec<&str> = out_dotdot.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(dd_vals, vec!["../"]);
    assert!(out_dotdot.directive.contains(CompletionDirective::NO_SPACE));
    assert!(out_dotdot.directive.contains(CompletionDirective::NO_FILE_COMP));

    // 2. `.` completes `./`, `../`, and dot-directories in DirsOnly mode
    let out_dot = complete_path(&ws.join("packages"), ".", PathMode::DirsOnly);
    let dot_vals: Vec<&str> = out_dot.items.iter().map(|i| i.value.as_str()).collect();
    assert!(
        dot_vals.contains(&"./") && dot_vals.contains(&"../") && dot_vals.contains(&".hidden_pkg/")
    );

    // 3. `../b` from `packages/alpha` resolves parent and completes `../beta/`
    let out_parent = complete_path(&alpha_cwd, "../b", PathMode::DirsOnly);
    let parent_vals: Vec<&str> = out_parent.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(parent_vals, vec!["../beta/"]);

    // 4. Nonexistent parent dir must still set NO_FILE_COMP so DirsOnly never falls back to arbitrary files
    let out_missing = complete_path(&alpha_cwd, "no_such_dir/x", PathMode::DirsOnly);
    assert!(out_missing.items.is_empty());
    assert!(out_missing.directive.contains(CompletionDirective::NO_FILE_COMP));

    // 5. Attached/inline `-C=` and `-C` propagate PathCompletion.flag_prefix
    let root = create_sample_root_spec();
    let out_inline = complete(
        &root,
        None,
        &CompletionRequest::new(vec!["vp".into(), "-C=../b".into()], 1, alpha_cwd),
    );
    assert_eq!(out_inline.items[0].value, "-C=../beta/");
    let pc = out_inline.path_completion.expect("Must populate path_completion metadata");
    assert_eq!(pc.flag_prefix, "-C=");
    assert_eq!(pc.mode, PathMode::DirsOnly);
}

fn test_read_manifest(dir: &std::path::Path) -> Option<vp_complete::project::PkgManifest> {
    let bytes = fs::read(dir.join("package.json")).ok()?;
    let val: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let name = val.get("name").and_then(|v| v.as_str()).map(str::to_string);
    let workspaces = val.get("workspaces").and_then(|w| {
        w.as_array().map(|arr| arr.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
    });
    let scripts = val
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|m| {
            m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()
        })
        .unwrap_or_default();
    let mut deps = Vec::new();
    for (sec, label) in [("dependencies", "dep"), ("devDependencies", "devDep")] {
        if let Some(obj) = val.get(sec).and_then(|v| v.as_object()) {
            for (k, v) in obj {
                deps.push((k.clone(), v.as_str().unwrap_or("").to_string(), label));
            }
        }
    }
    Some(vp_complete::project::PkgManifest { name, workspaces, scripts, deps })
}

fn test_vp_cfg(node_store: fn() -> PathBuf) -> vp_complete::project::VpCompletionConfig {
    fn empty_dir() -> PathBuf {
        PathBuf::new()
    }
    fn none_node() -> Option<PathBuf> {
        None
    }
    fn none_bin_js(_: &std::path::Path) -> Option<PathBuf> {
        None
    }
    vp_complete::project::VpCompletionConfig {
        global_bin: empty_dir,
        node_store,
        resolve_node: none_node,
        resolve_bin_js: none_bin_js,
        read_manifest: test_read_manifest,
    }
}

#[test]
fn test_engine_optional_value_and_trailing_var_arg_and_lexer_fixes() {
    use vp_complete::project::build_vp_spec;

    fn empty_store() -> PathBuf {
        PathBuf::new()
    }

    let temp = tempfile::TempDir::new().unwrap();
    let ws = temp.path();
    fs::create_dir_all(ws.join("packages/sub")).unwrap();
    fs::write(
        ws.join("package.json"),
        r#"{"name":"root","workspaces":["packages/*"],"scripts":{"build":"vite build"}}"#,
    )
    .unwrap();
    fs::write(ws.join("packages/sub/package.json"), r#"{"name":"@demo/sub"}"#).unwrap();

    let mut raw = clap::Command::new("vp");
    let cfg = test_vp_cfg(empty_store);
    let root = build_vp_spec(&mut raw, &cfg);

    // 1. Optional flag value (`vp staged --concurrent <Tab>`, `vp staged --concurrent --al<Tab>`, `vp staged -p f<Tab>`)
    let out_opt = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "staged".into(), "--concurrent".into(), "".into()],
            3,
            ws.to_path_buf(),
        ),
    );
    let opt_vals: Vec<&str> = out_opt.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(opt_vals, vec!["true", "false"]);

    let out_opt_next_flag = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "staged".into(), "--concurrent".into(), "--al".into()],
            3,
            ws.to_path_buf(),
        ),
    );
    assert!(out_opt_next_flag.items.iter().any(|i| i.value == "--allow-empty"));

    let out_short_opt = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "staged".into(), "-p".into(), "f".into()],
            3,
            ws.to_path_buf(),
        ),
    );
    let short_opt_vals: Vec<&str> = out_short_opt.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(short_opt_vals, vec!["false"]);

    // 2. Trailing vararg stops wrapper flag parsing (`vp exec node --fi`, `vp exec node --filter `, `vp run build --fi`, while `vp lint src/ --fi` still completes)
    let out_exec_child = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "exec".into(), "node".into(), "--fi".into()],
            3,
            ws.to_path_buf(),
        ),
    );
    assert!(
        !out_exec_child.items.iter().any(|i| i.value == "--filter"),
        "vp exec node --fi must not complete wrapper --filter: {:?}",
        out_exec_child.items
    );

    let out_exec_child_val = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "exec".into(), "node".into(), "--filter".into(), "".into()],
            4,
            ws.to_path_buf(),
        ),
    );
    assert!(
        !out_exec_child_val.items.iter().any(|i| i.value == "@demo/sub"),
        "vp exec node --filter <Tab> must not suggest workspace packages: {:?}",
        out_exec_child_val.items
    );

    let out_run_child = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "run".into(), "build".into(), "--fi".into()],
            3,
            ws.to_path_buf(),
        ),
    );
    assert!(!out_run_child.items.iter().any(|i| i.value == "--filter"));

    let out_lint_flag = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "lint".into(), "src/".into(), "--fi".into()],
            3,
            ws.to_path_buf(),
        ),
    );
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
    let tilde_slash =
        vp_complete::path::complete_path_with_home(ws, Some(&home), "~//pr", PathMode::DirsOnly);
    assert_eq!(tilde_slash.items[0].value, "~//proj/");
}

#[test]
fn test_workspace_yaml_globs_ancestor_fallback_and_node_semver() {
    use vp_complete::project::{
        build_vp_spec, extract_tasks_pure_rust_with, get_workspace_package_names_with,
    };

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

    let pkgs = get_workspace_package_names_with(ws, test_read_manifest);
    assert_eq!(pkgs, vec!["@demo/core", "@demo/t1", "@demo/web"]);

    // Subdirectory `vp remove` and pure-Rust task fallback walk ancestors
    let nested_dir = ws.join("apps/web/src/nested");
    let tasks = extract_tasks_pure_rust_with(&nested_dir, "web:", test_read_manifest);
    assert_eq!(tasks.iter().map(|t| t.value.as_str()).collect::<Vec<_>>(), vec!["web:dev"]);

    let store = ws.join("node_store");
    fs::create_dir_all(store.join("v26.0.0-rc.9")).unwrap();
    fs::create_dir_all(store.join("v26.0.0-rc.10")).unwrap();
    fs::create_dir_all(store.join("v26.0.0")).unwrap();
    static STORE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let _ = STORE_DIR.set(store);
    let mut raw = clap::Command::new("vp")
        .subcommand(clap::Command::new("remove"))
        .subcommand(clap::Command::new("env").subcommand(clap::Command::new("use")));
    let root = build_vp_spec(&mut raw, &test_vp_cfg(|| STORE_DIR.get().unwrap().clone()));

    let remove_out = complete(
        &root,
        None,
        &CompletionRequest::new(vec!["vp".into(), "remove".into(), "re".into()], 2, nested_dir),
    );
    assert!(remove_out.items.iter().any(|i| i.value == "react"));

    // Node version sorting: v26.0.0-rc.10 > v26.0.0-rc.9, stable > prerelease
    let node_out = complete(
        &root,
        None,
        &CompletionRequest::new(
            vec!["vp".into(), "env".into(), "use".into(), "v26".into()],
            3,
            ws.to_path_buf(),
        ),
    );
    let vers: Vec<&str> = node_out.items.iter().map(|i| i.value.as_str()).collect();
    assert_eq!(vers, vec!["v26.0.0", "v26.0.0-rc.10", "v26.0.0-rc.9"]);
}
