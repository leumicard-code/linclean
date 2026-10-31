// engine.rs - Core cleaning engine: parallel scanning, size calculation, safe deletion, and reporting.

use anyhow::Result;
use chrono::{DateTime, Local};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::SystemTime;
use walkdir::WalkDir;

/// Single item discovered during a cleanup scan.
#[derive(Debug, Clone)]
pub struct CleanItem {
    pub path: PathBuf,
    pub size: u64,
    pub category: String,
}

/// Definition of a cleanup category.
#[derive(Debug, Clone)]
pub struct CleanCategory {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// true = handled by an external tool (prune/vacuum) in addition to or instead of file scanning.
    pub command_based: bool,
}

pub const CATEGORIES: &[CleanCategory] = &[
    CleanCategory { id: "browsers",   name: "Browser Cache",          description: "Firefox / Chrome / Chromium cache", command_based: false },
    CleanCategory { id: "dnf",        name: "DNF Package Cache",      description: "/var/cache/dnf",                    command_based: false },
    CleanCategory { id: "tmp",        name: "Temporary Files",        description: "/tmp, /var/tmp (safe filter)",      command_based: false },
    CleanCategory { id: "logs",       name: "Rotated System Logs",    description: "/var/log rotated logs (.gz/.1/.old)", command_based: false },
    CleanCategory { id: "thumbnails", name: "Thumbnail Cache",        description: "~/.cache/thumbnails",               command_based: false },
    CleanCategory { id: "trash",      name: "Trash Bin",              description: "~/.local/share/Trash",              command_based: false },
    CleanCategory { id: "journal",    name: "Systemd Journal",        description: "systemd journal logs (vacuum)",     command_based: true },
    CleanCategory { id: "podman",     name: "Podman (Safe Prune)",    description: "Dangling images & cache (containers kept)", command_based: true },
    CleanCategory { id: "flatpak",    name: "Flatpak Unused & Cache", description: "Unused runtimes & app cache",       command_based: true },
];

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"))
}

fn dir_size(path: &Path) -> u64 {
    if path.is_file() {
        return fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    }
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn push_if_exists(items: &mut Vec<CleanItem>, path: PathBuf, category: &str) {
    if path.exists() {
        let size = dir_size(&path);
        if size > 0 || path.is_dir() {
            items.push(CleanItem { path, size, category: category.to_string() });
        }
    }
}

fn age_in_days(path: &Path) -> u64 {
    let modified: SystemTime = fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::now());
    let modified: DateTime<Local> = DateTime::from(modified);
    let now = Local::now();
    (now - modified).num_days().max(0) as u64
}

// ---------------------------------------------------------------------
// Category Scanners
// ---------------------------------------------------------------------

pub fn scan_browsers() -> Vec<CleanItem> {
    let mut items = Vec::new();
    let h = home();

    let ff_cache_root = h.join(".cache/mozilla/firefox");
    if ff_cache_root.is_dir() {
        if let Ok(entries) = fs::read_dir(&ff_cache_root) {
            for entry in entries.flatten() {
                let cache2 = entry.path().join("cache2");
                push_if_exists(&mut items, cache2, "browsers");
            }
        }
    }

    push_if_exists(&mut items, h.join(".cache/google-chrome"), "browsers");
    push_if_exists(&mut items, h.join(".cache/chromium"), "browsers");
    push_if_exists(&mut items, h.join(".config/google-chrome/Default/Cache"), "browsers");
    push_if_exists(&mut items, h.join(".config/chromium/Default/Cache"), "browsers");

    items
}

pub fn scan_dnf() -> Vec<CleanItem> {
    let mut items = Vec::new();
    push_if_exists(&mut items, PathBuf::from("/var/cache/dnf"), "dnf");
    push_if_exists(&mut items, PathBuf::from("/var/cache/yum"), "dnf");
    items
}

