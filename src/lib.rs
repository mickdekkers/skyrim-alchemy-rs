#![feature(hash_drain_filter)]

use anyhow::{anyhow, Context};
use itertools::Itertools;
use log_err::{LogErrOption, LogErrResult};
use nom::IResult;
use skyrim_savegame::{read_vsval_to_u32, ChangeForm, FormIdType, SaveFile, VSVal};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::game_data::GameData;
use crate::plugin_parser::utils::nom_err_to_anyhow_err;
use crate::plugin_parser::{
    form_id::FormIdContainer, ingredient::Ingredient, magic_effect::MagicEffect,
};
use crate::potions_list::PotionsList;

mod game_data;
mod plugin_parser;
mod potion;
mod potions_list;

fn get_load_order<PGame, PLocal>(
    game_path: PGame,
    local_path: Option<PLocal>,
) -> Result<Vec<String>, anyhow::Error>
where
    PGame: AsRef<Path>,
    PLocal: AsRef<Path>,
{
    let game_settings = match local_path {
        Some(local_path) => loadorder::GameSettings::with_local_path(
            loadorder::GameId::SkyrimSE,
            game_path.as_ref(),
            local_path.as_ref(),
        ),
        None => loadorder::GameSettings::new(loadorder::GameId::SkyrimSE, game_path.as_ref()),
    }?;
    let mut load_order = game_settings.into_load_order();
    // Read load order file contents
    load_order.load()?;
    log::debug!(
        "plugins file path: {:?}",
        load_order.game_settings().active_plugins_file()
    );
    let active_plugin_names = load_order.active_plugin_names();
    Ok(active_plugin_names.iter().map(|&s| s.into()).collect())
}

fn load_ingredients_and_effects_from_plugins<PGame>(
    game_path: PGame,
    load_order: &Vec<String>,
) -> Result<GameData, anyhow::Error>
where
    PGame: AsRef<Path>,
{
    if load_order.is_empty() {
        Err(anyhow!("Load order empty!"))?
    }

    let game_plugins_path = game_path.as_ref().join("Data");

    // TODO: use &str instead of String for keys
    let mut magic_effects = HashMap::<(String, u32), MagicEffect>::new();
    let mut ingredients = HashMap::<(String, u32), Ingredient>::new();
    let mut ingredient_effect_ids = HashSet::<(String, u32)>::new();

    for plugin_name in load_order.iter() {
        let plugin_path = game_plugins_path.join(plugin_name);

        let plugin_file = File::open(&plugin_path)?;
        // TODO: implement better (safer, streaming) file loading
        let plugin_mmap = unsafe { memmap2::MmapOptions::new().map(&plugin_file)? };
        let (plugin_ingredients, plugin_magic_effects) =
            plugin_parser::parse_plugin(&plugin_mmap, plugin_name, &game_plugins_path)?;

        log::debug!(
            "Plugin {:?} has {:?} ingredients and {:?} magic effects.",
            plugin_name,
            plugin_ingredients.len(),
            plugin_magic_effects.len()
        );

        for plugin_magic_effect in plugin_magic_effects.into_iter() {
            // Insert into magic effects hashmap, overwriting existing entry from previous plugins
            magic_effects.insert(
                plugin_magic_effect.get_global_form_id().to_owned_pair(),
                plugin_magic_effect,
            );
        }

        for plugin_ingredient in plugin_ingredients.into_iter() {
            // Add ingredient effect IDs to set of known used effects
            for plugin_ingredient_effect_id in plugin_ingredient
                .effects
                .iter()
                .map(|eff| eff.get_global_form_id())
            {
                ingredient_effect_ids.insert(plugin_ingredient_effect_id.to_owned_pair());
            }

            // Insert into magic effects hashmap, overwriting existing entry from previous plugins
            ingredients.insert(
                plugin_ingredient.get_global_form_id().to_owned_pair(),
                plugin_ingredient,
            );
        }
    }

    // Remove from the magic effects all those that are not used by ingredients
    log::debug!("Number of ingredients: {}", ingredients.len());
    log::debug!(
        "Number of magic effects before filtering: {}",
        magic_effects.len()
    );
    magic_effects.drain_filter(|key, _| !ingredient_effect_ids.contains(key));
    log::debug!(
        "Number of magic effects after filtering: {}",
        magic_effects.len()
    );

    let mut game_data = GameData::from_hashmaps(ingredients, magic_effects);
    game_data.purge_invalid();

    Ok(game_data)
}

