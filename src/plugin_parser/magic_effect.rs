use anyhow::anyhow;
use nom::error::ErrorKind;
use serde::{Deserialize, Serialize};

use std::num::NonZeroU32;

use nom::number::complete::{le_f32, le_u32};

// use crate::error::Error;
use esplugin::record::Record;

use crate::plugin_parser::utils::parse_zstring;

use super::form_id::{FormIdContainer, GlobalFormId};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MagicEffect {
    pub global_form_id: GlobalFormId,
    pub editor_id: String,
    pub name: Option<String>,
    pub description: String,
    pub flags: u32,
    pub is_hostile: bool,
    pub base_cost: f32,
}

impl MagicEffect {
    pub fn parse<FnGlobalizeFormId, FnParseLstring>(
        record: &Record,
        globalize_form_id: FnGlobalizeFormId,
        parse_lstring: FnParseLstring,
    ) -> Result<MagicEffect, anyhow::Error>
    where
        FnGlobalizeFormId: Fn(NonZeroU32) -> Result<GlobalFormId, anyhow::Error>,
        FnParseLstring: Fn(&[u8]) -> String,
    {
        magic_effect(record, globalize_form_id, parse_lstring)
    }
}

impl FormIdContainer for MagicEffect {
    fn get_global_form_id(&self) -> super::form_id::GlobalFormId {
        self.global_form_id
    }
}

// TODO: only parse magic effects which are actually used by ingredients?

fn magic_effect<FnGlobalizeFormId, FnParseLstring>(
    record: &Record,
    globalize_form_id: FnGlobalizeFormId,
    parse_lstring: FnParseLstring,
) -> Result<MagicEffect, anyhow::Error>
where
    FnGlobalizeFormId: Fn(NonZeroU32) -> Result<GlobalFormId, anyhow::Error>,
    FnParseLstring: Fn(&[u8]) -> String,
{
    assert!(&record.header_type() == b"MGEF");

    let form_id = record
        .header()
        .form_id()
        .ok_or_else(|| anyhow!("Magic effect record has no form ID: {:#?}", record))?;

    let global_form_id = globalize_form_id(form_id)?;

    let editor_id = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"EDID")
        .ok_or_else(|| {
            anyhow!(
                "Magic effect record is missing editor ID: {}",
                global_form_id
            )
        })
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
        .or_else(|| {
            log::warn!(
                "Magic effect record is missing description: {}",
                global_form_id
            );
            None
        })
        .map(|s| parse_lstring(s.data()))
        .unwrap_or_else(|| String::from(""));

    let (flags, base_cost) = {
        // TODO: get rid of double ??
        record
            .subrecords()
            .iter()
            .find(|s| s.subrecord_type() == b"DATA")
            .ok_or_else(|| anyhow!("Magic effect record is missing data: {}", global_form_id))
            .map(|s| {
                nom::sequence::pair(le_u32, le_f32)(s.data())
                    .map(|d| d.1)
                    .map_err(|err: nom::Err<(_, ErrorKind)>| {
                        anyhow!(
                            "Error parsing flags and base cost of magic effect record {}: {}",
                            global_form_id,
                            err.to_string()
                        )
                    })
            })??
    };

    let is_hostile = flags & 0x00000001 == 1;

    Ok(MagicEffect {
        global_form_id,
        editor_id,
        name: full_name,
        base_cost,
        description,
        flags,
        is_hostile,
    })
}
