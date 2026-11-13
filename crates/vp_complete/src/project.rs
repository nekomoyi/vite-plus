//! Node/JS workspace, task, binary, and CLI spec completion providers.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    BinKind, CmdSpec, CompleterFn, CompletionContext, CompletionDirective, CompletionItem,
    CompletionOutput, FlagSpec, ItemKind, PathMode, PositionalSpec, ShellDialect, ShellInputMode,
    ValueHint, classify_vp_binary, complete, complete_path, sort_by_cmp,
};

#[derive(Clone, Copy)]
enum OverlayKind {
    Plain,
    RootDirPositional,
    VitestSubcommands,
    VitepressSubcommands,
}

#[derive(Default)]
pub struct PkgManifest {
    pub name: Option<String>,
    pub workspaces: Option<Vec<String>>,
    pub scripts: Vec<(String, String)>,
    pub deps: Vec<(String, String, &'static str)>,
}

pub type ReadManifestFn = fn(&Path) -> Option<PkgManifest>;

pub struct VpCompletionConfig {
    pub global_bin: fn() -> PathBuf,
    pub node_store: fn() -> PathBuf,
    pub resolve_node: fn() -> Option<PathBuf>,
    pub resolve_bin_js: fn(&Path) -> Option<PathBuf>,
    pub read_manifest: ReadManifestFn,
}

/// Outcome of early `VP_COMPLETE` interception before runtime startup.
pub enum EarlyCompletionOutcome {
    /// `VP_COMPLETE` was empty/whitespace; proceed with normal CLI execution.
    ContinueCli,
    /// Unknown dialect; exit with failure.
    InvalidDialect,
    /// Handled registration hook (`newline == false`) or completion (`newline == true`).
    Handled { output: String, trailing_newline: bool },
}

#[cold]
pub fn dispatch_early_completion(
    dialect_var: &OsStr,
    build_vp: fn() -> CmdSpec,
    build_vpx: fn() -> CmdSpec,
) -> EarlyCompletionOutcome {
    let dialect_str = dialect_var.to_string_lossy();
    if dialect_str.trim().is_empty() {
        return EarlyCompletionOutcome::ContinueCli;
    }

    let Ok(dialect) = dialect_str.parse::<ShellDialect>() else {
        return EarlyCompletionOutcome::InvalidDialect;
    };

    let argv: Vec<OsString> = std::env::args_os().collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env_get = |key: &str| std::env::var(key).ok();

    match dialect.parse_input(&env_get, &argv, &cwd) {
        ShellInputMode::RegistrationHook => {
            let exe = std::env::current_exe().ok();
            EarlyCompletionOutcome::Handled {
                output: dialect.render_hook_with_exe(exe.as_deref()),
                trailing_newline: false,
            }
        }
        ShellInputMode::Complete(req) => {
            let is_vpx =
                req.words.first().is_some_and(|w| classify_vp_binary(w) == Some(BinKind::Vpx));
            let out = if is_vpx {
                let empty_root = CmdSpec::new("vp");
                let vpx_spec = build_vpx();
                complete(&empty_root, Some(&vpx_spec), &req)
            } else {
                let root = build_vp();
                complete(&root, None, &req)
            };
            EarlyCompletionOutcome::Handled {
                output: dialect.render_output(&req, &env_get, &out),
                trailing_newline: true,
            }
        }
    }
}

#[cold]
#[must_use]
pub fn build_vp_spec(raw_cli: &mut clap::Command, cfg: &VpCompletionConfig) -> CmdSpec {
    let mut spec = CmdSpec::from_clap_mut(raw_cli);
    for flag in &mut spec.flags {
        if flag.short == Some('C') && flag.long.is_none() {
            flag.chdir = true;
            flag.value_hint = ValueHint::DirPath;
        }
    }

    for local_sub in crate::spec::bundled_local_specs() {
        if let Some(pos) = spec.subcommands.iter().position(|s| s.name == local_sub.name) {
            spec.subcommands[pos] = local_sub;
        } else {
            spec.subcommands.push(local_sub);
        }
    }

    for (name, alias, about) in
        [("list", Some("ls"), "List installed packages"), ("rebuild", None, "Rebuild packages")]
    {
        if !spec.subcommands.iter().any(|s| s.name == name) {
            let mut cmd = CmdSpec::new(name).about(about);
            if let Some(a) = alias {
                cmd = cmd.visible_alias(a);
            }
            if let Some(pm) = spec.subcommands.iter().find(|s| s.name == "pm")
                && let Some(pm_sub) = pm.subcommands.iter().find(|s| s.name == name)
            {
                cmd.flags = pm_sub.flags.clone();
                cmd.positionals = pm_sub.positionals.clone();
            }
            spec.subcommands.push(cmd);
        }
    }

    if !spec.subcommands.iter().any(|s| s.name == "node") {
        spec.subcommands.push(
            CmdSpec::new("node").about("Run Node.js using the managed runtime").positional(
                PositionalSpec::new("args")
                    .multiple(true)
                    .trailing_var_arg(true)
                    .value_hint(ValueHint::AnyPath),
            ),
        );
    }

    for global_only_name in "env,upgrade,implode".split(',') {
        if let Some(sub) = spec.subcommands.iter_mut().find(|s| s.name == global_only_name) {
            sub.global_only = true;
        }
    }

    if !spec.subcommands.iter().any(|s| s.name == "help") {
        let help_completer = help_subcommand_completer(&spec.subcommands);
        spec.subcommands.push(
            CmdSpec::new("help")
                .about("Display help for a command")
                .positional(PositionalSpec::new("command").completer(help_completer)),
        );
    }

    enrich_vp_spec(&mut spec, cfg);

    spec
}

#[cold]
#[must_use]
pub fn build_vpx_spec(raw_cli: &mut clap::Command, cfg: &VpCompletionConfig) -> CmdSpec {
    let mut vpx = CmdSpec::new("vpx").about("Execute an npm package binary or script");

    if let Some(dlx_spec) = CmdSpec::from_clap_subcommand_mut(raw_cli, "dlx") {
        for flag in dlx_spec.flags {
            if flag.long.as_deref() != Some("help") {
                vpx.flags.push(flag);
            }
        }
    }

    vpx.flags.extend(FlagSpec::parse_tsv(crate::assets::slice(crate::assets::RANGE_VPX_EXTRA)));

    vpx.positional(PositionalSpec::new("target").completer(vpx_target_completer(cfg.global_bin)))
        .positional(
            PositionalSpec::new("args")
                .multiple(true)
                .trailing_var_arg(true)
                .value_hint(ValueHint::AnyPath),
        )
}

#[cold]
fn complete_help_subcommand(
    names: &[(CompletionItem, bool)],
    ctx: &CompletionContext,
) -> CompletionOutput {
    if ctx.positional_index != 0 {
        return CompletionOutput::new(vec![], CompletionDirective::DEFAULT);
    }
    let mut items = Vec::new();
    for (it, global_only) in names {
        if *global_only && ctx.is_local_bin {
            continue;
        }
        if ctx.prefix.is_empty() || it.value.starts_with(&ctx.prefix) {
            items.push(it.clone());
        }
    }
    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

#[cold]
fn help_subcommand_completer(subcommands: &[CmdSpec]) -> CompleterFn {
    let mut names = Vec::with_capacity(subcommands.len() + 1);
    for s in subcommands {
        if !s.hidden {
            names.push((CompletionItem::value(s.name.clone(), s.about.clone()), s.global_only));
        }
    }
    if !names.iter().any(|(item, _)| item.value == "help") {
        names.push((CompletionItem::value("help", Some("Display help for a command")), false));
    }

    Arc::new(move |ctx| complete_help_subcommand(&names, ctx))
}

#[cold]
fn passthrough_spec_for(cmd: &str) -> Option<(&'static [(usize, usize)], OverlayKind)> {
    use crate::assets::{
        RANGE_OXFMT, RANGE_OXLINT, RANGE_TSDOWN, RANGE_VITE_BUILD, RANGE_VITE_COMMON,
        RANGE_VITE_PREVIEW, RANGE_VITE_SERVER, RANGE_VITEPRESS, RANGE_VITEST,
    };
    match cmd {
        "lint" => Some((&[RANGE_OXLINT], OverlayKind::Plain)),
        "fmt" => Some((&[RANGE_OXFMT], OverlayKind::Plain)),
        "dev" => Some((&[RANGE_VITE_SERVER, RANGE_VITE_COMMON], OverlayKind::RootDirPositional)),
        "build" => Some((&[RANGE_VITE_BUILD, RANGE_VITE_COMMON], OverlayKind::RootDirPositional)),
        "preview" => {
            Some((&[RANGE_VITE_PREVIEW, RANGE_VITE_COMMON], OverlayKind::RootDirPositional))
        }
        "test" => Some((&[RANGE_VITEST], OverlayKind::VitestSubcommands)),
        "pack" => Some((&[RANGE_TSDOWN], OverlayKind::Plain)),
        "doc" => Some((&[RANGE_VITEPRESS], OverlayKind::VitepressSubcommands)),
        _ => None,
    }
}

#[cold]
fn enrich_vp_spec(root: &mut CmdSpec, cfg: &VpCompletionConfig) {
    let task_comp = task_completer_with(cfg.resolve_node, cfg.resolve_bin_js, cfg.read_manifest);
    let bin_comp = bin_completer(cfg.global_bin);
    let ws_comp = workspace_pkg_completer_with(cfg.read_manifest);
    let dep_comp = manifest_dep_completer_with(cfg.read_manifest);

    for sub in &mut root.subcommands {
        if let Some((ranges, kind)) = passthrough_spec_for(&sub.name) {
            apply_passthrough_overlay(sub, ranges, kind);
        }

        match sub.name.as_str() {
            "run" => {
                sub.positionals = vec![
                    PositionalSpec::new("task").completer(task_comp.clone()),
                    PositionalSpec::new("args")
                        .multiple(true)
                        .trailing_var_arg(true)
                        .value_hint(ValueHint::AnyPath),
                ];
            }
            "exec" => {
                sub.positionals = vec![
                    PositionalSpec::new("command").completer(bin_comp.clone()),
                    PositionalSpec::new("args")
                        .multiple(true)
                        .trailing_var_arg(true)
                        .value_hint(ValueHint::AnyPath),
                ];
            }
            "create" => {
                set_static_flag_values(sub, "package-manager", "pnpm,npm,yarn,bun");
            }
            "staged" => set_static_flag_values(sub, "concurrent", "true,false"),
            "env" => enrich_env_subcommands(sub, cfg.node_store),
            _ => {}
        }

        enrich_workspace_and_pm_completers(sub, &dep_comp, &ws_comp);
    }

    if let Some(pm) = root.subcommands.iter_mut().find(|s| s.name == "pm") {
        for sub in &mut pm.subcommands {
            enrich_workspace_and_pm_completers(sub, &dep_comp, &ws_comp);
        }
    }
}

#[cold]
fn apply_passthrough_overlay(
    target: &mut CmdSpec,
    flag_ranges: &[(usize, usize)],
    kind: OverlayKind,
) {
    let existing_help = target.flags.iter().find(|f| f.long.as_deref() == Some("help")).cloned();
    target.flags.clear();
    for &range in flag_ranges {
        target.flags.extend(FlagSpec::parse_tsv(crate::assets::slice(range)));
    }
    if let Some(help_flag) = existing_help
        && !target.flags.iter().any(|f| f.long.as_deref() == Some("help"))
    {
        target.flags.push(help_flag);
    }

    match kind {
        OverlayKind::Plain => {
            for pos in &mut target.positionals {
                pos.trailing_var_arg = false;
            }
        }
        OverlayKind::RootDirPositional => {
            target.positionals = vec![PositionalSpec::new("root").value_hint(ValueHint::DirPath)];
        }
        OverlayKind::VitestSubcommands => {
            let inherited = target.flags.clone();
            for sub_name in "run,watch,dev,related,bench,list".split(',') {
                target.subcommands.push(
                    CmdSpec::new(sub_name)
                        .about(format!("{sub_name} tests"))
                        .flags(inherited.clone())
                        .positional(
                            PositionalSpec::new("filters")
                                .multiple(true)
                                .value_hint(ValueHint::AnyPath),
                        ),
                );
            }
        }
        OverlayKind::VitepressSubcommands => {
            target.positionals.clear();
            let inherited = target.flags.clone();
            for sub_name in "dev,build,preview".split(',') {
                target.subcommands.push(
                    CmdSpec::new(sub_name)
                        .about(format!("{sub_name} documentation"))
                        .flags(inherited.clone()),
                );
            }
        }
    }
}

#[inline]
fn set_static_flag_values(sub: &mut CmdSpec, long: &str, csv: &str) {
    for flag in &mut sub.flags {
        if flag.long.as_deref() == Some(long) {
            let mut vals = Vec::new();
            for v in csv.split(',') {
                vals.push((v.to_string(), None));
            }
            flag.value_hint = ValueHint::StaticValues(vals);
        }
    }
}

#[cold]
fn set_first_positional_completer(
    sub: &mut CmdSpec,
    fallback_name: &str,
    multiple: bool,
    completer: CompleterFn,
) {
    if let Some(first) = sub.positionals.first_mut() {
        first.completer = Some(completer);
    } else {
        sub.positionals
            .push(PositionalSpec::new(fallback_name).multiple(multiple).completer(completer));
    }
}

#[inline]
fn is_dep_cmd_name(s: &str) -> bool {
    matches!(
        s,
        "remove" | "rm" | "uninstall" | "un" | "why" | "explain" | "outdated" | "update" | "up"
    )
}

#[cold]
fn enrich_workspace_and_pm_completers(
    sub: &mut CmdSpec,
    dep_comp: &CompleterFn,
    ws_comp: &CompleterFn,
) {
    if is_dep_cmd_name(&sub.name) || sub.visible_aliases.iter().any(|a| is_dep_cmd_name(a)) {
        set_first_positional_completer(sub, "package", true, dep_comp.clone());
    }

    let is_vite_debug_filter_cmd = matches!(sub.name.as_str(), "dev" | "build" | "preview");
    for flag in &mut sub.flags {
        let is_ws_filter = flag.short == Some('F')
            || (!is_vite_debug_filter_cmd && flag.long.as_deref() == Some("filter"));
        if is_ws_filter || flag.long.as_deref() == Some("resume-from") {
            flag.completer = Some(ws_comp.clone());
        }
    }
}

#[cold]
fn enrich_env_subcommands(env_cmd: &mut CmdSpec, node_store: fn() -> PathBuf) {
    let node_comp = installed_node_completer(node_store);
    let mut scopes = Vec::with_capacity(6);
    for s in "node,pm,npm,pnpm,yarn,bun".split(',') {
        scopes.push((s.to_string(), Some("runtime scope".to_string())));
    }
    let scope_hint = ValueHint::StaticValues(scopes);
    for sub in &mut env_cmd.subcommands {
        match sub.name.as_str() {
            "use" | "pin" | "uninstall" | "remove" | "default" => {
                set_first_positional_completer(sub, "version", false, node_comp.clone());
            }
            "on" | "off" | "current" | "print" | "list" | "list-remote" | "doctor" | "clean" => {
                if let Some(first) = sub.positionals.first_mut() {
                    first.value_hint = scope_hint.clone();
                } else {
                    sub.positionals
                        .push(PositionalSpec::new("scope").value_hint(scope_hint.clone()));
                }
            }
            _ => {}
        }
    }
}

// =============================================================================
// Task completion (offline Node fast-path + pure-Rust fallback, zero mpsc)
// =============================================================================

#[cold]
fn complete_task_positional(
    ctx: &CompletionContext,
    resolve_node: fn() -> Option<PathBuf>,
    resolve_bin_js: fn(&Path) -> Option<PathBuf>,
    read_manifest: ReadManifestFn,
) -> CompletionOutput {
    if ctx.positional_index != 0 {
        return CompletionOutput::new(vec![], CompletionDirective::DEFAULT);
    }
    let items = get_task_candidates_with(
        &ctx.cwd,
        &ctx.prefix,
        resolve_node,
        resolve_bin_js,
        read_manifest,
    );
    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

#[inline]
#[must_use]
pub fn task_completer_with(
    resolve_node: fn() -> Option<PathBuf>,
    resolve_bin_js: fn(&Path) -> Option<PathBuf>,
    read_manifest: ReadManifestFn,
) -> CompleterFn {
    Arc::new(move |ctx| complete_task_positional(ctx, resolve_node, resolve_bin_js, read_manifest))
}

#[cfg(unix)]
#[cold]
fn kill_process_group(pid: u32) {
    if pid > 0 {
        let _ = std::process::Command::new("/bin/sh")
            .args(["-c", &format!("kill -s KILL -- -{pid} 2>/dev/null")])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(windows)]
#[cold]
fn kill_windows_process_tree(child: &mut std::process::Child) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let pid = child.id();
    if pid > 0 {
        let pid_str = pid.to_string();
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid_str, "/T", "/F"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = child.kill();
}

#[cold]
fn terminate_timed_out_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        kill_process_group(child.id());
        let _ = child.kill();
    }
    #[cfg(windows)]
    {
        kill_windows_process_tree(child);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

#[cold]
fn read_child_stdout(stdout_pipe: Option<std::process::ChildStdout>) -> String {
    let mut buf = String::new();
    if let Some(mut pipe) = stdout_pipe {
        use std::io::Read;
        let _ = pipe.read_to_string(&mut buf);
    }
    buf
}

#[cold]
fn try_node_task_candidates(
    dir: &Path,
    prefix: &str,
    resolve_node: fn() -> Option<PathBuf>,
    resolve_bin_js: fn(&Path) -> Option<PathBuf>,
) -> Option<Vec<CompletionItem>> {
    if std::env::var_os("VP_COMPLETE_OFFLINE_ONLY_RUST").is_some_and(|v| v == "1") {
        return None;
    }
    let node_bin = resolve_node()?;
    let bin_js = resolve_bin_js(dir)?;
    let deadline = Instant::now() + Duration::from_millis(1500);

    let mut cmd = std::process::Command::new(&node_bin);
    cmd.arg(&bin_js)
        .arg("run")
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .env("VP_ENV_FORBIDDEN", "1")
        .env_remove("VP_COMPLETE");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().ok()?;
    let stdout_pipe = child.stdout.take();
    let reader = std::thread::spawn(move || read_child_stdout(stdout_pipe));

    let mut child_status = None;
    let completed = loop {
        if child_status.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => child_status = Some(status),
                Ok(None) => {}
                Err(_) => break false,
            }
        }
        if child_status.is_some() && reader.is_finished() {
            break true;
        }
        let now = Instant::now();
        if now >= deadline {
            break false;
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(10)));
    };

    if !completed {
        terminate_timed_out_child(&mut child);
        return None;
    }

    let stdout_str = reader.join().unwrap_or_default();
    if child_status?.success() && !stdout_str.trim().is_empty() {
        let items = parse_task_completion_lines(&stdout_str, prefix);
        if !items.is_empty() {
            return Some(items);
        }
    }
    None
}

#[cold]
#[must_use]
pub fn get_task_candidates_with(
    dir: &Path,
    prefix: &str,
    resolve_node: fn() -> Option<PathBuf>,
    resolve_bin_js: fn(&Path) -> Option<PathBuf>,
    read_manifest: ReadManifestFn,
) -> Vec<CompletionItem> {
    try_node_task_candidates(dir, prefix, resolve_node, resolve_bin_js)
        .unwrap_or_else(|| extract_tasks_pure_rust_with(dir, prefix, read_manifest))
}

#[cold]
fn find_enclosing_package_dir(
    dir: &Path,
    read_manifest: ReadManifestFn,
) -> Option<(PathBuf, PkgManifest)> {
    for anc in dir.ancestors() {
        if let Some(pkg) = read_manifest(anc) {
            return Some((anc.to_path_buf(), pkg));
        }
    }
    None
}

#[cold]
fn find_enclosing_vite_config_dir(dir: &Path) -> Option<PathBuf> {
    for anc in dir.ancestors() {
        if ["vite.config.ts", "vite.config.mts", "vite.config.js", "vite.config.mjs"]
            .iter()
            .any(|name| anc.join(name).is_file())
        {
            return Some(anc.to_path_buf());
        }
        if anc.join("package.json").is_file() || anc.join("pnpm-workspace.yaml").is_file() {
            break;
        }
    }
    None
}

#[cold]
#[must_use]
pub fn extract_tasks_pure_rust_with(
    dir: &Path,
    prefix: &str,
    read_manifest: ReadManifestFn,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    if let Some((pkg_dir, pkg)) = find_enclosing_package_dir(dir, read_manifest) {
        for (name, desc) in pkg.scripts {
            if is_valid_task_name(&name) {
                items.push(CompletionItem::value(name, (!desc.is_empty()).then_some(desc)));
            }
        }
        let cfg_dir = find_enclosing_vite_config_dir(dir).unwrap_or(pkg_dir);
        for (name, desc) in extract_vite_config_tasks(&cfg_dir) {
            if is_valid_task_name(&name) && !items.iter().any(|it| it.value == name) {
                items.push(CompletionItem::value(name, desc.filter(|s| !s.is_empty())));
            }
        }
    } else if let Some(cfg_dir) = find_enclosing_vite_config_dir(dir) {
        for (name, desc) in extract_vite_config_tasks(&cfg_dir) {
            if is_valid_task_name(&name) && !items.iter().any(|it| it.value == name) {
                items.push(CompletionItem::value(name, desc.filter(|s| !s.is_empty())));
            }
        }
    }

    let sanitized = sanitize_task_current_prefix(prefix);
    if !sanitized.is_empty() {
        items.retain(|item| item.value.starts_with(&sanitized));
    }
    CompletionItem::sort_by_value(&mut items);
    items
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsTok<'a> {
    Prop(&'a str),
    Str(&'a str),
    LBrace,
    RBrace,
    Colon,
    Comma,
    Other,
}

#[cold]
fn unescape_js_str(raw: &str) -> String {
    if !raw.contains('\\') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(esc) = chars.next() {
                out.push(match esc {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl JsTok<'_> {
    #[inline]
    fn is_prop(self, expected: &str) -> bool {
        match self {
            Self::Prop(s) => s == expected,
            Self::Str(s) => {
                if !s.contains('\\') {
                    s == expected
                } else {
                    unescape_js_str(s) == expected
                }
            }
            _ => false,
        }
    }

    #[cold]
    fn into_prop_string(self) -> Option<String> {
        match self {
            Self::Prop(s) => Some(s.to_string()),
            Self::Str(s) => Some(unescape_js_str(s)),
            _ => None,
        }
    }
}

#[cold]
fn is_regex_prefix_keyword(word: &str) -> bool {
    matches!(
        word,
        "return"
            | "typeof"
            | "void"
            | "delete"
            | "throw"
            | "new"
            | "in"
            | "of"
            | "instanceof"
            | "case"
            | "yield"
            | "await"
    )
}

#[inline]
const fn utf8_char_width(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

#[cold]
fn lex_js_tokens(src: &str) -> Vec<JsTok<'_>> {
    let mut toks = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut can_start_regex = true;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if b == b'/' && can_start_regex {
            i += 1;
            let mut in_class = false;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => {
                        i += 1;
                        if i < bytes.len() {
                            i += utf8_char_width(bytes[i]);
                        }
                    }
                    b'[' => {
                        in_class = true;
                        i += 1;
                    }
                    b']' => {
                        in_class = false;
                        i += 1;
                    }
                    b'/' if !in_class => {
                        i += 1;
                        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        break;
                    }
                    b'\n' | b'\r' => break,
                    cur => i += utf8_char_width(cur),
                }
            }
            toks.push(JsTok::Other);
            can_start_regex = false;
        } else if b == b'\'' || b == b'"' {
            let quote = b;
            i += 1;
            let start = i;
            let mut end = None;
            while i < bytes.len() {
                match bytes[i] {
                    cur if cur == quote => {
                        end = Some(i);
                        i += 1;
                        break;
                    }
                    b'\\' => {
                        i += 1;
                        if i < bytes.len() {
                            i += utf8_char_width(bytes[i]);
                        }
                    }
                    b'\n' | b'\r' => break,
                    cur => i += utf8_char_width(cur),
                }
            }
            toks.push(end.map_or(JsTok::Other, |e| JsTok::Str(&src[start..e])));
            can_start_regex = false;
        } else if b == b'`' {
            i += 1;
            let start = i;
            let mut end = None;
            let mut dynamic = false;
            while i < bytes.len() {
                let cur = bytes[i];
                if cur == b'`' {
                    end = Some(i);
                    i += 1;
                    break;
                } else if cur == b'\\' {
                    i += 1;
                    if i < bytes.len() {
                        i += utf8_char_width(bytes[i]);
                    }
                } else if cur == b'$' && bytes.get(i + 1) == Some(&b'{') {
                    dynamic = true;
                    i += 2;
                    let mut depth = 1usize;
                    while i < bytes.len() && depth > 0 {
                        match bytes[i] {
                            b'{' => {
                                depth += 1;
                                i += 1;
                            }
                            b'}' => {
                                depth -= 1;
                                i += 1;
                            }
                            b'\\' => {
                                i += 1;
                                if i < bytes.len() {
                                    i += utf8_char_width(bytes[i]);
                                }
                            }
                            inner => i += utf8_char_width(inner),
                        }
                    }
                } else {
                    i += utf8_char_width(cur);
                }
            }
            toks.push(match (dynamic, end) {
                (false, Some(e)) => JsTok::Str(&src[start..e]),
                _ => JsTok::Other,
            });
            can_start_regex = false;
        } else if b == b'{' {
            toks.push(JsTok::LBrace);
            can_start_regex = true;
            i += 1;
        } else if b == b'}' {
            toks.push(JsTok::RBrace);
            can_start_regex = false;
            i += 1;
        } else if b == b':' {
            toks.push(JsTok::Colon);
            can_start_regex = true;
            i += 1;
        } else if b == b',' {
            toks.push(JsTok::Comma);
            can_start_regex = true;
            i += 1;
        } else if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let nb = bytes[i];
                if nb.is_ascii_alphanumeric() || nb == b'_' || nb == b'$' {
                    i += 1;
                } else if !nb.is_ascii()
                    && let Some(nc) = src[i..].chars().next()
                    && nc.is_alphanumeric()
                {
                    i += nc.len_utf8();
                } else {
                    break;
                }
            }
            let ident = &src[start..i];
            can_start_regex = is_regex_prefix_keyword(ident);
            toks.push(JsTok::Prop(ident));
        } else if b.is_ascii() {
            can_start_regex = !matches!(b, b')' | b']' | b'.') && !b.is_ascii_digit();
            toks.push(JsTok::Other);
            i += 1;
        } else {
            let Some(ch) = src[i..].chars().next() else { break };
            if ch.is_whitespace() {
                i += ch.len_utf8();
            } else if ch.is_alphabetic() {
                let start = i;
                i += ch.len_utf8();
                while let Some(nc) = src[i..].chars().next() {
                    if nc.is_alphanumeric() || nc == '_' || nc == '$' {
                        i += nc.len_utf8();
                    } else {
                        break;
                    }
                }
                toks.push(JsTok::Prop(&src[start..i]));
                can_start_regex = false;
            } else {
                can_start_regex = true;
                toks.push(JsTok::Other);
                i += ch.len_utf8();
            }
        }
    }
    toks
}

#[cold]
fn find_matching_rbrace(toks: &[JsTok<'_>], lbrace_idx: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, tok) in toks.iter().enumerate().skip(lbrace_idx) {
        match tok {
            JsTok::LBrace => depth += 1,
            JsTok::RBrace => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

#[cold]
fn extract_vite_config_tasks(dir: &Path) -> Vec<(String, Option<String>)> {
    let mut cfg_path = None;
    for ext in ["js", "mjs", "ts", "cjs", "mts", "cts"] {
        let p = dir.join(format!("vite.config.{ext}"));
        if p.is_file() {
            cfg_path = Some(p);
            break;
        }
    }
    let Some(cfg_path) = cfg_path else {
        return Vec::new();
    };
    let Ok(src) = std::fs::read_to_string(cfg_path) else {
        return Vec::new();
    };
    let toks = lex_js_tokens(&src);
    let mut out = Vec::new();

    for i in 1..toks.len() {
        if !matches!(toks[i - 1], JsTok::LBrace | JsTok::Comma)
            || !toks[i].is_prop("run")
            || toks.get(i + 1) != Some(&JsTok::Colon)
            || toks.get(i + 2) != Some(&JsTok::LBrace)
        {
            continue;
        }
        let Some(run_end) = find_matching_rbrace(&toks, i + 2) else { continue };
        let mut j = i + 3;
        while j < run_end {
            if toks[j] == JsTok::LBrace {
                j = find_matching_rbrace(&toks, j).map_or(run_end, |end| end + 1);
                continue;
            }
            if matches!(toks[j - 1], JsTok::LBrace | JsTok::Comma)
                && toks[j].is_prop("tasks")
                && toks.get(j + 1) == Some(&JsTok::Colon)
                && toks.get(j + 2) == Some(&JsTok::LBrace)
            {
                let Some(tasks_end) = find_matching_rbrace(&toks, j + 2) else { break };
                let mut k = j + 3;
                while k < tasks_end {
                    if toks[k] == JsTok::LBrace {
                        k = find_matching_rbrace(&toks, k).map_or(tasks_end, |end| end + 1);
                        continue;
                    }
                    if matches!(toks[k - 1], JsTok::LBrace | JsTok::Comma)
                        && let Some(task_name) = toks[k].into_prop_string()
                        && toks.get(k + 1) == Some(&JsTok::Colon)
                    {
                        if let Some(JsTok::Str(cmd)) = toks.get(k + 2) {
                            out.push((task_name, Some(unescape_js_str(cmd))));
                            k += 3;
                            continue;
                        } else if toks.get(k + 2) == Some(&JsTok::LBrace)
                            && let Some(task_end) = find_matching_rbrace(&toks, k + 2)
                        {
                            let mut desc = None;
                            let mut m = k + 3;
                            while m < task_end {
                                if toks[m] == JsTok::LBrace {
                                    m = find_matching_rbrace(&toks, m)
                                        .map_or(task_end, |end| end + 1);
                                    continue;
                                }
                                if matches!(toks[m - 1], JsTok::LBrace | JsTok::Comma)
                                    && toks[m].is_prop("command")
                                    && toks.get(m + 1) == Some(&JsTok::Colon)
                                    && let Some(JsTok::Str(cmd)) = toks.get(m + 2)
                                {
                                    desc = Some(unescape_js_str(cmd));
                                }
                                m += 1;
                            }
                            out.push((task_name, desc));
                            k = task_end + 1;
                            continue;
                        }
                    }
                    k += 1;
                }
                break;
            }
            j += 1;
        }
    }
    out
}

#[cold]
#[must_use]
pub fn is_valid_task_name(name: &str) -> bool {
    let script = name.split_once('#').map_or(name, |(_, s)| s);
    !name.starts_with('#')
        && !script.is_empty()
        && !script.starts_with('#')
        && !script.starts_with("//")
        && !name.contains(char::is_whitespace)
}

#[cold]
#[must_use]
pub fn sanitize_task_current_prefix(current: &str) -> String {
    let trimmed = current.trim_matches(['"', '\'']);
    if !trimmed.bytes().any(|b| b == b'\\') {
        return trimmed.to_owned();
    }
    let mut out = String::with_capacity(trimmed.len());
    let mut chars = trimmed.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && matches!(chars.peek(), Some('#' | ':')) {
            continue;
        }
        out.push(ch);
    }
    out.trim_matches(['"', '\'']).to_owned()
}

#[cold]
#[must_use]
pub fn parse_task_completion_lines(stdout: &str, current: &str) -> Vec<CompletionItem> {
    let current_clean = sanitize_task_current_prefix(current);
    let mut out = Vec::new();
    for line in stdout.lines() {
        let Some((raw_name, raw_desc)) = line.split_once(": ") else { continue };
        let name = raw_name.trim();
        let desc = raw_desc.trim();
        if is_valid_task_name(name)
            && (current_clean.is_empty() || name.starts_with(&current_clean))
        {
            out.push(CompletionItem::value(name, (!desc.is_empty()).then_some(desc)));
        }
    }
    out
}

// =============================================================================
// Workspace, manifest dependency, binary, Node version & scope providers
// =============================================================================

#[cold]
#[must_use]
pub fn resolve_offline_node_in(node_store: &Path, skip_dirs: &[&Path]) -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os("VP_COMPLETE_TEST_NODE_BIN") {
        let p = PathBuf::from(custom);
        if p.is_file() {
            return Some(p);
        }
    }

    let rel_bin = if cfg!(windows) { "node.exe" } else { "bin/node" };

    if node_store.is_dir() {
        if let Some(pinned) = std::env::var_os("VP_NODE_VERSION") {
            let bin = node_store.join(pinned.to_string_lossy().as_ref()).join(rel_bin);
            if bin.is_file() {
                return Some(bin);
            }
        }

        if let Ok(entries) = std::fs::read_dir(node_store) {
            let mut best: Option<(String, PathBuf)> = None;
            for entry in entries.flatten() {
                let p = entry.path();
                if p.join(rel_bin).is_file() {
                    let ver = entry.file_name().to_string_lossy().into_owned();
                    let is_newer = best.as_ref().is_none_or(|(best_ver, _)| {
                        cmp_node_versions(&ver, best_ver) == std::cmp::Ordering::Less
                    });
                    if is_newer {
                        best = Some((ver, p));
                    }
                }
            }
            if let Some((_, ver_dir)) = best {
                return Some(ver_dir.join(rel_bin));
            }
        }
    }

    if let Some(path_var) = std::env::var_os("PATH") {
        let bin_name = if cfg!(windows) { "node.exe" } else { "node" };
        for dir in std::env::split_paths(&path_var) {
            if skip_dirs.iter().any(|&skip| dir == skip) {
                continue;
            }
            let candidate = dir.join(bin_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

#[cold]
fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

#[cold]
fn extract_pnpm_yaml_globs(bytes: &[u8]) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(strip_bom(bytes)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_packages = false;
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let is_indented = line.len() > trimmed.len();
        if !is_indented {
            if let Some(rest) = trimmed.strip_prefix("packages:") {
                in_packages = true;
                let rest = rest.trim();
                if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    for item in inner.split(',') {
                        let pat = item.trim().trim_matches(['\'', '"']);
                        if !pat.is_empty() {
                            out.push(pat.to_string());
                        }
                    }
                    in_packages = false;
                }
                continue;
            }
            if !trimmed.starts_with('-') {
                in_packages = false;
                continue;
            }
        }
        if in_packages && let Some(item) = trimmed.strip_prefix('-') {
            let pat = item.trim().trim_matches(['\'', '"']);
            if !pat.is_empty() {
                out.push(pat.to_string());
            }
        }
    }
    out
}

#[cold]
fn match_char_class(pat: &[u8], ch: u8) -> Option<usize> {
    if pat.is_empty() || pat[0] != b'[' {
        return None;
    }
    let mut i = 1usize;
    let negate = if i < pat.len() && (pat[i] == b'!' || pat[i] == b'^') {
        i += 1;
        true
    } else {
        false
    };
    let mut matched = false;
    let start = i;
    while i < pat.len() {
        if pat[i] == b']' && i > start {
            return (matched ^ negate).then_some(i + 1);
        }
        if i + 2 < pat.len() && pat[i + 1] == b'-' && pat[i + 2] != b']' {
            if pat[i] <= ch && ch <= pat[i + 2] {
                matched = true;
            }
            i += 3;
        } else {
            if pat[i] == ch {
                matched = true;
            }
            i += 1;
        }
    }
    None
}

#[cold]
fn seg_wildcard_match(pat: &[u8], txt: &[u8]) -> bool {
    let (mut pi, mut ti) = (0usize, 0usize);
    while pi < pat.len() {
        match pat[pi] {
            b'*' => {
                while pi < pat.len() && pat[pi] == b'*' {
                    pi += 1;
                }
                if pi == pat.len() {
                    return true;
                }
                for next_ti in ti..=txt.len() {
                    if seg_wildcard_match(&pat[pi..], &txt[next_ti..]) {
                        return true;
                    }
                }
                return false;
            }
            b'?' => {
                if ti >= txt.len() {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
            b'[' => {
                if ti >= txt.len() {
                    return false;
                }
                if let Some(advance) = match_char_class(&pat[pi..], txt[ti]) {
                    pi += advance;
                    ti += 1;
                } else if pat[pi] == txt[ti] {
                    pi += 1;
                    ti += 1;
                } else {
                    return false;
                }
            }
            c => {
                if ti >= txt.len() || c != txt[ti] {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
        }
    }
    ti == txt.len()
}

#[cold]
fn expand_braces(pat: &str, out: &mut Vec<String>) {
    if out.len() >= 32 {
        return;
    }
    let bytes = pat.as_bytes();
    let mut depth = 0usize;
    let mut open_idx = None;
    let mut close_idx = None;
    let mut has_comma = false;
    for (idx, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 {
                    open_idx = Some(idx);
                    has_comma = false;
                }
                depth += 1;
            }
            b',' if depth == 1 => has_comma = true,
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 && has_comma {
                    close_idx = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let (Some(start), Some(end)) = (open_idx, close_idx) else {
        out.push(pat.to_string());
        return;
    };
    let prefix = &pat[..start];
    let suffix = &pat[end + 1..];
    let inner = &pat[start + 1..end];
    let mut nested = 0usize;
    let mut part_start = 0usize;
    for (idx, &b) in inner.as_bytes().iter().enumerate() {
        match b {
            b'{' => nested += 1,
            b'}' => nested = nested.saturating_sub(1),
            b',' if nested == 0 => {
                let candidate = format!("{prefix}{}{suffix}", &inner[part_start..idx]);
                expand_braces(&candidate, out);
                part_start = idx + 1;
            }
            _ => {}
        }
    }
    let candidate = format!("{prefix}{}{suffix}", &inner[part_start..]);
    expand_braces(&candidate, out);
}

#[cold]
fn glob_opt_match(pat: Option<&str>, path: Option<&str>) -> bool {
    let Some(p) = pat else {
        return path.is_none();
    };
    if p.contains('{') && p.contains('}') {
        let mut expanded = Vec::new();
        expand_braces(p, &mut expanded);
        return expanded.iter().any(|sub| glob_opt_match_plain(Some(sub.as_str()), path));
    }
    glob_opt_match_plain(Some(p), path)
}

#[cold]
fn glob_opt_match_plain(pat: Option<&str>, path: Option<&str>) -> bool {
    let Some(p) = pat else {
        return path.is_none();
    };
    let (p_seg, p_rest) = match p.split_once('/') {
        Some((a, b)) => (a, Some(b)),
        None => (p, None),
    };
    if p_seg == "**" {
        return glob_opt_match_plain(p_rest, path)
            || path.is_some_and(|t| {
                glob_opt_match_plain(Some(p), t.split_once('/').map(|(_, rest)| rest))
            });
    }
    let Some(t) = path else {
        return false;
    };
    let (t_seg, t_rest) = match t.split_once('/') {
        Some((a, b)) => (a, Some(b)),
        None => (t, None),
    };
    seg_wildcard_match(p_seg.as_bytes(), t_seg.as_bytes()) && glob_opt_match_plain(p_rest, t_rest)
}

#[inline]
fn normalize_ws_pat(pat: &str) -> &str {
    pat.trim().trim_start_matches("./").trim_matches('/')
}

#[cold]
fn glob_walk_root_and_depth(ws_root: &Path, pat: &str) -> Option<(PathBuf, String, usize)> {
    let rel = normalize_ws_pat(pat);
    if rel.is_empty() {
        return Some((ws_root.to_path_buf(), String::new(), 0));
    }
    let mut base = ws_root.to_path_buf();
    let mut base_rel = String::new();
    let mut rem_count = 0usize;
    let mut has_globstar = false;
    let mut in_rem = false;
    for seg in rel.split('/') {
        if !in_rem && !seg.contains(['*', '?', '[', '{', '(', '!', '@']) && seg != ".." {
            base.push(seg);
            if !base_rel.is_empty() {
                base_rel.push('/');
            }
            base_rel.push_str(seg);
        } else {
            in_rem = true;
            rem_count += 1;
            if seg.contains("**") {
                has_globstar = true;
            }
        }
    }
    let depth = if has_globstar { 6 } else { rem_count };
    Some((base, base_rel, depth))
}

struct WsWalkState<'a> {
    ws_root: &'a Path,
    raw_globs: &'a [String],
    visited_dirs: Vec<PathBuf>,
    scanned_dirs: std::collections::BTreeMap<PathBuf, usize>,
    names: Vec<String>,
    read_manifest: ReadManifestFn,
}

impl WsWalkState<'_> {
    #[cold]
    fn matches_rel(&self, rel: &str) -> bool {
        let rel_opt = (!rel.is_empty()).then_some(rel);
        let mut matched = false;
        for g in self.raw_globs {
            let trimmed = g.trim();
            if let Some(neg) = trimmed.strip_prefix('!') {
                let norm = normalize_ws_pat(neg);
                if !norm.is_empty() && glob_opt_match(Some(norm), rel_opt) {
                    return false;
                }
            } else if !matched {
                let norm = normalize_ws_pat(trimmed);
                if !norm.is_empty() && glob_opt_match(Some(norm), rel_opt) {
                    matched = true;
                }
            }
        }
        matched
    }

    #[cold]
    fn walk(&mut self, dir: &Path, rel: &str, rem_depth: usize) {
        if self.visited_dirs.len() >= 512 || self.scanned_dirs.len() >= 2048 || !dir.is_dir() {
            return;
        }
        if let Some(prev_depth) = self.scanned_dirs.get(dir)
            && *prev_depth >= rem_depth
        {
            return;
        }
        self.scanned_dirs.insert(dir.to_path_buf(), rem_depth);

        if dir != self.ws_root {
            if dir.join("pnpm-workspace.yaml").is_file() {
                return;
            }
            if let Some(pkg) = (self.read_manifest)(dir) {
                if pkg.workspaces.is_some() {
                    return;
                }
                if !self.visited_dirs.iter().any(|d| d == dir) {
                    self.visited_dirs.push(dir.to_path_buf());
                    if self.matches_rel(rel)
                        && let Some(name) = pkg.name
                        && !self.names.contains(&name)
                    {
                        self.names.push(name);
                    }
                }
            }
        }
        if rem_depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                continue;
            }
            let fname = entry.file_name();
            let s = fname.to_string_lossy();
            if s.starts_with('.')
                || matches!(
                    s.as_ref(),
                    "node_modules" | "dist" | "target" | "coverage" | "bower_components"
                )
            {
                continue;
            }
            let child_rel = if rel.is_empty() { s.into_owned() } else { format!("{rel}/{s}") };
            self.walk(&entry.path(), &child_rel, rem_depth - 1);
        }
    }
}

#[cold]
fn find_workspace(
    cwd: &Path,
    read_manifest: ReadManifestFn,
) -> Option<(PathBuf, Option<String>, Vec<String>)> {
    let mut nearest_single_pkg: Option<String> = None;
    for anc in cwd.ancestors() {
        let pnpm_yaml = anc.join("pnpm-workspace.yaml");
        if pnpm_yaml.is_file()
            && let Ok(yaml_bytes) = std::fs::read(&pnpm_yaml)
        {
            let root_name = read_manifest(anc).and_then(|m| m.name);
            let globs = extract_pnpm_yaml_globs(&yaml_bytes);
            let globs = if globs.is_empty() { vec!["**".into()] } else { globs };
            return Some((anc.to_path_buf(), root_name, globs));
        }
        if let Some(pkg) = read_manifest(anc) {
            if let Some(globs) = pkg.workspaces {
                let globs = if globs.is_empty() { vec!["**".into()] } else { globs };
                return Some((anc.to_path_buf(), pkg.name, globs));
            }
            if nearest_single_pkg.is_none() {
                nearest_single_pkg = pkg.name;
            }
        }
    }
    nearest_single_pkg.map(|name| (cwd.to_path_buf(), Some(name), Vec::new()))
}

#[cold]
#[must_use]
pub fn get_workspace_package_names_with(cwd: &Path, read_manifest: ReadManifestFn) -> Vec<String> {
    let Some((ws_root, root_name, raw_globs)) = find_workspace(cwd, read_manifest) else {
        return Vec::new();
    };

    let mut state = WsWalkState {
        ws_root: &ws_root,
        raw_globs: &raw_globs,
        visited_dirs: Vec::new(),
        scanned_dirs: std::collections::BTreeMap::new(),
        names: Vec::new(),
        read_manifest,
    };

    if let Some(name) = root_name {
        state.names.push(name);
    }

    for g in &raw_globs {
        let trimmed = g.trim();
        if !trimmed.is_empty()
            && !trimmed.starts_with('!')
            && let Some((base, base_rel, depth)) = glob_walk_root_and_depth(&ws_root, trimmed)
        {
            state.walk(&base, &base_rel, depth);
        }
    }

    sort_by_cmp(&mut state.names, |a, b| a.cmp(b));
    state.names
}

#[cold]
fn complete_manifest_deps(
    ctx: &CompletionContext,
    read_manifest: ReadManifestFn,
) -> CompletionOutput {
    let mut items: Vec<CompletionItem> = Vec::new();

    if let Some((_, pkg)) = find_enclosing_package_dir(&ctx.cwd, read_manifest) {
        for (pkg_name, ver_str, label) in pkg.deps {
            if pkg_name.starts_with(&ctx.prefix) && !items.iter().any(|it| it.value == pkg_name) {
                let desc = if ver_str.is_empty() {
                    label.to_string()
                } else {
                    format!("{label} ({ver_str})")
                };
                items.push(CompletionItem::value(pkg_name, Some(desc)));
            }
        }
    }

    for name in get_workspace_package_names_with(&ctx.cwd, read_manifest) {
        if name.starts_with(&ctx.prefix) && !items.iter().any(|it| it.value == name) {
            items.push(CompletionItem::value(name, Some("workspace package")));
        }
    }

    CompletionItem::sort_by_value(&mut items);
    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

#[inline]
#[must_use]
pub fn manifest_dep_completer_with(read_manifest: ReadManifestFn) -> CompleterFn {
    Arc::new(move |ctx| complete_manifest_deps(ctx, read_manifest))
}

#[cold]
fn complete_workspace_pkgs(
    ctx: &CompletionContext,
    read_manifest: ReadManifestFn,
) -> CompletionOutput {
    let mut items = Vec::new();
    for name in get_workspace_package_names_with(&ctx.cwd, read_manifest) {
        if name.starts_with(&ctx.prefix) {
            items.push(CompletionItem::value(name, Some("workspace package")));
        }
    }
    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

#[inline]
#[must_use]
pub fn workspace_pkg_completer_with(read_manifest: ReadManifestFn) -> CompleterFn {
    Arc::new(move |ctx| complete_workspace_pkgs(ctx, read_manifest))
}

#[cold]
fn normalize_bin_filename(name: &str) -> &str {
    if cfg!(windows) {
        for ext in ".cmd,.exe,.ps1".split(',') {
            if let Some(stem) = name.strip_suffix(ext) {
                return stem;
            }
        }
    }
    name
}

#[cold]
fn scan_bin_dir(
    dir: &Path,
    prefix: &str,
    label: &str,
    skip_vp: bool,
    items: &mut Vec<CompletionItem>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let clean_name = normalize_bin_filename(&name_str);
        if !(skip_vp && matches!(clean_name, "vp" | "vpr" | "vpx"))
            && clean_name.starts_with(prefix)
            && !items.iter().any(|it| it.value == clean_name)
        {
            items.push(CompletionItem::value(clean_name, Some(label)));
        }
    }
}

#[cold]
fn complete_bins(ctx: &CompletionContext, global_bin_fn: fn() -> PathBuf) -> CompletionOutput {
    let mut items = Vec::new();

    for dir in ctx.cwd.ancestors() {
        let bin_dir = dir.join("node_modules/.bin");
        if bin_dir.is_dir() {
            scan_bin_dir(&bin_dir, &ctx.prefix, "local bin", false, &mut items);
        }
    }

    let gbin = global_bin_fn();
    if gbin.is_dir() {
        scan_bin_dir(&gbin, &ctx.prefix, "global bin", true, &mut items);
    }

    CompletionItem::sort_by_value(&mut items);
    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

#[inline]
#[must_use]
pub fn bin_completer(global_bin_fn: fn() -> PathBuf) -> CompleterFn {
    Arc::new(move |ctx| complete_bins(ctx, global_bin_fn))
}

#[cold]
fn parse_semver_key(ver: &str) -> Option<([u32; 3], &str)> {
    let s = ver.strip_prefix('v').unwrap_or(ver);
    let (core, pre) = s.split_once('-').unwrap_or((s, ""));
    let mut nums = [0u32; 3];
    let mut parts = core.split('.');
    nums[0] = parts.next()?.parse().ok()?;
    for slot in &mut nums[1..] {
        if let Some(p) = parts.next() {
            *slot = p.parse().ok()?;
        }
    }
    Some((nums, pre))
}

#[cold]
fn cmp_prerelease_desc(a: &str, b: &str) -> std::cmp::Ordering {
    // Compare descending: empty (stable) > non-empty (prerelease), then larger identifier first.
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return std::cmp::Ordering::Equal,
        (true, false) => return std::cmp::Ordering::Less,
        (false, true) => return std::cmp::Ordering::Greater,
        (false, false) => {}
    }
    let mut ia = a.split('.');
    let mut ib = b.split('.');
    loop {
        match (ia.next(), ib.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (Some(_), None) => return std::cmp::Ordering::Less,
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            (Some(sa), Some(sb)) => {
                let ord = match (sa.parse::<u64>(), sb.parse::<u64>()) {
                    (Ok(na), Ok(nb)) => nb.cmp(&na),
                    // Per SemVer 2.0.0: numeric identifiers always have lower precedence than non-numeric.
                    (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
                    (Err(_), Ok(_)) => std::cmp::Ordering::Less,
                    (Err(_), Err(_)) => sb.cmp(sa),
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

#[cold]
fn cmp_node_versions(a: &String, b: &String) -> std::cmp::Ordering {
    match (parse_semver_key(a), parse_semver_key(b)) {
        (Some((na, prea)), Some((nb, preb))) => {
            nb.cmp(&na).then_with(|| cmp_prerelease_desc(prea, preb))
        }
        _ => b.cmp(a),
    }
}

#[cold]
fn complete_installed_nodes(
    ctx: &CompletionContext,
    node_store_fn: fn() -> PathBuf,
) -> CompletionOutput {
    let mut versions = Vec::new();
    let store = node_store_fn();
    if store.is_dir()
        && let Ok(entries) = std::fs::read_dir(&store)
    {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let ver_str = entry.file_name().to_string_lossy().into_owned();
                if ver_str.starts_with('v')
                    || ver_str.as_bytes().first().is_some_and(u8::is_ascii_digit)
                {
                    versions.push(ver_str);
                }
            }
        }
    }

    sort_by_cmp(&mut versions, cmp_node_versions);

    let mut items = Vec::new();
    for ver in versions {
        if ver.starts_with(&ctx.prefix) {
            items.push(CompletionItem::value(ver, Some("installed node")));
        }
    }

    for alias in "lts,latest".split(',') {
        if alias.starts_with(&ctx.prefix) {
            items.push(CompletionItem::value(alias, Some("alias")));
        }
    }

    CompletionOutput::new(
        items,
        CompletionDirective::KEEP_ORDER | CompletionDirective::NO_FILE_COMP,
    )
}

#[inline]
#[must_use]
pub fn installed_node_completer(node_store_fn: fn() -> PathBuf) -> CompleterFn {
    Arc::new(move |ctx| complete_installed_nodes(ctx, node_store_fn))
}

#[cold]
fn complete_vpx_target(
    ctx: &CompletionContext,
    global_bin_fn: fn() -> PathBuf,
) -> CompletionOutput {
    if ctx.positional_index != 0 {
        return CompletionOutput::new(vec![], CompletionDirective::DEFAULT);
    }

    let script_mode = || {
        let mut exts = Vec::with_capacity(8);
        for ext in "ts,mts,cts,tsx,js,mjs,cjs,jsx".split(',') {
            exts.push(ext.to_string());
        }
        PathMode::FilesWithExtensions(exts)
    };

    let is_path_prefix = ctx.prefix == "."
        || ctx.prefix == ".."
        || ctx.prefix.starts_with("./")
        || ctx.prefix.starts_with("../")
        || ctx.prefix.starts_with('/')
        || ctx.prefix.starts_with('~')
        || ctx.prefix.contains('/')
        || ctx.prefix.contains('\\');

    if is_path_prefix {
        return complete_path(&ctx.cwd, &ctx.prefix, script_mode());
    }

    let bin_out = complete_bins(ctx, global_bin_fn);
    let path_out = complete_path(&ctx.cwd, &ctx.prefix, script_mode());

    let mut items = bin_out.items;
    for it in path_out.items {
        if !items.iter().any(|existing| existing.value == it.value) {
            items.push(it);
        }
    }

    let mut directive = CompletionDirective::NO_FILE_COMP;
    if items.iter().any(|it| it.kind == ItemKind::Directory) {
        directive |= CompletionDirective::NO_SPACE;
    }

    CompletionOutput::new(items, directive)
}

#[inline]
#[must_use]
pub fn vpx_target_completer(global_bin_fn: fn() -> PathBuf) -> CompleterFn {
    Arc::new(move |ctx| complete_vpx_target(ctx, global_bin_fn))
}
