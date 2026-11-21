#![forbid(unsafe_code)]
#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "completion adapters use standard library paths and strings for shell input"
)]

mod dispatch;
pub mod engine;
pub mod lexer;
pub mod path;
pub mod shells;
pub mod spec;

pub use dispatch::{EarlyCompletionOutcome, dispatch_early_completion};
pub use engine::{Argument, ValueProvider, complete};
pub use lexer::{BashLexResult, BashQuoteMode, parse_bash_line, parse_bash_line_with_wordbreaks};
pub use path::{PathMode, complete_path};
pub use shells::{
    BashAdapter, FishAdapter, NuAdapter, ParseDialectError, ShellAdapter, ShellDialect,
    ShellInputMode, ZshAdapter,
};
pub use spec::{
    CmdSpec, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput,
    CompletionRequest, FlagSpec, ItemKind, PathCompletion, PositionalSpec, ValueHint,
};
