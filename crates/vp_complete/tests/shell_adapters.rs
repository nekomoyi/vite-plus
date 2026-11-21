#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_types,
    reason = "test suite uses standard test types and macros"
)]

use std::{ffi::OsString, path::PathBuf};

use vp_complete::{
    CompletionDirective, CompletionItem, CompletionOutput, CompletionRequest, ItemKind,
    ShellDialect, ShellInputMode, dispatch_early_completion,
};

#[test]
fn test_shell_dialect_parsing() {
    assert_eq!("bash".parse::<ShellDialect>().unwrap(), ShellDialect::Bash);
    assert_eq!("BASH".parse::<ShellDialect>().unwrap(), ShellDialect::Bash);
    assert_eq!("zsh".parse::<ShellDialect>().unwrap(), ShellDialect::Zsh);
    assert_eq!("Zsh".parse::<ShellDialect>().unwrap(), ShellDialect::Zsh);
    assert_eq!("fish".parse::<ShellDialect>().unwrap(), ShellDialect::Fish);
    assert_eq!("nu".parse::<ShellDialect>().unwrap(), ShellDialect::Nu);
    assert_eq!("nushell".parse::<ShellDialect>().unwrap(), ShellDialect::Nu);

    assert!("unknown_shell".parse::<ShellDialect>().is_err());
}

#[test]
fn test_backward_compat_registration_hook_mode() {
    let unused = |_: &CompletionRequest| panic!("registration must not build command specs");
    let vp_complete::EarlyCompletionOutcome::Handled(hook) =
        dispatch_early_completion("bash".as_ref(), unused)
    else {
        panic!("expected registration hook")
    };
    assert!(hook.contains("_vp_complete"));
}

#[test]
fn test_bash_adapter_prefix_trimming_and_escaping() {
    let adapter = ShellDialect::Bash.adapter();
    let req = CompletionRequest::new(
        vec!["vp".into(), "run".into(), "build:wa".into()],
        2,
        PathBuf::from("/workspace"),
    );
    let env = |k: &str| match k {
        "COMP_WORDBREAKS" => Some(" \t\n\"'><=;|&(:".into()),
        _ => None,
    };
    let out = CompletionOutput::new(
        vec![
            CompletionItem::value("build:watch", Some("Build and watch")),
            CompletionItem::value("build:wasm", Some("Build WebAssembly")),
        ],
        CompletionDirective::DEFAULT,
    );

    let rendered = adapter.render_output(&req, &env, &out);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines[0], "watch", "Must trim 'build:' prefix from 'build:watch'");
    assert_eq!(lines[1], "wasm", "Must trim 'build:' prefix from 'build:wasm'");
    assert_eq!(lines[2], ":0", "Final line must be directive bits");
}

#[test]
fn test_bash_adapter_space_escaping() {
    let adapter = ShellDialect::Bash.adapter();
    let req = CompletionRequest::new(
        vec!["vp".into(), "-C".into(), "my".into()],
        2,
        PathBuf::from("/workspace"),
    );
    let env = |_k: &str| None;
    let out = CompletionOutput::new(
        vec![CompletionItem::directory("my dir/")],
        CompletionDirective::NO_SPACE,
    );

    let rendered = adapter.render_output(&req, &env, &out);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines[0], "my\\ dir/", "Spaces must be backslash-escaped in bash candidates");
    assert_eq!(lines[1], ":2", "NO_SPACE directive is 2");
}

