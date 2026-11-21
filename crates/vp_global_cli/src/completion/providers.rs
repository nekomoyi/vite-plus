use std::path::Path;

pub(super) struct PkgManifest {
    pub(super) scripts: Vec<(String, String)>,
}

fn read_pkg_manifest(dir: &Path) -> Option<PkgManifest> {
    let bytes = std::fs::read(dir.join("package.json")).ok()?;
    let stripped = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let val: serde_json::Value = serde_json::from_slice(stripped).ok()?;
    let obj = val.as_object()?;

    let mut scripts = Vec::new();
    if let Some(s) = obj.get("scripts").and_then(|v| v.as_object()) {
        for (k, v) in s {
            if let Some(cmd) = v.as_str() {
                scripts.push((k.clone(), cmd.to_string()));
            }
        }
    }

    Some(PkgManifest { scripts })
}

pub(super) fn enclosing_manifest(dir: &Path) -> Option<PkgManifest> {
    dir.ancestors().find_map(read_pkg_manifest)
}
