use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use vp_complete::CompletionItem;

use super::providers::enclosing_manifest;
#[cfg(unix)]
fn kill_process_group(pid: u32) {
    if pid > 0 {
        let _ = std::process::Command::new("/bin/sh")
            .args(["-c", &format!("kill -s KILL -- -{pid} 2>/dev/null")])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(windows)]
fn kill_windows_process_tree(child: &mut std::process::Child) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let pid = child.id();
    if pid > 0 {
        let pid_str = pid.to_string();
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid_str, "/T", "/F"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = child.kill();
}

fn terminate_timed_out_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        kill_process_group(child.id());
        let _ = child.kill();
    }
    #[cfg(windows)]
    {
        kill_windows_process_tree(child);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn read_child_stdout(stdout_pipe: Option<std::process::ChildStdout>) -> String {
    let mut buf = String::new();
    if let Some(mut pipe) = stdout_pipe {
        use std::io::Read;
        let _ = pipe.read_to_string(&mut buf);
    }
    buf
}

fn try_node_task_candidates(
    dir: &Path,
    prefix: &str,
    resolve_node: fn() -> Option<PathBuf>,
    resolve_bin_js: fn(&Path) -> Option<PathBuf>,
) -> Option<Vec<CompletionItem>> {
    if std::env::var_os("VP_COMPLETE_OFFLINE_ONLY_RUST").is_some_and(|v| v == "1") {
        return None;
    }
    let node_bin = resolve_node()?;
    let bin_js = resolve_bin_js(dir)?;
    let deadline = Instant::now() + Duration::from_millis(1500);

    let mut cmd = std::process::Command::new(&node_bin);
    cmd.arg(&bin_js)
        .arg("run")
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .env("VP_ENV_FORBIDDEN", "1")
        .env_remove("VP_COMPLETE");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().ok()?;
    let stdout_pipe = child.stdout.take();
    let reader = std::thread::spawn(move || read_child_stdout(stdout_pipe));

    let mut child_status = None;
    let completed = loop {
        if child_status.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => child_status = Some(status),
                Ok(None) => {}
                Err(_) => break false,
            }
        }
        if child_status.is_some() && reader.is_finished() {
            break true;
        }
        let now = Instant::now();
        if now >= deadline {
            break false;
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(10)));
    };

    if !completed {
        terminate_timed_out_child(&mut child);
        return None;
    }

    let stdout_str = reader.join().unwrap_or_default();
    child_status?.success().then(|| parse_task_completion_lines(&stdout_str, prefix))
}

#[must_use]
pub(super) fn get_task_candidates_with(
    dir: &Path,
    prefix: &str,
    resolve_node: fn() -> Option<PathBuf>,
    resolve_bin_js: fn(&Path) -> Option<PathBuf>,
) -> Vec<CompletionItem> {
    try_node_task_candidates(dir, prefix, resolve_node, resolve_bin_js)
        .unwrap_or_else(|| extract_static_tasks(dir, prefix))
}

fn find_enclosing_vite_config_dir(dir: &Path) -> Option<&vt_path::AbsolutePath> {
    for ancestor in dir.ancestors() {
        let absolute = vt_path::AbsolutePath::new(ancestor)?;
        if vp_static_config::has_config_file(absolute) {
            return Some(absolute);
        }
        if ancestor.join("package.json").is_file() || ancestor.join("pnpm-workspace.yaml").is_file()
        {
            break;
        }
    }
    None
}

#[must_use]
pub(super) fn extract_static_tasks(dir: &Path, prefix: &str) -> Vec<CompletionItem> {
    let mut tasks = std::collections::BTreeMap::new();
    if let Some(pkg) = enclosing_manifest(dir) {
        tasks.extend(pkg.scripts);
    }
    if let Some(config_dir) = find_enclosing_vite_config_dir(dir)
        && let Some(vp_static_config::FieldValue::Json(run)) =
            vp_static_config::resolve_static_config(config_dir).get("run")
        && let Some(config_tasks) = run.get("tasks").and_then(serde_json::Value::as_object)
    {
        for (name, task) in config_tasks {
            let description = task.as_str().or_else(|| task.get("command")?.as_str());
            tasks.entry(name.clone()).or_insert_with(|| description.unwrap_or_default().to_owned());
        }
    }
    let prefix = sanitize_task_current_prefix(prefix);
    tasks
        .into_iter()
        .filter(|(name, _)| is_valid_task_name(name) && name.starts_with(&prefix))
        .map(|(name, desc)| CompletionItem::value(name, (!desc.is_empty()).then_some(desc)))
        .collect()
}

#[must_use]
pub(super) fn is_valid_task_name(name: &str) -> bool {
    let script = name.split_once('#').map_or(name, |(_, s)| s);
    !name.starts_with('#')
        && !script.is_empty()
        && !script.starts_with('#')
        && !script.starts_with("//")
        && !name.contains(char::is_whitespace)
}

#[must_use]
pub(super) fn sanitize_task_current_prefix(current: &str) -> String {
    let trimmed = current.trim_matches(['"', '\'']);
    if !trimmed.bytes().any(|b| b == b'\\') {
        return trimmed.to_owned();
    }
    let mut out = String::with_capacity(trimmed.len());
    let mut chars = trimmed.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && matches!(chars.peek(), Some('#' | ':')) {
            continue;
        }
        out.push(ch);
    }
    out.trim_matches(['"', '\'']).to_owned()
}

#[must_use]
pub(super) fn parse_task_completion_lines(stdout: &str, current: &str) -> Vec<CompletionItem> {
    let current_clean = sanitize_task_current_prefix(current);
    let mut out = Vec::new();
    for line in stdout.lines() {
        let Some((raw_name, raw_desc)) = line.split_once(": ") else { continue };
        let name = raw_name.trim();
        let desc = raw_desc.trim();
        if is_valid_task_name(name)
            && (current_clean.is_empty() || name.starts_with(&current_clean))
        {
            out.push(CompletionItem::value(name, (!desc.is_empty()).then_some(desc)));
        }
    }
    out
}

#[cfg(test)]
mod tests;
