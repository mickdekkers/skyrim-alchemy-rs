use anyhow::anyhow;
use itertools::Itertools;
use nom::error::ErrorKind;
use serde::{Deserialize, Serialize};

use std::hash::Hash;

use std::num::NonZeroU32;

use nom::number::complete::{le_f32, le_u32};
use nom::sequence::separated_pair;

// use crate::error::Error;
use esplugin::record::Record;

use crate::plugin_parser::utils::{le_slice_to_u32, parse_zstring};

use super::form_id::{FormIdContainer, GlobalFormId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ingredient {
    pub global_form_id: GlobalFormId,
    pub editor_id: String,
    pub name: Option<String>,
    pub effects: Vec<IngredientEffect>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct IngredientEffect {
    pub global_form_id: GlobalFormId,
    pub duration: u32,
    pub magnitude: f32,
}

impl Ingredient {
    pub fn parse<FnGlobalizeFormId, FnParseLstring>(
        record: &Record,
        globalize_form_id: FnGlobalizeFormId,
        parse_lstring: FnParseLstring,
    ) -> Result<Ingredient, anyhow::Error>
    where
        FnGlobalizeFormId: Fn(NonZeroU32) -> Result<GlobalFormId, anyhow::Error>,
        FnParseLstring: Fn(&[u8]) -> String,
    {
        ingredient(record, globalize_form_id, parse_lstring)
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
    fn get_global_form_id(&self) -> crate::plugin_parser::form_id::GlobalFormId {
        self.global_form_id
    }
}

impl Hash for Ingredient {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.get_global_form_id().hash(state);
    }
}

impl PartialEq for Ingredient {
    fn eq(&self, other: &Self) -> bool {
        self.get_global_form_id() == other.get_global_form_id()
    }
}

impl Eq for Ingredient {}

impl FormIdContainer for IngredientEffect {
    fn get_global_form_id(&self) -> super::form_id::GlobalFormId {
        self.global_form_id
    }
}

fn ingredient<FnGlobalizeFormId, FnParseLstring>(
    record: &Record,
    globalize_form_id: FnGlobalizeFormId,
    parse_lstring: FnParseLstring,
) -> Result<Ingredient, anyhow::Error>
where
    FnGlobalizeFormId: Fn(NonZeroU32) -> Result<GlobalFormId, anyhow::Error>,
    FnParseLstring: Fn(&[u8]) -> String,
{
    assert!(&record.header_type() == b"INGR");

    let form_id = record
        .header()
        .form_id()
        .ok_or_else(|| anyhow!("Ingredient record has no form ID: {:#?}", record))?;

    let global_form_id = globalize_form_id(form_id)?;

    let editor_id = record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"EDID")
        .map(|s| parse_zstring(s.data()))
        .ok_or_else(|| anyhow!("Ingredient record is missing editor ID: {}", global_form_id))?;

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
            b"EFID" => current_effect_id = Some(le_slice_to_u32(sr.data())),
            b"EFIT" => {
                if let Some(efid) = current_effect_id {
                    let (magnitude, duration) = separated_pair(le_f32, le_u32, le_u32)(sr.data())
                        .map_err(|err: nom::Err<(_, ErrorKind)>| {
                            anyhow!(
                                "Error parsing effects of ingredient record {}: {}",
                                global_form_id,
                                err.to_string()
                            )
                        })?
                        .1;

                    let global_form_id = globalize_form_id(
                        std::num::NonZeroU32::new(efid).expect("expected EFID to be non-zero"),
                    )?;
                    effects.push(IngredientEffect {
                        global_form_id,
                        duration,
                        magnitude,
                    });
                } else {
                    Err(anyhow!(
                        "Error parsing effects of ingredient record {}: EFIT appeared before EFID",
                        global_form_id
                    ))?
                }
                current_effect_id = None;
            }
            _ => (),
        }
    }

    // Sort to make later usage more optimized
    effects.sort_by_key(|eff| eff.get_global_form_id());

    Ok(Ingredient {
        global_form_id,
        editor_id,
        name: full_name,
        effects,
    })
}