/// Protected prefixes in /tmp and /var/tmp that must never be deleted:
/// active container runtimes (Podman), systemd private mounts, X11/Wayland/KDE sockets,
/// SSH/tmux sessions, and audio server sockets.
const TMP_PROTECTED_PREFIXES: &[&str] = &[
    "podman-",          // /tmp/podman-run-UID (runroot when XDG_RUNTIME_DIR is unset)
    "containers-",      // /tmp/containers-user-UID
    "containers",       // /var/tmp/containers (build storage)
    "storage-run-",     // containers/storage runroot
    "buildah",
    "systemd-private-",
    ".X11-unix",
    ".ICE-unix",
    ".XIM-unix",
    ".font-unix",
    ".Test-unix",
    ".X",               // .X0-lock, etc.
    "tmux-",
    "ssh-",
    "pulse-",
    "pipewire-",
    "sddm",
    "kio",
    "plasma",
    "xauth",
    "krb5cc",
    "dbus-",
    "flatpak",
    ".flatpak",
    "snap-private-tmp",
];

fn is_protected_tmp_entry(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if TMP_PROTECTED_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return true;
    }
    // Never touch symlinks, sockets, pipes, or device nodes — only regular files and directories.
    match fs::symlink_metadata(path) {
        Ok(m) => {
            let ft = m.file_type();
            !(ft.is_file() || ft.is_dir())
        }
        Err(_) => true,
    }
}

pub fn scan_tmp(min_age_days: u64) -> Vec<CleanItem> {
    let mut items = Vec::new();
    for base in ["/tmp", "/var/tmp"] {
        let base = PathBuf::from(base);
        if !base.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if is_protected_tmp_entry(&path) {
                    continue;
                }
                if age_in_days(&path) >= min_age_days {
                    let size = dir_size(&path);
                    items.push(CleanItem { path, size, category: "tmp".to_string() });
                }
            }
        }
    }
    items
}

pub fn scan_logs() -> Vec<CleanItem> {
    let mut items = Vec::new();
    let base = PathBuf::from("/var/log");
    if !base.is_dir() {
        return items;
    }
    for entry in WalkDir::new(&base).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_rotated = name.ends_with(".gz")
            || name.ends_with(".old")
            || name.contains(".log.")
            || name
                .rsplit('.')
                .next()
                .map(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty())
                .unwrap_or(false);
        if is_rotated {
            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            items.push(CleanItem { path: path.to_path_buf(), size, category: "logs".to_string() });
        }
    }
    items
}

pub fn scan_thumbnails() -> Vec<CleanItem> {
    let mut items = Vec::new();
    push_if_exists(&mut items, home().join(".cache/thumbnails"), "thumbnails");
    items
}

pub fn scan_trash() -> Vec<CleanItem> {
    let mut items = Vec::new();
    push_if_exists(&mut items, home().join(".local/share/Trash/files"), "trash");
    push_if_exists(&mut items, home().join(".local/share/Trash/info"), "trash");
    items
}

/// Scans Podman temporary/cache directories (dangling images are handled by `prune_podman`).
pub fn scan_podman() -> Vec<CleanItem> {
    let mut items = Vec::new();
    let h = home();
    push_if_exists(&mut items, h.join(".cache/containers"), "podman");
    push_if_exists(&mut items, h.join(".local/share/containers/storage/tmp"), "podman");
    push_if_exists(&mut items, PathBuf::from("/var/tmp/containers"), "podman");
    items
}

/// Scans Flatpak repo tmp and per-application caches (unused runtimes are handled by `prune_flatpak`).
pub fn scan_flatpak() -> Vec<CleanItem> {
    let mut items = Vec::new();
    let h = home();
    push_if_exists(&mut items, h.join(".cache/flatpak"), "flatpak");
    push_if_exists(&mut items, h.join(".local/share/flatpak/repo/tmp"), "flatpak");

    // Per-app cache: ~/.var/app/<app-id>/cache
    let apps_dir = h.join(".var/app");
    if apps_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&apps_dir) {
            for entry in entries.flatten() {
                push_if_exists(&mut items, entry.path().join("cache"), "flatpak");
            }
        }
    }
    items
}

