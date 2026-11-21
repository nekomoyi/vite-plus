//! Windows PowerShell and PowerShell Core profile installation.
use vt_path::{AbsolutePath, AbsolutePathBuf};

use super::profile_source::VitePlusSourceMatcher;

const UTF8_BOM: &str = "\u{feff}";

fn resolve_powershell_documents_with_system_dirs(
    detected_documents: Option<&std::path::Path>,
    user_home: &AbsolutePath,
    system_dirs: Option<(&std::path::Path, Option<&std::path::Path>)>,
) -> AbsolutePathBuf {
    if let Some(doc) = detected_documents
        && let Some(abs) = AbsolutePathBuf::new(doc.to_path_buf())
    {
        return abs;
    }
    if let Some((sys_home, Some(sys_doc))) = system_dirs
        && sys_home == user_home.as_path()
        && let Some(abs) = AbsolutePathBuf::new(sys_doc.to_path_buf())
    {
        return abs;
    }
    user_home.join("Documents")
}

pub(crate) fn resolve_powershell_documents_dir(
    detected_documents: Option<&std::path::Path>,
    user_home: &AbsolutePath,
) -> AbsolutePathBuf {
    let user_dirs = directories::UserDirs::new();
    let system_dirs = user_dirs.as_ref().map(|u| (u.home_dir(), u.document_dir()));
    resolve_powershell_documents_with_system_dirs(detected_documents, user_home, system_dirs)
}

