#![feature(hash_drain_filter, drain_filter)]

use ahash::{AHashMap, AHashSet};
use anyhow::anyhow;
use itertools::Itertools;
use load_order::LoadOrder;
use save_parser::read_saves;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::game_data::GameData;
use crate::plugin_parser::form_id::GlobalFormId;
use crate::plugin_parser::{
    form_id::FormIdContainer, ingredient::Ingredient, magic_effect::MagicEffect,
};
use crate::potions_list::PotionsList;

mod game_data;
mod load_order;
mod plugin_parser;
mod potion;
mod potions_list;
mod save_parser;

fn get_load_order<PGame, PLocal>(
    game_path: PGame,
    local_path: Option<PLocal>,
) -> Result<LoadOrder, anyhow::Error>
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
    Ok(LoadOrder::new(
        active_plugin_names.iter().map(|&s| s.into()).collect(),
    ))
}

fn load_ingredients_and_effects_from_plugins<PGame>(
    game_path: PGame,
    load_order: LoadOrder,
) -> Result<GameData, anyhow::Error>
where
    PGame: AsRef<Path>,
{
    if load_order.is_empty() {
        Err(anyhow!("Load order empty!"))?
    }

    let game_plugins_path = game_path.as_ref().join("Data");

    let mut magic_effects = AHashMap::<GlobalFormId, MagicEffect>::new();
    let mut ingredients = AHashMap::<GlobalFormId, Ingredient>::new();
    let mut ingredient_effect_ids = AHashSet::<GlobalFormId>::new();

    for plugin_name in load_order.iter() {
        let plugin_path = game_plugins_path.join(plugin_name);

        let plugin_file = File::open(&plugin_path)?;
        // TODO: implement better (safer, streaming) file loading
        let plugin_mmap = unsafe { memmap2::MmapOptions::new().map(&plugin_file)? };
        let (plugin_ingredients, plugin_magic_effects) = plugin_parser::parse_plugin(
            &plugin_mmap,
            plugin_name,
            &game_plugins_path,
            &load_order,
        )?;

        log::debug!(
            "Plugin {:?} has {:?} ingredients and {:?} magic effects.",
            plugin_name,
            plugin_ingredients.len(),
            plugin_magic_effects.len()
        );

        for plugin_magic_effect in plugin_magic_effects.into_iter() {
            // Insert into magic effects hashmap, overwriting existing entry from previous plugins
            magic_effects.insert(
                plugin_magic_effect.get_global_form_id(),
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
                ingredient_effect_ids.insert(plugin_ingredient_effect_id);
            }

            // Insert into magic effects hashmap, overwriting existing entry from previous plugins
            ingredients.insert(plugin_ingredient.get_global_form_id(), plugin_ingredient);
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

    let mut game_data = GameData::from_hashmaps(load_order, ingredients, magic_effects);
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
    log::debug!("Load order:\n{}", &load_order);

    let game_data = load_ingredients_and_effects_from_plugins(&game_path, load_order)?;
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

pub fn suggest_potions<PImport, PSaves>(
    import_path: PImport,
    saves_path: Option<PSaves>,
    ingredients_blacklist: &AHashSet<String>,
    ingredients_whitelist: &AHashSet<String>,
    limit: usize,
) -> Result<(), anyhow::Error>
where
    PImport: AsRef<Path>,
    PSaves: AsRef<Path>,
{
    let game_data = import_game_data(import_path)?;

    let _foo = read_saves(saves_path, &game_data)?;

    let mut potions_list = PotionsList::new(&game_data);
    potions_list.build_potions();

    if !ingredients_blacklist.is_empty() {
        log::debug!(
            "Applying ingredients blacklist: {}",
            ingredients_blacklist.iter().sorted().join(", ")
        );
    } else if !ingredients_whitelist.is_empty() {
        log::debug!(
            "Applying ingredients whitelist: {}",
            ingredients_whitelist.iter().sorted().join(", ")
        );
    }

    potions_list
        .get_potions()
        .filter(|p| {
            // If there's a whitelist, all the potion's ingredients must be in it.
            ingredients_whitelist.is_empty()
                || p.ingredients.iter().all(|ing| match ing.name.as_deref() {
                    None => false,
                    Some(name) => ingredients_whitelist.contains(name),
                })
        })
        .filter(|p| {
            // If there's a blacklist, none of the potion's ingredients must be in it.
            ingredients_blacklist.is_empty()
                || !p.ingredients.iter().any(|ing| match ing.name.as_deref() {
                    None => false,
                    Some(name) => ingredients_blacklist.contains(name),
                })
        })
        .take(limit)
        .for_each(|p| println!("{}\n", p));

    Ok(())
}
