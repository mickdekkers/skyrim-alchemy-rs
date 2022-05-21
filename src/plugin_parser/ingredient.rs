use anyhow::anyhow;
use std::io::{BufRead, Seek};
use std::num::NonZeroU32;

use encoding_rs::WINDOWS_1252;
use nom::bytes::complete::{tag, take};
use nom::combinator::{all_consuming, map, peek};
use nom::multi::length_data;
use nom::number::complete::le_u32;
use nom::sequence::{delimited, tuple};
use nom::IResult;

// use crate::error::Error;
use esplugin::record::Record;
use esplugin::record_id::RecordId;
use esplugin::GameId;

#[derive(Clone, PartialEq, Eq, Debug, Hash, Default)]
pub struct Ingredient<'a> {
    pub id: u32,
    pub mod_name: Option<&'a str>,
    pub name: Option<String>,
    // pub effects: Vec<IngredientEffect>,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash, Default)]
pub struct IngredientEffect {
    pub id: u32,
    pub duration: u32,
    pub magnitude: u32,
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
    let mod_name = get_master(id);
    // The first remaining six hex digits are the ID of the record itself
    let id = u32::from(id) & 0x00FFFFFF;

    let full_name = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"FULL")
        .map(|s| parse_lstring(s.data()));

    Ok(Ingredient {
        id,
        mod_name,
        name: full_name,
    })
}