#[test]
fn test_zsh_adapter_colon_escaping_and_descriptions() {
    let adapter = ShellDialect::Zsh.adapter();
    let req = CompletionRequest::new(
        vec!["vp".into(), "run".into(), "build:".into()],
        2,
        PathBuf::from("/workspace"),
    );
    let env = |_k: &str| None;
    let out = CompletionOutput::new(
        vec![
            CompletionItem::value("@vitejs/monorepo#build", Some("Build\nand\twatch")),
            CompletionItem::value("build:*", Some("All builds")),
            CompletionItem::value("plain:task", None::<String>),
        ],
        CompletionDirective::KEEP_ORDER,
    );

    let rendered = adapter.render_output(&req, &env, &out);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(
        lines[0], "V\t@vitejs/monorepo#build\t@vitejs/monorepo#build -- Build and watch",
        "Colons, @, /, and mid-word # (without EXTENDED_GLOB) must remain unescaped for compadd -Q"
    );
    assert_eq!(
        lines[1], "V\tbuild:\\*\tbuild:*                -- All builds",
        "Wildcard * must be escaped in match while display stays clean and padded"
    );
    assert_eq!(lines[2], "v\tplain:task\tplain:task");
    assert_eq!(lines[3], ":32", "KEEP_ORDER directive is 32");

    // With caller EXTENDED_GLOB=on, mid-word `#` and `^` ARE escaped
    let env_extglob = |k: &str| (k == "_VP_EXTGLOB").then(|| "on".into());
    let rendered_eg = adapter.render_output(&req, &env_extglob, &out);
    let eg_first = rendered_eg.lines().next().unwrap();
    assert!(
        eg_first.starts_with("V\t@vitejs/monorepo\\#build\t"),
        "EXTENDED_GLOB=on must escape mid-word #: got {eg_first}"
    );

    // Inside open quotes (_VP_COMP_QUOTE="\""), matches are emitted raw so Zsh handles quote closing
    let env_quoted = |k: &str| (k == "_VP_COMP_QUOTE").then(|| "\"".into());
    let rendered_q = adapter.render_output(&req, &env_quoted, &out);
    let q_lines: Vec<&str> = rendered_q.lines().collect();
    assert!(
        q_lines[1].starts_with("V\tbuild:*\t"),
        "Inside quotes, Zsh match must not backslash-escape *: got {}",
        q_lines[1]
    );
}

#[test]
fn test_zsh_adapter_parse_input_padding() {
    let adapter = ShellDialect::Zsh.adapter();
    let cwd = PathBuf::from("/workspace");
    // Simulate `vp run <Tab>` where zsh passed words up to 2 without trailing empty string
    let argv = vec![
        OsString::from("vp"),
        OsString::from("--"),
        OsString::from("vp"),
        OsString::from("run"),
    ];
    let env = |k: &str| match k {
        "VP_COMPLETE" => Some("zsh".into()),
        "_VP_COMPLETE_INDEX" => Some("2".into()),
        _ => None,
    };

    match adapter.parse_input(&env, &argv, &cwd) {
        ShellInputMode::Complete(req) => {
            assert_eq!(req.words.len(), 3, "Must pad missing trailing word");
            assert_eq!(req.words[2], "");
            assert_eq!(req.current_index, 2);
        }
        ShellInputMode::RegistrationHook => panic!("Expected Complete mode"),
    }
}

#[test]
fn test_fish_adapter_tsv_output() {
    let adapter = ShellDialect::Fish.adapter();
    let req = CompletionRequest::new(
        vec!["vp".into(), "run".into(), "b".into()],
        2,
        PathBuf::from("/workspace"),
    );
    let env = |_k: &str| None;
    let out = CompletionOutput::new(
        vec![
            CompletionItem::value("build:watch", Some("Build and watch")),
            CompletionItem::value("build", None::<String>),
        ],
        CompletionDirective::DEFAULT,
    );

    let rendered = adapter.render_output(&req, &env, &out);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines[0], "build:watch\tBuild and watch");
    assert_eq!(lines[1], "build");
    assert_eq!(lines.len(), 2, "Fish does NOT emit trailing :directive line");
}

