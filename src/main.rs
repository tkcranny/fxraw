use clap::{Parser, Subcommand};

mod detect;
mod fuji;
mod ptp;

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
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Detect => detect::run(),
        Commands::Probe => fuji::probe(),
        Commands::Convert { input, output } => fuji::convert(&input, output.as_deref()),
    }
}
