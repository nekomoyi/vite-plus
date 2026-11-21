use vt_path::AbsolutePathBuf;
use vt_str::Str;

/// Matches shell-profile `source` lines that reference *this* install's env
/// files, so a second Vite+ install's lines are left untouched.
///
/// The recognized spellings must mirror what the writers emit:
/// `install.sh`/`install.ps1` (shell PATH setup) and `render_env_content` in
/// `env/setup.rs`. `env/doctor.rs::check_profile_files` derives the same
/// variants for its profile scan; keep them in sync.
pub(crate) struct VitePlusSourceMatcher {
    /// Env-dir spellings with forward-slash separators: the absolute path,
    /// plus `$HOME`- and `~`-relative forms when the dir is under `$HOME`.
    roots: Vec<Str>,
}

impl VitePlusSourceMatcher {
    pub(crate) fn new(env_dir: &AbsolutePathBuf, user_home: &AbsolutePathBuf) -> Self {
        let mut roots = vec![normalize_path_separators(&env_dir.as_path().display().to_string())];

        if let Ok(Some(suffix)) = env_dir.strip_prefix(user_home) {
            // `RelativePathBuf` guarantees forward-slash separators.
            let suffix = vt_str::format!("{suffix}");
            if suffix.is_empty() {
                roots.push(Str::from("$HOME"));
                roots.push(Str::from("~"));
            } else {
                roots.push(vt_str::format!("$HOME/{suffix}"));
                roots.push(vt_str::format!("~/{suffix}"));
            }
        }

        Self { roots }
    }

    pub(crate) fn is_vite_plus_source_line(&self, line: &str, env_file: &str) -> bool {
        self.source_tail(line, env_file).is_some_and(|tail| tail.trim().is_empty())
    }

    /// Everything following a recognized source target. Setup can recognize a
    /// compound command, while cleanup may remove only a standalone source line.
    pub(crate) fn source_tail<'a>(&self, line: &'a str, env_file: &str) -> Option<&'a str> {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        let argument =
            trimmed.strip_prefix(". ").or_else(|| trimmed.strip_prefix("source "))?.trim_start();
        let (arg, tail) = source_line_arg(argument, env_file == "env.ps1")?;
        if !(tail.is_empty() || tail.starts_with(char::is_whitespace) || tail.starts_with(';')) {
            return None;
        }
        // PowerShell expands variables only in bare and double-quoted arguments.
        if env_file == "env.ps1" && argument.starts_with('\'') && arg.starts_with('$') {
            return None;
        }

        // Windows profiles may spell the path with backslashes (e.g. Nushell's
        // `source '~\.vite-plus\env.nu'`); compare in forward-slash form.
        let arg = normalize_path_separators(&arg);
        self.roots
            .iter()
            .any(|root| {
                let expected = join_path_ref(root, env_file);
                if cfg!(windows) && env_file == "env.ps1" {
                    arg.eq_ignore_ascii_case(&expected)
                } else {
                    arg == expected
                }
            })
            .then_some(tail)
    }
}

fn join_path_ref(root: &str, env_file: &str) -> Str {
    let separator = if root.ends_with('/') { "" } else { "/" };
    vt_str::format!("{root}{separator}{env_file}")
}

fn normalize_path_separators(path: &str) -> Str {
    Str::from(path.replace('\\', "/"))
}

fn source_line_arg(argument: &str, powershell: bool) -> Option<(Str, &str)> {
    if let Some(rest) = argument.strip_prefix('"') {
        let (path, tail) = rest.split_once('"')?;
        return Some((Str::from(path), tail));
    }
    if let Some(rest) = argument.strip_prefix('\'') {
        let mut path = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            if ch == '\'' {
                if !powershell || !chars.as_str().starts_with('\'') {
                    return Some((Str::from(path), chars.as_str()));
                }
                chars.next();
            }
            path.push(ch);
        }
        return None;
    }
    let end = argument.find(|c: char| c.is_whitespace() || c == ';').unwrap_or(argument.len());
    Some((Str::from(&argument[..end]), &argument[end..]))
}
