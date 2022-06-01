use clap::{Parser, Subcommand};
use log::LevelFilter;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    /// Makes logging more verbose. Pass once for debug log level, twice for trace log level.
    #[clap(short, parse(from_occurrences), global = true)]
    verbose: u8,
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Reads ingredients and magic effects game data using your load order and exports it to a JSON
    /// file for later usage.
    ExportGameData {
        /// Path to the game directory containing SkyrimSE.exe.
        #[clap(long)]
        game_path: String,
        /// Path to the directory containing plugins.txt. Defaults to "%LocalAppData%/Skyrim Special Edition" if not specified.
        #[clap(long)]
        local_path: Option<String>,
        /// Path to the JSON file that the game data will be written to.
        export_path: String,
    },

    // TODO: provide option to suggest potions using only ingredients that the player has
    /// Suggests potions to mix using the ingredients and magic effects in the game data.
    SuggestPotions {
        /// Path to the JSON file that contains the game data. This file can be obtained through the
        /// export-game-data subcommand.
        data_path: String,
    },
}

fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(match cli.verbose {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        })
        .init();

    match &cli.command {
        Commands::ExportGameData {
            game_path,
            local_path,
            export_path,
        } => {
            skyrim_alchemy_rs::parse_and_export_game_data(
                game_path,
                local_path.as_ref(),
                export_path,
            )?;
        }
        Commands::SuggestPotions { data_path } => {
            skyrim_alchemy_rs::suggest_potions(data_path)?;
        }
    }

    Ok(())
}
