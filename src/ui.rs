use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

const STEP_COUNT: u64 = 6;

pub struct ConvertProgress {
    verbose: bool,
    total: usize,
    bar: ProgressBar,
}

impl ConvertProgress {
    pub fn new(verbose: bool, total: usize) -> Self {
        let bar = if verbose {
            ProgressBar::hidden()
        } else {
            ProgressBar::new(STEP_COUNT)
        };
        Self { verbose, total, bar }
    }

    // -- batch-level ---------------------------------------------------------

    pub fn batch_header(&self, total: usize) {
        if self.verbose && total > 1 {
            println!("Converting {total} files\n");
        }
    }

    pub fn summary(&self, succeeded: u32, failed: u32) {
        self.bar.finish_and_clear();
        if self.total > 1 {
            let ok = style(format!("{succeeded} succeeded")).green().bold();
            let fail_style = if failed > 0 {
                style(format!("{failed} failed")).red().bold()
            } else {
                style(format!("{failed} failed")).dim()
            };
            println!("\n{ok}, {fail_style} out of {} files.", self.total);
        }
    }

    // -- file-level ----------------------------------------------------------

    pub fn file_start(&self, index: usize, input: &str, output: &str, raf_size_mb: f64) {
        let file_num = index + 1;
        if self.verbose {
            if self.total > 1 {
                println!("\n{}", "=".repeat(60));
                println!("[{file_num}/{}] {input}", self.total);
                println!("{}", "=".repeat(60));
            }
            println!("Input:  {input}");
            println!("Output: {output}");
            println!("RAF:    {raf_size_mb:.1} MB\n");
        } else {
            let prefix = if self.total > 1 {
                format!("[{file_num}/{}] ", self.total)
            } else {
                String::new()
            };
            let stem = std::path::Path::new(input)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            self.bar.reset();
            self.bar.set_position(0);
            self.bar.set_length(STEP_COUNT);
            self.bar.set_style(
                ProgressStyle::with_template(&format!(
                    "  {prefix}{{bar:24.cyan/dim}} {{pos}}/{{len}} {{msg}}  {stem}"
                ))
                .unwrap()
                .progress_chars("━╸─"),
            );
            self.bar.set_message("connecting…");
            self.bar.tick();
        }
    }

    pub fn file_done(&self, output: &str, size_mb: f64) {
        if self.verbose {
            println!("\n  Conversion complete!");
        } else {
            self.bar.set_position(STEP_COUNT);
            self.bar.set_message("done");
            self.bar.finish_and_clear();
            println!(
                "  {} {} ({:.1} MB)",
                style("✓").green().bold(),
                output,
                size_mb
            );
        }
    }

    pub fn file_failed(&self, input: &str, err: &str) {
        if self.verbose {
            eprintln!("\n  FAILED: {err}");
        } else {
            self.bar.finish_and_clear();
            eprintln!(
                "  {} {}: {}",
                style("✗").red().bold(),
                input,
                style(err).red()
            );
        }
    }

    // -- step-level ----------------------------------------------------------

    pub fn step(&self, num: u8, msg: &str) {
        if self.verbose {
            println!("[{num}/{STEP_COUNT}] {msg}");
        } else {
            self.bar.set_position(u64::from(num).saturating_sub(1));
            self.bar.set_message(msg.to_string());
        }
    }

    pub fn step_detail(&self, msg: &str) {
        if self.verbose {
            println!("  {msg}");
        }
    }

    // -- poll spinner --------------------------------------------------------

    pub fn poll_start(&self) {
        if !self.verbose {
            self.bar.set_position(5);
            self.bar.set_style(
                ProgressStyle::with_template(
                    "  {spinner:.cyan} {msg}  {elapsed_precise}"
                )
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
            );
            self.bar.set_message("waiting for camera…");
            self.bar.enable_steady_tick(Duration::from_millis(100));
        }
    }

    pub fn poll_tick(&self, attempt: u32, max_polls: u32) {
        if self.verbose {
            print!("  Poll {attempt}/{max_polls}... ");
        }
    }

    pub fn poll_result(&self, msg: &str) {
        if self.verbose {
            println!("{msg}");
        }
    }

    pub fn poll_done(&self) {
        if !self.verbose {
            self.bar.disable_steady_tick();
        }
    }

    // -- validation / info messages ------------------------------------------

    pub fn meta_info(&self, input: &str, iso: Option<u32>, dr: Option<u32>) {
        if self.verbose {
            let iso_str = iso.map(|i| format!("ISO {i}")).unwrap_or("ISO ?".into());
            let dr_str = dr.map(|d| format!("DR{d}")).unwrap_or("DR?".into());
            println!("{input}: shot at {iso_str}, {dr_str}");
        }
    }

    pub fn dr_clamped(&self, msg: &str) {
        if self.verbose {
            eprintln!("  {msg}");
        }
    }

    pub fn camera_info(&self, manufacturer: &str, model: &str, fw: &str) {
        if self.verbose {
            println!("  {manufacturer} {model} (fw {fw})");
        }
    }

    pub fn usb_mode(&self, mode: u16) {
        if self.verbose {
            println!("  USB mode = {mode} (expect 6 for RAW CONV)");
            if mode != 6 {
                eprintln!("\n  WARNING: Camera may not be in RAW CONV mode.");
                eprintln!(
                    "  Set camera to: Connection Setting -> Connection Mode -> USB RAW CONV./BACKUP RESTORE"
                );
                eprintln!("  Then reconnect USB and retry.\n");
            }
        }
    }

    pub fn usb_mode_unreadable(&self) {
        if self.verbose {
            println!("  Could not read 0xD16E (connection mode)");
        }
    }

    pub fn recipe_header(&self, name: &str, slug: &str) {
        if self.verbose {
            println!("Recipe: {name} ({slug})\n");
        } else {
            println!(
                "  {} {}",
                style("recipe").dim(),
                style(format!("{name} ({slug})")).bold()
            );
        }
    }

    pub fn keep_wb_notice(&self) {
        if self.verbose {
            println!("--keep-wb: using original white balance from RAF\n");
        }
    }
}
