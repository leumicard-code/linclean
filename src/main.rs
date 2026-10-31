// main.rs - Command-Line Interface (CLI) for LinClean.

mod engine;

use std::io::{self, Write};
use std::time::Instant;

use clap::{Parser, Subcommand};
use colored::Colorize;
use humansize::{format_size, DECIMAL};

use engine::{CleanItem, CATEGORIES};

#[derive(Parser)]
#[command(
    name = "linclean",
    version = "0.2.0",
    about = "Fast, safe system cleaner for Linux (CLI + GUI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan selected categories and display reclaimable space (no files are deleted)
    Scan {
        /// Comma-separated categories: browsers,dnf,tmp,logs,thumbnails,trash,podman,flatpak,journal (default: all)
        #[arg(short, long)]
        categories: Option<String>,

        /// Minimum age in days for temporary files in /tmp and /var/tmp
        #[arg(long, default_value_t = 1)]
        tmp_age: u64,

        /// Run category scanners in parallel threads
        #[arg(short, long, default_value_t = true)]
        parallel: bool,

        /// Show every discovered item instead of truncating at 15 per category
        #[arg(long)]
        full: bool,
    },

    /// Delete junk files in the selected categories
    Clean {
        #[arg(short, long)]
        categories: Option<String>,

        #[arg(long, default_value_t = 1)]
        tmp_age: u64,

        /// Days of systemd journal logs to retain when vacuuming
        #[arg(long, default_value_t = 7)]
        journal_keep_days: u32,

        #[arg(short, long, default_value_t = true)]
        parallel: bool,

        /// Skip interactive confirmation prompt
        #[arg(short, long)]
        yes: bool,

        /// Preview what would be deleted without removing anything
        #[arg(long)]
        dry_run: bool,

        #[arg(long)]
        full: bool,
    },

    /// List all available cleanup categories
    List,

    /// Scan and save a detailed text report to the home directory
    Report {
        #[arg(short, long)]
        categories: Option<String>,

        #[arg(long, default_value_t = 1)]
        tmp_age: u64,

        #[arg(short, long, default_value_t = true)]
        parallel: bool,
    },
}

fn selected_categories(arg: &Option<String>) -> Vec<&'static str> {
    match arg {
        None => CATEGORIES.iter().map(|c| c.id).collect(),
        Some(s) => {
            let wanted: Vec<&str> = s.split(',').map(|x| x.trim()).collect();
            CATEGORIES
                .iter()
                .filter(|c| wanted.contains(&c.id))
                .map(|c| c.id)
                .collect()
        }
    }
}

fn confirm(prompt: &str) -> bool {
    print!("{} [y/N]: ", prompt);
    let _ = io::stdout().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    } else {
        false
    }
}

fn print_items(items: &[CleanItem], full: bool) {
    let limit = if full { items.len() } else { 15 };
    for item in items.iter().take(limit) {
        println!(
            "  {} {} ({})",
            format!("[{}]", item.category).cyan(),
            item.path.display(),
            format_size(item.size, DECIMAL).yellow()
        );
    }
    if !full && items.len() > limit {
        println!(
            "  {} (use --full to display all)",
            format!("... and {} more items", items.len() - limit).dimmed()
        );
    }
}

