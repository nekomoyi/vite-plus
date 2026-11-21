use vp_complete::{CmdSpec, PositionalSpec, ValueHint};

pub(super) fn bundled_local_specs() -> Vec<CmdSpec> {
    serde_json::from_str(include_str!("local_specs.json"))
        .expect("generated local command definitions")
}

#[must_use]
pub(super) fn build_vp_spec(local_bin: bool) -> CmdSpec {
    let mut spec = CmdSpec::from_clap(crate::cli::raw_cli_command());
    for flag in &mut spec.flags {
        if flag.short == Some('C') && flag.long.is_none() {
            flag.chdir = true;
            flag.value_hint = ValueHint::DirPath;
        }
    }

    // Shared PM commands come from the same Clap enum in both binaries.
    // All other local commands are supplied by the generated local spec below.
    if local_bin {
        spec.subcommands.retain(|sub| {
            <vp_pm_cli::PackageManagerCommand as clap::Subcommand>::has_subcommand(&sub.name)
        });
    }

    for local_sub in bundled_local_specs() {
        if let Some(pos) = spec.subcommands.iter().position(|s| s.name == local_sub.name) {
            spec.subcommands[pos] = local_sub;
        } else {
            spec.subcommands.push(local_sub);
        }
    }

    if !local_bin {
        add_global_commands(&mut spec);
    }

    if !spec.subcommands.iter().any(|s| s.name == "help") {
        spec.subcommands.push(
            CmdSpec::new("help")
                .about("Display help for a command")
                .positional(PositionalSpec::new("command")),
        );
    }

    enrich_vp_spec(&mut spec);

    spec
}

fn add_global_commands(spec: &mut CmdSpec) {
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
}

#[must_use]
pub(super) fn build_vpx_spec() -> CmdSpec {
    let mut vpx = CmdSpec::new("vpx").about("Execute an npm package binary or script");

    let mut cmd = crate::cli::raw_cli_command();
    cmd.build();
    if let Some(dlx_spec) = cmd.find_subcommand("dlx").map(CmdSpec::from_built_clap) {
        for flag in dlx_spec.flags {
            if flag.long.as_deref() != Some("help") {
                vpx.flags.push(flag);
            }
        }
    }

    vpx.positional(PositionalSpec::new("target")).positional(
        PositionalSpec::new("args")
            .multiple(true)
            .trailing_var_arg(true)
            .value_hint(ValueHint::AnyPath),
    )
}

fn enrich_vp_spec(root: &mut CmdSpec) {
    for sub in &mut root.subcommands {
        match sub.name.as_str() {
            "create" => {
                set_static_flag_values(sub, "package-manager", &["pnpm", "npm", "yarn", "bun"]);
            }
            "staged" => set_static_flag_values(sub, "concurrent", &["true", "false"]),
            "env" => enrich_env_subcommands(sub),
            _ => {}
        }
    }
}

fn set_static_flag_values(sub: &mut CmdSpec, long: &str, values: &[&str]) {
    for flag in &mut sub.flags {
        if flag.long.as_deref() == Some(long) {
            let mut vals = Vec::new();
            for v in values {
                vals.push((v.to_string(), None));
            }
            flag.value_hint = ValueHint::StaticValues(vals);
        }
    }
}

fn enrich_env_subcommands(env_cmd: &mut CmdSpec) {
    let mut scopes = Vec::with_capacity(6);
    for s in ["node", "pm", "npm", "pnpm", "yarn", "bun"] {
        scopes.push((s.to_string(), Some("runtime scope".to_string())));
    }
    let scope_hint = ValueHint::StaticValues(scopes);
    for sub in &mut env_cmd.subcommands {
        match sub.name.as_str() {
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