#[test]
fn test_nu_adapter_json_output() {
    let adapter = ShellDialect::Nu.adapter();
    let req = CompletionRequest::new(
        vec!["vp".into(), "run".into(), "".into()],
        2,
        PathBuf::from("/workspace"),
    );
    let env = |_k: &str| None;
    let out = CompletionOutput::new(
        vec![
            CompletionItem::value("build", Some("Build project")),
            CompletionItem::directory("my dir/"),
        ],
        CompletionDirective::DEFAULT,
    );

    let rendered = adapter.render_output(&req, &env, &out);
    let parsed: serde_json::Value =
        serde_json::from_str(&rendered).expect("Nu adapter must output valid JSON");
    let arr = parsed.as_array().expect("Must be JSON array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["value"], "build");
    assert_eq!(arr[0]["description"], "Build project");
    assert_eq!(arr[1]["value"], "\"my dir/\"");
    assert!(arr[1].get("description").is_none(), "None description must be omitted in JSON");
}

#[test]
fn test_shell_adapter_snapshots() {
    let req = CompletionRequest::new(
        vec!["vp".into(), "run".into(), "build:wa".into()],
        2,
        PathBuf::from("/workspace"),
    );
    let env = |k: &str| match k {
        "COMP_WORDBREAKS" => Some(" \t\n\"'><=;|&(:".into()),
        _ => None,
    };
    let out = CompletionOutput::new(
        vec![
            CompletionItem::new("build:watch", Some("Build and watch"), ItemKind::Value),
            CompletionItem::new("my dir/", None::<String>, ItemKind::Directory),
            CompletionItem::new("-flag-file.ts", Some("TypeScript file"), ItemKind::File),
            CompletionItem::new(
                "special#target",
                Some("Special target #1\nwith newline"),
                ItemKind::Value,
            ),
        ],
        CompletionDirective::NO_SPACE,
    );

    let bash_out = ShellDialect::Bash.adapter().render_output(&req, &env, &out);
    insta::assert_snapshot!("bash_adapter_rendered_output", bash_out);

    let zsh_out = ShellDialect::Zsh.adapter().render_output(&req, &env, &out);
    insta::assert_snapshot!("zsh_adapter_rendered_output", zsh_out);

    let fish_out = ShellDialect::Fish.adapter().render_output(&req, &env, &out);
    insta::assert_snapshot!("fish_adapter_rendered_output", fish_out);

    let nu_out = ShellDialect::Nu.adapter().render_output(&req, &env, &out);
    insta::assert_snapshot!("nu_adapter_rendered_output", nu_out);
}

#[test]
fn test_fish_hook_hardened_builtins() {
    let hook = ShellDialect::Fish.adapter().render_hook();
    assert!(
        hook.contains("builtin commandline -opc"),
        "Fish hook must use 'builtin commandline -opc'"
    );
    assert!(
        hook.contains("builtin commandline -ct"),
        "Fish hook must use 'builtin commandline -ct'"
    );
    assert!(hook.contains("builtin count $tokens"), "Fish hook must use 'builtin count $tokens'");
    assert!(
        hook.contains("command \"$bin\""),
        "Fish hook must invoke binary using 'command \"$bin\"'"
    );
    assert!(
        hook.contains("builtin complete -c vp --keep-order"),
        "Fish hook must use 'builtin complete -c vp --keep-order'"
    );
    assert!(
        hook.contains("builtin complete -c vpr --keep-order"),
        "Fish hook must use 'builtin complete -c vpr --keep-order'"
    );
    assert!(
        hook.contains("builtin complete -c vpx --keep-order"),
        "Fish hook must use 'builtin complete -c vpx --keep-order'"
    );
}

#[test]
fn test_nu_adapter_comp_line_fallback() {
    let adapter = ShellDialect::Nu.adapter();
    let cwd = PathBuf::from("/workspace");
    let argv = vec![OsString::from("vp")];
    let env = |k: &str| match k {
        "VP_COMPLETE" => Some("nu".into()),
        "COMP_LINE" => Some("vp run build".into()),
        "COMP_POINT" => Some("12".into()),
        _ => None,
    };

    match adapter.parse_input(&env, &argv, &cwd) {
        ShellInputMode::Complete(req) => {
            assert_eq!(req.words, vec!["vp", "run", "build"]);
            assert_eq!(req.current_index, 2);
            assert_eq!(req.cwd, cwd);
        }
        ShellInputMode::RegistrationHook => {
            panic!("Expected Complete mode via COMP_LINE/COMP_POINT fallback");
        }
    }
}

#[test]
fn test_nu_hook_uses_modern_custom_completer() {
    let hook = ShellDialect::Nu.adapter().render_hook();
    assert!(
        hook.contains("def \"nu-complete vp\" [buffer: string, place: any]"),
        "NuAdapter::render_hook must emit modern record-aware custom completer, got: {hook}"
    );
    assert!(
        !hook.contains("{|spans|"),
        "NuAdapter::render_hook must not emit Nushell 0.116-deprecated single-arg external completer"
    );
}

#[test]
fn test_bash_escaped_and_quoted_paths() {
    let adapter = ShellDialect::Bash.adapter();

    // 1. Backslash-escaped space in unquoted path: `vp node my\ dir/fi`
    let line1 = "vp node my\\ dir/fi";
    let env1 = |k: &str| match k {
        "_VP_COMP_LINE" => Some(line1.into()),
        "_VP_COMP_POINT" => Some(line1.len().to_string()),
        "_VP_COMP_WORDBREAKS" => Some(" \t\n\"'><=;|&(:".into()),
        _ => None,
    };
    let ShellInputMode::Complete(req1) = adapter.parse_input(&env1, &[], &PathBuf::from("/ws"))
    else {
        panic!("Expected Complete");
    };
    assert_eq!(req1.words, vec!["vp", "node", "my dir/fi"]);
    let out1 = CompletionOutput::new(
        vec![CompletionItem::new("my dir/file.js", None::<String>, ItemKind::File)],
        CompletionDirective::DEFAULT,
    );
    let rendered1 = adapter.render_output(&req1, &env1, &out1);
    let lines1: Vec<&str> = rendered1.lines().collect();
    assert_eq!(lines1[0], "my\\ dir/file.js");

    // 2. Open double quote: `vp node "my dir/fi` -> spaces NOT escaped, `$` escaped
    let line2 = "vp node \"my dir/fi";
    let env2 = |k: &str| match k {
        "_VP_COMP_LINE" => Some(line2.into()),
        "_VP_COMP_POINT" => Some(line2.len().to_string()),
        "_VP_COMP_WORDBREAKS" => Some(" \t\n\"'><=;|&(:".into()),
        _ => None,
    };
    let ShellInputMode::Complete(req2) = adapter.parse_input(&env2, &[], &PathBuf::from("/ws"))
    else {
        panic!("Expected Complete");
    };
    let out2 = CompletionOutput::new(
        vec![CompletionItem::new("my dir/file$1.js", None::<String>, ItemKind::File)],
        CompletionDirective::DEFAULT,
    );
    let rendered2 = adapter.render_output(&req2, &env2, &out2);
    let lines2: Vec<&str> = rendered2.lines().collect();
    assert_eq!(lines2[0], "my dir/file\\$1.js");

    // 3. Open single quote: `vp node 'my dir/fi` -> `$` NOT escaped, `'` escaped as `'\''`
    let line3 = "vp node 'my dir/fi";
    let env3 = |k: &str| match k {
        "_VP_COMP_LINE" => Some(line3.into()),
        "_VP_COMP_POINT" => Some(line3.len().to_string()),
        "_VP_COMP_WORDBREAKS" => Some(" \t\n\"'><=;|&(:".into()),
        _ => None,
    };
    let ShellInputMode::Complete(req3) = adapter.parse_input(&env3, &[], &PathBuf::from("/ws"))
    else {
        panic!("Expected Complete");
    };
    let out3 = CompletionOutput::new(
        vec![CompletionItem::new("my dir/it's$1.js", None::<String>, ItemKind::File)],
        CompletionDirective::DEFAULT,
    );
    let rendered3 = adapter.render_output(&req3, &env3, &out3);
    let lines3: Vec<&str> = rendered3.lines().collect();
    assert_eq!(lines3[0], "my dir/it'\\''s$1.js");
}

#[test]
fn test_zsh_fish_nu_parse_quoted_inputs() {
    let cwd = PathBuf::from("/workspace");

    // Zsh unquotes all words
    let zsh_argv = vec![
        OsString::from("vp"),
        OsString::from("--"),
        OsString::from("vp"),
        OsString::from("-C"),
        OsString::from("\"packages/sub pkg\""),
        OsString::from("\"my dir/fi"),
    ];
    let zsh_env = |k: &str| match k {
        "_VP_COMPLETE_INDEX" => Some("3".into()),
        _ => None,
    };
    let ShellInputMode::Complete(zsh_req) =
        ShellDialect::Zsh.adapter().parse_input(&zsh_env, &zsh_argv, &cwd)
    else {
        panic!("Expected Zsh Complete");
    };
    assert_eq!(zsh_req.words, vec!["vp", "-C", "packages/sub pkg", "my dir/fi"]);

    // Fish preserves already-unquoted prior tokens and unquotes only words[idx]
    let fish_argv = vec![
        OsString::from("vp"),
        OsString::from("--"),
        OsString::from("vp"),
        OsString::from("it's-already-unquoted"),
        OsString::from("\"my dir/fi"),
    ];
    let fish_env = |k: &str| match k {
        "_VP_COMPLETE_INDEX" => Some("2".into()),
        _ => None,
    };
    let ShellInputMode::Complete(fish_req) =
        ShellDialect::Fish.adapter().parse_input(&fish_env, &fish_argv, &cwd)
    else {
        panic!("Expected Fish Complete");
    };
    assert_eq!(fish_req.words, vec!["vp", "it's-already-unquoted", "my dir/fi"]);

    // Nushell unquotes double, single, backtick, and raw quotes across spans
    let nu_argv = vec![
        OsString::from("vp"),
        OsString::from("--"),
        OsString::from("vp"),
        OsString::from("-C"),
        OsString::from("\"packages/sub pkg\""),
        OsString::from("`my dir/fi"),
    ];
    let nu_env = |k: &str| match k {
        "_VP_COMPLETE_INDEX" => Some("3".into()),
        _ => None,
    };
    let ShellInputMode::Complete(nu_req) =
        ShellDialect::Nu.adapter().parse_input(&nu_env, &nu_argv, &cwd)
    else {
        panic!("Expected Nu Complete");
    };
    assert_eq!(nu_req.words, vec!["vp", "-C", "packages/sub pkg", "my dir/fi"]);
}

#[test]
fn test_nu_apostrophe_and_tilde_space_quoting() {
    let adapter = ShellDialect::Nu.adapter();
    let req = CompletionRequest::new(
        vec!["vp".into(), "node".into(), "~/".into()],
        2,
        PathBuf::from("/workspace"),
    );
    let env = |k: &str| match k {
        "HOME" => Some("/Users/alice".into()),
        _ => None,
    };
    let out = CompletionOutput::new(
        vec![
            CompletionItem::new("abc'def", None::<String>, ItemKind::File),
            CompletionItem::directory("~/Library/Application Support/"),
            CompletionItem::directory("~/Documents/"),
        ],
        CompletionDirective::NO_SPACE,
    );

    let rendered = adapter.render_output(&req, &env, &out);
    let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr[0]["value"], "\"abc'def\"");
    assert_eq!(arr[1]["value"], "\"/Users/alice/Library/Application Support/\"");
    assert_eq!(arr[2]["value"], "~/Documents/");
}

