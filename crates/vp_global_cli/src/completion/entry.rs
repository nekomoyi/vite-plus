use vp_complete::CmdSpec;

use super::catalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum BinKind {
    Vp,
    Vpr,
    Vpx,
}

#[must_use]
pub(super) fn classify_vp_binary(word0: &str) -> Option<BinKind> {
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

/// Resolve the binary alias and visibility once, before scanning arguments.
pub(super) fn select_entry(word: &str) -> CmdSpec {
    let kind = classify_vp_binary(word);
    if kind == Some(BinKind::Vpx) {
        return catalog::build_vpx_spec();
    }
    let components = word.split(['/', '\\']);
    let local_bin = components.clone().zip(components.skip(1)).any(|(parent, dir)| {
        parent.eq_ignore_ascii_case("node_modules") && dir.eq_ignore_ascii_case(".bin")
    });
    let mut root = catalog::build_vp_spec(local_bin);
    if kind == Some(BinKind::Vpr)
        && let Some(index) = root.subcommands.iter().position(|command| command.name == "run")
    {
        let mut run = root.subcommands.swap_remove(index);
        for mut flag in root.flags {
            if flag.global || flag.chdir {
                flag.leading_only = !flag.global;
                run.flags.push(flag);
            }
        }
        return run;
    }
    root
}
