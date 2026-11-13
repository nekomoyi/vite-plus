#![forbid(unsafe_code)]
#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "completion engine operates directly on standard Path/PathBuf and String types without heavyweight runtime dependencies"
)]

pub(crate) mod assets;
pub mod engine;
pub mod lexer;
pub mod path;
pub mod project;
pub mod shells;
pub mod spec;

pub use engine::{BinKind, classify_vp_binary, complete};
pub use lexer::{BashLexResult, BashQuoteMode, parse_bash_line, parse_bash_line_with_wordbreaks};
pub use path::{PathMode, complete_path};
pub use project::{
    EarlyCompletionOutcome, PkgManifest, ReadManifestFn, VpCompletionConfig, build_vp_spec,
    build_vpx_spec, dispatch_early_completion, get_task_candidates_with,
    get_workspace_package_names_with, is_valid_task_name, parse_task_completion_lines,
    resolve_offline_node_in, sanitize_task_current_prefix, task_completer_with,
};
pub use shells::{
    BashAdapter, FishAdapter, NuAdapter, ParseDialectError, PwshAdapter, ShellAdapter,
    ShellDialect, ShellInputMode, ZshAdapter, handle_env_completion, try_handle_env_completion,
};
pub use spec::{
    CmdSpec, CompleterFn, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput,
    CompletionRequest, FlagSpec, ItemKind, PathCompletion, PositionalSpec, ValueHint,
    bundled_local_specs, render_local_specs_tsv, sort_by_cmp,
};
