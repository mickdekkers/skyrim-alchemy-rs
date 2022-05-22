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

use crate::plugin_parser::utils::{le_slice_to_u32, parse_zstring, split_form_id};

#[derive(Clone, PartialEq, Debug, Default)]
pub struct MagicEffect {
    pub mod_name: String,
    pub id: u32,
    pub editor_id: String,
    pub name: Option<String>,
    pub description: String,
    pub flags: u32,
    pub base_cost: f32,
}

impl MagicEffect {
    pub fn parse<FnGetMaster, FnParseLstring>(
        record: &Record,
        get_master: FnGetMaster,
        parse_lstring: FnParseLstring,
    ) -> Result<MagicEffect, anyhow::Error>
    where
        FnGetMaster: Fn(NonZeroU32) -> Option<String>,
        FnParseLstring: Fn(&[u8]) -> String,
    {
        magic_effect(record, get_master, parse_lstring)
    }
}

// TODO: only parse magic effects which are actually used by ingredients?

fn magic_effect<FnGetMaster, FnParseLstring>(
    record: &Record,
    get_master: FnGetMaster,
    parse_lstring: FnParseLstring,
) -> Result<MagicEffect, anyhow::Error>
where
    FnGetMaster: Fn(NonZeroU32) -> Option<String>,
    FnParseLstring: Fn(&[u8]) -> String,
{
    assert!(&record.header_type() == b"MGEF");

    let id = record
        .header()
        .form_id()
        .ok_or_else(|| anyhow!("Magic effect record has no form ID"))?;

    let (mod_name, id) = split_form_id(id, &get_master)?;

    let editor_id = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"EDID")
        .ok_or_else(|| anyhow!("Record is missing editor ID"))
        .map(|s| parse_zstring(s.data()))?;

    let full_name = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"FULL")
        .map(|s| parse_lstring(s.data()));

    let description = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"DNAM")
        .ok_or_else(|| anyhow!("Record is missing description"))
        .map(|s| parse_lstring(s.data()))?;

    let (flags, base_cost) = {
        // TODO: get rid of double ??
        record
            .subrecords()
            .iter()
            .find(|s| s.subrecord_type() == b"DATA")
            .ok_or_else(|| anyhow!("Record is missing data"))
            .map(|s| {
                nom::sequence::pair(le_u32, le_f32)(s.data())
                    .map(|d| d.1)
                    .map_err(|err: nom::Err<(_, ErrorKind)>| {
                        anyhow!("error parsing ingredient effects: {}", err.to_string())
                    })
            })??
    };

    Ok(MagicEffect {
        mod_name,
        id: u32::from(id),
        editor_id,
        name: full_name,
        base_cost,
        description,
        flags,
    })
}
