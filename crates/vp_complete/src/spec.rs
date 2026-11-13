use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

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

    #[inline]
    pub fn subcommand(value: impl Into<String>, description: Option<impl Into<String>>) -> Self {
        Self::new(value, description, ItemKind::Subcommand)
    }

    #[inline]
    pub fn flag(value: impl Into<String>, description: Option<impl Into<String>>) -> Self {
        Self::new(value, description, ItemKind::Flag)
    }

    #[inline]
    pub fn directory(value: impl Into<String>) -> Self {
        Self::new(value, None::<String>, ItemKind::Directory)
    }

    #[inline]
    pub fn file(value: impl Into<String>) -> Self {
        Self::new(value, None::<String>, ItemKind::File)
    }

    #[inline]
    pub fn value(value: impl Into<String>, description: Option<impl Into<String>>) -> Self {
        Self::new(value, description, ItemKind::Value)
    }

    #[inline]
    pub fn sort_by_value(items: &mut [Self]) {
        sort_by_cmp(items, |a, b| a.value.cmp(&b.value));
    }
}

/// Compact insertion sort for small completion candidate slices (avoids instantiating
/// multi-kilobyte `ipnsort`/`quicksort`/`heapsort` templates per item type).
#[cold]
pub fn sort_by_cmp<T>(slice: &mut [T], cmp: fn(&T, &T) -> std::cmp::Ordering) {
    for i in 1..slice.len() {
        let mut j = i;
        while j > 0 && cmp(&slice[j - 1], &slice[j]).is_gt() {
            slice.swap(j - 1, j);
            j -= 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct CompletionRequest {
    pub words: Vec<String>,
    pub current_index: usize,
    pub cwd: PathBuf,
}

impl CompletionRequest {
    #[inline]
    pub fn new(words: Vec<String>, current_index: usize, cwd: PathBuf) -> Self {
        Self { words, current_index, cwd }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext {
    pub cwd: PathBuf,
    pub prefix: String,
    pub words: Vec<String>,
    pub current_index: usize,
    pub positional_index: usize,
    pub is_local_bin: bool,
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
    #[inline]
    pub fn new(items: Vec<CompletionItem>, directive: CompletionDirective) -> Self {
        Self { items, directive, effective_cwd: None, path_completion: None }
    }

    #[inline]
    pub fn from_items(items: Vec<CompletionItem>) -> Self {
        Self::new(items, CompletionDirective::DEFAULT)
    }

    #[inline]
    pub fn empty() -> Self {
        Self::default()
    }

    #[inline]
    #[must_use]
    pub fn with_path_completion(mut self, mode: PathMode, flag_prefix: impl Into<String>) -> Self {
        self.path_completion = Some(PathCompletion { mode, flag_prefix: flag_prefix.into() });
        self
    }

    #[inline]
    #[must_use]
    pub fn cwd_or<'a>(&'a self, default: &'a Path) -> &'a Path {
        self.effective_cwd.as_deref().unwrap_or(default)
    }
}

pub type CompleterFn = Arc<dyn Fn(&CompletionContext) -> CompletionOutput + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueHint {
    None,
    AnyPath,
    FilePath { extensions: Vec<String> },
    DirPath,
    StaticValues(Vec<(String, Option<String>)>),
}

#[derive(Clone)]
#[must_use]
pub struct FlagSpec {
    pub long: Option<String>,
    pub short: Option<char>,
    pub aliases: Vec<String>,
    pub short_aliases: Vec<char>,
    pub about: Option<String>,
    pub takes_value: bool,
    pub optional_value: bool,
    pub multiple: bool,
    pub global: bool,
    pub hidden: bool,
    pub chdir: bool,
    pub value_hint: ValueHint,
    pub completer: Option<CompleterFn>,
}

impl std::fmt::Debug for FlagSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlagSpec")
            .field("long", &self.long)
            .field("short", &self.short)
            .field("aliases", &self.aliases)
            .field("short_aliases", &self.short_aliases)
            .field("about", &self.about)
            .field("takes_value", &self.takes_value)
            .field("optional_value", &self.optional_value)
            .field("multiple", &self.multiple)
            .field("global", &self.global)
            .field("hidden", &self.hidden)
            .field("chdir", &self.chdir)
            .field("value_hint", &self.value_hint)
            .field("completer", &self.completer.as_ref().map(|_| "<completer>"))
            .finish()
    }
}

impl FlagSpec {
    pub fn new() -> Self {
        Self {
            long: None,
            short: None,
            aliases: Vec::new(),
            short_aliases: Vec::new(),
            about: None,
            takes_value: false,
            optional_value: false,
            multiple: false,
            global: false,
            hidden: false,
            chdir: false,
            value_hint: ValueHint::None,
            completer: None,
        }
    }

    #[inline]
    pub fn long(mut self, long: impl Into<String>) -> Self {
        self.long = Some(long.into());
        self
    }

    #[inline]
    pub fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }

    #[inline]
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    #[inline]
    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    #[inline]
    pub fn short_aliases(mut self, aliases: impl IntoIterator<Item = char>) -> Self {
        self.short_aliases.extend(aliases);
        self
    }

    #[inline]
    pub fn takes_value(mut self, takes: bool) -> Self {
        self.takes_value = takes;
        self
    }

    #[inline]
    pub fn optional_value(mut self, optional: bool) -> Self {
        self.optional_value = optional;
        self
    }

    #[inline]
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    #[inline]
    pub fn global(mut self, global: bool) -> Self {
        self.global = global;
        self
    }

    #[inline]
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    #[inline]
    pub fn chdir(mut self, chdir: bool) -> Self {
        self.chdir = chdir;
        if chdir && self.value_hint == ValueHint::None {
            self.value_hint = ValueHint::DirPath;
        }
        self
    }

    #[inline]
    pub fn value_hint(mut self, hint: ValueHint) -> Self {
        self.value_hint = hint;
        self
    }

    #[inline]
    pub fn completer(mut self, completer: CompleterFn) -> Self {
        self.completer = Some(completer);
        self
    }

    #[cold]
    #[must_use]
    pub fn parse_tsv(tsv: &str) -> Vec<Self> {
        let mut out = Vec::new();
        for raw_line in tsv.lines() {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            out.push(parse_flag_tsv_row(line));
        }
        out
    }
}

impl Default for FlagSpec {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
#[must_use]
pub struct PositionalSpec {
    pub name: String,
    pub about: Option<String>,
    pub multiple: bool,
    pub last: bool,
    pub trailing_var_arg: bool,
    pub value_hint: ValueHint,
    pub completer: Option<CompleterFn>,
}

impl std::fmt::Debug for PositionalSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositionalSpec")
            .field("name", &self.name)
            .field("about", &self.about)
            .field("multiple", &self.multiple)
            .field("last", &self.last)
            .field("trailing_var_arg", &self.trailing_var_arg)
            .field("value_hint", &self.value_hint)
            .field("completer", &self.completer.as_ref().map(|_| "<completer>"))
            .finish()
    }
}

impl PositionalSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: None,
            multiple: false,
            last: false,
            trailing_var_arg: false,
            value_hint: ValueHint::None,
            completer: None,
        }
    }

    #[inline]
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    #[inline]
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    #[inline]
    pub fn last(mut self, last: bool) -> Self {
        self.last = last;
        self
    }

    #[inline]
    pub fn trailing_var_arg(mut self, trailing: bool) -> Self {
        self.trailing_var_arg = trailing;
        self
    }

    #[inline]
    pub fn value_hint(mut self, hint: ValueHint) -> Self {
        self.value_hint = hint;
        self
    }

    #[inline]
    pub fn completer(mut self, completer: CompleterFn) -> Self {
        self.completer = Some(completer);
        self
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct CmdSpec {
    pub name: String,
    pub about: Option<String>,
    pub visible_aliases: Vec<String>,
    pub hidden_aliases: Vec<String>,
    pub hidden: bool,
    pub global_only: bool,
    pub flags: Vec<FlagSpec>,
    pub positionals: Vec<PositionalSpec>,
    pub subcommands: Vec<CmdSpec>,
}

impl CmdSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: None,
            visible_aliases: Vec::new(),
            hidden_aliases: Vec::new(),
            hidden: false,
            global_only: false,
            flags: Vec::new(),
            positionals: Vec::new(),
            subcommands: Vec::new(),
        }
    }

    #[inline]
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    #[inline]
    pub fn visible_alias(mut self, alias: impl Into<String>) -> Self {
        self.visible_aliases.push(alias.into());
        self
    }

    #[inline]
    pub fn visible_aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.visible_aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    #[inline]
    pub fn hidden_alias(mut self, alias: impl Into<String>) -> Self {
        self.hidden_aliases.push(alias.into());
        self
    }

    #[inline]
    pub fn hidden_aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.hidden_aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    #[inline]
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    #[inline]
    pub fn global_only(mut self, global_only: bool) -> Self {
        self.global_only = global_only;
        self
    }

    #[inline]
    pub fn flag(mut self, flag: FlagSpec) -> Self {
        self.flags.push(flag);
        self
    }

    #[inline]
    pub fn flags(mut self, flags: impl IntoIterator<Item = FlagSpec>) -> Self {
        self.flags.extend(flags);
        self
    }

    #[inline]
    pub fn positional(mut self, positional: PositionalSpec) -> Self {
        self.positionals.push(positional);
        self
    }

    #[inline]
    pub fn positionals(mut self, positionals: impl IntoIterator<Item = PositionalSpec>) -> Self {
        self.positionals.extend(positionals);
        self
    }

    #[inline]
    pub fn subcommand(mut self, subcommand: CmdSpec) -> Self {
        self.subcommands.push(subcommand);
        self
    }

    #[inline]
    pub fn subcommands(mut self, subcommands: impl IntoIterator<Item = CmdSpec>) -> Self {
        self.subcommands.extend(subcommands);
        self
    }

    #[cold]
    #[must_use]
    pub fn parse_tsv(tsv: &str) -> Vec<Self> {
        let mut roots = Vec::new();
        let mut stack: Vec<Self> = Vec::new();
        let pop_to = |depth: usize, stack: &mut Vec<Self>, roots: &mut Vec<Self>| {
            while stack.len() > depth {
                if let Some(done) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.subcommands.push(done);
                    } else {
                        roots.push(done);
                    }
                }
            }
        };

        for raw_line in tsv.lines() {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("C\t") {
                let mut parts = rest.splitn(6, '\t');
                let depth = parts.next().and_then(|d| d.parse::<usize>().ok()).unwrap_or(0);
                let name = unescape_tsv_field(parts.next().unwrap_or(""));
                let vis_csv = parts.next().unwrap_or("");
                let hid_csv = parts.next().unwrap_or("");
                let mods = parts.next().unwrap_or("");
                let about_raw = parts.next().unwrap_or("");

                pop_to(depth, &mut stack, &mut roots);

                let mut cmd = Self::new(name);
                if !about_raw.is_empty() {
                    cmd.about = Some(unescape_tsv_field(about_raw));
                }
                if !vis_csv.is_empty() {
                    for item in vis_csv.split(',') {
                        cmd.visible_aliases.push(unescape_tsv_field(item));
                    }
                }
                if !hid_csv.is_empty() {
                    for item in hid_csv.split(',') {
                        cmd.hidden_aliases.push(unescape_tsv_field(item));
                    }
                }
                for ch in mods.chars() {
                    match ch {
                        'h' => cmd.hidden = true,
                        'G' => cmd.global_only = true,
                        _ => {}
                    }
                }
                stack.push(cmd);
            } else if let Some(rest) = line.strip_prefix("F\t") {
                if let Some(cur) = stack.last_mut() {
                    cur.flags.push(parse_flag_tsv_row(rest));
                }
            } else if let Some(rest) = line.strip_prefix("P\t")
                && let Some(cur) = stack.last_mut()
            {
                let mut parts = rest.splitn(4, '\t');
                let name = unescape_tsv_field(parts.next().unwrap_or(""));
                let mods = parts.next().unwrap_or("");
                let hint_raw = parts.next().unwrap_or("");
                let about_raw = parts.next().unwrap_or("");
                let mut pos = PositionalSpec::new(name).value_hint(decode_value_hint(hint_raw));
                for ch in mods.chars() {
                    match ch {
                        'm' => pos.multiple = true,
                        'l' => pos.last = true,
                        't' => pos.trailing_var_arg = true,
                        _ => {}
                    }
                }
                if !about_raw.is_empty() {
                    pos.about = Some(unescape_tsv_field(about_raw));
                }
                cur.positionals.push(pos);
            }
        }
        pop_to(0, &mut stack, &mut roots);
        roots
    }

    /// Build [`CmdSpec`] from a mutable [`clap::Command`] without cloning or dropping it here.
    #[inline]
    pub fn from_clap_mut(cmd: &mut clap::Command) -> Self {
        cmd.build();
        Self::from_built_clap(cmd)
    }

    /// Build [`CmdSpec`] directly from an owned [`clap::Command`] without cloning its AST.
    #[inline]
    pub fn from_clap_owned(mut cmd: clap::Command) -> Self {
        Self::from_clap_mut(&mut cmd)
    }

    /// Build [`CmdSpec`] for a single named subcommand from a mutable [`clap::Command`].
    #[inline]
    #[must_use]
    pub fn from_clap_subcommand_mut(cmd: &mut clap::Command, subcommand: &str) -> Option<Self> {
        cmd.build();
        cmd.find_subcommand(subcommand).map(Self::from_built_clap)
    }

    /// Build [`CmdSpec`] for a single named subcommand from an owned [`clap::Command`] without cloning.
    #[inline]
    #[must_use]
    pub fn from_clap_subcommand(mut cmd: clap::Command, subcommand: &str) -> Option<Self> {
        Self::from_clap_subcommand_mut(&mut cmd, subcommand)
    }

    #[inline]
    pub fn from_clap(cmd: &clap::Command) -> Self {
        Self::from_clap_owned(cmd.clone())
    }

    #[cold]
    fn from_built_clap(cmd: &clap::Command) -> Self {
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
                    completer: None,
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
                    value_hint,
                    completer: None,
                });
            }
        }

        for sub in cmd.get_subcommands() {
            spec.subcommands.push(CmdSpec::from_built_clap(sub));
        }
        spec
    }
}

