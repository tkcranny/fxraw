use clap::{Parser, Subcommand};

mod detect;
mod fuji;
mod profile;
mod ptp;
mod recipes;

use profile::{parse_exposure_comp, FilmSimulation, GrainEffect};

#[derive(Parser)]
#[command(name = "fuji-usb-test")]
#[command(about = "Detect and interact with a Fujifilm X100VI camera over USB")]
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

    /// Convert a RAF file to JPEG using the camera's image processor
    Convert {
        /// Path to the input RAF file
        input: String,

        /// Path for the output JPEG (defaults to <input>.jpg)
        #[arg(short, long)]
        output: Option<String>,

        /// Use a built-in recipe preset (name or partial match)
        #[arg(short, long)]
        recipe: Option<String>,

        /// Film simulation (overrides recipe if both given)
        #[arg(short, long, value_enum)]
        film_sim: Option<FilmSimulation>,

        /// Grain effect (overrides recipe if both given)
        #[arg(short, long, value_enum)]
        grain: Option<GrainEffect>,

        /// Exposure compensation: +1.3, -0.7, +2, 0 (or +1 1/3, -2/3)
        #[arg(short, long, value_name = "EV", allow_hyphen_values = true)]
        exposure_comp: Option<String>,
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
                        eprintln!("Run `fuji-usb-test recipes` to list available presets.");
                        std::process::exit(1);
                    }
                }
            } else {
                recipes::list_recipes();
            }
        }
        Commands::Convert {
            input,
            output,
            recipe,
            film_sim,
            grain,
            exposure_comp,
        } => {
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
                        println!("Recipe: {} ({})\n", r.name, r.slug);
                        r.to_settings()
                    }
                    None => {
                        eprintln!("Recipe '{}' not found.", name);
                        eprintln!("Run `fuji-usb-test recipes` to list available presets.");
                        std::process::exit(1);
                    }
                }
            } else {
                Default::default()
            };

            settings.merge_cli(film_sim, grain, ev);
            fuji::convert(&input, output.as_deref(), &settings);
        }
    }
}
