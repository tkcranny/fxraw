use clap::{Parser, Subcommand};
use fjx::profile::{parse_exposure_comp, FilmSimulation, GrainEffect, GrainSize};
use fjx::{analyse, config, detect, fuji, ptp, recipes, ui};

#[derive(Parser)]
#[command(name = "fjx")]
#[command(about = "Fujifilm X100VI USB RAW converter with film simulation recipes")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Detect a connected Fujifilm X100VI camera
    Detect,

    /// Probe the camera's PTP capabilities (operations, properties, formats)
    Probe,

    /// List available built-in recipes, or show details for a specific one
    Recipes {
        /// Recipe slug or name to show details for
        slug: Option<String>,
    },

    /// Analyse a RAF or JPEG — show EXIF recipe settings and closest recipe matches
    Analyse {
        /// Path to a RAF or JPEG file
        file: String,
    },

    /// Disable macOS PTP daemons so the camera can be used without sudo (requires sudo, one-time)
    Setup {
        /// Re-enable the PTP daemons (undo a previous setup)
        #[arg(long)]
        undo: bool,
    },

    /// Project config: create, validate, or convert from fjx.toml
    Project {
        #[command(subcommand)]
        subcommand: ProjectCommand,
    },

    /// Convert one or more RAF files to JPEG using the camera's image processor
    Convert {
        /// Path(s) to the input RAF file(s)
        #[arg(required = true)]
        inputs: Vec<String>,

        /// Output JPEG path or directory (directory is created if needed; defaults to <input>-<suffix>.jpg)
        #[arg(short, long)]
        output: Option<String>,

        /// Use a built-in recipe preset (name or partial match)
        #[arg(short, long)]
        recipe: Option<String>,

        /// Film simulation (overrides recipe if both given)
        #[arg(short, long, value_enum)]
        film_sim: Option<FilmSimulation>,

        /// Grain effect (overrides recipe if both given): off, weak, strong
        #[arg(short, long, value_enum)]
        grain: Option<GrainEffect>,

        /// Grain size when grain is on (overrides recipe): small, large
        #[arg(long = "grain-size", value_enum)]
        grain_size: Option<GrainSize>,

        /// Exposure compensation: +1.3, -0.7, +2, 0 (or +1 1/3, -2/3)
        #[arg(short, long, value_name = "EV", allow_hyphen_values = true)]
        exposure_comp: Option<String>,

        /// Show detailed step-by-step output (default: clean progress display)
        #[arg(short = 'v', long)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// Write fjx.toml (and optionally create _RAF); optional recipe as first output
    Create {
        /// Recipe slug to use as first [[output]] (default: reggies-portra)
        recipe_slug: Option<String>,
        /// Overwrite existing fjx.toml
        #[arg(long)]
        force: bool,
    },
    /// Load fjx.toml, check recipes and paths, ensure override keys match RAWs
    Validate,
    /// Run conversions from config (raw_dir + all outputs + overrides)
    Convert {
        /// Re-generate JPEGs even when output already exists for that profile
        #[arg(long)]
        force: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Detect => detect::run(),
        Commands::Probe => fuji::probe(),
        Commands::Recipes { slug } => {
            if let Some(query) = slug {
                match recipes::find(&query) {
                    Some(r) => recipes::show_recipe(r),
                    None => {
                        eprintln!("Recipe '{}' not found.", query);
                        eprintln!("Run `fjx recipes` to list available presets.");
                        std::process::exit(1);
                    }
                }
            } else {
                recipes::list_recipes();
            }
        }
        Commands::Analyse { file } => analyse::run(&file),
        Commands::Setup { undo } => {
            let result = if undo {
                ptp::enable_ptp_daemons()
            } else {
                ptp::disable_ptp_daemons()
            };
            match result {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Project { subcommand } => match subcommand {
            ProjectCommand::Create {
                recipe_slug,
                force,
            } => project_create(recipe_slug.as_deref(), force),
            ProjectCommand::Validate => project_validate(),
            ProjectCommand::Convert { force } => project_convert(force),
        },
        Commands::Convert {
            inputs,
            output,
            recipe,
            film_sim,
            grain,
            grain_size,
            exposure_comp,
            verbose,
        } => {
            let ui = ui::ConvertProgress::new(verbose, inputs.len());

            let ev = exposure_comp.map(|s| {
                parse_exposure_comp(&s).unwrap_or_else(|e| {
                    eprintln!("Invalid exposure compensation '{s}': {e}");
                    eprintln!("Examples: 0, +1, -0.3, +1.7, -2  (or +1/3, -2 2/3)");
                    std::process::exit(1);
                })
            });

            let mut settings = if let Some(ref name) = recipe {
                match recipes::find(name) {
                    Some(r) => {
                        ui.recipe_header(&r.name, &r.slug);
                        r.to_settings()
                    }
                    None => {
                        eprintln!("Recipe '{}' not found.", name);
                        eprintln!("Run `fjx recipes` to list available presets.");
                        std::process::exit(1);
                    }
                }
            } else {
                Default::default()
            };

            settings.merge_cli(film_sim, grain, grain_size, ev);

            let suffix = recipe.as_deref().unwrap_or("converted");

            let out_dir: Option<std::path::PathBuf> = output.as_ref().and_then(|o| {
                let p = std::path::Path::new(o);
                if p.is_dir() || o.ends_with('/') || o.ends_with(std::path::MAIN_SEPARATOR) || inputs.len() > 1 {
                    Some(p.to_path_buf())
                } else {
                    None
                }
            });

            if let Some(ref dir) = out_dir {
                if !dir.exists() {
                    // Collect path components we're about to create (for chown after)
                    let mut to_chown: Vec<std::path::PathBuf> = Vec::new();
                    let mut p = dir.as_path();
                    loop {
                        if !p.as_os_str().is_empty() && !p.exists() {
                            to_chown.push(p.to_path_buf());
                        }
                        match p.parent() {
                            Some(parent) if parent != p => p = parent,
                            _ => break,
                        }
                    }
                    std::fs::create_dir_all(dir).unwrap_or_else(|e| {
                        eprintln!("Error creating output directory '{}': {e}", dir.display());
                        std::process::exit(1);
                    });
                    for path in &to_chown {
                        fuji::chown_to_sudo_user(&path.to_string_lossy());
                    }
                }
            }

            let out_file: Option<&str> = if out_dir.is_none() {
                output.as_deref()
            } else {
                None
            };
            if out_file.is_some() && inputs.len() > 1 {
                eprintln!("Error: --output as a file path cannot be used with multiple inputs.");
                eprintln!("Pass a directory instead: -o outdir/");
                std::process::exit(1);
            }

            let jobs: Vec<(String, String)> = inputs
                .iter()
                .map(|input| {
                    let out = if let Some(f) = out_file {
                        f.to_string()
                    } else {
                        let name = {
                            let stem = std::path::Path::new(input)
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy();
                            format!("{stem}-{suffix}.jpg")
                        };
                        match out_dir {
                            Some(ref dir) => dir.join(&name).to_string_lossy().into_owned(),
                            None => name,
                        }
                    };
                    (input.clone(), out)
                })
                .collect();

            let mut camera = fuji::open_camera();
            fuji::convert(&mut *camera, &jobs, &settings, &ui, true);
        }
    }
}

// ---------------------------------------------------------------------------
// Project subcommand implementations
// ---------------------------------------------------------------------------

/// Path for display: relative to `root` when possible, otherwise the path as-is.
fn path_display_relative_to(path: &std::path::Path, root: &std::path::Path) -> std::path::PathBuf {
    path.strip_prefix(root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Resolve to an absolute path so that listing raw_dir does not depend on cwd.
fn to_absolute_path(p: &std::path::Path) -> std::path::PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(p)
    }
}

fn project_create(recipe_slug: Option<&str>, force: bool) {
    let cwd = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });
    let config_path = cwd.join(config::CONFIG_FILENAME);
    if config_path.exists() && !force {
        eprintln!("{} already exists. Use --force to overwrite.", config_path.display());
        std::process::exit(1);
    }
    let recipe = recipe_slug.unwrap_or("reggies-portra");
    let toml_content = format!(
        r#"# Fujifilm X100VI project config — use with: fjx project convert

# Directory containing RAF files (default: ./_RAF)
raw_dir = "./_RAF"

# Recipe slug for each [[output]].recipe. Examples: reggies-portra, kodachrome-64-2. Run `fjx recipes` to list all.

# Conversion outputs: each [[output]] gets its own directory.
[[output]]
recipe = "{recipe}"
# suffix = "classic"                 # output dir and filename suffix (default: recipe)
# film_sim = "provia"                # override: provia, velvia, classic-chrome, etc.
# grain = "weak"                     # weak | strong
# grain_size = "small"               # small | large
# exposure_comp = "+0.3"             # EV, e.g. +1, -0.7, +1/3
# wb_mode = "auto"                   # override WB (default: recipe's white balance)
# [[output.overrides]]               # per-RAF overrides within this output
#   match = "DSCF*.RAF"             # filename or glob
#   film_sim = "classic-chrome"
#   grain = "strong"
#   exposure_comp = "-0.3"

# Add more [[output]] blocks for additional recipes.

# Global overrides (key = filename or glob; apply to matching RAFs in all outputs):
# [overrides]
# "DSCF0001.RAF" = {{ film_sim = "velvia", exclude_outputs = ["portra-400"] }}
"#,
        recipe = recipe
    );
    std::fs::write(&config_path, toml_content).unwrap_or_else(|e| {
        eprintln!("Error writing {}: {}", config_path.display(), e);
        std::process::exit(1);
    });
    println!("Wrote {}", config_path.display());
    let raw_dir = cwd.join("_RAF");
    if !raw_dir.exists() {
        if std::fs::create_dir_all(&raw_dir).is_ok() {
            fuji::chown_to_sudo_user(&raw_dir.to_string_lossy());
            println!("Created {}", raw_dir.display());
        }
    }
}

