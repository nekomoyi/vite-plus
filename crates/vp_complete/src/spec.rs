use std::path::{Path, PathBuf};

use crate::path::PathMode;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct CompletionDirective: u32 {
        const DEFAULT = 0;
        const NO_SPACE = 1 << 1;     // 2
        const NO_FILE_COMP = 1 << 2; // 4
        const KEEP_ORDER = 1 << 5;   // 32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Subcommand,
    Flag,
    Directory,
    File,
    Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct CompletionItem {
    pub value: String,
    pub description: Option<String>,
    pub kind: ItemKind,
}

impl CompletionItem {
    pub fn new(
        value: impl Into<String>,
        description: Option<impl Into<String>>,
        kind: ItemKind,
    ) -> Self {
        Self { value: value.into(), description: description.map(Into::into), kind }
    }

    pub fn subcommand(value: impl Into<String>, description: Option<impl Into<String>>) -> Self {
        Self::new(value, description, ItemKind::Subcommand)
    }

    pub fn flag(value: impl Into<String>, description: Option<impl Into<String>>) -> Self {
        Self::new(value, description, ItemKind::Flag)
    }

    pub fn directory(value: impl Into<String>) -> Self {
        Self::new(value, None::<String>, ItemKind::Directory)
    }

    pub fn file(value: impl Into<String>) -> Self {
        Self::new(value, None::<String>, ItemKind::File)
    }

    pub fn value(value: impl Into<String>, description: Option<impl Into<String>>) -> Self {
        Self::new(value, description, ItemKind::Value)
    }

    pub fn sort_by_value(items: &mut [Self]) {
        items.sort_by(|a, b| a.value.cmp(&b.value));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct CompletionRequest {
    pub words: Vec<String>,
    pub current_index: usize,
    pub literal_tilde: bool,
    pub(crate) bash_cursor: Option<crate::lexer::BashCursor>,
    pub cwd: PathBuf,
}

impl CompletionRequest {
    pub fn new(words: Vec<String>, current_index: usize, cwd: PathBuf) -> Self {
        Self { words, current_index, cwd, literal_tilde: false, bash_cursor: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext<'a> {
    pub cwd: PathBuf,
    pub prefix: String,
    pub positional_index: usize,
    pub literal_tilde: bool,
    pub parent: Option<&'a str>,
}

impl CompletionContext<'_> {
    pub fn complete_path(&self, mode: PathMode) -> CompletionOutput {
        crate::path::complete_paths(
            &self.cwd,
            crate::path::current_home_dir().as_deref(),
            &self.prefix,
            mode,
            !self.literal_tilde,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathCompletion {
    pub mode: PathMode,
    pub flag_prefix: String,
}

#[derive(Clone, PartialEq, Eq, Default)]
#[must_use]
pub struct CompletionOutput {
    pub items: Vec<CompletionItem>,
    pub directive: CompletionDirective,
    pub effective_cwd: Option<PathBuf>,
    pub path_completion: Option<PathCompletion>,
}

impl std::fmt::Debug for CompletionOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("CompletionOutput");
        s.field("items", &self.items).field("directive", &self.directive);
        if let Some(cwd) = &self.effective_cwd {
            s.field("effective_cwd", cwd);
        }
        if let Some(pc) = &self.path_completion {
            s.field("path_completion", pc);
        }
        s.finish()
    }
}

impl CompletionOutput {
    pub fn new(items: Vec<CompletionItem>, directive: CompletionDirective) -> Self {
        Self { items, directive, effective_cwd: None, path_completion: None }
    }

    pub fn from_items(items: Vec<CompletionItem>) -> Self {
        Self::new(items, CompletionDirective::DEFAULT)
    }

    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_path_completion(mut self, mode: PathMode, flag_prefix: impl Into<String>) -> Self {
        self.path_completion = Some(PathCompletion { mode, flag_prefix: flag_prefix.into() });
        self
    }

    #[must_use]
    pub fn cwd_or<'a>(&'a self, default: &'a Path) -> &'a Path {
        self.effective_cwd.as_deref().unwrap_or(default)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueHint {
    #[default]
    None,
    AnyPath,
    FilePath {
        extensions: Vec<String>,
    },
    DirPath,
    StaticValues(Vec<(String, Option<String>)>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[must_use]
pub struct FlagSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<char>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub short_aliases: Vec<char>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub takes_value: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional_value: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub global: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chdir: bool,
    /// A wrapper option accepted only at the start of the argument list.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub leading_only: bool,
    #[serde(default, skip_serializing_if = "ValueHint::is_none")]
    pub value_hint: ValueHint,
}

impl ValueHint {
    fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

impl FlagSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn long(mut self, long: impl Into<String>) -> Self {
        self.long = Some(long.into());
        self
    }

    pub fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }

    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    pub fn short_aliases(mut self, aliases: impl IntoIterator<Item = char>) -> Self {
        self.short_aliases.extend(aliases);
        self
    }

    pub fn takes_value(mut self, takes: bool) -> Self {
        self.takes_value = takes;
        self
    }

    pub fn optional_value(mut self, optional: bool) -> Self {
        self.optional_value = optional;
        self
    }

    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    pub fn global(mut self, global: bool) -> Self {
        self.global = global;
        self
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    pub fn chdir(mut self, chdir: bool) -> Self {
        self.chdir = chdir;
        if chdir && self.value_hint == ValueHint::None {
            self.value_hint = ValueHint::DirPath;
        }
        self
    }

    pub fn value_hint(mut self, hint: ValueHint) -> Self {
        self.value_hint = hint;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[must_use]
pub struct PositionalSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trailing_var_arg: bool,
    #[serde(default, skip_serializing_if = "ValueHint::is_none")]
    pub value_hint: ValueHint,
}

impl PositionalSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    pub fn last(mut self, last: bool) -> Self {
        self.last = last;
        self
    }

    pub fn trailing_var_arg(mut self, trailing: bool) -> Self {
        self.trailing_var_arg = trailing;
        self
    }

    pub fn value_hint(mut self, hint: ValueHint) -> Self {
        self.value_hint = hint;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[must_use]
pub struct CmdSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visible_aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden_aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<FlagSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub positionals: Vec<PositionalSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subcommands: Vec<CmdSpec>,
}

impl CmdSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    pub fn visible_alias(mut self, alias: impl Into<String>) -> Self {
        self.visible_aliases.push(alias.into());
        self
    }

    pub fn visible_aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.visible_aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    pub fn hidden_alias(mut self, alias: impl Into<String>) -> Self {
        self.hidden_aliases.push(alias.into());
        self
    }

    pub fn hidden_aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.hidden_aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    pub fn flag(mut self, flag: FlagSpec) -> Self {
        self.flags.push(flag);
        self
    }

    pub fn flags(mut self, flags: impl IntoIterator<Item = FlagSpec>) -> Self {
        self.flags.extend(flags);
        self
    }

    pub fn positional(mut self, positional: PositionalSpec) -> Self {
        self.positionals.push(positional);
        self
    }

    pub fn positionals(mut self, positionals: impl IntoIterator<Item = PositionalSpec>) -> Self {
        self.positionals.extend(positionals);
        self
    }

    pub fn subcommand(mut self, subcommand: CmdSpec) -> Self {
        self.subcommands.push(subcommand);
        self
    }

    pub fn subcommands(mut self, subcommands: impl IntoIterator<Item = CmdSpec>) -> Self {
        self.subcommands.extend(subcommands);
        self
    }

    pub fn from_clap(mut cmd: clap::Command) -> Self {
        cmd.build();
        Self::from_built_clap(&cmd)
    }

    /// Convert an already-built command, including its inferred flags and subcommands.
    pub fn from_built_clap(cmd: &clap::Command) -> Self {
        let mut spec = CmdSpec::new(cmd.get_name());
        spec.about = cmd.get_about().map(ToString::to_string);
        spec.hidden = cmd.is_hide_set();
        for a in cmd.get_visible_aliases() {
            spec.visible_aliases.push(a.to_string());
        }
        for a in cmd.get_all_aliases() {
            if !spec.visible_aliases.iter().any(|v| v == a) {
                spec.hidden_aliases.push(a.to_string());
            }
        }

        for arg in cmd.get_arguments() {
            let value_hint = convert_clap_value_hint(arg);
            if arg.is_positional() {
                let multiple = arg.get_num_args().is_some_and(|range| range.max_values() > 1)
                    || matches!(arg.get_action(), clap::ArgAction::Append);
                spec.positionals.push(PositionalSpec {
                    name: arg.get_id().as_str().to_string(),
                    about: arg.get_help().map(ToString::to_string),
                    multiple,
                    last: arg.is_last_set(),
                    trailing_var_arg: arg.is_trailing_var_arg_set(),
                    value_hint,
                });
            } else {
                let accepts_values = arg.get_action().takes_values();
                let min_values = arg
                    .get_num_args()
                    .map_or(usize::from(accepts_values), |range| range.min_values());
                let mut aliases = Vec::new();
                if let Some(vis) = arg.get_visible_aliases() {
                    for a in vis {
                        aliases.push(a.to_string());
                    }
                }
                let mut short_aliases = Vec::new();
                if let Some(vis) = arg.get_visible_short_aliases() {
                    short_aliases.extend(vis);
                }
                spec.flags.push(FlagSpec {
                    long: arg.get_long().map(ToString::to_string),
                    short: arg.get_short(),
                    aliases,
                    short_aliases,
                    about: arg.get_help().map(ToString::to_string),
                    takes_value: accepts_values && min_values >= 1,
                    optional_value: accepts_values && min_values == 0,
                    multiple: matches!(
                        arg.get_action(),
                        clap::ArgAction::Append | clap::ArgAction::Count
                    ),
                    global: arg.is_global_set(),
                    hidden: arg.is_hide_set(),
                    chdir: false,
                    leading_only: false,
                    value_hint,
                });
            }
        }

        for sub in cmd.get_subcommands() {
            spec.subcommands.push(CmdSpec::from_built_clap(sub));
        }
        spec
    }
}

fn convert_clap_value_hint(arg: &clap::Arg) -> ValueHint {
    let pvs = arg.get_possible_values();
    if !pvs.is_empty() {
        let mut vals = Vec::with_capacity(pvs.len());
        for pv in pvs {
            vals.push((pv.get_name().to_string(), pv.get_help().map(ToString::to_string)));
        }
        return ValueHint::StaticValues(vals);
    }

    match arg.get_value_hint() {
        clap::ValueHint::DirPath => ValueHint::DirPath,
        clap::ValueHint::FilePath => ValueHint::FilePath { extensions: Vec::new() },
        clap::ValueHint::AnyPath => ValueHint::AnyPath,
        _ => ValueHint::None,
    }
}
