//! Shared shell detection and profile helpers.

use std::str::FromStr;

use directories::BaseDirs;
use vt_path::{AbsolutePath, AbsolutePathBuf};
use vt_str::Str;

/// Detected shell type for output formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    /// POSIX shell (bash, zsh, sh)
    Posix,
    /// Fish shell
    Fish,
    /// Nushell
    NuShell,
    /// PowerShell
    PowerShell,
    /// Windows cmd.exe
    Cmd,
}

impl FromStr for Shell {
    type Err = ();

    #[cold]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "sh" | "bash" | "zsh" => Ok(Shell::Posix),
            "fish" => Ok(Shell::Fish),
            "nu" | "nushell" => Ok(Shell::NuShell),
            "pwsh" | "powershell" => Ok(Shell::PowerShell),
            "cmd" => Ok(Shell::Cmd),
            _ => Err(()),
        }
    }
}

/// Detect the current shell:
/// 1. `VP_SHELL` environment variable
/// 2. Platform default
#[must_use]
pub fn detect_shell() -> Shell {
    let config = vp_shared::EnvConfig::get();

    // 1. Check VP_SHELL environment variable
    if let Some(vp_shell) = &config.vp_shell {
        if let Ok(shell) = Shell::from_str(vp_shell) {
            return shell;
        }
    }

    // 2. Platform default
    if cfg!(windows) { Shell::Cmd } else { Shell::Posix }
}

/// All shell profile files that interactive terminal sessions may source.
/// This matches the files that `install.sh` writes to and `vp implode` cleans.
#[cfg(not(windows))]
pub const ALL_SHELL_PROFILES: &[ShellProfile] = &[
    ShellProfile {
        root: ShellProfileRoot::Zsh,
        path: ".zshenv",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::Zsh,
        path: ".zshrc",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::Home,
        path: ".bash_profile",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::Home,
        path: ".bashrc",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::Home,
        path: ".profile",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::Fish,
        path: "fish/config.fish",
        env_file: "env.fish",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::Fish,
        path: "fish/conf.d/vite-plus.fish",
        env_file: "env.fish",
        kind: ShellProfileKind::Snippet,
    },
    ShellProfile {
        root: ShellProfileRoot::NushellConfig,
        path: "nushell/config.nu",
        env_file: "env.nu",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::NushellConfig,
        path: "nushell/env.nu",
        env_file: "env.nu",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::NushellData,
        path: "nushell/vendor/autoload/vite-plus.nu",
        env_file: "env.nu",
        kind: ShellProfileKind::Snippet,
    },
];

#[cfg(windows)]
pub const ALL_SHELL_PROFILES: &[ShellProfile] = &[
    ShellProfile {
        root: ShellProfileRoot::NushellConfig,
        path: "nushell/config.nu",
        env_file: "env.nu",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::NushellConfig,
        path: "nushell/env.nu",
        env_file: "env.nu",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::NushellData,
        path: "nushell/vendor/autoload/vite-plus.nu",
        env_file: "env.nu",
        kind: ShellProfileKind::Snippet,
    },
    ShellProfile {
        root: ShellProfileRoot::PowerShellDocuments,
        path: "PowerShell/Microsoft.PowerShell_profile.ps1",
        env_file: "env.ps1",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::PowerShellDocuments,
        path: "WindowsPowerShell/Microsoft.PowerShell_profile.ps1",
        env_file: "env.ps1",
        kind: ShellProfileKind::Main,
    },
];

