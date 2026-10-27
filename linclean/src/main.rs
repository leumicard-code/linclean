// main.rs - CLI ראשי לכלי הניקוי linclean

mod engine;

use clap::{Parser, Subcommand};
use colored::*;
use engine::{CleanItem, CATEGORIES};
use humansize::{format_size, DECIMAL};
use std::io::{self, Write};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "linclean")]
#[command(about = "כלי CLI לניקוי קבצים מיותרים בלינוקס (בסגנון BleachBit)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// הצגת כל הקטגוריות הזמינות
    List,

    /// סריקה בלבד (ללא מחיקה) - מציג כמה מקום ניתן לשחרר
    Scan {
        /// קטגוריות לסריקה, למשל: browsers,dnf,tmp,logs,thumbnails,trash,podman,flatpak,journal (ברירת מחדל: הכול)
        #[arg(short, long)]
        categories: Option<String>,

        /// גיל מינימלי בימים לקבצים זמניים ב-/tmp
        #[arg(long, default_value_t = 1)]
        tmp_age: u64,

        /// סריקה מקבילית (thread לכל קטגוריה) - מהיר יותר
        #[arg(short, long)]
        parallel: bool,

        /// הצגת כל השורות (ברירת מחדל: מוצגות עד 15 שורות לקטגוריה)
        #[arg(long)]
        full: bool,
    },

    /// ניקוי בפועל
    Clean {
        /// קטגוריות לניקוי (ברירת מחדל: הכול)
        #[arg(short, long)]
        categories: Option<String>,

        #[arg(long, default_value_t = 1)]
        tmp_age: u64,

        /// כמה ימים לשמור בלוג journald
        #[arg(long, default_value_t = 7)]
        journal_keep_days: u32,

        /// סריקה מקבילית לפני המחיקה
        #[arg(short, long)]
        parallel: bool,

        /// דילוג על שאלת האישור (מסוכן - השתמש בזהירות)
        #[arg(short, long)]
        yes: bool,

        /// מצב ניסיון - רק הדפסה, בלי מחיקה בפועל
        #[arg(long)]
        dry_run: bool,

        /// הצגת כל השורות בפלט
        #[arg(long)]
        full: bool,
    },

    /// יצירת דוח טקסטואלי מסודר (ללא מחיקה) ושמירתו לקובץ בבית המשתמש
    Report {
        #[arg(short, long)]
        categories: Option<String>,

        #[arg(long, default_value_t = 1)]
        tmp_age: u64,

        #[arg(short, long)]
        parallel: bool,
    },
}

const DEFAULT_LINE_LIMIT: usize = 15;

fn selected_categories(arg: &Option<String>) -> Vec<&'static str> {
    match arg {
        Some(s) => s
            .split(',')
            .map(|s| s.trim())
            .filter_map(|s| CATEGORIES.iter().find(|c| c.id == s).map(|c| c.id))
            .collect(),
        None => CATEGORIES.iter().map(|c| c.id).collect(),
    }
}

/// מדפיס פריטים, ובברירת מחדל מגביל ל-DEFAULT_LINE_LIMIT שורות עם הודעת "עוד X שורות"
fn print_items(items: &[CleanItem], full: bool) {
    let limit = if full { items.len() } else { DEFAULT_LINE_LIMIT.min(items.len()) };
    for item in items.iter().take(limit) {
        println!(
            "  [{}] {} ({})",
            item.category.blue(),
            item.path.display(),
            format_size(item.size, DECIMAL).yellow()
        );
    }
    if !full && items.len() > limit {
        println!(
            "  {}",
            format!("... ועוד {} שורות (השתמש ב---full כדי להציג הכול)", items.len() - limit).dimmed()
        );
    }
}