#[test]
fn test_bash_escapes_braces_and_bang_safely() {
    let adapter = ShellDialect::Bash.adapter();
    let out = CompletionOutput::new(
        vec![
            CompletionItem::new("name{one,two}.js", None::<String>, ItemKind::File),
            CompletionItem::new("hello!world.js", None::<String>, ItemKind::File),
            CompletionItem::new("!lead.js", None::<String>, ItemKind::File),
            CompletionItem::new("world!", None::<String>, ItemKind::File),
            CompletionItem::new("nefar\"", None::<String>, ItemKind::File),
            CompletionItem::new("nefar'", None::<String>, ItemKind::File),
        ],
        CompletionDirective::NO_FILE_COMP,
    );

    // Unquoted: braces and ! must be backslash-escaped
    let render = |line: &str, point: usize, version: &str| {
        let env = |k: &str| match k {
            "_VP_COMP_LINE" => Some(line.to_owned()),
            "_VP_COMP_POINT" => Some(point.to_string()),
            "_VP_BASH_V" => Some(version.to_owned()),
            _ => None,
        };
        let ShellInputMode::Complete(req) = adapter.parse_input(&env, &[], "/w".as_ref()) else {
            panic!("expected completion for {line:?}");
        };
        adapter.render_output(&req, &env, &out)
    };
    let unquoted_line = "vp node ";
    let rendered_unquoted = render(unquoted_line, unquoted_line.len(), "5");
    let lines_unquoted: Vec<&str> = rendered_unquoted.lines().collect();
    assert_eq!(lines_unquoted[0], r"name\{one,two\}.js");
    assert_eq!(lines_unquoted[1], r"hello\!world.js");

    // Double-quoted: braces stay literal, ! switches to single-quoted '!', trailing " appends
    // closing " so Readline's last-char check does not leave the argument unclosed.
    let dquoted_line = "vp node \"";
    let rendered_dquoted = render(dquoted_line, dquoted_line.len(), "5");
    let lines_dquoted: Vec<&str> = rendered_dquoted.lines().collect();
    assert_eq!(lines_dquoted[0], "name{one,two}.js");
    assert_eq!(lines_dquoted[1], "hello\"'!'\"world.js");
    assert_eq!(lines_dquoted[2], "\"\"'!'\"lead.js");
    assert_eq!(lines_dquoted[3], "world\"'!'\"\"");
    assert_eq!(lines_dquoted[4], "nefar\\\"\"");
    assert_eq!(lines_dquoted[5], "nefar'");

    // Single-quoted: trailing ' appends closing ' so Readline does not leave 'nefar'\'' unclosed.
    let squoted_line = "vp node '";
    let rendered_squoted = render(squoted_line, squoted_line.len(), "5");
    let lines_squoted: Vec<&str> = rendered_squoted.lines().collect();
    assert_eq!(lines_squoted[4], "nefar\"");
    assert_eq!(lines_squoted[5], "nefar'\\'''");

    // Cursor before existing closing quote: Bash 3.2 (_VP_BASH_V=3) keeps existing closing quote
    // at COMP_POINT, while Bash 5 (_VP_BASH_V=5) deletes and re-inserts it.
    let preclose_line = "vp node \"nefar\"";
    let preclose_point = preclose_line.len() - 1;
    let rendered_b3 = render(preclose_line, preclose_point, "3");
    let b3_lines: Vec<&str> = rendered_b3.lines().collect();
    assert_eq!(b3_lines[3], "world\"'!'\"");
    assert_eq!(b3_lines[4], "nefar\\\"");

    let rendered_b5 = render(preclose_line, preclose_point, "5");
    let b5_lines: Vec<&str> = rendered_b5.lines().collect();
    assert_eq!(b5_lines[3], "world\"'!'\"\"");
    assert_eq!(b5_lines[4], "nefar\\\"\"");
}

