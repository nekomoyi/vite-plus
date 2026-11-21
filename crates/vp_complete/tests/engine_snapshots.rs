#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_types,
    reason = "test suite uses standard test types and macros"
)]

use std::fs;

use tempfile::TempDir;
use vp_complete::{CmdSpec, CompletionDirective, ItemKind, PathMode, ValueHint, complete_path};

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

    let spec = CmdSpec::from_clap(cmd);
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
