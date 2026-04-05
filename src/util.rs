use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use chrono::{Timelike, Utc};
use regex::Regex;
use sha2::{Digest, Sha256};

pub fn normalize_path(path: &Path) -> PathBuf {
    let path = expand_tilde(path);
    if path.is_absolute() {
        return canonicalize_if_exists(&path).unwrap_or_else(|| collapse_path_components(path));
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let canonical_cwd = canonicalize_if_exists(&cwd).unwrap_or(cwd);
    let joined = canonical_cwd.join(path);

    canonicalize_if_exists(&joined).unwrap_or_else(|| collapse_path_components(joined))
}

fn canonicalize_if_exists(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

fn collapse_path_components(path: PathBuf) -> PathBuf {
    let mut parts: Vec<OsString> = Vec::new();
    let mut prefix: Option<OsString> = None;
    let mut has_root = false;

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let popped = parts.pop();
                if popped.is_none() && !has_root {
                    parts.push(component.as_os_str().to_os_string());
                }
            }
            Component::Normal(part) => parts.push(part.to_os_string()),
            Component::RootDir => has_root = true,
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if has_root {
        normalized.push(std::path::MAIN_SEPARATOR.to_string());
    }
    for part in parts {
        normalized.push(part);
    }

    normalized
}

pub fn normalize_optional_path(value: Option<PathBuf>) -> Option<PathBuf> {
    value.map(|p| normalize_path(&p))
}

pub fn utc_now_iso() -> String {
    Utc::now()
        .with_nanosecond(0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn slugify(value: &str, max_length: usize) -> String {
    let normalized = value.trim().to_lowercase();
    let non_slug = Regex::new(r"[^a-z0-9_-]+").expect("non_slug regex must compile");
    let multi_dash = Regex::new(r"-{2,}").expect("multi_dash regex must compile");

    let mut slug = non_slug.replace_all(&normalized, "-").into_owned();
    slug = multi_dash.replace_all(&slug, "-").into_owned();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        slug = "task".to_string();
    }

    let mut clipped: String = slug.chars().take(max_length).collect();
    while clipped.ends_with('-') {
        clipped.pop();
    }
    if clipped.is_empty() {
        "task".to_string()
    } else {
        clipped
    }
}

pub fn ensure_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn process_exists(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    #[cfg(unix)]
    {
        let rc = unsafe { libc::kill(pid, 0) };
        if rc == 0 {
            return true;
        }
        let err = std::io::Error::last_os_error();
        return err.raw_os_error() == Some(libc::EPERM);
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn expand_tilde(path: &Path) -> PathBuf {
    let Some(path_str) = path.to_str() else {
        return path.to_path_buf();
    };

    if path_str == "~" {
        return home_dir().unwrap_or_else(|| path.to_path_buf());
    }

    if let Some(rest) = path_str.strip_prefix("~/") {
        return home_dir()
            .map(|home| home.join(rest))
            .unwrap_or_else(|| path.to_path_buf());
    }

    if let Some(rest) = path_str.strip_prefix("~\\") {
        return home_dir()
            .map(|home| home.join(rest))
            .unwrap_or_else(|| path.to_path_buf());
    }

    path.to_path_buf()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::normalize_path;

    #[test]
    fn normalize_path_expands_tilde_prefix() {
        let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
            return;
        };

        let normalized = normalize_path(Path::new("~/codex-home"));
        assert_eq!(normalized, home.join("codex-home"));
    }

    #[test]
    fn normalize_path_collapses_dot_components_without_fs_access() {
        let cwd = std::env::current_dir().expect("cwd should exist for test");
        let canonical_cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
        let normalized = normalize_path(Path::new("a/../tasks.md"));

        assert_eq!(normalized, canonical_cwd.join("tasks.md"));
    }

    #[test]
    fn normalize_path_canonicalizes_existing_absolute_paths() {
        let unique = format!(
            "codex-worker-rs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time should be monotonic")
                .as_nanos()
        );
        let base = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(base.join("dir")).expect("test dir should be created");
        std::fs::write(base.join("dir").join("file.txt"), b"ok").expect("file should be created");

        let input = base.join("dir").join(".").join("file.txt");
        let normalized = normalize_path(&input);
        let canonical = std::fs::canonicalize(&input).expect("existing path should canonicalize");

        assert_eq!(normalized, canonical);
        std::fs::remove_dir_all(base).expect("test dir should be removed");
    }
}