#[cold]
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

// =============================================================================
// Compact zero-relocation `.rodata` TSV codec (`__TEXT.__const`, 0 pointers)
// =============================================================================

#[cold]
fn parse_flag_tsv_row(row: &str) -> FlagSpec {
    let mut parts = row.splitn(5, '\t');
    let short_csv = parts.next().unwrap_or("");
    let long_csv = parts.next().unwrap_or("");
    let mods = parts.next().unwrap_or("");
    let hint_raw = parts.next().unwrap_or("");
    let about_raw = parts.next().unwrap_or("");

    let mut flag = FlagSpec::new();
    if !short_csv.is_empty() {
        for (idx, item) in short_csv.split(',').enumerate() {
            if let Some(ch) = unescape_tsv_field(item).chars().next() {
                if idx == 0 {
                    flag.short = Some(ch);
                } else {
                    flag.short_aliases.push(ch);
                }
            }
        }
    }
    if !long_csv.is_empty() {
        for (idx, item) in long_csv.split(',').enumerate() {
            let s = unescape_tsv_field(item);
            if !s.is_empty() {
                if idx == 0 {
                    flag.long = Some(s);
                } else {
                    flag.aliases.push(s);
                }
            }
        }
    }
    for ch in mods.chars() {
        match ch {
            'v' => flag.takes_value = true,
            'o' => flag.optional_value = true,
            'm' => flag.multiple = true,
            'g' => flag.global = true,
            'h' => flag.hidden = true,
            'C' => flag.chdir = true,
            _ => {}
        }
    }
    flag.value_hint = decode_value_hint(hint_raw);
    if flag.chdir && flag.value_hint == ValueHint::None {
        flag.value_hint = ValueHint::DirPath;
    }
    if !about_raw.is_empty() {
        flag.about = Some(unescape_tsv_field(about_raw));
    }
    flag
}

