use std::{cmp::max, collections::HashMap, fmt::Display};

use arrayvec::ArrayVec;
use itertools::Itertools;
use once_cell::sync::OnceCell;

use crate::{
    game_data::GameData,
    plugin_parser::{
        form_id::{FormIdContainer, GlobalFormId},
        ingredient::{Ingredient, IngredientEffect},
        magic_effect::MagicEffect,
    },
};
use serde::{ser::SerializeSeq, Serialize, Serializer};

/// Minimum number of ingredients per potion
const MIN_INGREDIENTS: usize = 2;

/// Maximum number of ingredients per potion
const MAX_INGREDIENTS: usize = 3;

// TODO: read player alchemy skill and game settings to get real values (still excluding perks because mods)
const EFFECT_POWER_FACTOR: f32 = 6.0;

// TODO: make generic over FormIdContainer trait
fn ser_magic_effect_form_id<S>(x: &MagicEffect, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_u32(x.form_id)
}

fn ser_ingredients_vec<S>(x: &Vec<&Ingredient>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = s.serialize_seq(Some(x.len()))?;
    for item in x {
        seq.serialize_element(&item.form_id)?;
    }
    seq.end()
}

fn ser_once_cell_u32<S>(x: &OnceCell<u32>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // TODO: would be much nicer if we could call the getter here.
    s.serialize_u32(
        *x.get()
            .expect("OnceCell must be filled before serialization"),
    )
}

/// This is basically an `IngredientEffect` with some extra data + a ref to its `MagicEffect`
#[derive(Debug, Serialize)]
pub struct PotionEffect<'a> {
    #[serde(serialize_with = "ser_magic_effect_form_id")]
    pub magic_effect: &'a MagicEffect,
    base_magnitude: f32,
    base_duration: u32,
    #[serde(serialize_with = "ser_once_cell_u32")]
    magnitude: OnceCell<u32>,
    #[serde(serialize_with = "ser_once_cell_u32")]
    duration: OnceCell<u32>,
    #[serde(serialize_with = "ser_once_cell_u32")]
    gold_value: OnceCell<u32>,
}

// TODO: use enums for all the various flags

impl<'a> PotionEffect<'a> {
    pub fn from_ingredient_effect(igef: &'a IngredientEffect, game_data: &'a GameData) -> Self {
        PotionEffect {
            base_duration: igef.duration,
            base_magnitude: igef.magnitude,
            magic_effect: game_data
                .get_magic_effect(&igef.get_global_form_id())
                .unwrap(),
            duration: OnceCell::new(),
            magnitude: OnceCell::new(),
            gold_value: OnceCell::new(),
        }
    }

    /// Returns the actual magnitude, taking into account various factors
    ///
    /// Note: this does not currently include every factor so it won't be fully accurate
    pub fn get_magnitude(&self) -> u32 {
        *self.magnitude.get_or_init(|| {
            let magnitude = {
                // "No magnitude" flag
                if self.magic_effect.flags & 0x00000400 != 0 {
                    0.0
                } else {
                    self.base_magnitude
                }
            };

            let magnitude_factor = {
                // "Power affects magnitude" flag
                if self.magic_effect.flags & 0x00200000 != 0 {
                    EFFECT_POWER_FACTOR
                } else {
                    1.0
                }
            };

            f32::round(magnitude * magnitude_factor) as u32
        })
    }

    /// Returns the actual duration, taking into account various factors
    ///
    /// Note: this does not currently include every factor so it won't be fully accurate
    pub fn get_duration(&self) -> u32 {
        *self.duration.get_or_init(|| {
            let duration = {
                // "No duration" flag
                if self.magic_effect.flags & 0x00000200 != 0 {
                    0.0
                } else {
                    self.base_duration as f32
                }
            };

            let duration_factor = {
                // "Power affects duration" flag
                if self.magic_effect.flags & 0x00400000 != 0 {
                    EFFECT_POWER_FACTOR
                } else {
                    1.0
                }
            };

            f32::round(duration * duration_factor) as u32
        })
    }

    /// Returns the gold value of this effect with its magnitude and duration factored in
    pub fn get_gold_value(&self) -> u32 {
        *self.gold_value.get_or_init(|| {
            // See https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/INGR
            // and https://en.uesp.net/wiki/Skyrim:Alchemy_Effects#Strength_Equations
            let magnitude_factor = max(self.get_magnitude(), 1) as f32;
            let duration_factor: f32 = {
                let duration = self.get_duration();
                (match duration {
                    // A duration of 0 is treated as 10
                    0 => 10.0,
                    _ => duration as f32,
                }) / 10.0
            };

            (self.magic_effect.base_cost * (magnitude_factor * duration_factor).powf(1.1)) as u32
        })
    }

    pub fn get_description(&self) -> String {
        self.magic_effect
            .description
            .replace("<mag>", &self.get_magnitude().to_string())
            .replace("<dur>", &self.get_duration().to_string())
    }
}

impl<'a> FormIdContainer for PotionEffect<'a> {
    fn get_local_form_id(&self) -> u32 {
        self.magic_effect.form_id
    }

