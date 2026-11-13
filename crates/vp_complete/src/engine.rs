use std::path::PathBuf;

use crate::{
    path::{PathMode, complete_path, expand_tilde_item, resolve_dir, resolve_dir_literal},
    spec::{
        CmdSpec, CompletionContext, CompletionDirective, CompletionItem, CompletionOutput,
        CompletionRequest, FlagSpec, ItemKind, PositionalSpec, ValueHint,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinKind {
    Vp,
    Vpr,
    Vpx,
}

#[cold]
#[must_use]
pub fn classify_vp_binary(word0: &str) -> Option<BinKind> {
    let leaf = word0.rsplit(['/', '\\']).next().unwrap_or(word0);
    let lower = leaf.to_ascii_lowercase();
    let stem = [".exe", ".cmd", ".bat", ".ps1"]
        .iter()
        .find_map(|ext| lower.strip_suffix(ext))
        .unwrap_or(&lower);
    match stem {
        "vp" => Some(BinKind::Vp),
        "vpr" => Some(BinKind::Vpr),
        "vpx" => Some(BinKind::Vpx),
        _ => None,
    }
}

#[cold]
pub fn complete<'a>(
    root: &'a CmdSpec,
    vpx_spec: Option<&'a CmdSpec>,
    req: &CompletionRequest,
) -> CompletionOutput {
    if req.words.is_empty() || req.current_index == 0 {
        return CompletionOutput::empty();
    }

    let word0 = &req.words[0];
    let normalized_word0 = word0.to_ascii_lowercase().replace('\\', "/");
    let is_local_bin = normalized_word0.contains("node_modules/.bin/")
        || normalized_word0.ends_with("node_modules/.bin");

    let bin_kind = classify_vp_binary(word0);
    let is_vpr = bin_kind == Some(BinKind::Vpr);
    let (mut active_spec, mut ancestor_specs) = match bin_kind {
        Some(BinKind::Vpx) => (vpx_spec.unwrap_or(root), Vec::new()),
        Some(BinKind::Vpr) => {
            if let Some(run_spec) = root.subcommands.iter().find(|s| s.name == "run") {
                (run_spec, vec![root])
            } else {
                (root, Vec::new())
            }
        }
        _ => (root, Vec::new()),
    };

    let current_index = req.current_index.min(req.words.len() - 1);
    let current_word = &req.words[current_index];

    let mut effective_cwd = req.cwd.clone();
    let mut used_flag_keys = Vec::new();
    let mut past_double_dash = false;
    let mut trailing_args = false;
    let mut positional_index = 0;
    let mut expecting_flag_val: Option<&'a FlagSpec> = None;

    for (rel_idx, token) in req.words[1..current_index].iter().enumerate() {
        if let Some(flag) = expecting_flag_val.take() {
            if flag.takes_value || (!token.starts_with('-') && token != "--") {
                if flag.chdir {
                    effective_cwd = resolve_dir(&effective_cwd, token);
                }
                if !flag.multiple {
                    push_unique(&mut used_flag_keys, flag_key(flag));
                }
                continue;
            }
            if !flag.multiple {
                push_unique(&mut used_flag_keys, flag_key(flag));
            }
        }

        if past_double_dash || trailing_args {
            positional_index += 1;
            continue;
        }

        if token == "--" {
            past_double_dash = true;
            continue;
        }

        let allow_vpr_root_chdir = is_vpr && rel_idx == 0;

        if token.starts_with("--") {
            if let Some((flag_name, val)) = token.split_once('=') {
                if let Some(flag) =
                    find_flag(active_spec, &ancestor_specs, flag_name, allow_vpr_root_chdir)
                {
                    if flag.chdir {
                        effective_cwd = resolve_dir_literal(&effective_cwd, val);
                    }
                    if !flag.multiple {
                        push_unique(&mut used_flag_keys, flag_key(flag));
                    }
                }
            } else if let Some(flag) =
                find_flag(active_spec, &ancestor_specs, token, allow_vpr_root_chdir)
            {
                if flag.takes_value || flag.optional_value {
                    expecting_flag_val = Some(flag);
                } else if !flag.multiple {
                    push_unique(&mut used_flag_keys, flag_key(flag));
                }
            }
            continue;
        }

        if token.starts_with('-') && token.len() > 1 {
            consume_short_cluster_token(
                token,
                active_spec,
                &ancestor_specs,
                allow_vpr_root_chdir,
                &mut effective_cwd,
                &mut used_flag_keys,
                &mut expecting_flag_val,
            );
            continue;
        }

        // Non-flag token: subcommand transition only at positional_index == 0
        if positional_index == 0
            && let Some(sub) = find_subcommand(active_spec, token, is_local_bin)
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
        prefix: current_word.clone(),
        words: req.words.clone(),
        current_index,
        positional_index,
        is_local_bin,
    };

    let allow_current_vpr_chdir = is_vpr && current_index == 1;

    let mut out = compute_target_completions(
        active_spec,
        &ancestor_specs,
        &used_flag_keys,
        past_double_dash,
        trailing_args,
        expecting_flag_val,
        allow_current_vpr_chdir,
        &mut ctx,
    );

    if effective_cwd != req.cwd {
        out.effective_cwd = Some(effective_cwd);
    }
    out
}

type FlagKey<'a> = (Option<&'a str>, Option<char>);

