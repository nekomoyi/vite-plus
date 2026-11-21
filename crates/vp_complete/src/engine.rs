use crate::{
    path::{PathMode, expand_tilde_item},
    spec::{
        CmdSpec, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput,
        CompletionRequest, FlagSpec, ItemKind, PositionalSpec, ValueHint,
    },
};

/// The argument whose value is being completed. Hosts can supply project-specific candidates.
#[derive(Clone, Copy)]
pub enum Argument<'a> {
    Flag(&'a FlagSpec),
    Positional(&'a PositionalSpec),
}

pub type ValueProvider<'a> =
    dyn Fn(&CmdSpec, Argument<'_>, &CompletionContext) -> Option<CompletionOutput> + 'a;

pub fn complete<'a>(
    root: &'a CmdSpec,
    req: &CompletionRequest,
    provider: &ValueProvider<'_>,
) -> CompletionOutput {
    if req.current_index == 0 || req.current_index > req.words.len() {
        return CompletionOutput::empty();
    }

    let mut active_spec = root;
    let mut ancestor_specs = Vec::new();

    let current_index = req.current_index;
    let current_word = req.words.get(current_index).map_or("", String::as_str);

    let mut effective_cwd = req.cwd.clone();
    let mut used_flag_keys = Vec::new();
    let mut past_double_dash = false;
    let mut trailing_args = false;
    let mut positional_index = 0;
    let mut expecting_flag_val: Option<&'a FlagSpec> = None;

    for (rel_idx, token) in req.words[1..current_index].iter().enumerate() {
        if let Some(flag) = expecting_flag_val.take()
            && (flag.takes_value || !token.starts_with('-'))
        {
            if flag.chdir {
                effective_cwd = effective_cwd.join(token);
            }
            continue;
        }

        if past_double_dash || trailing_args {
            positional_index += 1;
            continue;
        }

        if token == "--" {
            past_double_dash = true;
            continue;
        }

        let leading = rel_idx == 0;

        if token.starts_with('-') && token.len() > 1 {
            let value = scan_flags(token, active_spec, &ancestor_specs, leading, |flag| {
                if !flag.multiple {
                    push_unique(&mut used_flag_keys, flag_key(flag));
                }
            });
            if let Some((flag, value_start)) = value {
                match value_start {
                    Some(start) if flag.chdir => {
                        effective_cwd = effective_cwd.join(&token[start..])
                    }
                    None => expecting_flag_val = Some(flag),
                    _ => {}
                }
            }
            continue;
        }

        // Non-flag token: subcommand transition only at positional_index == 0
        if positional_index == 0
            && let Some(sub) = find_subcommand(active_spec, token)
        {
            ancestor_specs.push(active_spec);
            active_spec = sub;
            positional_index = 0;
        } else {
            if select_positional(&active_spec.positionals, positional_index, past_double_dash)
                .is_some_and(|pos| pos.trailing_var_arg)
            {
                trailing_args = true;
            }
            positional_index += 1;
            if select_positional(&active_spec.positionals, positional_index, past_double_dash)
                .is_some_and(|pos| pos.trailing_var_arg)
            {
                trailing_args = true;
            }
        }
    }

    let mut ctx = CompletionContext {
        cwd: effective_cwd.clone(),
        prefix: current_word.to_owned(),

        positional_index,
        literal_tilde: req.literal_tilde,
        parent: ancestor_specs.last().map(|cmd| cmd.name.as_str()),
    };

    let leading = current_index == 1;

    let mut out = compute_target_completions(
        active_spec,
        &ancestor_specs,
        &used_flag_keys,
        past_double_dash,
        trailing_args,
        expecting_flag_val,
        leading,
        &mut ctx,
        provider,
    );

    if effective_cwd != req.cwd {
        out.effective_cwd = Some(effective_cwd);
    }
    out
}

type FlagKey<'a> = (Option<&'a str>, Option<char>);

fn push_unique<T: PartialEq>(vec: &mut Vec<T>, val: T) -> bool {
    if vec.contains(&val) {
        false
    } else {
        vec.push(val);
        true
    }
}

