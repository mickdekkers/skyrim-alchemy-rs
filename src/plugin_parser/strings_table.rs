use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use bsa::Reader;

fn strip_ext_from_plugin_name(plugin_name: &str) -> String {
    let plugin_name_without_ext = PathBuf::from_str(plugin_name)
        .expect("plugin_name should be valid path")
        .file_stem()
        .expect("plugin_name should not be empty")
        .to_str()
        .unwrap()
        .to_string();

    plugin_name_without_ext
}

fn get_bsa_name(plugin_name: &str) -> String {
    let plugin_name_without_ext = strip_ext_from_plugin_name(plugin_name);
    match plugin_name_without_ext.to_lowercase().as_str() {
        "dawnguard" | "dragonborn" | "hearthfires" | "skyrim" | "update" => {
            String::from("Skyrim - Interface.bsa")
        }
        _ => plugin_name_without_ext + ".bsa",
    }
}

fn get_strings_path(plugin_name: &str) -> String {
    format!(
        "strings/{}_english.strings",
        strip_ext_from_plugin_name(plugin_name).to_lowercase()
    )
}

#[derive(Debug)]
pub enum StringsLocation {
    /// A path to within a .bsa file. Consists of three parts in order:
    /// - The path to the .bsa file on disk
    /// - The dir in the .bsa file
    /// - The name of the strings file within the .bsa file
    BsaPath(String, String, String),
    /// A path on disk.
    DiskPath(String),
}

/// Tries to find a strings file for the given plugin name.
/// - Returns `Some(StringsLocation::DiskPath)` if found directly on disk.
/// - Returns `Some(StringsLocation::BsaPath)` if found in a .bsa file.
/// - Returns `None` if not found.
pub fn find_strings_file(plugin_name: &str, game_plugins_path: &Path) -> Option<StringsLocation> {
    assert_eq!(plugin_name.contains(|c| c == '/' || c == '\\'), false);
    let strings_path = get_strings_path(plugin_name);
    let strings_path_on_disk = game_plugins_path.join(&strings_path);

    // TODO: maybe handle fs errors explicitly instead of coercing to false?
    if strings_path_on_disk.exists() {
        return Some(StringsLocation::DiskPath(
            strings_path_on_disk.as_os_str().to_str().unwrap().into(),
        ));
    }

    let bsa_path = game_plugins_path.join(get_bsa_name(plugin_name));

    let mut bsa: bsa::SomeReaderV10X<_> = bsa::open(&bsa_path)
        .map_err(|err| println!("error opening bsa: {:?}", err))
        .ok()?;

    let (dir_name_in_bsa, file_name_in_bsa) = strings_path.split_once('/')?;

    let bsa_dirs_list = bsa
        .list()
        .map_err(|err| println!("error listing bsa dirs: {:?}", err))
        .ok()?;

    let dir_in_bsa = bsa_dirs_list.iter().find(|dir| {
        dir.id.name.as_ref().expect("dir in bsa should have name") == dir_name_in_bsa
    })?;

    let _file_in_bsa = dir_in_bsa.files.iter().find(|file| {
        file.id.name.as_ref().expect("file in bsa should have name") == file_name_in_bsa
    })?;

    Some(StringsLocation::BsaPath(
        bsa_path.as_os_str().to_str()?.into(),
        dir_name_in_bsa.into(),
        file_name_in_bsa.into(),
    ))
}