#[inline]
fn push_unique<T: PartialEq>(vec: &mut Vec<T>, val: T) -> bool {
    if vec.contains(&val) {
        false
    } else {
        vec.push(val);
        true
    }
}

#[cold]
#[expect(clippy::too_many_arguments)]
fn compute_target_completions<'a>(
    active_spec: &'a CmdSpec,
    ancestor_specs: &[&'a CmdSpec],
    used_flag_keys: &[FlagKey<'a>],
    past_double_dash: bool,
    trailing_args: bool,
    expecting_flag_val: Option<&'a FlagSpec>,
    allow_vpr_root_chdir: bool,
    ctx: &mut CompletionContext,
) -> CompletionOutput {
    // Case 1: Cursor is fulfilling an expected flag value from the preceding token
    if let Some(flag) = expecting_flag_val {
        if flag.takes_value {
            return complete_flag_value(flag, ctx);
        }
        if !ctx.prefix.starts_with('-') {
            let opt_out = complete_flag_value(flag, ctx);
            if !opt_out.items.is_empty() || opt_out.path_completion.is_some() {
                return opt_out;
            }
        }
    }

    let parse_wrapper_flags = !past_double_dash && !trailing_args;

    // Case 2: Cursor is on an inline or attached flag value before `--` / trailing varargs
    if parse_wrapper_flags && ctx.prefix.starts_with('-') {
        if ctx.prefix.starts_with("--") {
            if let Some((flag_name, prefix_val)) = ctx.prefix.split_once('=')
                && let Some(flag) =
                    find_flag(active_spec, ancestor_specs, flag_name, allow_vpr_root_chdir)
                && (flag.takes_value || flag.optional_value)
            {
                let head = format!("{flag_name}=");
                ctx.prefix = prefix_val.to_string();
                return prefix_items(complete_flag_value(flag, ctx), &head);
            }
        } else if ctx.prefix.len() > 1
            && let Some(out) = try_complete_short_cluster_value(
                active_spec,
                ancestor_specs,
                allow_vpr_root_chdir,
                ctx,
            )
        {
            return out;
        }
    }

    // Case 3: Cursor is on a flag prefix
    if parse_wrapper_flags && ctx.prefix.starts_with('-') {
        return complete_flags(
            active_spec,
            ancestor_specs,
            used_flag_keys,
            &ctx.prefix,
            allow_vpr_root_chdir,
        );
    }

    // Case 4: Non-flag completions (subcommands and/or positionals)
    let mut items = Vec::new();
    let mut directive = CompletionDirective::DEFAULT;
    let mut path_completion = None;
    let mut had_subcommands = false;

    if parse_wrapper_flags && ctx.positional_index == 0 && !active_spec.subcommands.is_empty() {
        let sub_out = complete_subcommands(active_spec, &ctx.prefix, ctx.is_local_bin);
        had_subcommands = !sub_out.items.is_empty();
        items.extend(sub_out.items);
        directive |= sub_out.directive;
    }

    if let Some(pos) =
        select_positional(&active_spec.positionals, ctx.positional_index, past_double_dash)
    {
        let pos_out = complete_positional(pos, ctx);
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

#[cold]
fn prefix_items(mut out: CompletionOutput, prefix: &str) -> CompletionOutput {
    for item in &mut out.items {
        if matches!(item.kind, ItemKind::File | ItemKind::Directory) {
            expand_tilde_item(&mut item.value);
        }
        item.value.insert_str(0, prefix);
    }
    if let Some(pc) = &mut out.path_completion {
        pc.flag_prefix.insert_str(0, prefix);
    }
    out
}

#[cold]
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

#[cold]
fn consume_short_cluster_token<'a>(
    token: &str,
    active_spec: &'a CmdSpec,
    ancestor_specs: &[&'a CmdSpec],
    allow_vpr_root_chdir: bool,
    effective_cwd: &mut PathBuf,
    used_flag_keys: &mut Vec<FlagKey<'a>>,
    expecting_flag_val: &mut Option<&'a FlagSpec>,
) {
    let cluster = &token[1..];
    let mut short_buf = [b'-', 0, 0, 0, 0];
    for (byte_offset, ch) in cluster.char_indices() {
        let after_idx = byte_offset + ch.len_utf8();
        let rest = &cluster[after_idx..];

        let ch_len = ch.encode_utf8(&mut short_buf[1..]).len();
        let short_name = std::str::from_utf8(&short_buf[..=ch_len]).unwrap_or("-");
        let allow_chdir = allow_vpr_root_chdir && byte_offset == 0;
        let Some(flag) = find_flag(active_spec, ancestor_specs, short_name, allow_chdir) else {
            return;
        };

        if let Some(val) = rest.strip_prefix('=') {
            if flag.chdir {
                *effective_cwd = resolve_dir_literal(effective_cwd, val);
            }
            if !flag.multiple {
                push_unique(used_flag_keys, flag_key(flag));
            }
            return;
        }

        if (flag.takes_value || flag.optional_value) && !rest.is_empty() {
            if flag.chdir {
                *effective_cwd = resolve_dir_literal(effective_cwd, rest);
            }
            if !flag.multiple {
                push_unique(used_flag_keys, flag_key(flag));
            }
            return;
        }

        if flag.takes_value || flag.optional_value {
            *expecting_flag_val = Some(flag);
            return;
        }

        if !flag.multiple {
            push_unique(used_flag_keys, flag_key(flag));
        }
    }
}

#[cold]
fn try_complete_short_cluster_value(
    active_spec: &CmdSpec,
    ancestor_specs: &[&CmdSpec],
    allow_vpr_root_chdir: bool,
    ctx: &mut CompletionContext,
) -> Option<CompletionOutput> {
    let mut short_buf = [b'-', 0, 0, 0, 0];
    let word_copy = ctx.prefix.clone();
    let cluster = &word_copy[1..];
    for (byte_offset, ch) in cluster.char_indices() {
        let after_idx = byte_offset + ch.len_utf8();
        let rest = &cluster[after_idx..];
        let head = &word_copy[..=after_idx];

        let ch_len = ch.encode_utf8(&mut short_buf[1..]).len();
        let short_name = std::str::from_utf8(&short_buf[..=ch_len]).unwrap_or("-");
        let allow_chdir = allow_vpr_root_chdir && byte_offset == 0;
        let flag = find_flag(active_spec, ancestor_specs, short_name, allow_chdir)?;

        if let Some(val_prefix) = rest.strip_prefix('=') {
            if flag.takes_value || flag.optional_value {
                let head_eq = format!("{head}=");
                ctx.prefix = val_prefix.to_string();
                return Some(prefix_items(complete_flag_value(flag, ctx), &head_eq));
            }
            return None;
        }

        if flag.takes_value || flag.optional_value {
            if !rest.is_empty() && (!flag.chdir || !rest.starts_with('-')) {
                ctx.prefix = rest.to_string();
                return Some(prefix_items(complete_flag_value(flag, ctx), head));
            }
            return None;
        }
    }
    None
}

#[inline]
fn flag_key(flag: &FlagSpec) -> FlagKey<'_> {
    (flag.long.as_deref(), flag.short)
}

#[cold]
fn find_flag<'a>(
    active: &'a CmdSpec,
    ancestors: &[&'a CmdSpec],
    name: &str,
    allow_vpr_root_chdir: bool,
) -> Option<&'a FlagSpec> {
    for f in &active.flags {
        if flag_matches(f, name) {
            return Some(f);
        }
    }
    for anc in ancestors.iter().rev() {
        for f in &anc.flags {
            if (f.global || (allow_vpr_root_chdir && f.chdir)) && flag_matches(f, name) {
                return Some(f);
            }
        }
    }
    None
}

#[cold]
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

#[cold]
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

#[cold]
fn find_subcommand<'a>(active: &'a CmdSpec, name: &str, is_local_bin: bool) -> Option<&'a CmdSpec> {
    active.subcommands.iter().find(|sub| {
        !(is_local_bin && sub.global_only)
            && (sub.name == name
                || sub.visible_aliases.iter().any(|a| a == name)
                || sub.hidden_aliases.iter().any(|a| a == name))
    })
}