#[expect(clippy::too_many_arguments)]
fn compute_target_completions<'a>(
    active_spec: &'a CmdSpec,
    ancestor_specs: &[&'a CmdSpec],
    used_flag_keys: &[FlagKey<'a>],
    past_double_dash: bool,
    trailing_args: bool,
    expecting_flag_val: Option<&'a FlagSpec>,
    leading: bool,
    ctx: &mut CompletionContext,
    provider: &ValueProvider<'_>,
) -> CompletionOutput {
    // Case 1: Cursor is fulfilling an expected flag value from the preceding token
    if let Some(flag) = expecting_flag_val {
        if flag.takes_value {
            return complete_value(active_spec, Argument::Flag(flag), ctx, provider);
        }
        if !ctx.prefix.starts_with('-') {
            let opt_out = complete_value(active_spec, Argument::Flag(flag), ctx, provider);
            if !opt_out.items.is_empty() || opt_out.path_completion.is_some() {
                return opt_out;
            }
        }
    }

    let parse_wrapper_flags = !past_double_dash && !trailing_args;

    // Case 2: Cursor is on an inline or attached flag value before `--` / trailing varargs
    if parse_wrapper_flags && ctx.prefix.starts_with('-') {
        if let Some((flag, Some(start))) =
            scan_flags(&ctx.prefix, active_spec, ancestor_specs, leading, |_| {})
        {
            let value = ctx.prefix.split_off(start);
            let head = std::mem::replace(&mut ctx.prefix, value);
            return prefix_items(
                complete_value(active_spec, Argument::Flag(flag), ctx, provider),
                &head,
                ctx.literal_tilde,
            );
        }
        return complete_flags(active_spec, ancestor_specs, used_flag_keys, &ctx.prefix, leading);
    }

    // Case 4: Non-flag completions (subcommands and/or positionals)
    let mut items = Vec::new();
    let mut directive = CompletionDirective::DEFAULT;
    let mut path_completion = None;
    let mut had_subcommands = false;

    if parse_wrapper_flags && ctx.positional_index == 0 && !active_spec.subcommands.is_empty() {
        let sub_out = complete_subcommands(active_spec, &ctx.prefix);
        had_subcommands = !sub_out.items.is_empty();
        items.extend(sub_out.items);
        directive |= sub_out.directive;
    }

    if let Some(pos) =
        select_positional(&active_spec.positionals, ctx.positional_index, past_double_dash)
    {
        let pos_out = complete_value(active_spec, Argument::Positional(pos), ctx, provider);
        if !had_subcommands {
            path_completion = pos_out.path_completion;
        }
        items.extend(pos_out.items);
        directive |= pos_out.directive;
    }

    let mut out = CompletionOutput::new(items, directive);
    out.path_completion = path_completion;
    out
}

fn prefix_items(mut out: CompletionOutput, prefix: &str, literal_tilde: bool) -> CompletionOutput {
    for item in &mut out.items {
        if !literal_tilde && matches!(item.kind, ItemKind::File | ItemKind::Directory) {
            expand_tilde_item(&mut item.value);
        }
        item.value.insert_str(0, prefix);
    }
    if let Some(pc) = &mut out.path_completion {
        pc.flag_prefix.insert_str(0, prefix);
    }
    out
}

fn select_positional(
    positionals: &[PositionalSpec],
    positional_index: usize,
    past_double_dash: bool,
) -> Option<&PositionalSpec> {
    if past_double_dash && let Some(last_pos) = positionals.iter().rfind(|p| p.last) {
        return Some(last_pos);
    }

    let mut idx = 0usize;
    for pos in positionals {
        if pos.last {
            continue;
        }
        if pos.multiple || idx == positional_index {
            return Some(pos);
        }
        idx += 1;
    }
    None
}

/// Visit recognized flags in order. For a value-taking option, return its
/// attached value offset, or None when the value occupies the next word.
fn scan_flags<'a>(
    token: &str,
    active: &'a CmdSpec,
    ancestors: &[&'a CmdSpec],
    leading: bool,
    mut visit: impl FnMut(&'a FlagSpec),
) -> Option<(&'a FlagSpec, Option<usize>)> {
    if token.starts_with("--") {
        let (name, value) =
            token.split_once('=').map_or((token, None), |(name, _)| (name, Some(name.len() + 1)));
        let flag = find_flag(active, ancestors, name, leading)?;
        visit(flag);
        return (flag.takes_value || flag.optional_value).then_some((flag, value));
    }
    for (offset, ch) in token.strip_prefix('-')?.char_indices() {
        let end = 1 + offset + ch.len_utf8();
        let flag = find_flag(active, ancestors, &format!("-{ch}"), leading && offset == 0)?;
        visit(flag);
        if flag.takes_value || flag.optional_value {
            let suffix = &token[end..];
            let start = (!suffix.is_empty()).then(|| end + usize::from(suffix.starts_with('=')));
            return Some((flag, start));
        }
    }
    None
}

fn flag_key(flag: &FlagSpec) -> FlagKey<'_> {
    (flag.long.as_deref(), flag.short)
}

fn find_flag<'a>(
    active: &'a CmdSpec,
    ancestors: &[&'a CmdSpec],
    name: &str,
    leading: bool,
) -> Option<&'a FlagSpec> {
    for f in &active.flags {
        if (!f.leading_only || leading) && flag_matches(f, name) {
            return Some(f);
        }
    }
    for anc in ancestors.iter().rev() {
        for f in &anc.flags {
            if f.global && flag_matches(f, name) {
                return Some(f);
            }
        }
    }
    None
}

fn for_each_flag_name(flag: &FlagSpec, mut visit: impl FnMut(String)) {
    if let Some(long) = &flag.long {
        visit(format!("--{long}"));
    }
    for alias in &flag.aliases {
        visit(format!("--{alias}"));
    }
    if let Some(short) = flag.short {
        visit(format!("-{short}"));
    }
    for sa in &flag.short_aliases {
        visit(format!("-{sa}"));
    }
}

