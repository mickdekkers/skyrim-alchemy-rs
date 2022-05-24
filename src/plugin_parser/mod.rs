// TODO: should return easily mergeable/updatable struct of all ingredients and magic effects. See https://github.com/cguebert/SkyrimAlchemyHelper/tree/master/libs/modParser
// See https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format

use std::{num::NonZeroU32, path::Path};

use esplugin::record::Record;
use itertools::{Either, Itertools};

use crate::plugin_parser::{
    ingredient::Ingredient,
    magic_effect::MagicEffect,
    strings_table::StringsTable,
    utils::{le_slice_to_u32, parse_lstring, parse_string, parse_zstring},
};

use self::utils::nom_err_to_anyhow_err;

pub(crate) mod form_id;
mod group;
pub(crate) mod ingredient;
pub(crate) mod magic_effect;
mod strings_table;
mod utils;

pub fn parse_plugin<'a>(
    input: &'a [u8],
    plugin_name: &str,
    game_plugins_path: &Path,
) -> Result<(Vec<Ingredient>, Vec<MagicEffect>), anyhow::Error> {
    let (remaining_input, header_record) =
        Record::parse(&input, esplugin::GameId::SkyrimSE, false).map_err(nom_err_to_anyhow_err)?;

    // println!("header_record: {:#?}", header_record);

    const COUNT_OFFSET: usize = 4;
    let _record_and_group_count = header_record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"HEDR" && s.data().len() > COUNT_OFFSET)
        .map(|s| le_slice_to_u32(&s.data()[COUNT_OFFSET..]));

    let masters: Vec<String> = header_record
        .subrecords()
        .iter()
        .filter_map(|s| match s.subrecord_type() == b"MAST" {
            true => Some(parse_zstring(s.data())),
            false => None,
        })
        .collect();

    let is_localized = (header_record.header().flags() & 0x80) != 0;

    println!("plugin name: {:?}", plugin_name);
    // println!("masters: {:#?}", masters);
    println!("is_localized: {:?}", is_localized);

    let strings_table = match is_localized {
        true => StringsTable::new(plugin_name, game_plugins_path),
        false => None,
    };

    let get_master = |form_id: NonZeroU32| -> Option<String> {
        // See https://en.uesp.net/wiki/Skyrim:Form_ID
        let mod_id = (u32::from(form_id) >> 24) as usize;
        let num_masters = masters.len();
        if mod_id == num_masters {
            Some(String::from(plugin_name))
        } else if mod_id < num_masters {
            Some(masters[mod_id].clone())
        } else {
            // TODO: add logging
            None
        }
    };

    let parse_lstring =
        |data: &[u8]| -> String { parse_lstring(data, is_localized, &strings_table) };

    // println!("record_and_group_count: {:#?}", record_and_group_count);
    // let (input2, record_ids) = parse_record_ids(input1, game_id, &header_record, filename)?;

    let skip_group_records = |label| match &label {
        // We're only interested in ingredients and magic effects.
        b"INGR" | b"MGEF" => false,
        _ => true,
    };

    let mut interesting_groups = Vec::new();
    let mut input1 = remaining_input;
    while !input1.is_empty() {
        let (input2, group) =
            group::Group::parse(input1, skip_group_records).map_err(nom_err_to_anyhow_err)?;
        if group.group_records.len() > 0 {
            interesting_groups.push(group);
        }
        input1 = input2;
    }

    // println!("interesting_groups: {:#?}", interesting_groups);
    // println!("interesting_groups length: {:#?}", interesting_groups.len());

    interesting_groups.iter().for_each(|ig| {
        let _label = parse_string(&ig.header.label);
        let _num_records = ig.group_records.len();
        // println!("Group {:?} has {:?} records.", label, num_records);
    });

    // Note: we are assuming there is at most one group per group type in each plugin
    let ingredient_group = interesting_groups
        .iter()
        .find(|ig| &ig.header.label == b"INGR");

    // TODO: if all records failed to parse, that's probably a problem

    let ingredients = {
        if let Some(ig) = ingredient_group {
            let (ingredients, errors): (Vec<_>, Vec<_>) = ig
                .group_records
                .iter()
                .filter_map(|rec| {
                    match rec {
                        group::GroupRecord::Group(_) => {
                            // TODO: add logging
                            // Unexpected subgroup, AFAICT ingredient groups don't have subgroups
                            None
                        }
                        group::GroupRecord::Record(rec) => {
                            if &rec.header_type() != b"INGR" {
                                // TODO: add logging
                                // Unexpected non-ingredient record
                                None
                            } else {
                                Some(rec)
                            }
                        }
                    }
                })
                .map(|rec| Ingredient::parse(rec, get_master, parse_lstring))
                .partition_map(|r| match r {
                    Ok(v) => Either::Left(v),
                    Err(v) => Either::Right(v),
                });

            if errors.len() > 0 {
                println!(
                    "Failed to parse {} ingredients records: {:#?}",
                    errors.len(),
                    errors
                );
            }
            // println!("Ingredients: {:#?}", ingredients);
            ingredients
        } else {
            Vec::new()
        }
    };

    let magic_effects_group = interesting_groups
        .iter()
        .find(|ig| &ig.header.label == b"MGEF");

    let magic_effects = {
        if let Some(ig) = magic_effects_group {
            let (magic_effects, errors): (Vec<_>, Vec<_>) = ig
                .group_records
                .iter()
                .filter_map(|rec| {
                    match rec {
                        group::GroupRecord::Group(_) => {
                            // TODO: add logging
                            // Unexpected subgroup, AFAICT magic effect groups don't have subgroups
                            None
                        }
                        group::GroupRecord::Record(rec) => {
                            if &rec.header_type() != b"MGEF" {
                                // TODO: add logging
                                // Unexpected non-magic effect record
                                None
                            } else {
                                Some(rec)
                            }
                        }
                    }
                })
                .map(|rec| MagicEffect::parse(rec, get_master, parse_lstring))
                .partition_map(|r| match r {
                    Ok(v) => Either::Left(v),
                    Err(v) => Either::Right(v),
                });

            if errors.len() > 0 {
                println!(
                    "Failed to parse {} magic effects records: {:#?}",
                    errors.len(),
                    errors
                );
            }
            // println!("Magic effects: {:#?}", magic_effects);
            magic_effects
        } else {
            Vec::new()
        }
    };

    // TODO: convert to more useful representation

    // println!(
    //     "first group label: {:#?}",
    //     record_type_to_string(&first_group.header.label)
    // );
    // first_group.header.label
    // Ok((
    //     input2,
    //     PluginData {
    //         header_record,
    //         record_ids,
    //     },
    // ))
    Ok((ingredients, magic_effects))
}