/// IDE-relevant profile files that GUI-launched applications can see.
/// GUI apps don't run through an interactive shell, so only login/environment
/// files reliably affect them.
/// - macOS: `.zshenv` is sourced for all zsh invocations (including IDE env resolution)
/// - Linux: `.profile` is sourced by X11 display managers; `.zshenv` covers Wayland + zsh
#[cfg(target_os = "macos")]
pub const IDE_SHELL_PROFILES: &[ShellProfile] = &[
    ShellProfile {
        root: ShellProfileRoot::Zsh,
        path: ".zshenv",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::Home,
        path: ".profile",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
];

#[cfg(target_os = "linux")]
pub const IDE_SHELL_PROFILES: &[ShellProfile] = &[
    ShellProfile {
        root: ShellProfileRoot::Home,
        path: ".profile",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
    ShellProfile {
        root: ShellProfileRoot::Zsh,
        path: ".zshenv",
        env_file: "env",
        kind: ShellProfileKind::Main,
    },
];

#[cfg(windows)]
pub const IDE_SHELL_PROFILES: &[ShellProfile] = &[];

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub const IDE_SHELL_PROFILES: &[ShellProfile] = &[ShellProfile {
    root: ShellProfileRoot::Home,
    path: ".profile",
    env_file: "env",
    kind: ShellProfileKind::Main,
}];

pub struct ShellProfile {
    pub root: ShellProfileRoot,
    pub path: &'static str,
    pub env_file: &'static str,
    pub kind: ShellProfileKind,
}

#[cfg(any(windows, test))]
pub const UTF8_BOM: &str = "\u{feff}";

#[derive(Clone, Copy)]
pub enum ShellProfileRoot {
    #[cfg_attr(windows, allow(dead_code))]
    Home,
    #[cfg_attr(windows, allow(dead_code))]
    Zsh,
    #[cfg_attr(windows, allow(dead_code))]
    Fish,
    NushellConfig,
    NushellData,
    #[cfg(any(windows, test))]
    PowerShellDocuments,
}

#[derive(Clone, Copy)]
pub enum ShellProfileKind {
    Main,
    Snippet,
}

/// Abbreviate a path for display: replace `$HOME` prefix with `~`.
pub(crate) fn abbreviate_home_path(path: &AbsolutePath, user_home: &AbsolutePath) -> Str {
    match path.strip_prefix(user_home) {
        Ok(Some(suffix)) => vt_str::format!("~/{suffix}"),
        _ => Str::from(path.to_string()),
    }
}

#[cfg(any(windows, test))]
pub(crate) fn resolve_powershell_documents_with_system_dirs(
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

#[cfg(any(windows, test))]
pub(crate) fn resolve_powershell_documents_dir(
    detected_documents: Option<&std::path::Path>,
    user_home: &AbsolutePath,
) -> AbsolutePathBuf {
    let user_dirs = directories::UserDirs::new();
    let system_dirs = user_dirs.as_ref().map(|u| (u.home_dir(), u.document_dir()));
    resolve_powershell_documents_with_system_dirs(detected_documents, user_home, system_dirs)
}

#[cfg(any(windows, test))]
#[cold]
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
    let mut expected_targets = vec![normalize_pwsh_source_path(&env_ps1.to_string())];
    if let Ok(Some(rel)) = config.dirs.config.strip_prefix(&config.user_home) {
        let rel = normalize_pwsh_source_path(&rel.to_string());
        let suffix = if rel.is_empty() { "env.ps1".to_string() } else { format!("{rel}/env.ps1") };
        expected_targets.push(format!("~/{suffix}"));
        expected_targets.push(format!("$home/{suffix}"));
    }

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
            if clean.lines().any(|l| {
                powershell_source_line_target(l).is_some_and(|target| {
                    let norm = normalize_pwsh_source_path(&target);
                    expected_targets.iter().any(|exp| norm == *exp)
                })
            }) {
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
                let encoded_append = encode_powershell_profile_chunk(encoding, &append_text);
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

#[cfg(any(windows, test))]
#[cold]
fn normalize_pwsh_source_path(path: &str) -> String {
    let forward = path.replace('\\', "/");
    if cfg!(windows) { forward.to_ascii_lowercase() } else { forward }
}

#[cfg(any(windows, test))]
#[cold]
fn powershell_source_line_target(line: &str) -> Option<String> {
    let trimmed = line.strip_prefix('\u{feff}').unwrap_or(line).trim_start();
    let rest = trimmed.strip_prefix(". ").or_else(|| trimmed.strip_prefix("source "))?.trim_start();
    if let Some(body) = rest.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = body.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    let _ = chars.next();
                    out.push('\'');
                } else {
                    return Some(out);
                }
            } else {
                out.push(c);
            }
        }
        return None;
    }
    if let Some(body) = rest.strip_prefix('"') {
        return body.find('"').map(|end| body[..end].to_string());
    }
    rest.split_whitespace().next().map(str::to_string)
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PowerShellProfileEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

#[cfg(any(windows, test))]
pub(crate) struct DecodedPowerShellProfile {
    pub(crate) encoding: PowerShellProfileEncoding,
    pub(crate) had_bom: bool,
    pub(crate) text: String,
}

#[cfg(any(windows, test))]
#[cold]
pub(crate) fn decode_powershell_profile(bytes: &[u8]) -> std::io::Result<DecodedPowerShellProfile> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let text = std::str::from_utf8(rest)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        return Ok(DecodedPowerShellProfile {
            encoding: PowerShellProfileEncoding::Utf8,
            had_bom: true,
            text: text.to_string(),
        });
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        if !rest.len().is_multiple_of(2) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "odd byte length in UTF-16LE PowerShell profile",
            ));
        }
        let units: Vec<u16> =
            rest.as_chunks::<2>().0.iter().copied().map(u16::from_le_bytes).collect();
        let text = String::from_utf16(&units)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        return Ok(DecodedPowerShellProfile {
            encoding: PowerShellProfileEncoding::Utf16Le,
            had_bom: true,
            text,
        });
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        if !rest.len().is_multiple_of(2) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "odd byte length in UTF-16BE PowerShell profile",
            ));
        }
        let units: Vec<u16> =
            rest.as_chunks::<2>().0.iter().copied().map(u16::from_be_bytes).collect();
        let text = String::from_utf16(&units)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        return Ok(DecodedPowerShellProfile {
            encoding: PowerShellProfileEncoding::Utf16Be,
            had_bom: true,
            text,
        });
    }
    // Check NUL absence first: std::str::from_utf8 accepts NUL bytes in ASCII UTF-16LE text.
    if !bytes.contains(&0)
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return Ok(DecodedPowerShellProfile {
            encoding: PowerShellProfileEncoding::Utf8,
            had_bom: false,
            text: text.to_string(),
        });
    }
    // BOM-less UTF-16LE heuristic (ASCII characters have high byte == 0x00)
    if bytes.len() >= 2
        && bytes.len().is_multiple_of(2)
        && bytes.as_chunks::<2>().0.iter().any(|c| c[0] != 0 && c[1] == 0)
    {
        let units: Vec<u16> =
            bytes.as_chunks::<2>().0.iter().copied().map(u16::from_le_bytes).collect();
        if let Ok(text) = String::from_utf16(&units) {
            return Ok(DecodedPowerShellProfile {
                encoding: PowerShellProfileEncoding::Utf16Le,
                had_bom: false,
                text,
            });
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "unsupported PowerShell profile text encoding",
    ))
}

