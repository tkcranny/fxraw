use clap::{Parser, Subcommand};

mod detect;
mod fuji;
mod profile;
mod ptp;

use profile::{FilmSimulation, GrainEffect, RecipeSettings};

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

    /// Convert a RAF file to JPEG using the camera's image processor
    Convert {
        /// Path to the input RAF file
        input: String,

        /// Path for the output JPEG (defaults to <input>.jpg)
        #[arg(short, long)]
        output: Option<String>,

        /// Film simulation to apply
        #[arg(short, long, value_enum)]
        film_sim: Option<FilmSimulation>,

        /// Grain effect
        #[arg(short, long, value_enum)]
        grain: Option<GrainEffect>,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Detect => detect::run(),
        Commands::Probe => fuji::probe(),
        Commands::Convert {
            input,
            output,
            film_sim,
            grain,
        } => {
            let recipe = RecipeSettings { film_sim, grain };
            fuji::convert(&input, output.as_deref(), &recipe);
        }
    }
}
