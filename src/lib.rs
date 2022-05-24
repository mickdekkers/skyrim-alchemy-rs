#![feature(hash_drain_filter)]

use anyhow::{anyhow, Context};
use arrayvec::ArrayVec;
use itertools::Itertools;
use lazy_static::lazy_static;
use potion::Potion;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use std::time::Instant;

use crate::plugin_parser::{
    form_id::FormIdContainer, ingredient::Ingredient, magic_effect::MagicEffect,
};

mod plugin_parser;
mod potion;

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
        let plugin_mmap = unsafe { memmap::MmapOptions::new().map(&plugin_file)? };
        let (plugin_ingredients, plugin_magic_effects) =
            plugin_parser::parse_plugin(&plugin_mmap, plugin_name, &GAME_PLUGINS_PATH)?;

        println!(
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

    println!("Number of ingredients: {}", ingredients.len());
    println!(
        "Number of magic effects before filtering: {}",
        magic_effects.len()
    );
    magic_effects.drain_filter(|key, _| !ingredient_effect_ids.contains(key));
    println!(
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
    let _save_file = gimme_save_file()?;
    // println!("{:#?}", save_file);
    let load_order = gimme_load_order()?;
    // println!("Load order:\n{:#?}", &load_order);
    let (ingredients, magic_effects) = load_ingredients_and_effects_from_plugins(&load_order)?;

    let serialized_ingredients =
        serde_json::to_string_pretty(&ingredients.values().collect_vec()).unwrap();
    let serialized_magic_effects =
        serde_json::to_string_pretty(&magic_effects.values().collect_vec()).unwrap();

    fs::write("data/ingredients.json", serialized_ingredients)?;
    fs::write("data/magic_effects.json", serialized_magic_effects)?;

    // TODO: sort ingredients by name

    let mut test_potion_ingredients = ArrayVec::<&Ingredient, 3>::new();
    // Wheat
    test_potion_ingredients.push(ingredients.get(&("Skyrim.esm".into(), 307386)).unwrap());
    // Giant's Toe
    test_potion_ingredients.push(ingredients.get(&("Skyrim.esm".into(), 240996)).unwrap());

    let test_potion = Potion::from_ingredients(&test_potion_ingredients, &magic_effects);

    println!("Test potion:\n{}", test_potion.unwrap());

    // Note: temporarily storing the combinations and then using par_iter is about twice as fast as
    // using par_bridge directly on the combinations iterator (at the cost of some ram)
    let start = Instant::now();
    let combos_3: Vec<_> = ingredients.values().combinations(3).collect();
    println!(
        "Found {} possible 3-ingredient combos (in {:?})",
        combos_3.len(),
        start.elapsed()
    );

    let start = Instant::now();
    let valid_combos_3: Vec<_> = combos_3
        .par_iter()
        .filter(|combo| {
            let a = combo.get(0).unwrap();
            let b = combo.get(1).unwrap();
            let c = combo.get(2).unwrap();

            // Ensure all three ingredients share an effect with at least one of the others
            (a.shares_effects_with(b) && (c.shares_effects_with(a) || c.shares_effects_with(b)))
                || (a.shares_effects_with(c) && b.shares_effects_with(c))
        })
        .collect();
    println!(
        "Found {} valid 3-ingredient combos (in {:?})",
        valid_combos_3.len(),
        start.elapsed()
    );

    let start = Instant::now();
    let mut potions_3: Vec<_> = valid_combos_3
        .par_iter()
        .map(|combo| {
            let ingredients = ArrayVec::<_, 3>::from_iter(combo.iter().copied());
            Potion::from_ingredients(&ingredients, &magic_effects)
                .expect("ingredients combo should be valid Potion")
        })
        .collect();
    potions_3.sort_by_key(|pot| pot.get_gold_value());
    potions_3.reverse();
    println!(
        "Created {} Potion instances (in {:?})",
        potions_3.len(),
        start.elapsed()
    );

    println!();

    println!(
        "Top 10 potions with 3 ingredients:\n\n{}",
        potions_3.iter().take(10).join("\n\n")
    );

    Ok(())
}
