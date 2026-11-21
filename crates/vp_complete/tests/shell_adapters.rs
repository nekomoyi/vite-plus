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