pub fn parse_and_export_game_data<PGame, PLocal, PExport>(
    game_path: PGame,
    local_path: Option<PLocal>,
    export_path: PExport,
) -> Result<(), anyhow::Error>
where
    PGame: AsRef<Path>,
    PLocal: AsRef<Path>,
    PExport: AsRef<Path>,
{
    let load_order = get_load_order(&game_path, local_path)?;
    log::debug!("Load order:\n{:#?}", &load_order);

    let game_data = load_ingredients_and_effects_from_plugins(&game_path, &load_order)?;
    let serialized_game_data = serde_json::to_string_pretty(&game_data).unwrap();
    fs::write(export_path, serialized_game_data)?;

    Ok(())
}

pub fn import_game_data<PImport>(import_path: PImport) -> Result<GameData, anyhow::Error>
where
    PImport: AsRef<Path>,
{
    let file = File::open(import_path)?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|err| anyhow!(err.to_string()))
}

pub fn suggest_potions<PImport>(import_path: PImport) -> Result<(), anyhow::Error>
where
    PImport: AsRef<Path>,
{
    let game_data = import_game_data(import_path)?;

    let mut potions_list = PotionsList::new(&game_data);
    potions_list.build_potions();

    // You probably don't want to uncomment this, it'll write out a 600Mb+ json file
    // {
    //     let serialized_potions = serde_json::to_string_pretty(&potions_list).unwrap();
    //     fs::write("data/potions.json", serialized_potions)?;
    // }

    let unwanted_ingredients = HashSet::<&str>::from_iter(vec!["Jarrin Root"]);
    let wanted_ingredients = HashSet::<&str>::from_iter(vec![]);

    potions_list
        .get_potions()
        .filter(|p| {
            wanted_ingredients.is_empty()
                || p.ingredients.iter().any(|ing| {
                    wanted_ingredients.contains(ing.name.as_deref().unwrap_or("__nope__"))
                })
        })
        .filter(|p| {
            unwanted_ingredients.is_empty()
                || !p.ingredients.iter().any(|ing| {
                    unwanted_ingredients.contains(ing.name.as_deref().unwrap_or("__nope__"))
                })
        })
        .take(100)
        .for_each(|p| println!("{}\n", p));

    Ok(())
}

fn gimme_save_file() -> Result<skyrim_savegame::SaveFile, anyhow::Error> {
    let file_data = fs::read(
        "data/Save327_CE738163_0_52756279_WhiterunBanneredMare_001512_20220531222232_10_1.ess",
    )
    .context("Failed to open save file")?;
    // TODO: this may panic. Catch somehow?
    Ok(skyrim_savegame::parse_save_file(file_data))
}