#[cfg(any(windows, test))]
#[cold]
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
    out.extend_from_slice(&encode_powershell_profile_chunk(encoding, text));
    out
}

#[cfg(any(windows, test))]
#[cold]
fn encode_powershell_profile_chunk(encoding: PowerShellProfileEncoding, text: &str) -> Vec<u8> {
    match encoding {
        PowerShellProfileEncoding::Utf8 => text.as_bytes().to_vec(),
        PowerShellProfileEncoding::Utf16Le => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out
        }
        PowerShellProfileEncoding::Utf16Be => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for unit in text.encode_utf16() {
                out.extend_from_slice(&unit.to_be_bytes());
            }
            out
        }
    }
}

#[cold]
#[inline(never)]
pub(crate) fn resolve_profile_path(
    profile: &ShellProfile,
    user_home: &AbsolutePathBuf,
) -> AbsolutePathBuf {
    let base_dirs = BaseDirs::new();
    let root = match profile.root {
        ShellProfileRoot::Home => user_home.clone(),
        ShellProfileRoot::Zsh => std::env::var_os("ZDOTDIR")
            .and_then(|value| AbsolutePathBuf::new(value.into()))
            .unwrap_or_else(|| user_home.clone()),
        ShellProfileRoot::Fish => std::env::var_os("XDG_CONFIG_HOME")
            .and_then(|value| AbsolutePathBuf::new(value.into()))
            .unwrap_or_else(|| user_home.join(".config")),
        ShellProfileRoot::NushellConfig => std::env::var_os("XDG_CONFIG_HOME")
            .and_then(|value| AbsolutePathBuf::new(value.into()))
            .or_else(|| AbsolutePathBuf::new(base_dirs?.config_dir().into()))
            .unwrap_or_else(|| user_home.join(".config")),
        ShellProfileRoot::NushellData => std::env::var_os("XDG_DATA_HOME")
            .and_then(|value| AbsolutePathBuf::new(value.into()))
            .or_else(|| AbsolutePathBuf::new(base_dirs?.data_dir().into()))
            .unwrap_or_else(|| user_home.join(".local/share")),
        #[cfg(any(windows, test))]
        ShellProfileRoot::PowerShellDocuments => resolve_powershell_documents_dir(None, user_home),
    };
    root.join(profile.path)
}

