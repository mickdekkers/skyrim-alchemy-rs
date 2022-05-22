use anyhow::anyhow;
use nom::error::ErrorKind;
use std::io::{BufRead, Seek};
use std::num::NonZeroU32;

use encoding_rs::WINDOWS_1252;
use nom::bytes::complete::{tag, take};
use nom::combinator::{all_consuming, map, peek};
use nom::multi::length_data;
use nom::number::complete::{le_f32, le_u32};
use nom::sequence::{delimited, separated_pair, tuple};
use nom::IResult;

// use crate::error::Error;
use esplugin::record::Record;
use esplugin::record_id::RecordId;
use esplugin::GameId;

use crate::plugin_parser::utils::{le_slice_to_u32, parse_zstring};

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Ingredient<'a> {
    pub id: u32,
    pub editor_id: String,
    pub mod_name: &'a str,
    pub name: Option<String>,
    pub effects: Vec<IngredientEffect>,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct IngredientEffect {
    pub id: u32,
    pub duration: u32,
    pub magnitude: f32,
}

impl<'a> Ingredient<'a> {
    pub fn parse<FnGetMaster, FnParseLstring>(
        record: &Record,
        get_master: FnGetMaster,
        parse_lstring: FnParseLstring,
    ) -> Result<Ingredient<'a>, anyhow::Error>
    where
        FnGetMaster: Fn(NonZeroU32) -> Option<&'a str>,
        FnParseLstring: Fn(&[u8]) -> String,
    {
        ingredient(record, get_master, parse_lstring)
    }
}

fn ingredient<'a, FnGetMaster, FnParseLstring>(
    record: &Record,
    get_master: FnGetMaster,
    parse_lstring: FnParseLstring,
) -> Result<Ingredient<'a>, anyhow::Error>
where
    FnGetMaster: Fn(NonZeroU32) -> Option<&'a str>,
    FnParseLstring: Fn(&[u8]) -> String,
{
    assert!(&record.header_type() == b"INGR");

    let id = record
        .header()
        .form_id()
        .ok_or_else(|| anyhow!("Ingredient record has no form ID"))?;

    // See https://en.uesp.net/wiki/Skyrim:Form_ID
    let mod_name = get_master(id)
        .ok_or_else(|| anyhow!("Ingredient record has invalid master reference in form ID"))?;
    // The first remaining six hex digits are the ID of the record itself
    let id = u32::from(id) & 0x00FFFFFF;

    let editor_id = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"EDID")
        .map(|s| parse_zstring(s.data()))
        .ok_or_else(|| anyhow!("Record is missing editor ID"))?;

    let full_name = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"FULL")
        .map(|s| parse_lstring(s.data()));

    // TODO: cap to 4
    let mut effects = Vec::new();
    let mut current_effect_id = None;
    for sr in record
        .subrecords()
        .iter()
        // ENIT is a required field that appears just before the effects we care about
        .skip_while(|sr| sr.subrecord_type() != b"ENIT")
        .skip(1)
    {
        match sr.subrecord_type() {
            // TODO: do we need/want to get_master for this form ID?
            b"EFID" => current_effect_id = Some(le_slice_to_u32(sr.data())),
            b"EFIT" => {
                if let Some(efid) = current_effect_id {
                    let (magnitude, duration) = separated_pair(le_f32, le_u32, le_u32)(sr.data())
                        .map_err(|err: nom::Err<(_, ErrorKind)>| {
                            anyhow!("error parsing ingredient effects: {}", err.to_string())
                        })?
                        .1;
                    effects.push(IngredientEffect {
                        id: efid,
                        duration,
                        magnitude,
                    });
                } else {
                    Err(anyhow!("EFIT appeared before EFID"))?
                }
                current_effect_id = None;
            }
            _ => (),
        }
    }

    // TODO: when merging ingredients lists from multiple plugins, do this https://github.com/cguebert/SkyrimAlchemyHelper/blob/7904e2bcfe5d6561652928bd815213a1e0ba95e8/libs/modParser/ConfigParser.cpp#L118

    Ok(Ingredient {
        id,
        editor_id,
        mod_name,
        name: full_name,
        effects,
    })
}