pub fn do_stuff_with_save_file() -> Result<(), anyhow::Error> {
    let save_file = gimme_save_file()?;
    log::info!("{:#?}", save_file);

    let player_change_form = save_file
        .change_forms
        .iter()
        .find(|cf| {
            matches!(
                get_change_form_data_type(cf),
                Some(ChangeFormDataType::Actor)
            ) && ({
                let form_id = get_real_form_id(&cf.form_id, &save_file).log_unwrap();

                // Is player change form
                form_id == 0x14
            })
        })
        .log_expect("save game contains no player data");

    dbg!(player_change_form);

    // See https://en.uesp.net/wiki/Skyrim_Mod:ChangeFlags#Initial_type
    // Note: assumes ACHR change form type
    let initial_type: u32 = {
        if matches!(player_change_form.form_id, FormIdType::Created(_)) {
            5
            // CHANGE_REFR_PROMOTED or CHANGE_REFR_CELL_CHANGED flags
        } else if player_change_form.change_flags & 0x02000000 != 0
            || player_change_form.change_flags & 0x00000008 != 0
        {
            6
            // CHANGE_REFR_HAVOK_MOVE or CHANGE_REFR_MOVE flags
        } else if player_change_form.change_flags & 0x00000004 != 0
            || player_change_form.change_flags & 0x00000002 != 0
        {
            4
        } else {
            0
        }
    };
    let initial_type_size: u32 = match initial_type {
        0 => 0,
        1 => 8,
        2 => 10,
        3 => 4,
        4 => 27,
        5 => 31,
        6 => 34,
        other => panic!("unknown initial type {}", other),
    };

    let (remaining_data, _) = nom::sequence::tuple((
        nom::combinator::cond(
            initial_type_size != 0,
            // Skip initial data
            nom::bytes::complete::take::<_, &[u8], nom::error::Error<_>>(initial_type_size),
        ),
        nom::combinator::cond(
            // CHANGE_REFR_HAVOK_MOVE flag
            player_change_form.change_flags & 0x00000004 != 0,
            // Skip havok data
            nom::multi::length_count(read_vsval, nom::number::complete::le_u8),
        ),
        // Skip unknown integer + unknown data
        nom::bytes::complete::take(std::mem::size_of::<u32>() + std::mem::size_of::<u8>() * 4),
        nom::combinator::cond(
            // CHANGE_FORM_FLAGS flag
            player_change_form.change_flags & 0x00000001 != 0,
            // Skip flag + unknown
            nom::bytes::complete::take(std::mem::size_of::<u32>() + std::mem::size_of::<u16>()),
        ),
        nom::combinator::cond(
            // CHANGE_REFR_BASEOBJECT flag
            player_change_form.change_flags & 0x00000080 != 0,
            // Skip base object ref ID
            nom::bytes::complete::take(3usize),
        ),
        nom::combinator::cond(
            // CHANGE_REFR_SCALE flag
            player_change_form.change_flags & 0x00000010 != 0,
            // Skip scale float
            nom::number::complete::le_f32,
        ),
    ))(player_change_form.data.as_ref())
    .map_err(nom_err_to_anyhow_err)?;

    // Now comes the extra data (probably), which we don't have enough information on to skip

    // TODO: scan the remaining changeform data for known refIDs to find the inventory
    // Construct skyrim_savegame::RefId out of 3 consecutive bytes, then convert that to a form ID and see if that is in a map of known ingredient form IDs
    // if it is, parse the next four bytes as an i32 (or u32?), which would indicate the count
    // probably need to use iter().windows() https://doc.rust-lang.org/stable/std/primitive.slice.html#method.windows
    // also see if we can skip the next n bytes if an ingredient is found
    // can do a sanity check on the count to see if that's in a reasonable range too
    // would be cool if we could use rayon, but probably not needed

    // TODO: somehow prevent / filter out false positives in case some random bytes happen to match a known form ID. Perhaps consider index where found and eliminate outliers at start and end? Inventory entries should be fairly close together, though each entry can also have zero or more extra datas (I'm guessing these will be rather small?)

    Ok(())
}

#[derive(Debug)]
enum ChangeFormDataType {
    Actor,
}

/// Returns `Some(ChangeFormDataType)` if it's a data type we care about
fn get_change_form_data_type(change_form: &ChangeForm) -> Option<ChangeFormDataType> {
    // Look at lower 6 bits
    match change_form.data_type & 0x3F {
        1 => Some(ChangeFormDataType::Actor),
        _ => None,
    }
}

fn get_real_form_id(raw_form_id: &FormIdType, save_file: &SaveFile) -> Result<u32, anyhow::Error> {
    match raw_form_id {
        FormIdType::Index(value) => Ok(*save_file
            .form_id_array
            .get(*value as usize)
            .ok_or_else(|| anyhow!("form ID index not in form ID array: {}", value))?),
        FormIdType::Default(value) => Ok(*value),
        FormIdType::Created(value) => Ok(0xFF000000 | *value),
        FormIdType::Unknown(_) => Err(anyhow!("encountered unknown form ID type")),
    }
}

/// Reads a vsval to u32. If it has an invalid size indicator, returns 0
pub fn read_vsval(input: &[u8]) -> IResult<&[u8], u32> {
    let (input, first_byte) = nom::number::complete::le_u8(input)?;
    let val_type_enc = first_byte & 0b00000011;
    match val_type_enc {
        0 => Ok((input, ((first_byte & 0b11111100) >> 2) as u32)),
        1 => {
            let first_byte = first_byte as u16;
            let (input, second_byte) = nom::number::complete::le_u8(input)?;
            Ok((
                input,
                // TODO: XOR, correct?
                (((second_byte as u16) << 8 ^ first_byte) >> 2) as u32,
            ))
        }
        2 => {
            let first_byte = first_byte as u32;
            let (input, second_byte) = nom::number::complete::le_u8(input)?;
            let (input, third_byte) = nom::number::complete::le_u8(input)?;
            Ok((
                input,
                (((third_byte as u32) << 16 ^ (second_byte as u32) << 8 ^ first_byte) >> 2),
            ))
        }
        _ => {
            log::error!("Found invalid vsval!");
            Ok((input, 0))
        }
    }
}
