use anyhow::anyhow;
use itertools::Itertools;
use nom::error::ErrorKind;
use serde::Serialize;

use std::hash::Hash;

use std::num::NonZeroU32;

use nom::number::complete::{le_f32, le_u32};
use nom::sequence::separated_pair;

// use crate::error::Error;
use esplugin::record::Record;

use crate::plugin_parser::utils::{le_slice_to_u32, parse_zstring, split_form_id};

use super::form_id::FormIdContainer;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Ingredient {
    pub mod_name: String,
    pub id: u32,
    pub editor_id: String,
    pub name: Option<String>,
    pub effects: Vec<IngredientEffect>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize)]
pub struct IngredientEffect {
    pub mod_name: String,
    pub id: u32,
    pub duration: u32,
    pub magnitude: f32,
}

impl Ingredient {
    pub fn parse<FnGetMaster, FnParseLstring>(
        record: &Record,
        get_master: FnGetMaster,
        parse_lstring: FnParseLstring,
    ) -> Result<Ingredient, anyhow::Error>
    where
        FnGetMaster: Fn(NonZeroU32) -> Option<String>,
        FnParseLstring: Fn(&[u8]) -> String,
    {
        ingredient(record, get_master, parse_lstring)
    }

    /// Returns whether the ingredient shares any effects with another ingredient (and thus can be combined)
    pub fn shares_effects_with(&self, other: &Ingredient) -> bool {
        // Note: effects vecs are sorted and (essentially) limited to 4 elements, so this shouldn't be too slow
        self.effects
            .iter()
            .any(|self_effect| other.effects.iter().contains(self_effect))
    }
}

impl FormIdContainer for Ingredient {
    fn get_form_id_pair(&self) -> super::form_id::FormIdPair {
        (self.mod_name.clone(), self.id)
    }

    fn get_form_id_pair_ref(&self) -> super::form_id::FormIdPairRef {
        (self.mod_name.as_str(), self.id)
    }
}

impl Hash for Ingredient {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // mod_name + id are enough to tell ingredients apart
        self.mod_name.hash(state);
        self.id.hash(state);
    }
}

impl PartialEq for Ingredient {
    fn eq(&self, other: &Self) -> bool {
        self.mod_name == other.mod_name && self.id == other.id
    }
}

impl Eq for Ingredient {}

impl FormIdContainer for IngredientEffect {
    fn get_form_id_pair(&self) -> super::form_id::FormIdPair {
        (self.mod_name.clone(), self.id)
    }

    fn get_form_id_pair_ref(&self) -> super::form_id::FormIdPairRef {
        (self.mod_name.as_str(), self.id)
    }
}

fn ingredient<'a, FnGetMaster, FnParseLstring>(
    record: &Record,
    get_master: FnGetMaster,
    parse_lstring: FnParseLstring,
) -> Result<Ingredient, anyhow::Error>
where
    FnGetMaster: Fn(NonZeroU32) -> Option<String>,
    FnParseLstring: Fn(&[u8]) -> String,
{
    assert!(&record.header_type() == b"INGR");

    let id = record
        .header()
        .form_id()
        .ok_or_else(|| anyhow!("Ingredient record has no form ID"))?;

    let (mod_name, id) = split_form_id(id, &get_master)?;

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

                    let (mod_name, efid) =
                        split_form_id(std::num::NonZeroU32::new(efid).unwrap(), &get_master)?;
                    effects.push(IngredientEffect {
                        mod_name,
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

    // Sort to make later usage more optimized
    effects.sort_by_key(|eff| eff.get_form_id_pair());

    Ok(Ingredient {
        mod_name,
        id,
        editor_id,
        name: full_name,
        effects,
    })
}
