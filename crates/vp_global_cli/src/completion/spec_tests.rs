use std::{fs, path::PathBuf};

use tempfile::TempDir;
use vp_complete::CompletionRequest;

use super::*;

fn complete_values(words: &[&str], cwd: &Path) -> Vec<String> {
    let req = CompletionRequest::new(
        words.iter().map(|s| (*s).to_string()).collect(),
        words.len() - 1,
        cwd.to_path_buf(),
    );
    complete_entry(&req).items.into_iter().map(|it| it.value).collect()
}

fn assert_has_all(actual: &[String], expected: &[&str], context: &str) {
    for item in expected {
        assert!(actual.iter().any(|v| v == item), "{context} missing {item}: {actual:?}");
    }
}

#[test]
fn shorthand_accepts_only_a_leading_directory_option() {
    let ws = TempDir::new().unwrap();
    fs::create_dir(ws.path().join("-tmp")).unwrap();
    for bin in ["vpr", "vpr.cmd", "VPR.EXE"] {
        for (word, expected) in [("-C-t", "-C-tmp/"), ("-C=-t", "-C=-tmp/")] {
            assert_has_all(&complete_values(&[bin, word], ws.path()), &[expected], bin);
        }
        assert_has_all(&complete_values(&[bin, "-"], ws.path()), &["-C"], bin);
        assert!(!complete_values(&[bin, "-r", "-"], ws.path()).iter().any(|v| v == "-C"));
        assert!(!complete_values(&["vp", "run", "-"], ws.path()).iter().any(|v| v == "-C"));
    }
    for bin in ["vpx.cmd", "VPX.BAT", r"C:\bin\vpx.PS1", "vpx.EXE"] {
        assert_eq!(entry::classify_vp_binary(bin), Some(entry::BinKind::Vpx));
    }
}

#[test]
fn test_bimodal_subcommand_alias_matching() {
    let cwd = PathBuf::from("/workspace");

    let req_empty = CompletionRequest::new(vec!["vp".into(), "".into()], 1, cwd.clone());
    let out_empty = complete_entry(&req_empty);
    assert!(out_empty.items.iter().any(|it| it.value == "install"));
    assert!(out_empty.items.iter().any(|it| it.value == "remove"));
    assert!(out_empty.items.iter().any(|it| it.value == "list"));
    assert!(!out_empty.items.iter().any(|it| it.value == "i"));
    assert!(!out_empty.items.iter().any(|it| it.value == "un"));
    assert!(!out_empty.items.iter().any(|it| it.value == "rm"));

    let req_un = CompletionRequest::new(vec!["vp".into(), "un".into()], 1, cwd);
    let out_un = complete_entry(&req_un);
    let values: Vec<&str> = out_un.items.iter().map(|it| it.value.as_str()).collect();
    assert!(values.contains(&"un"), "Must contain un: {values:?}");
    assert!(values.contains(&"uninstall"), "Must contain uninstall: {values:?}");
    assert!(values.contains(&"unlink"), "Must contain unlink: {values:?}");
}
