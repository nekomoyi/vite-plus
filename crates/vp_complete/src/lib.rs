#![forbid(unsafe_code)]
#![expect(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "completion adapters use standard library paths and strings for shell input"
)]

pub mod path;
pub mod spec;

pub use path::{PathMode, complete_path};
pub use spec::{
    CmdSpec, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput,
    CompletionRequest, FlagSpec, ItemKind, PathCompletion, PositionalSpec, ValueHint,
};
