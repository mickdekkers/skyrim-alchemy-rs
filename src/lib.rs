use anyhow::{anyhow, Context};
use lazy_static::lazy_static;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::{fs, io::BufReader};

mod plugin_parser;

lazy_static! {
    static ref GAME_PATH: PathBuf =
        PathBuf::from(&"H:/SteamLibrary/steamapps/common/Skyrim Special Edition");
    static ref GAME_PLUGINS_PATH: PathBuf = GAME_PATH.join(&"Data");
}

// const GAME_PATH: &str = ;
// const GAME_PLUGINS_DIR: &str = Path::join(Path::new())

fn gimme_load_order() -> Result<Vec<String>, anyhow::Error> {
    let game_settings = loadorder::GameSettings::new(loadorder::GameId::SkyrimSE, &GAME_PATH)?;
    let mut load_order = game_settings.into_load_order();
    // Read load order file contents
    load_order.load()?;
    // let plugins_file_path = load_order.game_settings().active_plugins_file().clone().into_os_string().into_string().unwrap();
    // println!("plugins file path: {:?}", plugins_file_path);
    let active_plugin_names = load_order.active_plugin_names();
    Ok(active_plugin_names.iter().map(|&s| s.into()).collect())
}

fn gimme_save_file() -> Result<skyrim_savegame::SaveFile, anyhow::Error> {
    let file_data = fs::read(
        "data/Save67_9C94C7CA_0_416D656C6961_RiftenBeeandBarb_001538_20220520233103_8_1.ess",
    )
    .context("Failed to open save file")?;
    // TODO: this may panic. Catch somehow?
    Ok(skyrim_savegame::parse_save_file(file_data))
}

fn gimme_plugin_test(load_order: &Vec<String>) -> Result<(), anyhow::Error> {
    let test_plugin = load_order
        .iter()
        .nth(9)
        .ok_or(anyhow!("Load order empty!"))?;
    let plugin_path = GAME_PLUGINS_PATH.join(test_plugin);
    // let mut plugin = esplugin::Plugin::new(esplugin::GameId::SkyrimSE, &plugin_path);
    // // Load plugin data
    // plugin.parse_file(true)?;
    // println!("Plugin:\n{:#?}", plugin);
    // let description = plugin.description()?;
    // println!("Plugin description:\n{:#?}", &description);
    // let header_version = plugin.header_version();
    // println!("Plugin header version:\n{:#?}", header_version);

    let plugin_file = File::open(&plugin_path)?;
    // TODO: implement better (safer, streaming) file loading
    let plugin_mmap = unsafe { memmap::MmapOptions::new().map(&plugin_file)? };
    plugin_parser::parse_plugin(&plugin_mmap, test_plugin)?;
    Ok(())
}

pub fn do_the_thing() -> Result<(), anyhow::Error> {
    let save_file = gimme_save_file()?;
    println!("{:#?}", save_file);
    let load_order = gimme_load_order()?;
    println!("Load order:\n{:#?}", &load_order);
    gimme_plugin_test(&load_order)?;
    Ok(())
}