#[cold]
fn escape_tsv_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

#[cold]
fn escape_tsv_val_atom(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '|' => out.push_str("\\p"),
            '=' => out.push_str("\\e"),
            ',' => out.push_str("\\c"),
            _ => out.push(ch),
        }
    }
    out
}

#[cold]
fn unescape_tsv_field(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('p') => out.push('|'),
                Some('e') => out.push('='),
                Some('c') => out.push(','),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cold]
fn decode_value_hint(raw: &str) -> ValueHint {
    if raw.is_empty() {
        ValueHint::None
    } else if raw == "any" {
        ValueHint::AnyPath
    } else if raw == "dir" {
        ValueHint::DirPath
    } else if raw == "file" {
        ValueHint::FilePath { extensions: Vec::new() }
    } else if let Some(exts) = raw.strip_prefix("file:") {
        let mut extensions = Vec::new();
        if !exts.is_empty() {
            for item in exts.split(',') {
                extensions.push(unescape_tsv_field(item));
            }
        }
        ValueHint::FilePath { extensions }
    } else if let Some(csv) = raw.strip_prefix("vals:") {
        let mut vals = Vec::new();
        if !csv.is_empty() {
            for v in csv.split(',') {
                vals.push((unescape_tsv_field(v), None));
            }
        }
        ValueHint::StaticValues(vals)
    } else if let Some(entries) = raw.strip_prefix("vals|") {
        let mut vals = Vec::new();
        if !entries.is_empty() {
            for entry in entries.split('|') {
                vals.push(match entry.split_once('=') {
                    Some((v, d)) => (unescape_tsv_field(v), Some(unescape_tsv_field(d))),
                    None => (unescape_tsv_field(entry), None),
                });
            }
        }
        ValueHint::StaticValues(vals)
    } else {
        ValueHint::None
    }
}

