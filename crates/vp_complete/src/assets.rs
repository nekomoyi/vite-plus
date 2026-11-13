//! Lazily inflated static shell hooks and CLI completion tables.
//!
//! All completion scripts and TSV spec tables are stored as human-readable source
//! files in git, packed into a single dictionary-shared raw DEFLATE stream at build
//! time, and inflated at most once per process when completion or shell setup runs.

use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/bundled_assets_meta.rs"));

static BUNDLED_DEFLATE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bundled_assets.deflate"));
static BUNDLED_TEXT: OnceLock<String> = OnceLock::new();

#[cold]
fn bundled_text() -> &'static str {
    BUNDLED_TEXT.get_or_init(|| {
        let mut out = vec![0u8; RAW_ASSETS_LEN];
        let mut dec = flate2::Decompress::new(false);
        let res = dec.decompress(BUNDLED_DEFLATE, &mut out, flate2::FlushDecompress::Finish);
        debug_assert!(matches!(res, Ok(flate2::Status::StreamEnd)));
        String::from_utf8(out).unwrap_or_default()
    })
}

#[inline]
pub(crate) fn slice(range: (usize, usize)) -> &'static str {
    &bundled_text()[range.0..range.1]
}
