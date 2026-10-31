// ============================================================================
// gui.rs - LinClean Native Desktop GUI (BleachBit-style, built with wxDragon)
// ============================================================================
//
// Architecture:
//   1. `build_header_section`     -> Top title & subtitle banner
//   2. `build_categories_section` -> Left column: category checkboxes & select buttons
//   3. `build_results_section`    -> Right column: preview log & summary label
//   4. `build_actions_section`    -> Bottom bar: Scan, Clean, Save Report, Progress
//   5. Event wiring               -> Flat, top-to-bottom event handlers
// ============================================================================

mod engine;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use humansize::{format_size, DECIMAL};
use wxdragon::prelude::*;

use engine::{CleanItem, CATEGORIES};

// ============================================================================
// Shared UI Handles & State
// ============================================================================

/// Holds a category metadata entry alongside its live GUI checkbox widget.
#[derive(Clone)]
struct CategoryRow {
    id: &'static str,
    checkbox: CheckBox,
}

/// Widgets returned by `build_categories_section`.
struct CategoriesSection {
    sizer: StaticBoxSizer,
    rows: Vec<CategoryRow>,
    btn_select_all: Button,
    btn_clear_all: Button,
}

/// Widgets returned by `build_results_section`.
struct ResultsSection {
    sizer: StaticBoxSizer,
    output_log: TextCtrl,
    summary_label: StaticText,
}

/// Widgets returned by `build_actions_section`.
struct ActionsSection {
    sizer: BoxSizer,
    btn_scan: Button,
    btn_clean: Button,
    btn_report: Button,
    progress: Gauge,
    status_label: StaticText,
}

// ============================================================================
// 1. Header Section Builder
// ============================================================================

fn build_header_section(panel: &Panel, main_sizer: &BoxSizer) {
    let title = StaticText::builder(panel)
        .with_label("LinClean — Linux System Cleaner")
        .build();

    if let Some(font) = Font::builder()
        .with_point_size(16)
        .with_weight(FontWeight::Bold)
        .build()
    {
        title.set_font(&font);
    }

    let subtitle = StaticText::builder(panel)
        .with_label("Fast, safe BleachBit-style junk & cache cleaner for Linux")
        .build();

    main_sizer.add(&title, 0, SizerFlag::Top | SizerFlag::Left | SizerFlag::Right, 12);
    main_sizer.add(&subtitle, 0, SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 10);
}

// ============================================================================
// 2. Left Column: Categories Section Builder
// ============================================================================

fn build_categories_section(panel: &Panel) -> CategoriesSection {
    let sizer = StaticBoxSizerBuilder::new_with_label(
        Orientation::Vertical,
        panel,
        " Cleanup Categories ",
    )
    .build();

    let mut rows = Vec::with_capacity(CATEGORIES.len());

    for cat in CATEGORIES {
        let suffix = if cat.command_based { " [cmd]" } else { "" };
        let label = format!("{}  —  {}{}", cat.name, cat.description, suffix);

        let checkbox = CheckBox::builder(panel)
            .with_label(&label)
            .build();

        // Enable safe everyday categories by default; leave root-only journal unchecked.
        let default_checked = cat.id != "journal" && cat.id != "dnf" && cat.id != "logs";
        checkbox.set_value(default_checked);

        sizer.add(&checkbox, 0, SizerFlag::All | SizerFlag::Expand, 5);

        rows.push(CategoryRow {
            id: cat.id,
            checkbox,
        });
    }

    sizer.add_stretch_spacer(1);

    // Bottom row inside categories box: [Select All] [Clear Selection]
    let btn_row = BoxSizer::builder(Orientation::Horizontal).build();

    let btn_select_all = Button::builder(panel)
        .with_label("Select All")
        .build();

    let btn_clear_all = Button::builder(panel)
        .with_label("Clear All")
        .build();

    btn_row.add(&btn_select_all, 1, SizerFlag::All, 4);
    btn_row.add(&btn_clear_all, 1, SizerFlag::All, 4);

    sizer.add_sizer(&btn_row, 0, SizerFlag::Expand | SizerFlag::Top, 6);

    CategoriesSection {
        sizer,
        rows,
        btn_select_all,
        btn_clear_all,
    }
}

