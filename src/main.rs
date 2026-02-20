use clap::{Parser, Subcommand};

mod detect;
mod fuji;
mod profile;
mod ptp;
mod recipes;

use profile::{FilmSimulation, GrainEffect};

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

    /// List available built-in recipes
    Recipes,

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
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Detect => detect::run(),
        Commands::Probe => fuji::probe(),
        Commands::Recipes => recipes::list_recipes(),
        Commands::Convert {
            input,
            output,
            recipe,
            film_sim,
            grain,
        } => {
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

            settings.merge_cli(film_sim, grain);
            fuji::convert(&input, output.as_deref(), &settings);
        }
    }
}
