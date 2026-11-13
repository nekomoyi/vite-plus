#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_types,
    reason = "test suite uses standard test types and macros"
)]

use std::{ffi::OsString, path::PathBuf};

use vp_complete::{
    CompletionDirective, CompletionItem, CompletionOutput, CompletionRequest, ItemKind,
    ShellDialect, ShellInputMode, try_handle_env_completion,
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
    assert_eq!("powershell".parse::<ShellDialect>().unwrap(), ShellDialect::PowerShell);
    assert_eq!("pwsh".parse::<ShellDialect>().unwrap(), ShellDialect::PowerShell);
    assert_eq!("PWSH".parse::<ShellDialect>().unwrap(), ShellDialect::PowerShell);

    assert!("unknown_shell".parse::<ShellDialect>().is_err());
}

#[test]
fn test_backward_compat_registration_hook_mode() {
    let cwd = PathBuf::from("/workspace");
    let argv = vec![OsString::from("vp")];
    let env = |k: &str| match k {
        "VP_COMPLETE" => Some("bash".into()),
        _ => None,
    };

    let result = try_handle_env_completion(&env, &argv, &cwd, || {
        panic!("build_specs must not be called in registration hook mode")
    });
    assert!(result.is_some(), "Must return hook script");
    assert!(
        result.unwrap().contains("_vp_complete"),
        "Hook script must contain completion function"
    );
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
fn test_pwsh_adapter_quoting_and_flags() {
    let adapter = ShellDialect::PowerShell.adapter();
    let req = CompletionRequest::new(
        vec!["vp".into(), "test".into(), "".into()],
        2,
        PathBuf::from("/workspace"),
    );
    let env = |_k: &str| None;
    let out = CompletionOutput::new(
        vec![
            CompletionItem::new("my file.txt", Some("file desc"), ItemKind::File),
            CompletionItem::new("-strange-name.js", Some("flag like file"), ItemKind::File),
            CompletionItem::new("special#char", Some("hash tag"), ItemKind::Value),
        ],
        CompletionDirective::NO_FILE_COMP,
    );

    let rendered = adapter.render_output(&req, &env, &out);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines.len(), 4, "3 items + 1 directive line");
    assert!(lines[0].starts_with("'my file.txt'\t"));
    assert!(
        lines[1].starts_with(".\\-strange-name.js\t")
            || lines[1].starts_with("./-strange-name.js\t"),
        "Files starting with '-' must be prefixed with .\\ or ./: got '{}'",
        lines[1]
    );
    assert!(lines[2].starts_with("'special#char'\t"), "# must trigger single quoting");
    assert_eq!(lines[3], ":4", "NO_FILE_COMP is 4");

    // Verify hook has zero Invoke-Expression
    let hook = adapter.render_hook();
    assert!(
        !hook.contains("Invoke-Expression") && !hook.contains("iex "),
        "Pwsh hook must never contain Invoke-Expression"
    );
    assert!(hook.contains("TabExpansion2"), "Pwsh hook must define TabExpansion2");
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

    let pwsh_out = ShellDialect::PowerShell.adapter().render_output(&req, &env, &out);
    insta::assert_snapshot!("pwsh_adapter_rendered_output", pwsh_out);
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
fn test_pwsh_hook_script_input_set_ast_isolation_and_exit_code() {
    let adapter = ShellDialect::PowerShell.adapter();
    let hook = adapter.render_hook();

    // 1. ScriptInputSet must forward through MapStringInputToParsedInput -> TabExpansion2 -ast
    assert!(
        hook.contains("MapStringInputToParsedInput"),
        "ScriptInputSet must parse via MapStringInputToParsedInput and forward to AstInputSet"
    );
    assert!(
        hook.contains("[AllowEmptyString()]"),
        "ScriptInputSet $inputScript must allow empty strings"
    );
    assert!(
        hook.contains("$inputScript.Length"),
        "$cursorColumn must default to $inputScript.Length"
    );

    // 2. Innermost CommandAst selection + pipeline/separator boundary check
    assert!(
        hook.contains("FindAll(") && hook.contains("Select-Object -Last 1"),
        "PowerShell hook must select innermost CommandAst via FindAll + Select-Object -Last 1"
    );

    // 3. Preserve $global:LASTEXITCODE across native completion invocation
    assert!(
        hook.contains("LASTEXITCODE"),
        "PowerShell hook must save and restore $global:LASTEXITCODE"
    );

    // 4. Register-ArgumentCompleter -Native must actually invoke completion helper (not empty body)
    let (_, after_reg) = hook
        .split_once("Register-ArgumentCompleter")
        .expect("Must contain Register-ArgumentCompleter");
    assert!(
        after_reg.contains("__vp_complete_ast"),
        "Register-ArgumentCompleter -Native script block must invoke __vp_complete_ast, got: {after_reg}"
    );

    // 5. Empty/whitespace item value/tooltip must pad with single space ' ' to avoid PS 7.6 ArgumentNullException
    let req = CompletionRequest::new(vec!["vp".into(), "".into()], 1, PathBuf::from("/workspace"));
    let out = CompletionOutput::new(
        vec![CompletionItem::new("", None::<String>, ItemKind::Value)],
        CompletionDirective::NO_FILE_COMP,
    );
    let rendered = adapter.render_output(&req, &|_| None, &out);
    let first_line = rendered.lines().next().unwrap();
    let cols: Vec<&str> = first_line.split('\t').collect();
    assert_eq!(cols.len(), 4);
    assert!(!cols[1].is_empty(), "listItemText must never be empty");
    assert!(!cols[3].is_empty(), "toolTip must never be empty");
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
        ShellDialect::Zsh.parse_input(&zsh_env, &zsh_argv, &cwd)
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
        ShellDialect::Fish.parse_input(&fish_env, &fish_argv, &cwd)
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
    let ShellInputMode::Complete(nu_req) = ShellDialect::Nu.parse_input(&nu_env, &nu_argv, &cwd)
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
fn test_pwsh_special_operator_quoting_and_extension_guard() {
    let adapter = ShellDialect::PowerShell.adapter();
    let req = CompletionRequest::new(vec!["vp".into(), "".into()], 1, PathBuf::from("/workspace"));
    let out = CompletionOutput::new(
        vec![
            CompletionItem::value("ab`cd", None::<String>),
            CompletionItem::value("a,b", None::<String>),
            CompletionItem::value("@scope/pkg", None::<String>),
            CompletionItem::value("arr[0]", None::<String>),
            CompletionItem::value("a<b", None::<String>),
            CompletionItem::value("a>b", None::<String>),
        ],
        CompletionDirective::DEFAULT,
    );

    let rendered = adapter.render_output(&req, &|_| None, &out);
    let lines: Vec<&str> = rendered.lines().collect();
    assert!(lines[0].starts_with("'ab`cd'\t"), "got: {}", lines[0]);
    assert!(lines[1].starts_with("'a,b'\t"), "got: {}", lines[1]);
    assert!(lines[2].starts_with("'@scope/pkg'\t"), "got: {}", lines[2]);
    assert!(lines[3].starts_with("'arr[0]'\t"), "got: {}", lines[3]);
    assert!(lines[4].starts_with("'a<b'\t"), "got: {}", lines[4]);
    assert!(lines[5].starts_with("'a>b'\t"), "got: {}", lines[5]);

    let hook = adapter.render_hook();
    assert!(
        !hook.contains("GetFileNameWithoutExtension"),
        "PowerShell hook must not strip arbitrary extensions like vp.txt"
    );
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
    let unquoted_line = "vp node ";
    let req_unquoted =
        CompletionRequest::new(vec!["vp".into(), "node".into(), "".into()], 2, PathBuf::from("/w"));
    let env_unquoted = |k: &str| match k {
        "_VP_COMP_LINE" => Some(unquoted_line.to_string()),
        "_VP_COMP_POINT" => Some(unquoted_line.len().to_string()),
        _ => None,
    };
    let rendered_unquoted = adapter.render_output(&req_unquoted, &env_unquoted, &out);
    let lines_unquoted: Vec<&str> = rendered_unquoted.lines().collect();
    assert_eq!(lines_unquoted[0], r"name\{one,two\}.js");
    assert_eq!(lines_unquoted[1], r"hello\!world.js");

    // Double-quoted: braces stay literal, ! switches to single-quoted '!', trailing " appends
    // closing " so Readline's last-char check does not leave the argument unclosed.
    let dquoted_line = "vp node \"";
    let req_dquoted =
        CompletionRequest::new(vec!["vp".into(), "node".into(), "".into()], 2, PathBuf::from("/w"));
    let env_dquoted = |k: &str| match k {
        "_VP_COMP_LINE" => Some(dquoted_line.to_string()),
        "_VP_COMP_POINT" => Some(dquoted_line.len().to_string()),
        _ => None,
    };
    let rendered_dquoted = adapter.render_output(&req_dquoted, &env_dquoted, &out);
    let lines_dquoted: Vec<&str> = rendered_dquoted.lines().collect();
    assert_eq!(lines_dquoted[0], "name{one,two}.js");
    assert_eq!(lines_dquoted[1], "hello\"'!'\"world.js");
    assert_eq!(lines_dquoted[2], "\"\"'!'\"lead.js");
    assert_eq!(lines_dquoted[3], "world\"'!'\"\"");
    assert_eq!(lines_dquoted[4], "nefar\\\"\"");
    assert_eq!(lines_dquoted[5], "nefar'");

    // Single-quoted: trailing ' appends closing ' so Readline does not leave 'nefar'\'' unclosed.
    let squoted_line = "vp node '";
    let env_squoted = |k: &str| match k {
        "_VP_COMP_LINE" => Some(squoted_line.to_string()),
        "_VP_COMP_POINT" => Some(squoted_line.len().to_string()),
        _ => None,
    };
    let rendered_squoted = adapter.render_output(&req_dquoted, &env_squoted, &out);
    let lines_squoted: Vec<&str> = rendered_squoted.lines().collect();
    assert_eq!(lines_squoted[4], "nefar\"");
    assert_eq!(lines_squoted[5], "nefar'\\'''");

    // Cursor before existing closing quote: Bash 3.2 (_VP_BASH_V=3) keeps existing closing quote
    // at COMP_POINT, while Bash 5 (_VP_BASH_V=5) deletes and re-inserts it.
    let preclose_line = "vp node \"nefar\"";
    let preclose_point = preclose_line.len() - 1;
    let req_preclose = CompletionRequest::new(
        vec!["vp".into(), "node".into(), "nefar".into()],
        2,
        PathBuf::from("/w"),
    );
    let rendered_b3 = adapter.render_output(
        &req_preclose,
        &|k| match k {
            "_VP_COMP_LINE" => Some(preclose_line.into()),
            "_VP_COMP_POINT" => Some(preclose_point.to_string()),
            "_VP_BASH_V" => Some("3".into()),
            _ => None,
        },
        &out,
    );
    let b3_lines: Vec<&str> = rendered_b3.lines().collect();
    assert_eq!(b3_lines[3], "world\"'!'\"");
    assert_eq!(b3_lines[4], "nefar\\\"");

    let rendered_b5 = adapter.render_output(
        &req_preclose,
        &|k| match k {
            "_VP_COMP_LINE" => Some(preclose_line.into()),
            "_VP_COMP_POINT" => Some(preclose_point.to_string()),
            "_VP_BASH_V" => Some("5".into()),
            _ => None,
        },
        &out,
    );
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
        let ShellInputMode::Complete(req) = ShellDialect::Bash.parse_input(&env, &argv, &cwd)
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
        let ShellInputMode::Complete(req) = ShellDialect::Bash.parse_input(&env, &argv, &cwd)
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

#[test]
fn test_powershell_preserves_attached_flag_prefixes_while_escaping_dash_paths() {
    let adapter = ShellDialect::PowerShell.adapter();
    let req = CompletionRequest::new(vec!["vp".into(), "-Cpro".into()], 1, PathBuf::from("/w"));

    let mut attached_out = CompletionOutput::new(
        vec![CompletionItem::directory("-Cproject/"), CompletionItem::directory("-C=project/")],
        CompletionDirective::NO_SPACE | CompletionDirective::NO_FILE_COMP,
    );
    attached_out.path_completion = Some(vp_complete::PathCompletion {
        mode: vp_complete::PathMode::DirsOnly,
        flag_prefix: "-C".into(),
    });
    let rendered_attached = adapter.render_output(&req, &|_| None, &attached_out);
    let attached_lines: Vec<&str> = rendered_attached.lines().collect();
    assert!(attached_lines[0].starts_with("-Cproject/\t"));
    assert!(attached_lines[1].starts_with("-C=project/\t"));

    let mut standalone_out = CompletionOutput::new(
        vec![CompletionItem::directory("-dashdir/")],
        CompletionDirective::NO_SPACE | CompletionDirective::NO_FILE_COMP,
    );
    standalone_out.path_completion = Some(vp_complete::PathCompletion {
        mode: vp_complete::PathMode::DirsOnly,
        flag_prefix: String::new(),
    });
    let rendered_standalone = adapter.render_output(&req, &|_| None, &standalone_out);
    assert!(rendered_standalone.starts_with("./-dashdir/\t"));
}

#[test]
fn test_zsh_files_directive_and_effective_cwd_file_fallback() {
    let temp = tempfile::TempDir::new().unwrap();
    let other_cwd = temp.path().join("other");
    let sub_cwd = temp.path().join("packages/sub");
    std::fs::create_dir_all(&other_cwd).unwrap();
    std::fs::create_dir_all(&sub_cwd).unwrap();
    std::fs::write(sub_cwd.join("target_file.mjs"), "export {};\n").unwrap();

    // 1. Zsh PathCompletion renders native `__vp_files` directive with mode, prefix, extensions, and base dir
    let mut path_out = CompletionOutput::new(
        vec![CompletionItem::directory("-C=packages/")],
        CompletionDirective::NO_SPACE | CompletionDirective::NO_FILE_COMP,
    );
    path_out.path_completion = Some(vp_complete::PathCompletion {
        mode: vp_complete::PathMode::FilesWithExtensions(vec![".ts".into(), "mjs".into()]),
        flag_prefix: "-C=".into(),
    });
    path_out.effective_cwd = Some(sub_cwd.clone());

    let req = CompletionRequest::new(vec!["vp".into(), "-C=tar".into()], 1, other_cwd.clone());
    let zsh_rendered = ShellDialect::Zsh.adapter().render_output(&req, &|_| None, &path_out);
    assert!(
        zsh_rendered.starts_with("__vp_files\nm:ext\np:-C=\nd:")
            && zsh_rendered.contains("\ng:ts|mjs\n:6"),
        "Unexpected Zsh __vp_files output: {zsh_rendered}"
    );

    // 2. Empty output without NO_FILE_COMP uses `effective_cwd` across all 5 shells
    let mut fallback_out = CompletionOutput::new(vec![], CompletionDirective::DEFAULT);
    fallback_out.effective_cwd = Some(sub_cwd.clone());
    let fb_req = CompletionRequest::new(
        vec![
            "vp".into(),
            "-C".into(),
            "../packages/sub".into(),
            "passthrough".into(),
            "tar".into(),
        ],
        4,
        other_cwd,
    );
    for dialect in
        [ShellDialect::Bash, ShellDialect::Fish, ShellDialect::Nu, ShellDialect::PowerShell]
    {
        let rendered = dialect.adapter().render_output(&fb_req, &|_| None, &fallback_out);
        assert!(
            rendered.contains("target_file.mjs"),
            "{dialect:?} fallback must resolve against effective_cwd, got: {rendered}"
        );
    }
    let zsh_fb = ShellDialect::Zsh.adapter().render_output(&fb_req, &|_| None, &fallback_out);
    assert!(
        zsh_fb.starts_with("__vp_files\nm:any\nd:")
            && zsh_fb.contains(sub_cwd.to_string_lossy().as_ref()),
        "Zsh fallback with effective_cwd must emit __vp_files with d:<effective_cwd>, got: {zsh_fb}"
    );
}

#[test]
fn test_pwsh_raw_cur_unquoting() {
    let cwd = PathBuf::from("/workspace");
    let cases = [
        ("'project''s dir/sub", "project's dir/sub"),
        ("'closed''quote'", "closed'quote"),
        ("\"double`\"quote", "double\"quote"),
        ("\"closed`\"quote\"", "closed\"quote"),
        ("backtick` space", "backtick space"),
        ("--config=\"vite` config", "--config=vite config"),
        ("-C='my ''dir''/sub", "-C=my 'dir'/sub"),
    ];
    for (raw, expected) in cases {
        let argv = vec![
            OsString::from("vp"),
            OsString::from("--"),
            OsString::from("vp"),
            OsString::from(raw),
        ];
        let env = |k: &str| match k {
            "_VP_COMPLETE_INDEX" => Some("1".into()),
            "_VP_PWSH_RAW_CUR" => Some("1".into()),
            _ => None,
        };
        let ShellInputMode::Complete(req) = ShellDialect::PowerShell.parse_input(&env, &argv, &cwd)
        else {
            panic!("Expected Complete for {raw}");
        };
        assert_eq!(req.words[1], expected, "Failed unquoting {raw}");
    }
}

#[test]
fn test_fallback_attached_flag_and_delimiter_sanitization() {
    let temp = tempfile::TempDir::new().unwrap();
    let ws = temp.path();
    std::fs::write(ws.join("alpha.txt"), "a").unwrap();

    // 1. File fallback strips and re-attaches `--unknown=prefix` in Fish/Nu (and PowerShell/Zsh when effective_cwd is set)
    let fallback_out = CompletionOutput::new(vec![], CompletionDirective::DEFAULT);
    let fb_req = CompletionRequest::new(
        vec!["vp".into(), "passthrough".into(), "--unknown=alp".into()],
        2,
        ws.to_path_buf(),
    );
    for dialect in [ShellDialect::Fish, ShellDialect::Nu] {
        let rendered = dialect.adapter().render_output(&fb_req, &|_| None, &fallback_out);
        assert!(
            rendered.contains("--unknown=alpha.txt"),
            "{dialect:?} fallback must preserve `--unknown=`, got: {rendered}"
        );
    }
    let mut cwd_fallback_out = CompletionOutput::new(vec![], CompletionDirective::DEFAULT);
    cwd_fallback_out.effective_cwd = Some(ws.to_path_buf());
    let pwsh_fb =
        ShellDialect::PowerShell.adapter().render_output(&fb_req, &|_| None, &cwd_fallback_out);
    assert!(
        pwsh_fb.contains("--unknown=alpha.txt"),
        "PowerShell effective_cwd fallback must preserve `--unknown=`, got: {pwsh_fb}"
    );
    let zsh_fb = ShellDialect::Zsh.adapter().render_output(&fb_req, &|_| None, &cwd_fallback_out);
    assert!(
        zsh_fb.contains("p:--unknown="),
        "Zsh effective_cwd fallback must emit p:--unknown=, got: {zsh_fb}"
    );

    // 2. Line/tab protocol dialects drop items containing `\n`/`\t` rather than forging frames/directives
    let poison_out = CompletionOutput::new(
        vec![
            CompletionItem::value("good", Some("ok")),
            CompletionItem::value("bad\n:0", Some("injected")),
            CompletionItem::value("bad\tcol", Some("tab")),
        ],
        CompletionDirective::NO_FILE_COMP,
    );
    let req = CompletionRequest::new(vec!["vp".into(), "".into()], 1, ws.to_path_buf());
    for dialect in [ShellDialect::Bash, ShellDialect::Fish, ShellDialect::PowerShell] {
        let rendered = dialect.adapter().render_output(&req, &|_| None, &poison_out);
        assert!(rendered.contains("good"), "{dialect:?} must retain safe item: {rendered}");
        assert!(
            !rendered.contains("bad\n") && !rendered.contains("\n:0\n"),
            "{dialect:?} must drop newline-breaking values: {rendered}"
        );
        if dialect != ShellDialect::Bash {
            assert!(
                !rendered.contains("injected") && !rendered.contains("bad"),
                "{dialect:?} must drop tab/newline-breaking values: {rendered}"
            );
        }
    }

    // 3. PowerShell smart quotes must trigger quoting and double single-quote variants
    let smart_out = CompletionOutput::new(
        vec![
            CompletionItem::value("x\u{2019}$(Write-Output INJECTED)\u{2018}y", Some("smart")),
            CompletionItem::value("x\u{201C}$(Write-Output INJECTED)\u{201D}y", Some("dsmart")),
        ],
        CompletionDirective::NO_FILE_COMP,
    );
    let pwsh_smart = ShellDialect::PowerShell.adapter().render_output(&req, &|_| None, &smart_out);
    assert!(
        pwsh_smart.contains("'x\u{2019}\u{2019}$(Write-Output INJECTED)\u{2018}\u{2018}y'"),
        "PowerShell must double Unicode single quotes: {pwsh_smart}"
    );
    assert!(
        pwsh_smart.contains("'x\u{201C}$(Write-Output INJECTED)\u{201D}y'"),
        "PowerShell must single-quote Unicode double quotes: {pwsh_smart}"
    );

    // 4. Nushell attached quoted flag values unquote cleanly and newlines are double-quoted + escaped
    let nu_argv = vec![
        OsString::from("vp"),
        OsString::from("--"),
        OsString::from("vp"),
        OsString::from("--config=\"vit"),
    ];
    let nu_env = |k: &str| (k == "_VP_COMPLETE_INDEX").then(|| "1".into());
    let ShellInputMode::Complete(nu_flag_req) = ShellDialect::Nu.parse_input(&nu_env, &nu_argv, ws)
    else {
        panic!("Expected Nu Complete");
    };
    assert_eq!(nu_flag_req.words[1], "--config=vit");

    let nu_nl_out = CompletionOutput::new(
        vec![CompletionItem::value("abc\nwhoami", Some("nl"))],
        CompletionDirective::NO_FILE_COMP,
    );
    let nu_rendered = ShellDialect::Nu.adapter().render_output(&req, &|_| None, &nu_nl_out);
    assert!(
        nu_rendered.contains(r#""\"abc\\nwhoami\"""#),
        "Nushell must quote and escape newlines: {nu_rendered}"
    );
}