/// Collects file-based cleanup items either in parallel or sequentially.
fn collect_items(cats: &[&'static str], tmp_age: u64, parallel: bool) -> Vec<CleanItem> {
    if parallel {
        engine::scan_all_parallel(cats, tmp_age)
    } else {
        cats.iter()
            .filter(|id| **id != "journal")
            .flat_map(|id| engine::scan_category(id, tmp_age))
            .collect()
    }
}

fn main() {
    let cli = Cli::parse();

    let Some(cmd) = &cli.command else {
        println!("{}", "LinClean — Linux System Cleaner".bold().green());
        println!("Usage: linclean <COMMAND>  (or run `linclean-gui` for the graphical interface)\n");
        println!("Commands:");
        println!("  list     List all cleanup categories");
        println!("  scan     Scan and preview reclaimable space");
        println!("  clean    Clean selected categories (supports --dry-run)");
        println!("  report   Generate and save a cleanup report to ~/\n");
        println!("Run 'linclean --help' for full options.");
        return;
    };

    match cmd {
        Commands::List => {
            println!("{}", "Available Cleanup Categories:".bold().underline());
            for c in CATEGORIES {
                let tag = if c.command_based { " [cmd]" } else { "" };
                println!(
                    "  {:<12} - {} ({}){}",
                    c.id.green().bold(),
                    c.name,
                    c.description.dimmed(),
                    tag.yellow()
                );
            }
        }

        Commands::Scan { categories, tmp_age, parallel, full } => {
            let cats = selected_categories(categories);
            let start = Instant::now();

            let all_items = collect_items(&cats, *tmp_age, *parallel);

            for cat in &cats {
                if *cat == "journal" {
                    continue;
                }
                let cat_items: Vec<CleanItem> =
                    all_items.iter().filter(|i| i.category == *cat).cloned().collect();
                if !cat_items.is_empty() {
                    println!("\n{} {}", "==>".cyan(), cat.bold());
                    print_items(&cat_items, *full);
                }
            }

            if cats.contains(&"podman") {
                println!(
                    "\n{}",
                    "[podman] Running 'clean --categories podman' will also run 'podman image prune -f' (dangling images only; containers are kept)".dimmed()
                );
            }
            if cats.contains(&"flatpak") {
                println!(
                    "{}",
                    "[flatpak] Running 'clean --categories flatpak' will also run 'flatpak uninstall --unused'".dimmed()
                );
            }
            if cats.contains(&"journal") {
                println!(
                    "{}",
                    "[journal] Handled by journalctl vacuum during 'clean'".dimmed()
                );
            }

            let total = engine::total_size(&all_items);
            println!(
                "\n{} {}   ({:.2}s{})",
                "Total reclaimable:".bold().green(),
                format_size(total, DECIMAL).bold(),
                start.elapsed().as_secs_f64(),
                if *parallel { ", parallel".dimmed().to_string() } else { "".to_string() }
            );
        }

        Commands::Clean {
            categories,
            tmp_age,
            journal_keep_days,
            parallel,
            yes,
            dry_run,
            full,
        } => {
            let cats = selected_categories(categories);
            let all_items = collect_items(&cats, *tmp_age, *parallel);

            if all_items.is_empty()
                && !cats.contains(&"journal")
                && !cats.contains(&"podman")
                && !cats.contains(&"flatpak")
            {
                println!("{}", "Nothing to clean.".yellow());
                return;
            }

            if !all_items.is_empty() {
                println!("{}", "The following items will be deleted:".bold());
                print_items(&all_items, *full);
                let total = engine::total_size(&all_items);
                println!("\n{} {}", "Total:".bold().green(), format_size(total, DECIMAL).bold());
            }
            if cats.contains(&"podman") {
                println!("{}", "+ Will run: podman image prune -f (dangling images only; containers are kept)".cyan());
            }
            if cats.contains(&"flatpak") {
                println!("{}", "+ Will run: flatpak uninstall --unused -y".cyan());
            }
            if cats.contains(&"journal") {
                println!(
                    "{} {} {}",
                    "+ Will run:".cyan(),
                    "journalctl --vacuum-time=".cyan(),
                    format!("{}d", journal_keep_days).cyan()
                );
            }

            if *dry_run {
                println!("{}", "\n(dry-run: no files were deleted)".dimmed());
                return;
            }

            if !*yes && !confirm("\nProceed with cleanup?") {
                println!("{}", "Aborted.".red());
                return;
            }

            let mut deleted = 0u64;
            let mut errors = 0u64;
            let total = engine::total_size(&all_items);
            for item in &all_items {
                match engine::delete_item(item) {
                    Ok(_) => deleted += 1,
                    Err(e) => {
                        errors += 1;
                        eprintln!("{} {}: {}", "Error deleting".red(), item.path.display(), e);
                    }
                }
            }
            if deleted > 0 || errors > 0 {
                println!(
                    "{} Deleted {} items ({} errors). Reclaimed ~{}",
                    "✓".green().bold(),
                    deleted,
                    errors,
                    format_size(total, DECIMAL)
                );
            }

            if cats.contains(&"podman") {
                println!("\n{}", "Cleaning Podman dangling images...".cyan());
                match engine::prune_podman() {
                    Ok(out) => println!("{}", out.trim()),
                    Err(e) => eprintln!("{} {}", "Podman cleanup error:".red(), e),
                }
            }
            if cats.contains(&"flatpak") {
                println!("\n{}", "Cleaning Flatpak unused runtimes...".cyan());
                match engine::prune_flatpak() {
                    Ok(out) => println!("{}", out.trim()),
                    Err(e) => eprintln!("{} {}", "Flatpak cleanup error:".red(), e),
                }
            }
            if cats.contains(&"journal") {
                println!("\n{}", "Vacuuming systemd journal (requires root)...".cyan());
                match engine::vacuum_journal(*journal_keep_days) {
                    Ok(out) => println!("{}", out.trim()),
                    Err(e) => eprintln!("{} {}", "Journald cleanup error:".red(), e),
                }
            }
        }

        Commands::Report { categories, tmp_age, parallel } => {
            let cats = selected_categories(categories);
            println!("{}", "Scanning to generate report...".cyan());
            let all_items = collect_items(&cats, *tmp_age, *parallel);

            let report = engine::build_report(&all_items);
            println!("\n{}", report);

            match engine::save_report(&report) {
                Ok(path) => println!("{} {}", "Report saved to:".green().bold(), path.display()),
                Err(e) => eprintln!("{} {}", "Failed to save report:".red(), e),
            }
        }
    }
}
