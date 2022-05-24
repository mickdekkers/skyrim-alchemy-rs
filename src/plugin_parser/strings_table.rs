use std::{
    cell::RefCell,
    fs::File,
    io::Read,
    mem,
    ops::DerefMut,
    path::{Path, PathBuf},
    str::FromStr,
};

use bsa::Reader;
use nom::{error::ErrorKind, number::complete::le_u32};

use crate::plugin_parser::utils::parse_zstring;

use super::utils::nom_err_to_anyhow_err;

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
    /// A path to within a .bsa file. Consists of two parts:
    /// - The path to the .bsa file on disk
    /// - A `bsa::read::File` object which describes the file within the .bsa
    BsaPath(String, bsa::read::File),
    /// A path on disk.
    DiskPath(String),
}

/// Tries to find a strings file for the given plugin name.
/// - Returns `Some(StringsLocation::DiskPath)` if found directly on disk.
/// - Returns `Some(StringsLocation::BsaPath)` if found in a .bsa file.
/// - Returns `None` if not found.
fn find_strings_file(plugin_name: &str, game_plugins_path: &Path) -> Option<StringsLocation> {
    assert!(!plugin_name.contains(|c| c == '/' || c == '\\'));
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

    let file_in_bsa = dir_in_bsa.files.iter().find(|file| {
        file.id.name.as_ref().expect("file in bsa should have name") == file_name_in_bsa
    })?;

    Some(StringsLocation::BsaPath(
        bsa_path.as_os_str().to_str()?.into(),
        file_in_bsa.clone(),
    ))
}

pub struct StringsTable {
    location: StringsLocation,
    data: RefCell<Vec<u8>>,
    did_load: RefCell<bool>,
    directory: RefCell<Vec<(u32, u32)>>,
}

impl StringsTable {
    // TODO: return Result instead of Option
    pub fn new(plugin_name: &str, game_plugins_path: &Path) -> Option<Self> {
        Some(Self {
            location: find_strings_file(plugin_name, game_plugins_path)?,
            data: RefCell::new(Vec::new()),
            did_load: RefCell::new(false),
            directory: RefCell::new(Vec::new()),
        })
    }

    fn load(&self) -> Result<(), anyhow::Error> {
        if *self.did_load.borrow() {
            return Ok(());
        }

        {
            let mut data = self.data.borrow_mut();

            match &self.location {
                StringsLocation::DiskPath(p) => {
                    let mut file = File::open(&p)?;
                    file.read_to_end(&mut data)?;
                }
                StringsLocation::BsaPath(bsa_path, file_in_bsa) => {
                    let mut bsa: bsa::SomeReaderV10X<_> = bsa::open(&bsa_path)?;
                    bsa.extract(file_in_bsa, &mut data.deref_mut())?;
                }
            }
        }

        self.load_directory()?;
        *self.did_load.borrow_mut() = true;

        Ok(())
    }

    fn load_directory(&self) -> Result<(), anyhow::Error> {
        let mut data = self.data.borrow_mut();

        let (remaining_input, (num_strings, strings_size)) =
            nom::sequence::pair(le_u32, le_u32)(data.as_slice())
                .map_err(|err: nom::Err<(_, ErrorKind)>| nom_err_to_anyhow_err(err))?;

        let mut directory_entries: Vec<(u32, u32)> = nom::multi::count(
            nom::sequence::pair(le_u32, le_u32),
            num_strings as usize,
        )(remaining_input)
        .map_err(|err: nom::Err<(_, ErrorKind)>| nom_err_to_anyhow_err(err))?
        .1;

        // Sort by ID for binary search
        directory_entries.sort_by_key(|e| e.0);
        *self.directory.borrow_mut() = directory_entries;

        // Skip two u32s, plus two u32s for every directory entry
        let data_start_offset = mem::size_of::<u32>() * 2 * (1 + num_strings as usize);

        data.drain(..data_start_offset);
        assert_eq!(data.len(), strings_size as usize);

        Ok(())
    }

    pub fn get(&self, id: u32) -> Option<String> {
        self.load()
            .map_err(|err| println!("failed to load strings table: {:?}", err))
            .ok()?;

        let directory = self.directory.borrow();
        let offset = directory[directory.binary_search_by_key(&id, |e| e.0).ok()?].1;

        let data = self.data.borrow();
        // TODO: maybe just use string slice syntax instead of nom take here
        // TODO: error handling
        let (string_data, _) =
            nom::bytes::complete::take::<_, _, nom::error::Error<_>>(offset)(data.as_slice())
                .unwrap();

        Some(parse_zstring(string_data))
    }
}
