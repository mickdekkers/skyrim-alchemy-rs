use anyhow::anyhow;
use nom::error::ErrorKind;
use serde::{Deserialize, Serialize};

use std::num::NonZeroU32;

use nom::number::complete::{le_f32, le_u32};

// use crate::error::Error;
use esplugin::record::Record;

use crate::plugin_parser::utils::{parse_zstring, split_form_id};

use super::form_id::FormIdContainer;

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MagicEffect {
    pub form_id: u32,
    pub mod_name: String,
    pub id: u32,
    pub editor_id: String,
    pub name: Option<String>,
    pub description: String,
    pub flags: u32,
    pub is_hostile: bool,
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

impl FormIdContainer for MagicEffect {
    fn get_local_form_id(&self) -> u32 {
        self.form_id
    }

    fn get_global_form_id(&self) -> super::form_id::GlobalFormId {
        super::form_id::GlobalFormId::new(self.mod_name.as_str(), self.id)
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

    let form_id = record
        .header()
        .form_id()
        .ok_or_else(|| anyhow!("Magic effect record has no form ID: {:#?}", record))?;

    let (mod_name, id) = split_form_id(form_id, &get_master)?;

    let editor_id = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"EDID")
        .ok_or_else(|| {
            anyhow!(
                "Magic effect record is missing editor ID: {}:{}",
                mod_name,
                id
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
                "Magic effect record is missing description: {}:{}",
                mod_name,
                editor_id
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
            .ok_or_else(|| {
                anyhow!(
                    "Magic effect record is missing data: {}:{}",
                    mod_name,
                    editor_id
                )
            })
            .map(|s| {
                nom::sequence::pair(le_u32, le_f32)(s.data())
                    .map(|d| d.1)
                    .map_err(|err: nom::Err<(_, ErrorKind)>| {
                        anyhow!(
                            "Error parsing flags and base cost of magic effect record {}:{}: {}",
                            mod_name,
                            editor_id,
                            err.to_string()
                        )
                    })
            })??
    };

    let is_hostile = flags & 0x00000001 == 1;

    Ok(MagicEffect {
        form_id: u32::from(form_id),
        mod_name,
        id,
        editor_id,
        name: full_name,
        base_cost,
        description,
        flags,
        is_hostile,
    })
}
