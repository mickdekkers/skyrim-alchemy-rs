use std::{cmp::max, collections::HashMap};

use arrayvec::ArrayVec;
use itertools::Itertools;
use once_cell::sync::OnceCell;

use crate::plugin_parser::{
    form_id::FormIdContainer,
    ingredient::{Ingredient, IngredientEffect},
    magic_effect::MagicEffect,
};

/// Minimum number of ingredients per potion
const MIN_INGREDIENTS: usize = 2;

/// Maximum number of ingredients per potion
const MAX_INGREDIENTS: usize = 3;

// TODO: read player alchemy skill and game settings to get real values (still excluding perks because mods)
const EFFECT_POWER_FACTOR: f32 = 6.0;

/// This is basically an `IngredientEffect` with some extra data + a ref to its `MagicEffect`
struct PotionEffect<'a> {
    pub effect: &'a MagicEffect,
    base_magnitude: f32,
    base_duration: u32,
    magnitude: OnceCell<u32>,
    duration: OnceCell<u32>,
    gold_value: OnceCell<u32>,
}

// TODO: use enums for all the various flags

impl<'a> PotionEffect<'a> {
    pub fn from_ingredient_effect(
        igef: &IngredientEffect,
        all_magic_effects: &'a HashMap<(String, u32), MagicEffect>,
    ) -> Self {
        PotionEffect {
            base_duration: igef.duration,
            base_magnitude: igef.magnitude,
            effect: all_magic_effects.get(&igef.get_form_id_pair()).unwrap(),
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
                if self.effect.flags & 0x00000400 != 0 {
                    0.0
                } else {
                    self.base_magnitude
                }
            };

            let magnitude_factor = {
                // "Power affects magnitude" flag
                if self.effect.flags & 0x00200000 != 0 {
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
                if self.effect.flags & 0x00000200 != 0 {
                    0.0
                } else {
                    self.base_duration as f32
                }
            };

            let duration_factor = {
                // "Power affects duration" flag
                if self.effect.flags & 0x00400000 != 0 {
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

            let gold_value =
                (self.effect.base_cost * (magnitude_factor * duration_factor).powf(1.1)) as u32;
            gold_value
        })
    }
}

impl<'a> FormIdContainer for PotionEffect<'a> {
    fn get_form_id_pair(&self) -> crate::plugin_parser::form_id::FormIdPair {
        (self.effect.mod_name.clone(), self.effect.id)
    }

    fn get_form_id_pair_ref(&self) -> crate::plugin_parser::form_id::FormIdPairRef<'a> {
        (self.effect.mod_name.as_str(), self.effect.id)
    }
}

struct Potion<'a> {
    pub ingredients: Vec<&'a Ingredient>,
    /// Potion's effects sorted by strength descending
    pub effects: Vec<PotionEffect<'a>>,
    gold_value: OnceCell<u32>,
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

impl<'a> Potion<'a> {
    pub fn get_gold_value(&self) -> u32 {
        *self.gold_value.get_or_init(|| {
            // See https://en.uesp.net/wiki/Skyrim:Alchemy_Effects#Multiple-Effect_Potions
            self.effects.iter().map(|eff| eff.get_gold_value()).sum()
        })
    }

    pub fn from_ingredients(
        ingredients: &'a ArrayVec<Ingredient, MAX_INGREDIENTS>,
        all_magic_effects: &'a HashMap<(String, u32), MagicEffect>,
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
                .duplicates_by(|igef| igef.get_form_id_pair_ref())
                .count()
                > 0
        }) {
            return Err(PotionCraftError::InvalidIngredient(ing_with_dup_effects));
        }

        // TODO: somehow provide feedback about which ingredients were used? Since you can brew perfectly valid potions with 3 ingredients where only 2 have a shared effect and the third does not contribute

        let ingredients_effects = ingredients
            .iter()
            .flat_map(|ig| ig.effects.iter())
            .sorted_by_key(|igef| (&igef.mod_name, igef.id))
            .collect_vec();

        assert!(ingredients_effects.len() == ingredients.len() * 4);

        let ingredients_effects_counts = ingredients_effects
            .iter()
            .counts_by(|igef| (&igef.mod_name, igef.id));

        if ingredients_effects_counts.values().all(|count| *count < 2) {
            return Err(PotionCraftError::NoSharedEffects);
        }

        // active effects are those that appear in more than one ingredient
        let active_effects = ingredients_effects
            .iter()
            .filter(|igef| {
                *(ingredients_effects_counts
                    .get(&(&igef.mod_name, igef.id))
                    .unwrap())
                    > 1
            })
            .map(|igef| PotionEffect::from_ingredient_effect(igef, all_magic_effects))
            .coalesce(|potef1, potef2| {
                if potef1.get_form_id_pair_ref() == potef2.get_form_id_pair_ref() {
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
            ingredients: ingredients.iter().collect_vec(),
            gold_value: OnceCell::new(),
        })
    }

    pub fn get_potion_type(&self) -> PotionType {}
}
