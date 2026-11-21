#![expect(
    clippy::disallowed_macros,
    clippy::print_stdout,
    reason = "completion CLI mock must print completion output directly to stdout"
)]

use vp_complete::{
    CmdSpec, EarlyCompletionOutcome, FlagSpec, PositionalSpec, ValueHint, dispatch_early_completion,
};

fn build_spec() -> CmdSpec {
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
            PositionalSpec::new("script").about("Script name to execute").value_hint(
                ValueHint::StaticValues(vec![
                    ("@vitejs/vite-monorepo#build".into(), Some("Build monorepo workspace".into())),
                    ("build:watch".into(), Some("Build and watch".into())),
                    ("build:*".into(), Some("Run all build tasks".into())),
                    ("dev".into(), Some("Start dev server".into())),
                ]),
            ),
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
                .flag(
                    FlagSpec::new().long("config").takes_value(true).value_hint(
                        ValueHint::FilePath { extensions: vec!["ts".into(), "js".into()] },
                    ),
                )
                .flag(FlagSpec::new().long("mode").takes_value(true)),
        )
        .subcommand(CmdSpec::new("passthrough").about("Command with default file fallback"))
}

fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    let Some(dialect) = std::env::var_os("VP_COMPLETE") else { return ExitCode::SUCCESS };
    match dispatch_early_completion(&dialect, |req| {
        vp_complete::complete(&build_spec(), req, &|_, _, _| None)
    }) {
        EarlyCompletionOutcome::Handled(output) => print!("{output}"),
        EarlyCompletionOutcome::InvalidDialect => return ExitCode::FAILURE,
        EarlyCompletionOutcome::ContinueCli => {}
    }
    ExitCode::SUCCESS
}