fn confirm(prompt: &str) -> bool {
    print!("{} [y/N]: ", prompt);
    let _ = io::stdout().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// אוסף את כל הפריטים (מבוססי-קבצים) עבור קבוצת קטגוריות, במקביל או בטור
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

    match &cli.command {
        Commands::List => {
            println!("{}", "קטגוריות זמינות:".bold());
            for cat in CATEGORIES {
                let tag = if cat.command_based { " [command]".dimmed() } else { "".normal() };
                println!("  {:<12} - {:<25} ({}){}", cat.id.green(), cat.name, cat.description, tag);
            }
        }

        Commands::Scan { categories, tmp_age, parallel, full } => {
            let cats = selected_categories(categories);
            let start = Instant::now();

            let all_items = collect_items(&cats, *tmp_age, *parallel);

            // הצגה מקובצת לפי קטגוריה
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
                println!("\n{}", "[podman] הרצה עם 'clean --categories podman' תריץ גם 'podman system prune'".dimmed());
            }
            if cats.contains(&"flatpak") {
                println!("{}", "[flatpak] הרצה עם 'clean --categories flatpak' תריץ גם 'flatpak uninstall --unused'".dimmed());
            }
            if cats.contains(&"journal") {
                println!("{}", "[journal] מטופל ע\"י vacuum בזמן clean, לא בסריקה".dimmed());
            }

            let total = engine::total_size(&all_items);
            println!(
                "\n{} {}   ({:.2}s{})",
                "סה\"כ ניתן לשחרר:".bold().green(),
                format_size(total, DECIMAL).bold(),
                start.elapsed().as_secs_f64(),
                if *parallel { ", מקבילי".dimmed().to_string() } else { "".to_string() }
            );
        }

        Commands::Clean { categories, tmp_age, journal_keep_days, parallel, yes, dry_run, full } => {
            let cats = selected_categories(categories);
            let all_items = collect_items(&cats, *tmp_age, *parallel);

            if all_items.is_empty()
                && !cats.contains(&"journal")
                && !cats.contains(&"podman")
                && !cats.contains(&"flatpak")
            {
                println!("{}", "לא נמצא מה לנקות.".yellow());
                return;
            }

            if !all_items.is_empty() {
                println!("{}", "הפריטים הבאים ימחקו:".bold());
                print_items(&all_items, *full);
                let total = engine::total_size(&all_items);
                println!("\n{} {}", "סה\"כ:".bold().green(), format_size(total, DECIMAL).bold());
            }
            if cats.contains(&"podman") {
                println!("{}", "+ יופעל: podman system prune -f --volumes".cyan());
            }
            if cats.contains(&"flatpak") {
                println!("{}", "+ יופעל: flatpak uninstall --unused -y".cyan());
            }
            if cats.contains(&"journal") {
                println!("{} {} {}", "+ יופעל:".cyan(), "journalctl --vacuum-time=".cyan(), format!("{}d", journal_keep_days).cyan());
            }

            if *dry_run {
                println!("{}", "\n(dry-run: לא בוצעה מחיקה בפועל)".dimmed());
                return;
            }

            if !*yes && !confirm("\nלהמשיך בניקוי?") {
                println!("{}", "בוטל.".red());
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
                        eprintln!("{} {}: {}", "שגיאה במחיקת".red(), item.path.display(), e);
                    }
                }
            }
            if deleted > 0 || errors > 0 {
                println!(
                    "{} נמחקו {} פריטים, {} שגיאות. שוחרר בערך {}",
                    "✓".green().bold(),
                    deleted,
                    errors,
                    format_size(total, DECIMAL)
                );
            }

            if cats.contains(&"podman") {
                println!("\n{}", "מנקה Podman...".cyan());
                match engine::prune_podman() {
                    Ok(out) => println!("{}", out.trim()),
                    Err(e) => eprintln!("{} {}", "שגיאה בניקוי podman (מותקן?):".red(), e),
                }
            }
            if cats.contains(&"flatpak") {
                println!("\n{}", "מנקה Flatpak...".cyan());
                match engine::prune_flatpak() {
                    Ok(out) => println!("{}", out.trim()),
                    Err(e) => eprintln!("{} {}", "שגיאה בניקוי flatpak (מותקן?):".red(), e),
                }
            }
            if cats.contains(&"journal") {
                println!("\n{}", "מנקה journald (דורש root)...".cyan());
                match engine::vacuum_journal(*journal_keep_days) {
                    Ok(out) => println!("{}", out.trim()),
                    Err(e) => eprintln!("{} {}", "שגיאה בניקוי journald:".red(), e),
                }
            }
        }

        Commands::Report { categories, tmp_age, parallel } => {
            let cats = selected_categories(categories);
            println!("{}", "סורק לצורך דוח...".cyan());
            let all_items = collect_items(&cats, *tmp_age, *parallel);

            let report = engine::build_report(&all_items);
            println!("\n{}", report);

            match engine::save_report(&report) {
                Ok(path) => println!("{} {}", "הדוח נשמר ב:".green().bold(), path.display()),
                Err(e) => eprintln!("{} {}", "שגיאה בשמירת הדוח:".red(), e),
            }
        }
    }
}