#[cold]
fn complete_from_hint(hint: &ValueHint, ctx: &CompletionContext) -> CompletionOutput {
    match hint {
        ValueHint::DirPath => complete_path(&ctx.cwd, &ctx.prefix, PathMode::DirsOnly),
        ValueHint::FilePath { extensions } => {
            complete_path(&ctx.cwd, &ctx.prefix, PathMode::FilesWithExtensions(extensions.clone()))
        }
        ValueHint::AnyPath => complete_path(&ctx.cwd, &ctx.prefix, PathMode::Any),
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

#[cold]
fn complete_flag_value(flag: &FlagSpec, ctx: &CompletionContext) -> CompletionOutput {
    if let Some(c) = &flag.completer {
        return c(ctx);
    }
    if flag.chdir && flag.value_hint == ValueHint::None {
        return complete_path(&ctx.cwd, &ctx.prefix, PathMode::DirsOnly);
    }
    complete_from_hint(&flag.value_hint, ctx)
}

#[cold]
fn complete_positional(pos: &PositionalSpec, ctx: &CompletionContext) -> CompletionOutput {
    pos.completer.as_ref().map_or_else(|| complete_from_hint(&pos.value_hint, ctx), |c| c(ctx))
}

#[cold]
fn complete_flags<'a>(
    active: &'a CmdSpec,
    ancestors: &[&'a CmdSpec],
    used_flag_keys: &[FlagKey<'a>],
    prefix: &str,
    allow_vpr_root_chdir: bool,
) -> CompletionOutput {
    let mut items: Vec<CompletionItem> = Vec::new();

    let mut collect_from_spec = |flags: &'a [FlagSpec], only_inherited: bool| {
        for flag in flags {
            let inherited_ok = flag.global || (allow_vpr_root_chdir && flag.chdir);
            if (only_inherited && !inherited_ok)
                || flag.hidden
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

#[cold]
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

#[cold]
fn complete_subcommands(active: &CmdSpec, prefix: &str, is_local_bin: bool) -> CompletionOutput {
    let mut items = Vec::new();

    if prefix.is_empty() {
        for sub in &active.subcommands {
            if sub.hidden || (is_local_bin && sub.global_only) {
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
            if is_local_bin && sub.global_only {
                continue;
            }
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