#[test]
fn test_bash_leading_env_assignments_stripped() {
    let cwd = PathBuf::from("/workspace");
    let argv = vec![OsString::from("vp"), OsString::from("--")];

    for (line, expected_words, expected_idx) in [
        ("CI=1 vp unins", vec!["vp", "unins"], 1),
        ("FOO=\"a b\" BAR+=baz vpr bu", vec!["vpr", "bu"], 1),
        ("FOO=$(git rev-parse HEAD) vp unins", vec!["vp", "unins"], 1),
        ("FOO=$(echo \"a ) b\" $(bar)) BAR+=`id -u` vp run dev", vec!["vp", "run", "dev"], 2),
        ("FOO=\"$(echo \"inner quote\" arg)\" vp unins", vec!["vp", "unins"], 1),
        ("FOO=$(echo \"$(printf \"(\")\") vp run bu", vec!["vp", "run", "bu"], 2),
        ("FOO=\"`printf '\"'`\" vp run bu", vec!["vp", "run", "bu"], 2),
        ("echo \"`vp run bu", vec!["vp", "run", "bu"], 2),
        ("echo $(vp unins", vec!["vp", "unins"], 1),
        ("(vp unins", vec!["vp", "unins"], 1),
        ("echo ok; CI=1 vp run dev", vec!["vp", "run", "dev"], 2),
        ("vp run FOO=bar", vec!["vp", "run", "FOO=bar"], 2),
        ("\"CI=1\" vp unins", vec!["CI=1", "vp", "unins"], 2),
        ("CI=1", vec!["CI=1"], 0),
        ("CI=1 ", vec![""], 0),
    ] {
        let env = |k: &str| match k {
            "_VP_COMP_LINE" => Some(line.to_string()),
            "_VP_COMP_POINT" => Some(line.len().to_string()),
            _ => None,
        };
        let ShellInputMode::Complete(req) =
            ShellDialect::Bash.adapter().parse_input(&env, &argv, &cwd)
        else {
            panic!("Expected Bash Complete for {line}");
        };
        assert_eq!(req.words, expected_words, "words mismatch for {line:?}");
        assert_eq!(req.current_index, expected_idx, "index mismatch for {line:?}");
    }
}

