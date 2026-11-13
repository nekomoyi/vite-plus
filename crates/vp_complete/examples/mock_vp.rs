#![expect(
    clippy::disallowed_macros,
    clippy::print_stdout,
    reason = "completion CLI mock must print completion output directly to stdout"
)]

use std::sync::Arc;

use vp_complete::{
    CmdSpec, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput, FlagSpec,
    PositionalSpec, ValueHint, handle_env_completion,
};

fn build_specs() -> (CmdSpec, Option<CmdSpec>) {
    let root =
        CmdSpec::new("vp")
            .about("Vite+ command line interface")
            .flag(
                FlagSpec::new()
                    .short('C')
                    .about("Run as if vp was started in <dir>")
                    .takes_value(true)
                    .chdir(true)
                    .value_hint(ValueHint::DirPath),
            )
            .subcommand(CmdSpec::new("run").about("Run a project script or task").positional(
                PositionalSpec::new("script").about("Script name to execute").completer(Arc::new(
                    |ctx: &CompletionContext| {
                        let items = vec![
                            CompletionItem::value(
                                "@vitejs/vite-monorepo#build",
                                Some("Build monorepo workspace"),
                            ),
                            CompletionItem::value("build:watch", Some("Build and watch")),
                            CompletionItem::value("build:*", Some("Run all build tasks")),
                            CompletionItem::value("dev", Some("Start dev server")),
                        ]
                        .into_iter()
                        .filter(|it| it.value.starts_with(&ctx.prefix))
                        .collect();
                        CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
                    },
                )),
            ))
            .subcommand(
                CmdSpec::new("uninstall")
                    .about("Remove dependencies")
                    .visible_aliases(vec!["un", "unlink"]),
            )
            .subcommand(CmdSpec::new("node").about("Run script with Node").positional(
                PositionalSpec::new("file").about("Entry script").value_hint(ValueHint::AnyPath),
            ))
            .subcommand(
                CmdSpec::new("dev")
                    .about("Start dev server")
                    .flag(FlagSpec::new().long("config").takes_value(true).value_hint(
                        ValueHint::FilePath { extensions: vec!["ts".into(), "js".into()] },
                    ))
                    .flag(FlagSpec::new().long("mode").takes_value(true)),
            )
            .subcommand(CmdSpec::new("passthrough").about("Command with default file fallback"));
    (root, None)
}

fn main() {
    if let Some(out) = handle_env_completion(build_specs) {
        print!("{out}");
    }
}
