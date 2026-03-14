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

        /// Keep the original white balance from the RAF instead of applying the recipe's WB
        #[arg(long)]
        keep_wb: bool,

        /// Show detailed step-by-step output (default: clean progress display)
        #[arg(short = 'v', long)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// Write fjx.toml (and optionally create _RAF); optional recipe as first output
    Create {
        /// Recipe slug to use as first [[output]] (default: classic-chrome)
        recipe_slug: Option<String>,
        /// Overwrite existing fjx.toml
        #[arg(long)]
        force: bool,
    },
    /// Load fjx.toml, check recipes and paths, ensure override keys match RAWs
    Validate,
    /// Run conversions from config (raw_dir + all outputs + overrides)
    Convert,
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
            ProjectCommand::Convert => project_convert(),
        },
        Commands::Convert {
            inputs,
            output,
            recipe,
            film_sim,
            grain,
            grain_size,
            exposure_comp,
            keep_wb,
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

            if keep_wb {
                settings.white_balance = None;
                settings.wb_temp = None;
                settings.wb_shift_r = None;
                settings.wb_shift_b = None;
                ui.keep_wb_notice();
            }

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
            fuji::convert(&mut *camera, &jobs, &settings, &ui);
        }
    }
}

// ---------------------------------------------------------------------------
// Project subcommand implementations
// ---------------------------------------------------------------------------

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
    let recipe = recipe_slug.unwrap_or("classic-chrome");
    let toml_content = format!(
        r#"# Fujifilm X100VI project config — use with: fjx project convert

# Directory containing RAF files (default: ./_RAF)
raw_dir = "./_RAF"

# Conversion outputs: each [[output]] gets its own directory.
[[output]]
recipe = "{}"
"#,
        recipe
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
    let project_root = to_absolute_path(project_root);
    let config = match config::load_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            std::process::exit(1);
        }
    };
    match config::validate_config(&project_root, &config) {
        Ok(()) => {
            println!("Config is valid.");
            let rafs = config::list_rafs_in_raw_dir(&project_root, &config.raw_dir).unwrap_or_default();
            println!("  raw_dir '{}': {} RAF(s)", config.raw_dir, rafs.len());
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("  {e}");
            }
            std::process::exit(1);
        }
    }
}

fn project_convert() {
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
    let total_jobs: usize = batches.iter().flat_map(|b| b.batches.iter().map(|(_, j)| j.len())).sum();
    if total_jobs == 0 {
        eprintln!(
            "No RAF files to convert in raw_dir '{}'. Add .raf files and run again.",
            config.raw_dir
        );
        return;
    }
    let ui = ui::ConvertProgress::new(false, total_jobs);
    let mut camera = fuji::open_camera();
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
                eprintln!("Error creating output dir {}: {}", output_batch.output_dir.display(), e);
                std::process::exit(1);
            });
            for path in &to_chown {
                fuji::chown_to_sudo_user(&path.to_string_lossy());
            }
        }
        for (settings, jobs) in &output_batch.batches {
            if !jobs.is_empty() {
                eprintln!(
                    "Converting {} file(s) to {} …",
                    jobs.len(),
                    output_batch.output_dir.display()
                );
                fuji::convert(&mut *camera, jobs, settings, &ui);
            }
            for (_, out_path) in jobs {
                all_written.push(std::path::PathBuf::from(out_path));
            }
        }
    }
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
