#![feature(hash_drain_filter)]

use anyhow::{anyhow, Context};
use lazy_static::lazy_static;
use log_err::{LogErrOption, LogErrResult};
use skyrim_savegame::{ChangeForm, FormIdType, SaveFile};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::path::PathBuf;

use crate::plugin_parser::{
    form_id::FormIdContainer, ingredient::Ingredient, magic_effect::MagicEffect,
};
use crate::potions_list::PotionsList;

mod plugin_parser;
mod potion;
mod potions_list;

lazy_static! {
    static ref GAME_PATH: PathBuf =
        PathBuf::from(&"H:/SteamLibrary/steamapps/common/Skyrim Special Edition");
    static ref GAME_PLUGINS_PATH: PathBuf = GAME_PATH.join(&"Data");
}

fn gimme_load_order() -> Result<Vec<String>, anyhow::Error> {
    let game_settings = loadorder::GameSettings::new(loadorder::GameId::SkyrimSE, &GAME_PATH)?;
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

fn gimme_save_file() -> Result<skyrim_savegame::SaveFile, anyhow::Error> {
    let file_data = fs::read(
        "data/Save67_9C94C7CA_0_416D656C6961_RiftenBeeandBarb_001538_20220520233103_8_1.ess",
    )
    .context("Failed to open save file")?;
    // TODO: this may panic. Catch somehow?
    Ok(skyrim_savegame::parse_save_file(file_data))
}

fn load_ingredients_and_effects_from_plugins(
    load_order: &Vec<String>,
) -> Result<
    (
        HashMap<(String, u32), Ingredient>,
        HashMap<(String, u32), MagicEffect>,
    ),
    anyhow::Error,
> {
    if load_order.is_empty() {
        Err(anyhow!("Load order empty!"))?
    }

    // TODO: use &str instead of String for keys
    let mut magic_effects = HashMap::<(String, u32), MagicEffect>::new();
    let mut ingredients = HashMap::<(String, u32), Ingredient>::new();
    let mut ingredient_effect_ids = HashSet::<(String, u32)>::new();

    for plugin_name in load_order.iter() {
        let plugin_path = GAME_PLUGINS_PATH.join(plugin_name);

        let plugin_file = File::open(&plugin_path)?;
        // TODO: implement better (safer, streaming) file loading
        let plugin_mmap = unsafe { memmap2::MmapOptions::new().map(&plugin_file)? };
        let (plugin_ingredients, plugin_magic_effects) =
            plugin_parser::parse_plugin(&plugin_mmap, plugin_name, &GAME_PLUGINS_PATH)?;

        log::debug!(
            "Plugin {:?} has {:?} ingredients and {:?} magic effects.",
            plugin_name,
            plugin_ingredients.len(),
            plugin_magic_effects.len()
        );

        for plugin_magic_effect in plugin_magic_effects.into_iter() {
            // Insert into magic effects hashmap, overwriting existing entry from previous plugins
            magic_effects.insert(plugin_magic_effect.get_form_id_pair(), plugin_magic_effect);
        }

        for plugin_ingredient in plugin_ingredients.into_iter() {
            // Add ingredient effect IDs to set of known used effects
            for plugin_ingredient_effect_id in plugin_ingredient
                .effects
                .iter()
                .map(|eff| eff.get_form_id_pair())
            {
                ingredient_effect_ids.insert(plugin_ingredient_effect_id);
            }

            // Insert into magic effects hashmap, overwriting existing entry from previous plugins
            ingredients.insert(plugin_ingredient.get_form_id_pair(), plugin_ingredient);
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

    // TODO: find way to avoid clone here (can't difference `&HashSet<(String, u32)>` and `&HashSet<&(String, u32)>)` because they're different types)
    let mgef_keys = magic_effects
        .keys()
        .cloned()
        .collect::<HashSet<(String, u32)>>();

    // TODO: if missing mgefs, identify which ingredients
    let num_missing_mgefs = ingredient_effect_ids.difference(&mgef_keys).count();
    assert!(num_missing_mgefs == 0);

    Ok((ingredients, magic_effects))
}

pub fn do_the_thing() -> Result<(), anyhow::Error> {
    let load_order = gimme_load_order()?;
    log::debug!("Load order:\n{:#?}", &load_order);
    let (ingredients, magic_effects) = load_ingredients_and_effects_from_plugins(&load_order)?;

    // let serialized_ingredients =
    //     serde_json::to_string_pretty(&ingredients.values().collect_vec()).unwrap();
    // let serialized_magic_effects =
    //     serde_json::to_string_pretty(&magic_effects.values().collect_vec()).unwrap();

    // fs::write("data/ingredients.json", serialized_ingredients)?;
    // fs::write("data/magic_effects.json", serialized_magic_effects)?;

    // let potions_list = PotionsList::build(ingredients, magic_effects);

    // potions_list
    //     .get_potions()
    //     .filter(|p| {
    //         p.ingredients.iter().all(|ig| {
    //             matches!(
    //                 ig.name.as_deref(),
    //                 Some("Lavender") | Some("Hanging Moss") | Some("Blue Mountain Flower")
    //             )
    //         })
    //     })
    //     .take(100)
    //     .for_each(|p| log::info!("{}\n", p));

    // TODO: filter PotionsList to include only ingredients that the player has

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