#[cfg(test)]
mod tests {
    use vp_shared::env_vars;

    use super::*;

    #[test]
    fn test_shell_from_str() {
        // POSIX shells
        assert_eq!(Shell::from_str("sh"), Ok(Shell::Posix));
        assert_eq!(Shell::from_str("bash"), Ok(Shell::Posix));
        assert_eq!(Shell::from_str("zsh"), Ok(Shell::Posix));

        // Other shells
        assert_eq!(Shell::from_str("fish"), Ok(Shell::Fish));
        assert_eq!(Shell::from_str("nu"), Ok(Shell::NuShell));
        assert_eq!(Shell::from_str("nushell"), Ok(Shell::NuShell));
        assert_eq!(Shell::from_str("powershell"), Ok(Shell::PowerShell));
        assert_eq!(Shell::from_str("pwsh"), Ok(Shell::PowerShell));
        assert_eq!(Shell::from_str("cmd"), Ok(Shell::Cmd));

        // Case insensitive
        assert_eq!(Shell::from_str("SH"), Ok(Shell::Posix));
        assert_eq!(Shell::from_str("BASH"), Ok(Shell::Posix));
        assert_eq!(Shell::from_str("Fish"), Ok(Shell::Fish));
        assert_eq!(Shell::from_str("POWERSHELL"), Ok(Shell::PowerShell));
        assert_eq!(Shell::from_str("Nu"), Ok(Shell::NuShell));

        // Invalid
        assert!(Shell::from_str("posix").is_err());
        assert!(Shell::from_str("invalid").is_err());
        assert!(Shell::from_str("").is_err());
    }

    #[test]
    fn test_detect_shell_vp_shell_explicit() {
        vp_shared::EnvConfig::with_vars(
            [
                (env_vars::VP_HOME, std::env::temp_dir().as_os_str()),
                (env_vars::VP_SHELL, std::ffi::OsStr::new("nu")),
            ],
            |_| {
                let shell = detect_shell();
                assert_eq!(shell, Shell::NuShell);
            },
        );
    }

    #[test]
    fn test_detect_shell_vp_shell_case_insensitive() {
        vp_shared::EnvConfig::with_vars(
            [
                (env_vars::VP_HOME, std::env::temp_dir().as_os_str()),
                (env_vars::VP_SHELL, std::ffi::OsStr::new("POWERSHELL")),
            ],
            |_| {
                let shell = detect_shell();
                assert_eq!(shell, Shell::PowerShell);
            },
        );
    }

    #[test]
    fn test_detect_shell_vp_shell_pwsh_alias() {
        vp_shared::EnvConfig::with_vars(
            [
                (env_vars::VP_HOME, std::env::temp_dir().as_os_str()),
                (env_vars::VP_SHELL, std::ffi::OsStr::new("pwsh")),
            ],
            |_| {
                let shell = detect_shell();
                assert_eq!(shell, Shell::PowerShell);
            },
        );
    }

    #[test]
    fn test_detect_shell_vp_shell_fish() {
        vp_shared::EnvConfig::with_vars(
            [
                (env_vars::VP_HOME, std::env::temp_dir().as_os_str()),
                (env_vars::VP_SHELL, std::ffi::OsStr::new("fish")),
            ],
            |_| {
                let shell = detect_shell();
                assert_eq!(shell, Shell::Fish);
            },
        );
    }

    #[test]
    fn test_detect_shell_defaults_without_vp_shell() {
        vp_shared::EnvConfig::with_vars([(env_vars::VP_HOME, std::env::temp_dir())], |_| {
            let shell = detect_shell();
            if cfg!(windows) {
                assert_eq!(shell, Shell::Cmd);
            } else {
                assert_eq!(shell, Shell::Posix);
            }
        });
    }

    #[test]
    fn test_detect_shell_invalid_vp_shell_falls_back_to_default() {
        vp_shared::EnvConfig::with_vars(
            [
                (env_vars::VP_HOME, std::env::temp_dir().as_os_str()),
                (env_vars::VP_SHELL, std::ffi::OsStr::new("invalid")),
            ],
            |_| {
                let shell = detect_shell();
                if cfg!(windows) {
                    assert_eq!(shell, Shell::Cmd);
                } else {
                    assert_eq!(shell, Shell::Posix);
                }
            },
        );
    }

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
