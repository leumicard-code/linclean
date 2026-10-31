# 🧹 LinClean — Linux System Cleaner (GUI & CLI)

[![Language: Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![GUI: wxDragon / wxWidgets](https://img.shields.io/badge/GUI-wxDragon%20%2F%20GTK3-blue.svg)](https://crates.io/crates/wxdragon)
[![Platform: Linux](https://img.shields.io/badge/Platform-Linux-success.svg)](https://www.kernel.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](#license)

**LinClean** is an ultra-fast, safe, BleachBit-style system cleaner for Linux written in **Rust**.  
It provides both a **native desktop GUI** (powered by `wxdragon` / wxWidgets) and a **full-featured CLI**, scanning gigabytes of cache and junk files in milliseconds using multithreaded directory traversal.

![LinClean Desktop GUI](assets/screenshot.png)

---

## ✨ Key Features

- ⚡ **Blazingly Fast Parallel Engine** — Each cleanup category is scanned concurrently in its own scoped thread (`std::thread::scope`), scanning gigabytes of data in ~10 ms.
- 🖥️ **Native Desktop GUI (`linclean-gui`)** — Clean two-column BleachBit-style interface using native GTK3/wxWidgets controls that blend seamlessly with KDE Breeze and GNOME themes.
- 💻 **Powerful CLI (`linclean`)** — Scriptable command-line interface with `--dry-run`, category filtering, and plain-text report generation.
- 🛡️ **Safety-First Design**:
  - **Container-Safe Podman Cleanup**: Runs `podman image prune -f` to remove only dangling untagged images — **stopped containers and volumes are never touched**.
  - **Protected `/tmp` & `/var/tmp` Scanner**: Automatically skips active runtime directories, sockets, symlinks, X11/Wayland locks, `systemd-private-*`, `podman-*`, `pipewire-*`, `ssh-*`, and `tmux-*`.
  - **Preview Before Delete**: Inspect every file and reclaimable byte before confirming deletion.

---

## 📦 Cleanup Categories

| ID | Category | Target Locations / Command | Type |
| :--- | :--- | :--- | :--- |
| `browsers` | **Browser Cache** | Firefox (`cache2`), Google Chrome & Chromium caches | File scan |
| `dnf` | **DNF Package Cache** | `/var/cache/dnf`, `/var/cache/yum` | File scan |
| `tmp` | **Temporary Files** | `/tmp`, `/var/tmp` (age-filtered, active runtime sockets excluded) | File scan |
| `logs` | **Rotated System Logs** | `/var/log` rotated archives (`.gz`, `.old`, `.log.1`, etc.) | File scan |
| `thumbnails` | **Thumbnail Cache** | `~/.cache/thumbnails` | File scan |
| `trash` | **Trash Bin** | `~/.local/share/Trash/files` & `info` | File scan |
| `podman` | **Podman (Safe Prune)** | `~/.cache/containers` + `podman image prune -f` | File + Command |
| `flatpak` | **Flatpak Unused & Cache** | `~/.cache/flatpak`, `~/.var/app/*/cache` + `flatpak uninstall --unused -y` | File + Command |
| `journal` | **Systemd Journal** | `journalctl --vacuum-time=7d` *(requires root)* | Command |

---

## 🏗️ Architecture

The codebase is intentionally small, flat, and easy to read:

```text
linclean/
├── Cargo.toml       # Defines both `linclean` (CLI) and `linclean-gui` (GUI) binaries
└── src/
    ├── engine.rs    # Parallel scanning engine, safe /tmp filter, deletion & report builder
    ├── gui.rs       # Native wxDragon GUI organized in top-to-bottom section builders
    └── main.rs      # Clap-based CLI interface
```

---

## 🚀 Building from Source

### Prerequisites (Ubuntu / Debian)
```bash
sudo apt-get install build-essential cmake ninja-build pkg-config \
    libgtk-3-dev libjpeg-dev libtiff-dev clang libclang-dev
```

### Build Release Binaries
```bash
cargo build --release
```

This produces two optimized binaries in `target/release/`:
- `target/release/linclean-gui` — Native graphical application
- `target/release/linclean` — Command-line utility

---

## 📖 Usage

### 1. Graphical Interface (GUI)
```bash
./target/release/linclean-gui
```
1. Select the desired cleanup categories in the left panel.
2. Click **Preview Scan** to inspect all matching paths and total reclaimable space.
3. Click **Clean Now** and confirm the native dialog to permanently remove the selected items.
4. Click **Save Report** at any time to export a timestamped summary to `~/linclean_report_YYYYMMDD_HHMMSS.txt`.

### 2. Command-Line Interface (CLI)

List all available categories:
```bash
linclean list
```

Scan all categories in parallel (no files deleted):
```bash
linclean scan
```

Scan specific categories only:
```bash
linclean scan --categories browsers,thumbnails,trash,flatpak
```

Dry-run cleanup (preview what would be deleted):
```bash
linclean clean --dry-run
```

Perform cleanup interactively:
```bash
linclean clean
```

Generate and save a detailed text report to `~/`:
```bash
linclean report
```

---

## 👤 Author

**Shay Kadosh** — System Architect & Lead Developer

## 📄 License

This project is open-source software licensed under the [MIT License](LICENSE).
