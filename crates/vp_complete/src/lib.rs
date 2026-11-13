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
pub mod spec;

pub use engine::{BinKind, classify_vp_binary, complete};
pub use lexer::{BashLexResult, BashQuoteMode, parse_bash_line, parse_bash_line_with_wordbreaks};
pub use path::{PathMode, complete_path};
pub use spec::{
    CmdSpec, CompleterFn, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput,
    CompletionRequest, FlagSpec, ItemKind, PathCompletion, PositionalSpec, ValueHint,
    bundled_local_specs, render_local_specs_tsv, sort_by_cmp,
};