fn project_validate() {
    let config_path = match config::find_project_config() {
        Some(p) => p,
        None => {
            eprintln!("No {} found in current or parent directories.", config::CONFIG_FILENAME);
            std::process::exit(1);
        }
    };
    let project_root = config_path.parent().unwrap_or(std::path::Path::new("."));
    let project_root = to_absolute_path(&project_root);
    let config = match config::load_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            std::process::exit(1);
        }
    };
    match config::validate_config(&project_root, &config) {
        Err(errors) => {
            eprintln!("Validation failed:");
            for e in &errors {
                eprintln!("  - {e}");
            }
            std::process::exit(1);
        }
        Ok(()) => {}
    }

    println!("Config is valid.\n");

    let rafs = config::list_rafs_in_raw_dir(&project_root, &config.raw_dir).unwrap_or_default();
    let batches = match config::expand_config(&project_root, &config) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error expanding config: {e}");
            std::process::exit(1);
        }
    };

    // Describe what operations will be done
    println!("Operations:");
    println!("  raw_dir \"{}\": {} RAF(s)", config.raw_dir, rafs.len());
    for (i, output_batch) in batches.iter().enumerate() {
        let total_jobs: usize = output_batch.batches.iter().map(|(_, j)| j.len()).sum();
        let rel = path_display_relative_to(&output_batch.output_dir, &project_root);
        let entry = &config.outputs[i];
        println!(
            "  output \"{}\" (recipe {}): {} → {} file(s)",
            rel.display(),
            entry.recipe,
            config.raw_dir,
            total_jobs
        );
    }

    // Show overrides and adjustments
    let has_global = !config.overrides.is_empty();
    let has_output_overrides = config.outputs.iter().any(|o| !o.overrides.is_empty());
    if has_global || has_output_overrides {
        println!("\nOverrides / adjustments:");
        for (key, ov) in &config.overrides {
            let (_, adjustments) = config::describe_global_override(key, ov);
            if !adjustments.is_empty() {
                println!("  global \"{}\": {}", key, adjustments.join(", "));
            }
            if !ov.exclude_outputs.is_empty() {
                println!("    exclude_outputs: [{}]", ov.exclude_outputs.join(", "));
            }
        }
        for entry in &config.outputs {
            let out_name = config::output_dir_name(entry);
            for ov in &entry.overrides {
                let (match_key, adjustments) = config::describe_output_override(ov);
                if !adjustments.is_empty() {
                    println!("  output \"{}\" match \"{}\": {}", out_name, match_key, adjustments.join(", "));
                }
            }
        }
    }
}