fn flag_matches(flag: &FlagSpec, token: &str) -> bool {
    if let Some(long) = token.strip_prefix("--") {
        flag.long.as_deref() == Some(long) || flag.aliases.iter().any(|a| a == long)
    } else if let Some(short_str) = token.strip_prefix('-') {
        let mut chars = short_str.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            flag.short == Some(ch) || flag.short_aliases.contains(&ch)
        } else {
            false
        }
    } else {
        false
    }
}

fn find_subcommand<'a>(active: &'a CmdSpec, name: &str) -> Option<&'a CmdSpec> {
    active.subcommands.iter().find(|sub| {
        sub.name == name
            || sub.visible_aliases.iter().any(|a| a == name)
            || sub.hidden_aliases.iter().any(|a| a == name)
    })
}

fn complete_from_hint(hint: &ValueHint, ctx: &CompletionContext) -> CompletionOutput {
    match hint {
        ValueHint::DirPath => ctx.complete_path(PathMode::DirsOnly),
        ValueHint::FilePath { extensions } => {
            ctx.complete_path(PathMode::FilesWithExtensions(extensions.clone()))
        }
        ValueHint::AnyPath => ctx.complete_path(PathMode::Any),
        ValueHint::StaticValues(vals) => {
            let mut items = Vec::new();
            for (val, desc) in vals {
                if val.starts_with(&ctx.prefix) {
                    items.push(CompletionItem::value(val.clone(), desc.clone()));
                }
            }
            CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
        }
        ValueHint::None => CompletionOutput::empty(),
    }
}

fn complete_value(
    command: &CmdSpec,
    arg: Argument<'_>,
    ctx: &CompletionContext,
    provider: &ValueProvider<'_>,
) -> CompletionOutput {
    provider(command, arg, ctx).unwrap_or_else(|| {
        complete_from_hint(
            match arg {
                Argument::Flag(flag) => &flag.value_hint,
                Argument::Positional(pos) => &pos.value_hint,
            },
            ctx,
        )
    })
}

fn complete_flags<'a>(
    active: &'a CmdSpec,
    ancestors: &[&'a CmdSpec],
    used_flag_keys: &[FlagKey<'a>],
    prefix: &str,
    leading: bool,
) -> CompletionOutput {
    let mut items: Vec<CompletionItem> = Vec::new();

    let mut collect_from_spec = |flags: &'a [FlagSpec], only_inherited: bool| {
        for flag in flags {
            let inherited_ok = flag.global;
            if (only_inherited && !inherited_ok)
                || flag.hidden
                || (flag.leading_only && !leading)
                || (!flag.multiple && used_flag_keys.contains(&flag_key(flag)))
            {
                continue;
            }
            for_each_flag_name(flag, |name| {
                if name.starts_with(prefix) && !items.iter().any(|it| it.value == name) {
                    items.push(CompletionItem::flag(name, flag.about.clone()));
                }
            });
        }
    };

    collect_from_spec(&active.flags, false);
    for anc in ancestors {
        collect_from_spec(&anc.flags, true);
    }

    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}

fn push_aliases_list(buf: &mut String, aliases: &[String]) {
    buf.push_str("(aliases: ");
    for (idx, a) in aliases.iter().enumerate() {
        if idx > 0 {
            buf.push_str(", ");
        }
        buf.push_str(a);
    }
    buf.push(')');
}

fn complete_subcommands(active: &CmdSpec, prefix: &str) -> CompletionOutput {
    let mut items = Vec::new();

    if prefix.is_empty() {
        for sub in &active.subcommands {
            if sub.hidden {
                continue;
            }
            let desc = match (&sub.about, sub.visible_aliases.as_slice()) {
                (Some(about), aliases) if !aliases.is_empty() && !about.contains("aliases:") => {
                    let mut s = String::with_capacity(about.len() + 20);
                    s.push_str(about);
                    s.push(' ');
                    push_aliases_list(&mut s, aliases);
                    Some(s)
                }
                (Some(about), _) => Some(about.clone()),
                (None, aliases) if !aliases.is_empty() => {
                    let mut s = String::new();
                    push_aliases_list(&mut s, aliases);
                    Some(s)
                }
                (None, _) => None,
            };
            items.push(CompletionItem::subcommand(sub.name.clone(), desc));
        }
    } else {
        for sub in &active.subcommands {
            if !sub.hidden && sub.name.starts_with(prefix) {
                items.push(CompletionItem::subcommand(sub.name.clone(), sub.about.clone()));
            }
            for alias in sub.visible_aliases.iter().chain(&sub.hidden_aliases) {
                if alias.starts_with(prefix) {
                    items.push(CompletionItem::subcommand(alias.clone(), sub.about.clone()));
                }
            }
        }
    }

    CompletionOutput::new(items, CompletionDirective::NO_FILE_COMP)
}
