use itertools::Itertools;
use std::{collections::HashMap, borrow::Borrow};

use crate::plugin_parser::{
    form_id::{FormIdContainer, GlobalFormId},
    ingredient::Ingredient,
    magic_effect::MagicEffect,
};

#[derive(thiserror::Error, Debug)]
#[error("the form ID {} is unknown", .form_id)]
pub struct UnknownFormIdError<'a> {
    pub form_id: GlobalFormId<'a>,
}

// TODO: validate more invalid data conditions
#[derive(thiserror::Error, Debug)]
pub enum IngredientError<'a> {
    #[error("ingredient {} references unknown magic effects: {}", get_ingredient_name_or_fallback(.0), .1.iter().join(", "))]
    ReferencesUnknownMagicEffects(&'a Ingredient, Vec<UnknownFormIdError<'a>>),
}

fn get_ingredient_name_or_fallback(ingredient: &Ingredient) -> &str {
    if let Some(name) = ingredient.name.as_deref() {
        name
    } else {
        &ingredient.editor_id
    }
}

// TODO: consider different (faster) way of getting a hashmap key for a GlobalFormId. Maybe RawEntryBuilder? https://doc.rust-lang.org/stable/std/collections/hash_map/struct.RawEntryBuilder.html
struct ModNamesLookupTable {
    mod_names: Vec<String>,
}

impl ModNamesLookupTable {
    pub fn new(mut mod_names: Vec<String>) -> Self {
        // Sort for binary searches
        mod_names.sort();
        Self { mod_names }
    }

    pub fn get_index(&self, mod_name: &str) -> Option<usize> {
        self.mod_names.binary_search_by(|x| x.as_str().cmp(mod_name)).ok()
    }

    /// Returns `Option<(usize, u32)>`, where the first element is the index of `form_id.plugin` in `mod_names` and the second element is the `form_id.id`
    pub fn index_global_form_id(&self, global_form_id: &GlobalFormId) -> Option<(usize, u32)> {
        self.get_index(global_form_id.plugin.borrow()).map(|index| (index, global_form_id.id))
    }
}

// TODO: serialize, deserialize
pub struct GameData {
    /// Mod/plugin names sorted alphabetically, not by load order
    mod_names: ModNamesLookupTable,
    ingredients: HashMap<(usize, u32), Ingredient>,
    magic_effects: HashMap<(usize, u32), MagicEffect>,
}

impl GameData {
    pub fn from_hashmaps(
        ingredients: HashMap<(String, u32), Ingredient>,
        magic_effects: HashMap<(String, u32), MagicEffect>,
    ) -> Self {
        let mod_names = ingredients.keys().chain(magic_effects.keys()).map(|form_id| &form_id.0).unique().cloned().collect::<Vec<String>>();
        let mod_names = ModNamesLookupTable::new(mod_names);

        let ingredients = ingredients.into_iter().map(|(k, v)| ((mod_names.get_index(k.0.as_str()).unwrap(), k.1), v)).collect();
        let magic_effects = magic_effects.into_iter().map(|(k, v)| ((mod_names.get_index(k.0.as_str()).unwrap(), k.1), v)).collect();

        Self {
            mod_names,
            ingredients,
            magic_effects,
        }
    }

    pub fn from_vecs(ingredients: Vec<Ingredient>, magic_effects: Vec<MagicEffect>) -> Self {
        let mod_names = ingredients.iter().map(|ing| &ing.mod_name).chain(magic_effects.iter().map(|mgef| &mgef.mod_name)).unique().cloned().collect::<Vec<String>>();
        let mod_names = ModNamesLookupTable::new(mod_names);

        let ingredients= ingredients
        .into_iter()
        .map(|ing| (mod_names.index_global_form_id(&ing.get_global_form_id()).unwrap(), ing))
        .collect();
        let magic_effects = magic_effects
        .into_iter()
        .map(|mgef| (mod_names.index_global_form_id(&mgef.get_global_form_id()).unwrap(), mgef))
        .collect();

        Self {
            mod_names,
            ingredients,
            magic_effects,
        }
    }

    pub fn get_ingredients(&self) -> &HashMap<(usize, u32), Ingredient> {
        &self.ingredients
    }

    pub fn get_magic_effects(&self) -> &HashMap<(usize, u32), MagicEffect> {
        &self.magic_effects
    }

    pub fn get_key_for(&self, global_form_id: &GlobalFormId) -> Option<(usize, u32)> {
        self.mod_names.index_global_form_id(global_form_id)
    }

    pub fn get_magic_effect(&self, global_form_id: &GlobalFormId) -> Option<&MagicEffect> {
        self.mod_names.index_global_form_id(global_form_id).and_then(|key| self.magic_effects.get(&key))
    }

    pub fn get_ingredient(&self, global_form_id: &GlobalFormId) -> Option<&Ingredient> {
        self.mod_names.index_global_form_id(global_form_id).and_then(|key| self.ingredients.get(&key))
    }

    pub fn validate(&self) -> Result<(), Vec<IngredientError>> {
        let ings_with_unknown_mgefs = self
            .ingredients
            .values()
            .filter_map(|ing| {
                let unknown_ingefs = ing
                    .effects
                    .iter()
                    .filter_map(|ingef| {
                        if self.magic_effects.contains_key(&self.get_key_for(&ingef.get_global_form_id()).unwrap()) {
                            None
                        } else {
                            Some(UnknownFormIdError {
                                form_id: ingef.get_global_form_id(),
                            })
                        }
                    })
                    .collect::<Vec<_>>();
                if unknown_ingefs.is_empty() {
                    None
                } else {
                    Some(IngredientError::ReferencesUnknownMagicEffects(
                        ing,
                        unknown_ingefs,
                    ))
                }
            })
            .collect::<Vec<_>>();

        if !ings_with_unknown_mgefs.is_empty() {
            return Err(ings_with_unknown_mgefs);
        }

        Ok(())
    }

    // TODO: maybe use pattern where you return different kind of struct to disallow improper usage
    // TODO: avoid double validate when calling purge_invalid after validate
    /// Purges invalid data from the `GameData` struct
    pub fn purge_invalid(&mut self) {
        let ingredients_form_ids_to_remove = {
            let ingredient_errors = match self.validate() {
                Ok(_) => return,
                Err(err) => err,
            };

            log::warn!(
                "Ignoring {} invalid ingredients: {}",
                ingredient_errors.len(),
                ingredient_errors.iter().join("\n")
            );

            ingredient_errors
                .iter()
                .map(|ing_err| match ing_err {
                    IngredientError::ReferencesUnknownMagicEffects(ing, _) => {
                        self.get_key_for(&ing.get_global_form_id()).unwrap()
                    }
                })
                .collect::<Vec<_>>()
        };

        for form_id in ingredients_form_ids_to_remove {
            self.ingredients.remove(&form_id);
        }
    }
}
