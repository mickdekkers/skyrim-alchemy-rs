use std::{num::NonZeroU32, path::Path};

use anyhow::anyhow;
use esplugin::record::Record;
use itertools::{Either, Itertools};

use crate::{
    load_order::LoadOrder,
    plugin_parser::{
        form_id::GlobalFormId,
        ingredient::Ingredient,
        magic_effect::MagicEffect,
        strings_table::StringsTable,
        utils::{le_slice_to_u32, parse_lstring, parse_string, parse_zstring},
    },
};

use self::utils::nom_err_to_anyhow_err;

pub(crate) mod form_id;
mod group;
pub(crate) mod ingredient;
pub(crate) mod magic_effect;
mod strings_table;
pub(crate) mod utils;

pub fn parse_plugin<'a>(
    input: &'a [u8],
    plugin_name: &str,
    game_plugins_path: &Path,
    load_order: &mut LoadOrder,
) -> Result<(Vec<Ingredient>, Vec<MagicEffect>), anyhow::Error> {
    log::trace!("Parsing plugin {}", plugin_name);

    let (remaining_input, header_record) =
        Record::parse(input, esplugin::GameId::SkyrimSE, false).map_err(nom_err_to_anyhow_err)?;

    log::trace!("Plugin header_record: {:#?}", header_record);

    const COUNT_OFFSET: usize = 4;
    let record_and_group_count = header_record
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

    let is_light_master = (header_record.header().flags() & 0x200) != 0;
    if is_light_master {
        load_order.plugin_is_esl_flagged(plugin_name);
    }

    log::trace!("Plugin masters: {:#?}", masters);
    log::trace!("Plugin is_localized: {:?}", is_localized);
    log::trace!("Plugin is_light_master: {:?}", is_light_master);

    let strings_table = match is_localized {
        true => StringsTable::new(plugin_name, game_plugins_path),
        false => None,
    };

    // Converts plugin-local form ID to proper (global) form ID
    let globalize_form_id = |form_id: NonZeroU32| -> Result<GlobalFormId, anyhow::Error> {
        // See https://en.uesp.net/wiki/Skyrim:Form_ID
        let mod_id = (u32::from(form_id) >> 24) as usize;
        let num_masters = masters.len();

        #[allow(clippy::comparison_chain)]
        let mod_name = {
            if mod_id == num_masters {
                Ok(String::from(plugin_name))
            } else if mod_id < num_masters {
                Ok(masters[mod_id].clone())
            } else {
                Err(anyhow!(
                    "record has invalid master reference in form ID {:x}",
                    form_id
                ))
            }
        }?;

        // TODO: ESL

        let is_esl = mod_name.ends_with(".esl");

        // The last six hex digits are the ID of the record itself
        let record_id = u32::from(form_id) & 0x00FFFFFF;

        let load_order_index = load_order
            .get_form_id_prefix(&mod_name)
            .ok_or_else(|| anyhow!("plugin {} not found in load order!", &mod_name))?;

        let load_order_index = load_order_index & 0xFF000000;

        let form_id = load_order_index as u32 | record_id;

        Ok(GlobalFormId::new(form_id))
    };

    let parse_lstring =
        |data: &[u8]| -> String { parse_lstring(data, is_localized, &strings_table) };

    log::trace!(
        "Plugin record_and_group_count: {:?}",
        record_and_group_count
    );

    // We're only interested in ingredients and magic effects.
    let skip_group_records = |label| !matches!(&label, b"INGR" | b"MGEF");

    let mut interesting_groups = Vec::new();
    let mut input1 = remaining_input;
    while !input1.is_empty() {
        let (input2, group) =
            group::Group::parse(input1, skip_group_records).map_err(nom_err_to_anyhow_err)?;
        if !group.group_records.is_empty() {
            interesting_groups.push(group);
        }
        input1 = input2;
    }

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
                            // AFAICT ingredient groups don't have subgroups
                            log::warn!("Found unexpected subgroup in INGR group, ignoring");
                            None
                        }
                        group::GroupRecord::Record(rec) => {
                            if &rec.header_type() != b"INGR" {
                                // Unexpected non-ingredient record
                                log::warn!(
                                    "Found unexpected non-INGR record in INGR group, ignoring"
                                );
                                None
                            } else {
                                Some(rec)
                            }
                        }
                    }
                })
                .map(|rec| Ingredient::parse(rec, globalize_form_id, parse_lstring))
                .partition_map(|r| match r {
                    Ok(v) => Either::Left(v),
                    Err(v) => Either::Right(v),
                });

            if !errors.is_empty() {
                log::error!(
                    "Failed to parse {} ingredients records: {:#?}",
                    errors.len(),
                    errors
                );
            }

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
                            // AFAICT magic effect groups don't have subgroups
                            log::warn!("Found unexpected subgroup in MGEF group, ignoring");
                            None
                        }
                        group::GroupRecord::Record(rec) => {
                            if &rec.header_type() != b"MGEF" {
                                // Unexpected non-magic effect record
                                log::warn!(
                                    "Found unexpected non-MGEF record in MGEF group, ignoring"
                                );
                                None
                            } else {
                                Some(rec)
                            }
                        }
                    }
                })
                .map(|rec| MagicEffect::parse(rec, globalize_form_id, parse_lstring))
                .partition_map(|r| match r {
                    Ok(v) => Either::Left(v),
                    Err(v) => Either::Right(v),
                });

            if !errors.is_empty() {
                log::error!(
                    "Failed to parse {} magic effects records: {:#?}",
                    errors.len(),
                    errors
                );
            }

            magic_effects
        } else {
            Vec::new()
        }
    };

    Ok((ingredients, magic_effects))
}