    fn get_global_form_id(&self) -> crate::plugin_parser::form_id::GlobalFormId {
        crate::plugin_parser::form_id::GlobalFormId::new(
            self.magic_effect.mod_name.as_str(),
            self.magic_effect.id,
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Potion<'a> {
    #[serde(serialize_with = "ser_ingredients_vec")]
    pub ingredients: Vec<&'a Ingredient>,
    /// Potion's effects sorted by strength descending
    pub effects: Vec<PotionEffect<'a>>,
    #[serde(serialize_with = "ser_once_cell_u32")]
    gold_value: OnceCell<u32>,
}

impl<'a> Display for Potion<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\n{}\nValue: {} gold\nIngredients:\n{}",
            self.get_potion_name(),
            self.get_potion_description(),
            self.get_gold_value(),
            self.ingredients
                .iter()
                .map(|ig| String::from("- ")
                    + ig.name.as_deref().unwrap_or("<MISSING_INGREDIENT_NAME>"))
                .join("\n")
        )
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PotionCraftError<'a> {
    #[error("cannot use the same ingredient more than once in a potion")]
    DuplicateIngredient(&'a Ingredient),
    // TODO: since this shouldn't happen with valid game data, panic instead?
    #[error("ingredient has invalid data (duplicate effects)")]
    InvalidIngredient(&'a Ingredient),
    #[error("must supply at least two ingredients")]
    NotEnoughIngredients,
    #[error("none of the ingredients have a shared effect")]
    NoSharedEffects,
}

#[derive(Debug)]
pub enum PotionType {
    Potion,
    Poison,
}

impl Display for PotionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            PotionType::Poison => write!(f, "Poison"),
            PotionType::Potion => write!(f, "Potion"),
        }
    }
}

impl<'a> Potion<'a> {
    pub fn get_gold_value(&self) -> u32 {
        *self.gold_value.get_or_init(|| {
            // See https://en.uesp.net/wiki/Skyrim:Alchemy_Effects#Multiple-Effect_Potions
            self.effects.iter().map(|eff| eff.get_gold_value()).sum()
        })
    }

    pub fn from_ingredients(
        ingredients: &ArrayVec<&'a Ingredient, MAX_INGREDIENTS>,
        game_data: &'a GameData,
    ) -> Result<Self, PotionCraftError<'a>> {
        if ingredients.len() < MIN_INGREDIENTS {
            return Err(PotionCraftError::NotEnoughIngredients);
        }

        if let Some(dup) = ingredients.iter().duplicates().next() {
            return Err(PotionCraftError::DuplicateIngredient(dup));
        }

        if let Some(ing_with_dup_effects) = ingredients.iter().find(|ig| {
            ig.effects
                .iter()
                .duplicates_by(|igef| igef.get_global_form_id())
                .count()
                > 0
        }) {
            return Err(PotionCraftError::InvalidIngredient(ing_with_dup_effects));
        }

        let ingredients_effects = ingredients
            .iter()
            .flat_map(|ig| ig.effects.iter())
            .sorted_by_key(|igef| igef.get_global_form_id())
            .collect_vec();

        // assert_eq!(ingredients_effects.len(), ingredients.len() * 4);

        let ingredients_effects_counts = ingredients_effects
            .iter()
            .counts_by(|igef| igef.get_global_form_id());

        if ingredients_effects_counts.values().all(|count| *count < 2) {
            return Err(PotionCraftError::NoSharedEffects);
        }

        // active effects are those that appear in more than one ingredient
        let active_effects = ingredients_effects
            .iter()
            .filter(|igef| {
                *(ingredients_effects_counts
                    .get(&igef.get_global_form_id())
                    .unwrap())
                    > 1
            })
            .map(|igef| PotionEffect::from_ingredient_effect(igef, game_data))
            .coalesce(|potef1, potef2| {
                if potef1.get_global_form_id() == potef2.get_global_form_id() {
                    // Select most valuable (strongest) version of each effect
                    Ok({
                        if potef1.get_gold_value() >= potef2.get_gold_value() {
                            potef1
                        } else {
                            potef2
                        }
                    })
                } else {
                    Err((potef1, potef2))
                }
            })
            .sorted_by(|potef1, potef2| {
                // Sort by gold value from largest to smallest
                potef1
                    .get_gold_value()
                    .cmp(&potef2.get_gold_value())
                    .reverse()
            })
            .collect_vec();

        Ok(Self {
            effects: active_effects,
            ingredients: ingredients.iter().copied().collect_vec(),
            gold_value: OnceCell::new(),
        })
    }

    pub fn get_primary_effect(&self) -> &PotionEffect<'a> {
        // The effects are sorted by strength descending
        // See https://en.uesp.net/wiki/Skyrim:Alchemy_Effects#Multiple-Effect_Potions
        self.effects.first().unwrap()
    }

    pub fn get_potion_type(&self) -> PotionType {
        match self.get_primary_effect().magic_effect.is_hostile {
            true => PotionType::Poison,
            false => PotionType::Potion,
        }
    }

    pub fn get_potion_name(&self) -> String {
        let type_string = self.get_potion_type().to_string();
        let primary_effect_name = self
            .get_primary_effect()
            .magic_effect
            .name
            .as_deref()
            .unwrap_or("<MISSING_EFFECT_NAME>");
        format!("{} of {}", type_string, primary_effect_name)
    }

    pub fn get_potion_description(&self) -> String {
        self.effects
            .iter()
            .map(|potef| potef.get_description())
            .join(" ")
    }
}