#[cold]
fn encode_value_hint(hint: &ValueHint) -> String {
    match hint {
        ValueHint::None => String::new(),
        ValueHint::AnyPath => "any".to_string(),
        ValueHint::DirPath => "dir".to_string(),
        ValueHint::FilePath { extensions } => {
            if extensions.is_empty() {
                "file".to_string()
            } else {
                let list = extensions.iter().map(|e| escape_tsv_val_atom(e)).collect::<Vec<_>>();
                format!("file:{}", list.join(","))
            }
        }
        ValueHint::StaticValues(vals) => {
            if vals.iter().all(|(v, d)| d.is_none() && !v.contains([',', '\\', '\t', '\n', '\r'])) {
                let list = vals.iter().map(|(v, _)| v.as_str()).collect::<Vec<_>>();
                format!("vals:{}", list.join(","))
            } else {
                let list = vals
                    .iter()
                    .map(|(v, d)| match d {
                        Some(desc) => {
                            format!("{}={}", escape_tsv_val_atom(v), escape_tsv_val_atom(desc))
                        }
                        None => escape_tsv_val_atom(v),
                    })
                    .collect::<Vec<_>>();
                format!("vals|{}", list.join("|"))
            }
        }
    }
}

/// Parse the bundled local CLI command definitions (`src/local_specs.tsv`).
#[inline]
#[must_use]
pub fn bundled_local_specs() -> Vec<CmdSpec> {
    CmdSpec::parse_tsv(crate::assets::slice(crate::assets::RANGE_LOCAL_SPECS))
}

