#![feature(try_blocks)]

use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use ahash::AHashSet;
use clap::{ArgGroup, Parser, Subcommand};
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
    #[clap(group(ArgGroup::new("ingredients-filter").args(&["ingredients-blacklist-path", "ingredients-whitelist-path"])))]
    SuggestPotions {
        /// If specified, potions containing any of the ingredients in the file will not be
        /// suggested. The file must contain one ingredient name per line.
        #[clap(long)]
        ingredients_blacklist_path: Option<String>,
        /// If specified, only potions containing only the ingredients in the file will be
        /// suggested. The file must contain one ingredient name per line.
        #[clap(long)]
        ingredients_whitelist_path: Option<String>,
        // TODO: validate limit arg (gte 1)
        /// Limit the number of suggestions to at most this many potions.
        #[clap(long, default_value_t = 20usize)]
        limit: usize,
        /// Path to the JSON file that contains the game data. This file can be obtained through the
        /// export-game-data subcommand.
        data_path: String,
    },
}

fn read_lines_to_hashset<P>(path: P) -> Result<AHashSet<String>, anyhow::Error>
where
    P: AsRef<Path>,
{
    let file = File::open(path)?;
    let buf = BufReader::new(file);
    let lines = buf
        .lines()
        .map(|l| l.expect("Failed to read line").trim().to_string())
        .collect::<AHashSet<String>>();
    Ok(lines)
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
        Commands::SuggestPotions {
            data_path,
            ingredients_blacklist_path: ingredients_blacklist_file,
            ingredients_whitelist_path: ingredients_whitelist_file,
            limit,
        } => {
            let ingredients_blacklist = ingredients_blacklist_file
                .as_ref()
                .map(read_lines_to_hashset)
                .transpose()?
                .unwrap_or_default();
            let ingredients_whitelist = ingredients_whitelist_file
                .as_ref()
                .map(read_lines_to_hashset)
                .transpose()?
                .unwrap_or_default();

            skyrim_alchemy_rs::suggest_potions(
                data_path,
                &ingredients_blacklist,
                &ingredients_whitelist,
                *limit,
            )?;
        }
    }

    Ok(())
}