pub fn delete_item(item: &CleanItem) -> Result<()> {
    if item.path.is_dir() {
        for entry in fs::read_dir(&item.path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                let _ = fs::remove_dir_all(&p);
            } else {
                let _ = fs::remove_file(&p);
            }
        }
    } else if item.path.is_file() {
        fs::remove_file(&item.path)?;
    }
    Ok(())
}

/// Vacuums systemd journal logs keeping only the last `keep_days` days (requires root).
pub fn vacuum_journal(keep_days: u32) -> Result<String> {
    let output = Command::new("journalctl")
        .arg(format!("--vacuum-time={}d", keep_days))
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

/// Safely prunes only dangling (untagged) Podman images — stopped containers are never touched.
pub fn prune_podman() -> Result<String> {
    let output = Command::new("podman")
        .args(["image", "prune", "-f"])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

/// Removes unused Flatpak runtimes and extensions.
pub fn prune_flatpak() -> Result<String> {
    let output = Command::new("flatpak")
        .args(["uninstall", "--unused", "-y"])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

/// Dispatches file-based scanning for a single category ID.
pub fn scan_category(id: &str, tmp_age_days: u64) -> Vec<CleanItem> {
    match id {
        "browsers" => scan_browsers(),
        "dnf" => scan_dnf(),
        "tmp" => scan_tmp(tmp_age_days),
        "logs" => scan_logs(),
        "thumbnails" => scan_thumbnails(),
        "trash" => scan_trash(),
        "podman" => scan_podman(),
        "flatpak" => scan_flatpak(),
        "journal" => Vec::new(),
        _ => Vec::new(),
    }
}

/// Parallel scanner: runs each selected category in its own scoped thread.
pub fn scan_all_parallel(ids: &[&'static str], tmp_age_days: u64) -> Vec<CleanItem> {
    thread::scope(|scope| {
        let handles: Vec<_> = ids
            .iter()
            .filter(|id| **id != "journal")
            .map(|id| {
                let id = *id;
                scope.spawn(move || scan_category(id, tmp_age_days))
            })
            .collect();

        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    })
}

pub fn total_size(items: &[CleanItem]) -> u64 {
    items.iter().map(|i| i.size).sum()
}

/// Builds a formatted plain-text report grouped by category.
pub fn build_report(items: &[CleanItem]) -> String {
    use humansize::{format_size, DECIMAL};
    use std::collections::BTreeMap;

    let mut by_cat: BTreeMap<&str, (u64, usize)> = BTreeMap::new();
    for item in items {
        let entry = by_cat.entry(item.category.as_str()).or_insert((0, 0));
        entry.0 += item.size;
        entry.1 += 1;
    }

    let now = Local::now();
    let mut report = String::new();
    report.push_str(&format!("LinClean Report - {}\n", now.format("%Y-%m-%d %H:%M:%S")));
    report.push_str(&"=".repeat(60));
    report.push('\n');

    for (cat, (size, count)) in &by_cat {
        report.push_str(&format!(
            "{:<12} | {:>4} items | {}\n",
            cat,
            count,
            format_size(*size, DECIMAL)
        ));
    }

    report.push_str(&"-".repeat(60));
    report.push('\n');
    report.push_str(&format!("Total reclaimable: {}\n\n", format_size(total_size(items), DECIMAL)));

    report.push_str("Detailed items:\n");
    for item in items {
        report.push_str(&format!(
            "  [{:<10}] {:>10}  {}\n",
            item.category,
            format_size(item.size, DECIMAL),
            item.path.display()
        ));
    }

    report
}

/// Saves a report to the user's home directory and returns its path.
pub fn save_report(content: &str) -> Result<PathBuf> {
    let now = Local::now();
    let filename = format!("linclean_report_{}.txt", now.format("%Y%m%d_%H%M%S"));
    let path = home().join(filename);
    fs::write(&path, content)?;
    Ok(path)
}