/// Render a slice of [`CmdSpec`] values into deterministic TSV (`local_specs.tsv`).
#[must_use]
pub fn render_local_specs_tsv(specs: &[CmdSpec], generator_hint: &str) -> String {
    use std::fmt::Write as _;

    let mut rows = Vec::new();
    for spec in specs {
        collect_cmd_tsv_rows(spec, 0, &mut rows);
    }

    let mut out = String::new();
    let _ = writeln!(out, "# @generated by `{generator_hint}`");
    let _ = writeln!(out, "# Do not edit manually; modify the source Clap definitions instead.");
    for row in &rows {
        let _ = writeln!(out, "{}", row.trim_end_matches('\t'));
    }
    out
}

fn concise_about(s: &str) -> &str {
    let trimmed = s.trim();
    let first = trimmed.split_once(". ").map_or(trimmed, |(head, _)| head);
    first.strip_suffix('.').unwrap_or(first)
}

fn collect_cmd_tsv_rows(spec: &CmdSpec, depth: usize, rows: &mut Vec<String>) {
    let vis =
        spec.visible_aliases.iter().map(|a| escape_tsv_val_atom(a)).collect::<Vec<_>>().join(",");
    let hid =
        spec.hidden_aliases.iter().map(|a| escape_tsv_val_atom(a)).collect::<Vec<_>>().join(",");
    let mut mods = String::new();
    if spec.hidden {
        mods.push('h');
    }
    if spec.global_only {
        mods.push('G');
    }
    let about =
        spec.about.as_deref().map(|s| escape_tsv_field(concise_about(s))).unwrap_or_default();
    rows.push(format!(
        "C\t{depth}\t{}\t{vis}\t{hid}\t{mods}\t{about}",
        escape_tsv_field(&spec.name)
    ));

    for flag in &spec.flags {
        let mut shorts = Vec::new();
        if let Some(s) = flag.short {
            shorts.push(escape_tsv_val_atom(&s.to_string()));
        }
        for s in &flag.short_aliases {
            shorts.push(escape_tsv_val_atom(&s.to_string()));
        }
        let mut longs = Vec::new();
        if let Some(l) = &flag.long {
            longs.push(escape_tsv_val_atom(l));
        }
        for a in &flag.aliases {
            longs.push(escape_tsv_val_atom(a));
        }
        let mut fmods = String::new();
        if flag.takes_value {
            fmods.push('v');
        }
        if flag.optional_value {
            fmods.push('o');
        }
        if flag.multiple {
            fmods.push('m');
        }
        if flag.global {
            fmods.push('g');
        }
        if flag.hidden {
            fmods.push('h');
        }
        if flag.chdir {
            fmods.push('C');
        }
        let hint = encode_value_hint(&flag.value_hint);
        let fabout =
            flag.about.as_deref().map(|s| escape_tsv_field(concise_about(s))).unwrap_or_default();
        rows.push(format!(
            "F\t{}\t{}\t{fmods}\t{hint}\t{fabout}",
            shorts.join(","),
            longs.join(",")
        ));
    }

    for pos in &spec.positionals {
        let mut pmods = String::new();
        if pos.multiple {
            pmods.push('m');
        }
        if pos.last {
            pmods.push('l');
        }
        if pos.trailing_var_arg {
            pmods.push('t');
        }
        let hint = encode_value_hint(&pos.value_hint);
        let pabout =
            pos.about.as_deref().map(|s| escape_tsv_field(concise_about(s))).unwrap_or_default();
        rows.push(format!("P\t{}\t{pmods}\t{hint}\t{pabout}", escape_tsv_field(&pos.name)));
    }

    for sub in &spec.subcommands {
        collect_cmd_tsv_rows(sub, depth + 1, rows);
    }
}