#[test]
fn test_bash_quoted_attached_flag_value_trims_flag_prefix() {
    let adapter = ShellDialect::Bash.adapter();
    let cwd = PathBuf::from("/w");
    let argv = vec![OsString::from("vp"), OsString::from("--")];

    for (line, full_candidate, expected_rendered) in [
        ("vp -C=\"pro", "-C=project space/", "project space/"),
        ("vp -C\"pro", "-Cproject space/", "project space/"),
        ("vp -C='pro", "-C=project space/", "project space/"),
        ("vp dev --config=\"vite", "--config=vite.config.ts", "vite.config.ts"),
        ("vp node \"nefar\\\"/\"chi", "nefar\"/child.js", "nefar\\\"/child.js"),
        ("vp dev --config=\"nefar\\\"/\"chi", "--config=nefar\"/child.ts", "nefar\\\"/child.ts"),
    ] {
        let env = |k: &str| match k {
            "_VP_COMP_LINE" => Some(line.to_string()),
            "_VP_COMP_POINT" => Some(line.len().to_string()),
            _ => None,
        };
        let ShellInputMode::Complete(req) =
            ShellDialect::Bash.adapter().parse_input(&env, &argv, &cwd)
        else {
            panic!("Expected Bash Complete for {line}");
        };
        let out = CompletionOutput::new(
            vec![CompletionItem::file(full_candidate)],
            CompletionDirective::NO_FILE_COMP,
        );
        let rendered = adapter.render_output(&req, &env, &out);
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines[0], expected_rendered, "mismatch for {line:?}");
    }
}