pub(crate) fn configure_powershell_profiles(
    config: &vp_shared::EnvConfig,
    documents_dir: Option<&std::path::Path>,
    create_missing: bool,
    is_shell_installed: impl Fn(&str) -> bool,
) -> std::io::Result<Vec<AbsolutePathBuf>> {
    let docs = resolve_powershell_documents_dir(documents_dir, &config.user_home);
    let profiles = [
        ("pwsh", "PowerShell/Microsoft.PowerShell_profile.ps1"),
        ("powershell", "WindowsPowerShell/Microsoft.PowerShell_profile.ps1"),
    ];
    let env_ps1 = config.dirs.config.join("env.ps1");
    let escaped_env_ps1 = env_ps1.to_string().replace('\'', "''");
    let source_line = format!(". '{escaped_env_ps1}'");
    let block = format!("# Vite+ bin (https://viteplus.dev)\n{source_line}\n");
    let matcher = VitePlusSourceMatcher::new(&config.dirs.config, &config.user_home);

    let mut configured = Vec::new();
    for (shell_bin, rel_path) in profiles {
        if !is_shell_installed(shell_bin) {
            continue;
        }
        let profile_path = docs.join(rel_path);
        if !profile_path.as_path().exists() {
            if !create_missing {
                continue;
            }
            if let Some(parent) = profile_path.parent() {
                std::fs::create_dir_all(parent.as_path())?;
            }
            let content = format!("{UTF8_BOM}{block}");
            std::fs::write(profile_path.as_path(), content)?;
            configured.push(profile_path);
        } else {
            let raw_bytes = std::fs::read(profile_path.as_path())?;
            let decoded = decode_powershell_profile(&raw_bytes)?;
            let encoding = decoded.encoding;
            let clean = decoded.text.strip_prefix('\u{feff}').unwrap_or(&decoded.text);
            if clean.lines().any(|line| matcher.source_tail(line, "env.ps1").is_some()) {
                continue;
            }
            let newline = if clean.contains("\r\n") { "\r\n" } else { "\n" };
            let append_text = format!(
                "{newline}# Vite+ bin (https://viteplus.dev){newline}{source_line}{newline}"
            );
            if !decoded.had_bom
                && encoding == PowerShellProfileEncoding::Utf8
                && !append_text.is_ascii()
            {
                let full_text = format!("{clean}{append_text}");
                let encoded = encode_powershell_profile_document(encoding, true, &full_text);
                std::fs::write(profile_path.as_path(), encoded)?;
            } else {
                let encoded_append =
                    encode_powershell_profile_document(encoding, false, &append_text);
                let mut file = std::fs::OpenOptions::new()
                    .create(false)
                    .append(true)
                    .open(profile_path.as_path())?;
                use std::io::Write;
                file.write_all(&encoded_append)?;
            }
            configured.push(profile_path);
        }
    }
    Ok(configured)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PowerShellProfileEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

pub(crate) struct DecodedPowerShellProfile {
    pub(crate) encoding: PowerShellProfileEncoding,
    pub(crate) had_bom: bool,
    pub(crate) text: String,
}

pub(crate) fn decode_powershell_profile(bytes: &[u8]) -> std::io::Result<DecodedPowerShellProfile> {
    use std::io::{Error, ErrorKind::InvalidData};

    let (encoding, data, had_bom) = if let Some(data) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        (PowerShellProfileEncoding::Utf8, data, true)
    } else if let Some(data) = bytes.strip_prefix(&[0xff, 0xfe]) {
        (PowerShellProfileEncoding::Utf16Le, data, true)
    } else if let Some(data) = bytes.strip_prefix(&[0xfe, 0xff]) {
        (PowerShellProfileEncoding::Utf16Be, data, true)
    } else if !bytes.contains(&0) {
        (PowerShellProfileEncoding::Utf8, bytes, false)
    } else if bytes.as_chunks::<2>().0.iter().any(|pair| pair[0] != 0 && pair[1] == 0) {
        // Windows PowerShell also writes BOM-less UTF-16LE.
        (PowerShellProfileEncoding::Utf16Le, bytes, false)
    } else {
        return Err(Error::new(InvalidData, "unsupported PowerShell profile text encoding"));
    };

    let text = match encoding {
        PowerShellProfileEncoding::Utf8 => {
            std::str::from_utf8(data).map_err(|err| Error::new(InvalidData, err))?.to_owned()
        }
        PowerShellProfileEncoding::Utf16Le | PowerShellProfileEncoding::Utf16Be => {
            let (pairs, remainder) = data.as_chunks::<2>();
            if !remainder.is_empty() {
                return Err(Error::new(
                    InvalidData,
                    "odd byte length in UTF-16 PowerShell profile",
                ));
            }
            let read = if encoding == PowerShellProfileEncoding::Utf16Le {
                u16::from_le_bytes
            } else {
                u16::from_be_bytes
            };
            let units: Vec<_> = pairs.iter().copied().map(read).collect();
            String::from_utf16(&units).map_err(|err| Error::new(InvalidData, err))?
        }
    };
    Ok(DecodedPowerShellProfile { encoding, had_bom, text })
}