fn project_convert(force: bool) {
    let config_path = match config::find_project_config() {
        Some(p) => p,
        None => {
            eprintln!("No {} found in current or parent directories.", config::CONFIG_FILENAME);
            std::process::exit(1);
        }
    };
    let project_root = config_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let project_root = to_absolute_path(&project_root);
    let config = match config::load_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            std::process::exit(1);
        }
    };
    if let Err(errors) = config::validate_config(&project_root, &config) {
        for e in &errors {
            eprintln!("  {e}");
        }
        std::process::exit(1);
    }
    let batches = match config::expand_config(&project_root, &config) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error expanding config: {e}");
            std::process::exit(1);
        }
    };
    // Count only jobs we will run (output missing, or --force)
    let total_jobs: usize = batches
        .iter()
        .flat_map(|b| &b.batches)
        .map(|(_, jobs)| {
            jobs
                .iter()
                .filter(|(_, out)| force || !std::path::Path::new(out).exists())
                .count()
        })
        .sum();
    if total_jobs == 0 {
        eprintln!(
            "No RAF files to convert in raw_dir '{}'. (All outputs exist; use --force to re-generate.)",
            config.raw_dir
        );
        return;
    }
    let ui = ui::ConvertProgress::new_with_display_prefix(
        false,
        total_jobs,
        Some(project_root.clone()),
    );
    let mut camera = fuji::open_camera();
    eprintln!("Opening camera session (one session for all batches)…");
    camera.open_session().unwrap_or_else(|e| {
        eprintln!("Failed to open session: {e}");
        std::process::exit(1);
    });
    let mut all_written: Vec<std::path::PathBuf> = Vec::new();
    for output_batch in &batches {
        if !output_batch.output_dir.exists() {
            let mut to_chown = Vec::new();
            let mut p = output_batch.output_dir.as_path();
            loop {
                if !p.as_os_str().is_empty() && !p.exists() {
                    to_chown.push(p.to_path_buf());
                }
                match p.parent() {
                    Some(parent) if parent != p => p = parent,
                    _ => break,
                }
            }
            std::fs::create_dir_all(&output_batch.output_dir).unwrap_or_else(|e| {
                let rel = path_display_relative_to(&output_batch.output_dir, &project_root);
                eprintln!("Error creating output dir {}: {}", rel.display(), e);
                std::process::exit(1);
            });
            for path in &to_chown {
                fuji::chown_to_sudo_user(&path.to_string_lossy());
            }
        }
        for (settings, jobs) in &output_batch.batches {
            let jobs_to_do: Vec<(String, String)> = jobs
                .iter()
                .filter(|(_, out)| force || !std::path::Path::new(out).exists())
                .cloned()
                .collect();
            if !jobs_to_do.is_empty() {
                let rel = path_display_relative_to(&output_batch.output_dir, &project_root);
                eprintln!(
                    "Converting {} file(s) to {} …",
                    jobs_to_do.len(),
                    rel.display()
                );
                fuji::convert(&mut *camera, &jobs_to_do, settings, &ui, false);
            }
            for (_, out_path) in jobs {
                if std::path::Path::new(out_path).exists() {
                    all_written.push(std::path::PathBuf::from(out_path));
                }
            }
        }
    }
    let _ = camera.close_session();
    // Create _ALL_OUTPUTS and hardlink each written JPEG
    let all_outputs_dir = project_root.join(config::ALL_OUTPUTS_DIR);
    if std::fs::create_dir_all(&all_outputs_dir).is_ok() {
        fuji::chown_to_sudo_user(&all_outputs_dir.to_string_lossy());
        for path in &all_written {
            if path.is_file() {
                let name = path.file_name().map(|n| n.to_owned()).unwrap_or_default();
                let link_path = all_outputs_dir.join(&name);
                let _ = std::fs::remove_file(&link_path);
                let _ = std::fs::hard_link(path, &link_path);
            }
        }
    }
}