// ============================================================================
// 3. Right Column: Scan Results & Output Log Builder
// ============================================================================

fn build_results_section(panel: &Panel) -> ResultsSection {
    let sizer = StaticBoxSizerBuilder::new_with_label(
        Orientation::Vertical,
        panel,
        " Scan Preview & Activity Log ",
    )
    .build();

    let summary_label = StaticText::builder(panel)
        .with_label("Ready — Click 'Preview Scan' to inspect reclaimable space.")
        .build();

    if let Some(font) = Font::builder()
        .with_point_size(11)
        .with_weight(FontWeight::Bold)
        .build()
    {
        summary_label.set_font(&font);
    }

    let output_log = TextCtrl::builder(panel)
        .with_value(
            "Select the categories on the left and click 'Preview Scan'.\n\
             No files are deleted during a preview scan.\n",
        )
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::DontWrap)
        .build();

    if let Some(mono_font) = Font::builder()
        .with_family(FontFamily::Teletype)
        .with_point_size(10)
        .build()
    {
        output_log.set_font(&mono_font);
    }

    sizer.add(&summary_label, 0, SizerFlag::All | SizerFlag::Expand, 6);
    sizer.add(&output_log, 1, SizerFlag::All | SizerFlag::Expand, 6);

    ResultsSection {
        sizer,
        output_log,
        summary_label,
    }
}

// ============================================================================
// 4. Bottom Bar: Action Buttons & Progress Bar Builder
// ============================================================================

