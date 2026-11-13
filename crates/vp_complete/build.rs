#![forbid(unsafe_code)]
#![allow(
    clippy::allow_attributes,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]

use std::{collections::HashMap, fmt::Write as fmt_write, fs, io::Write as io_write, path::Path};

use flate2::{Compression, Decompress, FlushDecompress, write::DeflateEncoder};

const HOOK_FILES: &[(&str, &str)] = &[
    ("RANGE_BASH_HOOK", "src/shells/hooks/bash.sh"),
    ("RANGE_ZSH_HOOK", "src/shells/hooks/zsh.zsh"),
    ("RANGE_FISH_HOOK", "src/shells/hooks/fish.fish"),
    ("RANGE_NU_HOOK", "src/shells/hooks/nu.nu"),
    ("RANGE_PWSH_HOOK", "src/shells/hooks/pwsh.ps1"),
];

const PASSTHROUGH_SECTIONS: &[(&str, &str)] = &[
    ("RANGE_VPX_EXTRA", "vpx_extra"),
    ("RANGE_VITE_COMMON", "vite_common"),
    ("RANGE_VITE_SERVER", "vite_server"),
    ("RANGE_VITE_BUILD", "vite_build"),
    ("RANGE_VITE_PREVIEW", "vite_preview"),
    ("RANGE_OXLINT", "oxlint"),
    ("RANGE_OXFMT", "oxfmt"),
    ("RANGE_VITEST", "vitest"),
    ("RANGE_TSDOWN", "tsdown"),
    ("RANGE_VITEPRESS", "vitepress"),
];

fn read_lf(manifest_dir: &Path, rel: &str) -> String {
    println!("cargo:rerun-if-changed={rel}");
    let path = manifest_dir.join(rel);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
        .replace("\r\n", "\n")
}

fn push_slice(buf: &mut String, s: &str) -> (usize, usize) {
    let start = buf.len();
    buf.push_str(s);
    (start, buf.len())
}

fn main() {
    let manifest_dir_str = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let manifest_dir = Path::new(&manifest_dir_str);
    let out_dir_str = std::env::var("OUT_DIR").expect("OUT_DIR");
    let out_dir = Path::new(&out_dir_str);

    let mut combined = String::with_capacity(40_960);
    let mut meta = String::new();

    for &(const_name, rel) in HOOK_FILES {
        let text = read_lf(manifest_dir, rel);
        let (start, end) = push_slice(&mut combined, &text);
        let _ = writeln!(meta, "pub(crate) const {const_name}: (usize, usize) = ({start}, {end});");
    }

    let passthrough_raw = read_lf(manifest_dir, "src/passthrough.tsv");
    let mut sections: HashMap<String, String> = HashMap::new();
    let mut current_sec: Option<String> = None;
    for line in passthrough_raw.split_inclusive('\n') {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let trimmed = line.trim();
        if let Some(sec) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current_sec = Some(sec.to_string());
            sections.entry(sec.to_string()).or_default();
        } else if let Some(sec) = &current_sec {
            sections.entry(sec.clone()).or_default().push_str(line);
        }
    }

    for &(const_name, sec_key) in PASSTHROUGH_SECTIONS {
        let body = sections
            .get(sec_key)
            .unwrap_or_else(|| panic!("missing [{sec_key}] in src/passthrough.tsv"));
        let (start, end) = push_slice(&mut combined, body);
        let _ = writeln!(meta, "pub(crate) const {const_name}: (usize, usize) = ({start}, {end});");
    }

    let local_raw = read_lf(manifest_dir, "src/local_specs.tsv");
    let mut local_clean = String::with_capacity(local_raw.len());
    for line in local_raw.split_inclusive('\n') {
        if !line.starts_with('#') && !line.trim().is_empty() {
            local_clean.push_str(line);
        }
    }
    let (ls_start, ls_end) = push_slice(&mut combined, &local_clean);
    let _ = writeln!(
        meta,
        "pub(crate) const RANGE_LOCAL_SPECS: (usize, usize) = ({ls_start}, {ls_end});"
    );
    let _ = writeln!(meta, "pub(crate) const RAW_ASSETS_LEN: usize = {};", combined.len());

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(combined.as_bytes()).expect("deflate write");
    let deflated = encoder.finish().expect("deflate finish");

    // Compile-time roundtrip check using the exact runtime inflation call.
    let mut verify = vec![0u8; combined.len()];
    let mut dec = Decompress::new(false);
    let status =
        dec.decompress(&deflated, &mut verify, FlushDecompress::Finish).expect("inflate verify");
    assert_eq!(status, flate2::Status::StreamEnd, "incomplete deflate stream");
    assert_eq!(verify, combined.as_bytes(), "deflate roundtrip mismatch");

    fs::write(out_dir.join("bundled_assets.deflate"), &deflated).expect("write deflate blob");
    fs::write(out_dir.join("bundled_assets_meta.rs"), &meta).expect("write meta rs");
}