pub(crate) fn encode_powershell_profile_document(
    encoding: PowerShellProfileEncoding,
    include_bom: bool,
    text: &str,
) -> Vec<u8> {
    let mut out = match (encoding, include_bom) {
        (PowerShellProfileEncoding::Utf8, true) => vec![0xEF, 0xBB, 0xBF],
        (PowerShellProfileEncoding::Utf16Le, true) => vec![0xFF, 0xFE],
        (PowerShellProfileEncoding::Utf16Be, true) => vec![0xFE, 0xFF],
        (_, false) => Vec::new(),
    };
    match encoding {
        PowerShellProfileEncoding::Utf8 => out.extend_from_slice(text.as_bytes()),
        PowerShellProfileEncoding::Utf16Le => {
            out.extend(text.encode_utf16().flat_map(u16::to_le_bytes))
        }
        PowerShellProfileEncoding::Utf16Be => {
            out.extend(text.encode_utf16().flat_map(u16::to_be_bytes))
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use vp_shared::env_vars;

    use super::{
        super::{ShellProfile, ShellProfileKind, ShellProfileRoot, resolve_profile_path},
        *,
    };

    #[test]
    fn test_resolve_powershell_documents_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let home = AbsolutePathBuf::new(temp_dir.path().to_path_buf()).unwrap();
        let fallback = resolve_powershell_documents_dir(None, &home);
        assert_eq!(fallback, home.join("Documents"));

        let custom_docs = temp_dir.path().join("OneDrive/Documents");
        let resolved = resolve_powershell_documents_dir(Some(&custom_docs), &home);
        assert_eq!(resolved.as_path(), custom_docs.as_path());
    }

    #[test]
    fn test_configure_powershell_profiles_create_missing_false() {
        let temp_dir = tempfile::tempdir().unwrap();
        let home = temp_dir.path().join("home");
        let docs = temp_dir.path().join("docs");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&docs).unwrap();

        vp_shared::EnvConfig::with_vars([(env_vars::VP_HOME, home.as_os_str())], |_| {
            let config = vp_shared::EnvConfig::get();
            let configured =
                configure_powershell_profiles(&config, Some(&docs), false, |_| true).unwrap();
            assert!(
                configured.is_empty(),
                "Must not create missing profiles when create_missing=false"
            );
            assert!(!docs.join("PowerShell/Microsoft.PowerShell_profile.ps1").exists());
            assert!(!docs.join("WindowsPowerShell/Microsoft.PowerShell_profile.ps1").exists());
        });
    }

    #[test]
    fn test_configure_powershell_profiles_create_missing_true_and_idempotent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let home = temp_dir.path().join("home");
        let docs = temp_dir.path().join("docs");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&docs).unwrap();

        vp_shared::EnvConfig::with_vars([(env_vars::VP_HOME, home.as_os_str())], |_| {
            let config = vp_shared::EnvConfig::get();
            // 1. Initial creation with create_missing = true
            let configured = configure_powershell_profiles(&config, Some(&docs), true, |bin| {
                bin == "pwsh" || bin == "powershell"
            })
            .unwrap();
            assert_eq!(configured.len(), 2, "Must create both profiles for installed shells");

            let pwsh_profile = docs.join("PowerShell/Microsoft.PowerShell_profile.ps1");
            let win_pwsh_profile = docs.join("WindowsPowerShell/Microsoft.PowerShell_profile.ps1");
            assert!(pwsh_profile.exists());
            assert!(win_pwsh_profile.exists());

            let content = std::fs::read_to_string(&pwsh_profile).unwrap();
            assert!(content.starts_with(UTF8_BOM), "Must start with UTF-8 BOM");
            assert!(content.contains(". '"), "Must source env.ps1 with single quotes");
            assert!(content.contains("env.ps1"), "Must reference env.ps1");

            // 2. Second run: idempotent!
            let second = configure_powershell_profiles(&config, Some(&docs), true, |bin| {
                bin == "pwsh" || bin == "powershell"
            })
            .unwrap();
            assert!(second.is_empty(), "Must not reconfigure already configured profiles");
            let content_after = std::fs::read_to_string(&pwsh_profile).unwrap();
            assert_eq!(content, content_after, "Content must not change on second run");
        });
    }

    #[test]
    fn test_powershell_documents_sandbox_and_onedrive_consistency() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sandbox_home = AbsolutePathBuf::new(temp_dir.path().join("sandbox_home")).unwrap();
        std::fs::create_dir_all(sandbox_home.as_path()).unwrap();

        // 1. resolve_profile_path with a sandboxed user_home must NOT leak into the host OS user's ~/Documents
        let profile = ShellProfile {
            root: ShellProfileRoot::PowerShellDocuments,
            path: "PowerShell/Microsoft.PowerShell_profile.ps1",
            env_file: "env.ps1",
            kind: ShellProfileKind::Main,
        };
        let resolved = resolve_profile_path(&profile, &sandbox_home);
        assert_eq!(
            resolved,
            sandbox_home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1"),
            "Sandboxed user_home must not leak host UserDirs::document_dir()"
        );

        // 2. Simulated OneDrive Known Folder Move: when system home == user_home, redirected OneDrive/Documents wins!
        let onedrive_docs = temp_dir.path().join("OneDrive/Documents");
        let redirected = resolve_powershell_documents_with_system_dirs(
            None,
            &sandbox_home,
            Some((sandbox_home.as_path(), Some(&onedrive_docs))),
        );
        assert_eq!(
            redirected.as_path(),
            onedrive_docs.as_path(),
            "OneDrive Documents redirection must be honored when system home == user_home"
        );

        // 3. When documents_dir is None and user_home matches real UserDirs::home_dir(),
        // resolve_powershell_documents_dir(None, &real_home) must agree with resolve_profile_path!
        if let Some(user_dirs) = directories::UserDirs::new() {
            if let Some(real_home) = AbsolutePathBuf::new(user_dirs.home_dir().to_path_buf()) {
                let via_configure_default = resolve_powershell_documents_dir(None, &real_home)
                    .join("PowerShell/Microsoft.PowerShell_profile.ps1");
                let via_profile_path = resolve_profile_path(&profile, &real_home);
                assert_eq!(
                    via_configure_default, via_profile_path,
                    "configure_powershell_profiles(None) and resolve_profile_path must resolve identical $PROFILE path"
                );
            }
        }
    }

    #[test]
    fn test_configure_powershell_preserves_utf16_profiles() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vp_home = temp_dir.path().join("vp_home");
        let docs = temp_dir.path().join("Documents");
        let pwsh_profile = docs.join("PowerShell/Microsoft.PowerShell_profile.ps1");
        let win_pwsh_profile = docs.join("WindowsPowerShell/Microsoft.PowerShell_profile.ps1");
        std::fs::create_dir_all(pwsh_profile.parent().unwrap()).unwrap();
        std::fs::create_dir_all(win_pwsh_profile.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&vp_home).unwrap();
        std::fs::write(vp_home.join("env.ps1"), "$env:VP_UTF16_TEST = 'loaded'\n").unwrap();

        let initial_le = "$env:EXISTING_LE = 'keep'\r\n";
        let mut le_bytes = vec![0xFF, 0xFE];
        for u in initial_le.encode_utf16() {
            le_bytes.extend_from_slice(&u.to_le_bytes());
        }
        std::fs::write(&pwsh_profile, &le_bytes).unwrap();

        let initial_be = "$env:EXISTING_BE = 'keep'\n";
        let mut be_bytes = vec![0xFE, 0xFF];
        for u in initial_be.encode_utf16() {
            be_bytes.extend_from_slice(&u.to_be_bytes());
        }
        std::fs::write(&win_pwsh_profile, &be_bytes).unwrap();

        vp_shared::EnvConfig::with_vars([(env_vars::VP_HOME, vp_home.as_os_str())], |_| {
            let config = vp_shared::EnvConfig::get();
            let configured = configure_powershell_profiles(&config, Some(&docs), false, |bin| {
                bin == "pwsh" || bin == "powershell"
            })
            .expect("UTF-16 profiles must configure cleanly without UTF-8 decode error");
            assert_eq!(configured.len(), 2);

            let updated_le = std::fs::read(&pwsh_profile).unwrap();
            assert!(updated_le.starts_with(&[0xFF, 0xFE]), "Must preserve UTF-16LE BOM");
            assert_eq!(updated_le.len() % 2, 0, "UTF-16LE file must stay 16-bit aligned");
            let le_units: Vec<u16> = updated_le[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .copied()
                .map(u16::from_le_bytes)
                .collect();
            let decoded_le = String::from_utf16(&le_units).expect("valid UTF-16LE");
            assert!(
                !decoded_le.contains('\u{feff}'),
                "Must not insert a second mid-file BOM: {decoded_le:?}"
            );
            assert!(decoded_le.contains("$env:EXISTING_LE = 'keep'"));
            assert!(decoded_le.contains("env.ps1'"));

            let updated_be = std::fs::read(&win_pwsh_profile).unwrap();
            assert!(updated_be.starts_with(&[0xFE, 0xFF]), "Must preserve UTF-16BE BOM");
            assert_eq!(updated_be.len() % 2, 0, "UTF-16BE file must stay 16-bit aligned");
            let be_units: Vec<u16> = updated_be[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .copied()
                .map(u16::from_be_bytes)
                .collect();
            let decoded_be = String::from_utf16(&be_units).expect("valid UTF-16BE");
            assert!(!decoded_be.contains('\u{feff}'));
            assert!(decoded_be.contains("$env:EXISTING_BE = 'keep'"));
            assert!(decoded_be.contains("env.ps1'"));

            let second = configure_powershell_profiles(&config, Some(&docs), false, |bin| {
                bin == "pwsh" || bin == "powershell"
            })
            .unwrap();
            assert!(second.is_empty(), "Second run on UTF-16 profiles must be idempotent");
            assert_eq!(std::fs::read(&pwsh_profile).unwrap(), updated_le);
            assert_eq!(std::fs::read(&win_pwsh_profile).unwrap(), updated_be);

            // 2. BOM-less UTF-16LE ASCII profile must still be detected as UTF-16LE (not UTF-8 with NULs)
            let bomless_text = "$env:BOMLESS_LE = 'keep'\n";
            let mut bomless_bytes = Vec::new();
            for u in bomless_text.encode_utf16() {
                bomless_bytes.extend_from_slice(&u.to_le_bytes());
            }
            std::fs::write(&pwsh_profile, &bomless_bytes).unwrap();
            let configured_bomless =
                configure_powershell_profiles(&config, Some(&docs), false, |bin| bin == "pwsh")
                    .unwrap();
            assert_eq!(configured_bomless.len(), 1);
            let updated_bomless = std::fs::read(&pwsh_profile).unwrap();
            assert_eq!(updated_bomless.len() % 2, 0);
            let bomless_units: Vec<u16> = updated_bomless
                .as_chunks::<2>()
                .0
                .iter()
                .copied()
                .map(u16::from_le_bytes)
                .collect();
            let decoded_bomless = String::from_utf16(&bomless_units).unwrap();
            assert!(decoded_bomless.contains("$env:BOMLESS_LE = 'keep'"));
            assert!(decoded_bomless.contains("env.ps1'"));
            let second_bomless =
                configure_powershell_profiles(&config, Some(&docs), false, |bin| bin == "pwsh")
                    .unwrap();
            assert!(
                second_bomless.is_empty(),
                "Second run on BOM-less UTF-16LE must be idempotent"
            );
        });
    }

    #[test]
    fn test_configure_powershell_unrelated_env_ps1_and_non_ascii_bom_upgrade() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vp_home = temp_dir.path().join("José_vp_home");
        let docs = temp_dir.path().join("Documents");
        let pwsh_profile = docs.join("PowerShell/Microsoft.PowerShell_profile.ps1");
        std::fs::create_dir_all(pwsh_profile.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&vp_home).unwrap();

        // Existing BOM-less UTF-8 profile sourcing an unrelated env.ps1 must NOT suppress setup,
        // and appending a non-ASCII path must upgrade the file to UTF-8 with BOM.
        let initial = ". 'C:\\tools\\env.ps1'\n";
        std::fs::write(&pwsh_profile, initial.as_bytes()).unwrap();

        vp_shared::EnvConfig::with_vars([(env_vars::VP_HOME, vp_home.as_os_str())], |_| {
            let config = vp_shared::EnvConfig::get();
            let configured =
                configure_powershell_profiles(&config, Some(&docs), false, |bin| bin == "pwsh")
                    .unwrap();
            assert_eq!(configured.len(), 1);

            let bytes = std::fs::read(&pwsh_profile).unwrap();
            assert!(
                bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
                "Appending non-ASCII path to BOM-less UTF-8 profile must add UTF-8 BOM"
            );
            let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
            assert!(text.contains("C:\\tools\\env.ps1"));
            assert!(text.contains("José_vp_home"));

            let second =
                configure_powershell_profiles(&config, Some(&docs), false, |bin| bin == "pwsh")
                    .unwrap();
            assert!(second.is_empty(), "Second run must be idempotent once own env.ps1 is added");
        });
    }
}