fn build_actions_section(panel: &Panel) -> ActionsSection {
    let sizer = BoxSizer::builder(Orientation::Vertical).build();

    // Row 1: Progress gauge + status text
    let progress = Gauge::builder(panel)
        .with_range(100)
        .build();
    progress.set_value(0);

    let status_label = StaticText::builder(panel)
        .with_label("Status: Idle")
        .build();

    sizer.add(&progress, 0, SizerFlag::Left | SizerFlag::Right | SizerFlag::Expand, 6);
    sizer.add(&status_label, 0, SizerFlag::All | SizerFlag::Expand, 6);

    // Row 2: Action buttons
    let btn_row = BoxSizer::builder(Orientation::Horizontal).build();

    let btn_scan = Button::builder(panel)
        .with_label("  Preview Scan  ")
        .build();

    let btn_clean = Button::builder(panel)
        .with_label("  Clean Now  ")
        .build();

    let btn_report = Button::builder(panel)
        .with_label("Save Report")
        .build();

    btn_row.add(&btn_scan, 0, SizerFlag::All, 6);
    btn_row.add(&btn_clean, 0, SizerFlag::All, 6);
    btn_row.add_stretch_spacer(1);
    btn_row.add(&btn_report, 0, SizerFlag::All, 6);

    sizer.add_sizer(&btn_row, 0, SizerFlag::Expand, 4);

    ActionsSection {
        sizer,
        btn_scan,
        btn_clean,
        btn_report,
        progress,
        status_label,
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Returns the list of currently checked category IDs.
fn collect_checked_ids(rows: &[CategoryRow]) -> Vec<&'static str> {
    rows.iter()
        .filter(|r| r.checkbox.get_value())
        .map(|r| r.id)
        .collect()
}

/// Formats the scan preview text displayed in the right-hand log pane.
fn format_preview_text(items: &[CleanItem], cats: &[&'static str], elapsed_sec: f64) -> String {
    let mut text = String::new();

    if items.is_empty()
        && !cats.contains(&"podman")
        && !cats.contains(&"flatpak")
        && !cats.contains(&"journal")
    {
        text.push_str("No junk files found for the selected categories.\n");
        return text;
    }

    text.push_str(&engine::build_report(items));
    text.push('\n');

    if cats.contains(&"podman") {
        text.push_str("[podman]  Will run: podman image prune -f (dangling images only; containers are safe)\n");
    }
    if cats.contains(&"flatpak") {
        text.push_str("[flatpak] Will run: flatpak uninstall --unused -y\n");
    }
    if cats.contains(&"journal") {
        text.push_str("[journal] Will run: journalctl --vacuum-time=7d (requires root)\n");
    }

    text.push_str(&format!("\nScan completed in {:.2}s (parallel).\n", elapsed_sec));
    text
}

// ============================================================================
// Application Entry Point
// ============================================================================

fn main() {
    SystemOptions::set_option_by_int("msw.no-manifest-check", 1);

    let _ = wxdragon::main(|_| {
        // ── Main Window & Root Layout ──
        let frame = Frame::builder()
            .with_title("LinClean — Linux System Cleaner")
            .with_size(Size::new(980, 660))
            .build();

        let panel = Panel::builder(&frame).build();
        let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

        // ── 1. Header ──
        build_header_section(&panel, &main_sizer);

        // ── 2 & 3. Split Body: Left (Categories) | Right (Results) ──
        let body_sizer = BoxSizer::builder(Orientation::Horizontal).build();

        let categories_ui = build_categories_section(&panel);
        let results_ui = build_results_section(&panel);

        // Left column fixed proportion (0), Right column expands (1)
        body_sizer.add_sizer(&categories_ui.sizer, 0, SizerFlag::All | SizerFlag::Expand, 8);
        body_sizer.add_sizer(&results_ui.sizer, 1, SizerFlag::All | SizerFlag::Expand, 8);

        main_sizer.add_sizer(&body_sizer, 1, SizerFlag::Expand, 0);

        // ── 4. Bottom Action Bar ──
        let actions_ui = build_actions_section(&panel);
        main_sizer.add_sizer(&actions_ui.sizer, 0, SizerFlag::All | SizerFlag::Expand, 8);

        panel.set_sizer(main_sizer, true);
        panel.layout();

        // Shared cache of the most recent scan results (used by "Save Report").
        let last_items: Arc<Mutex<Vec<CleanItem>>> = Arc::new(Mutex::new(Vec::new()));

        // ====================================================================
        // Event 1: Select All Categories
        // ====================================================================
        categories_ui.btn_select_all.on_click({
            let rows = categories_ui.rows.clone();
            move |_| {
                for row in &rows {
                    row.checkbox.set_value(true);
                }
            }
        });

        // ====================================================================
        // Event 2: Clear All Categories
        // ====================================================================
        categories_ui.btn_clear_all.on_click({
            let rows = categories_ui.rows.clone();
            move |_| {
                for row in &rows {
                    row.checkbox.set_value(false);
                }
            }
        });

        // ====================================================================
        // Event 3: Preview Scan
        // ====================================================================
        actions_ui.btn_scan.on_click({
            let rows = categories_ui.rows.clone();
            let output_log = results_ui.output_log;
            let summary_label = results_ui.summary_label;
            let progress = actions_ui.progress;
            let status_label = actions_ui.status_label;
            let last_items = Arc::clone(&last_items);

            move |_| {
                let cats = collect_checked_ids(&rows);
                if cats.is_empty() {
                    status_label.set_label("Status: Please select at least one category.");
                    return;
                }

                progress.set_value(25);
                status_label.set_label("Status: Scanning selected categories in parallel...");

                let start = Instant::now();
                let items = engine::scan_all_parallel(&cats, 1);
                let elapsed = start.elapsed().as_secs_f64();
                let total = engine::total_size(&items);

                let text = format_preview_text(&items, &cats, elapsed);
                output_log.set_value(&text);

                summary_label.set_label(&format!(
                    "Found {} items  •  Total reclaimable space: {}",
                    items.len(),
                    format_size(total, DECIMAL)
                ));
                status_label.set_label(&format!(
                    "Status: Scan complete in {:.2}s — Ready to clean.",
                    elapsed
                ));
                progress.set_value(100);

                if let Ok(mut guard) = last_items.lock() {
                    *guard = items;
                }
            }
        });

        // ====================================================================
        // Event 4: Clean Now (with native confirmation dialog)
        // ====================================================================
        actions_ui.btn_clean.on_click({
            let frame = frame;
            let rows = categories_ui.rows.clone();
            let output_log = results_ui.output_log;
            let summary_label = results_ui.summary_label;
            let progress = actions_ui.progress;
            let status_label = actions_ui.status_label;
            let last_items = Arc::clone(&last_items);

            move |_| {
                let cats = collect_checked_ids(&rows);
                if cats.is_empty() {
                    status_label.set_label("Status: Please select at least one category.");
                    return;
                }

                let items = engine::scan_all_parallel(&cats, 1);
                let total = engine::total_size(&items);

                let confirm_msg = format!(
                    "Are you sure you want to permanently clean {} items ({})?\n\n\
                     Selected categories: {}",
                    items.len(),
                    format_size(total, DECIMAL),
                    cats.join(", ")
                );

                let dlg = MessageDialog::builder(&frame, &confirm_msg, "Confirm System Cleanup")
                    .with_style(
                        MessageDialogStyle::YesNo
                            | MessageDialogStyle::IconWarning
                            | MessageDialogStyle::Centre,
                    )
                    .build();

                if dlg.show_modal() != ID_YES {
                    status_label.set_label("Status: Cleanup cancelled by user.");
                    return;
                }

                progress.set_value(10);
                status_label.set_label("Status: Cleaning files...");

                let mut log = String::new();
                let mut deleted = 0usize;
                let mut errors = 0usize;
                let total_count = items.len().max(1);

                for (idx, item) in items.iter().enumerate() {
                    match engine::delete_item(item) {
                        Ok(_) => {
                            deleted += 1;
                            log.push_str(&format!(
                                "[DELETED] {:>10}  {}\n",
                                format_size(item.size, DECIMAL),
                                item.path.display()
                            ));
                        }
                        Err(e) => {
                            errors += 1;
                            log.push_str(&format!(
                                "[ERROR]   {} ({})\n",
                                item.path.display(),
                                e
                            ));
                        }
                    }
                    let pct = 10 + ((idx + 1) * 70 / total_count) as i32;
                    progress.set_value(pct);
                }

                // External command-based cleanups
                if cats.contains(&"podman") {
                    log.push_str("\n--- Podman Safe Image Prune ---\n");
                    match engine::prune_podman() {
                        Ok(out) => log.push_str(out.trim()),
                        Err(e) => log.push_str(&format!("Error: {}", e)),
                    }
                    log.push('\n');
                }

                if cats.contains(&"flatpak") {
                    log.push_str("\n--- Flatpak Unused Runtimes ---\n");
                    match engine::prune_flatpak() {
                        Ok(out) => log.push_str(out.trim()),
                        Err(e) => log.push_str(&format!("Error: {}", e)),
                    }
                    log.push('\n');
                }

                if cats.contains(&"journal") {
                    log.push_str("\n--- Systemd Journal Vacuum (7 days) ---\n");
                    match engine::vacuum_journal(7) {
                        Ok(out) => log.push_str(out.trim()),
                        Err(e) => log.push_str(&format!("Error: {}", e)),
                    }
                    log.push('\n');
                }

                log.push_str(&format!(
                    "\nCleanup complete: {} items cleaned, {} errors. Reclaimed ~{}.\n",
                    deleted,
                    errors,
                    format_size(total, DECIMAL)
                ));

                output_log.set_value(&log);
                summary_label.set_label(&format!(
                    "Cleaned {} items ({} errors)  •  Reclaimed ~{}",
                    deleted,
                    errors,
                    format_size(total, DECIMAL)
                ));
                status_label.set_label("Status: Cleanup finished successfully.");
                progress.set_value(100);

                if let Ok(mut guard) = last_items.lock() {
                    guard.clear();
                }
            }
        });

        // ====================================================================
        // Event 5: Save Report to Disk
        // ====================================================================
        actions_ui.btn_report.on_click({
            let rows = categories_ui.rows.clone();
            let status_label = actions_ui.status_label;
            let last_items = Arc::clone(&last_items);

            move |_| {
                let items = {
                    let guard = last_items.lock().unwrap();
                    if guard.is_empty() {
                        let cats = collect_checked_ids(&rows);
                        engine::scan_all_parallel(&cats, 1)
                    } else {
                        guard.clone()
                    }
                };

                let report = engine::build_report(&items);
                match engine::save_report(&report) {
                    Ok(path) => {
                        status_label.set_label(&format!(
                            "Status: Report saved to {}",
                            path.display()
                        ));
                    }
                    Err(e) => {
                        status_label.set_label(&format!("Status: Failed to save report ({})", e));
                    }
                }
            }
        });

        frame.centre();
        frame.show(true);
    });
}
