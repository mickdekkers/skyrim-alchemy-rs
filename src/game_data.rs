use itertools::Itertools;
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use crate::{
    load_order::LoadOrder,
    plugin_parser::{
        form_id::{FormIdContainer, GlobalFormId},
        ingredient::Ingredient,
        magic_effect::MagicEffect,
    },
};

#[derive(thiserror::Error, Debug)]
#[error("the form ID {} is unknown", .form_id)]
pub struct UnknownFormIdError {
    pub form_id: GlobalFormId,
}

// TODO: validate more invalid data conditions
#[derive(thiserror::Error, Debug)]
pub enum IngredientError<'a> {
    #[error("ingredient {} references unknown magic effects: {}", get_ingredient_name_or_fallback(.0), .1.iter().join(", "))]
    ReferencesUnknownMagicEffects(&'a Ingredient, Vec<UnknownFormIdError>),
}

fn get_ingredient_name_or_fallback(ingredient: &Ingredient) -> &str {
    if let Some(name) = ingredient.name.as_deref() {
        name
    } else {
        &ingredient.editor_id
    }
}

// TODO: when serializing/deserializing game data, keep load order
pub struct GameData {
    load_order: LoadOrder,
    ingredients: HashMap<GlobalFormId, Ingredient>,
    magic_effects: HashMap<GlobalFormId, MagicEffect>,
}

impl Serialize for GameData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut gd = serializer.serialize_struct("GameData", 3)?;
        gd.serialize_field("load_order", &self.load_order.iter().collect::<Vec<_>>())?;
        gd.serialize_field(
            "ingredients",
            &self.ingredients.values().collect::<Vec<_>>(),
        )?;
        gd.serialize_field(
            "magic_effects",
            &self.magic_effects.values().collect::<Vec<_>>(),
        )?;
        gd.end()
    }
}

impl<'de> Deserialize<'de> for GameData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            LoadOrder,
            Ingredients,
            MagicEffects,
        }

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`load_order` or `ingredients` or `magic_effects`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "load_order" => Ok(Field::LoadOrder),
                            "ingredients" => Ok(Field::Ingredients),
                            "magic_effects" => Ok(Field::MagicEffects),
                            _ => Err(de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct GameDataVisitor;

        impl<'de> Visitor<'de> for GameDataVisitor {
            type Value = GameData;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct GameData")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<GameData, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let load_order = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let ingredients = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let magic_effects = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                Ok(GameData::from_vecs(load_order, ingredients, magic_effects))
            }

            fn visit_map<V>(self, mut map: V) -> Result<GameData, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut load_order = None;
                let mut ingredients = None;
                let mut magic_effects = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::LoadOrder => {
                            if load_order.is_some() {
                                return Err(de::Error::duplicate_field("load_order"));
                            }
                            load_order = Some(map.next_value()?);
                        }
                        Field::Ingredients => {
                            if ingredients.is_some() {
                                return Err(de::Error::duplicate_field("ingredients"));
                            }
                            ingredients = Some(map.next_value()?);
                        }
                        Field::MagicEffects => {
                            if magic_effects.is_some() {
                                return Err(de::Error::duplicate_field("magic_effects"));
                            }
                            magic_effects = Some(map.next_value()?);
                        }
                    }
                }
                let load_order =
                    load_order.ok_or_else(|| de::Error::missing_field("load_order"))?;
                let ingredients =
                    ingredients.ok_or_else(|| de::Error::missing_field("ingredients"))?;
                let magic_effects =
                    magic_effects.ok_or_else(|| de::Error::missing_field("magic_effects"))?;
                Ok(GameData::from_vecs(load_order, ingredients, magic_effects))
            }
        }

        const FIELDS: &[&str] = &["load_order", "ingredients", "magic_effects"];
        deserializer.deserialize_struct("GameData", FIELDS, GameDataVisitor)
    }
}

impl GameData {
    pub fn from_hashmaps(
        mut load_order: LoadOrder,
        mut ingredients: HashMap<GlobalFormId, Ingredient>,
        mut magic_effects: HashMap<GlobalFormId, MagicEffect>,
    ) -> Self {
        // Remove unused entries from the load order
        let used_indexes = ingredients
            .keys()
            .chain(magic_effects.keys())
            .map(|k| k.load_order_index);
        let index_remap_data = load_order.drain_unused(used_indexes);

        // Remap load order indexes in ingredient global form IDs
        for ingredient in ingredients.values_mut() {
            let new_index = *index_remap_data
                .get(&ingredient.global_form_id.load_order_index)
                .unwrap();
            ingredient.global_form_id.set_load_order_index(new_index);
        }

        // Create new ingredients hashmap with remapped global form IDs
        let ingredients = ingredients
            .into_iter()
            .map(|(_k, v)| (v.get_global_form_id(), v))
            .collect();

        // Remap load order indexes in magic_effect global form IDs
        for magic_effect in magic_effects.values_mut() {
            let new_index = *index_remap_data
                .get(&magic_effect.global_form_id.load_order_index)
                .unwrap();
            magic_effect.global_form_id.set_load_order_index(new_index);
        }

        // Create new magic_effects hashmap with remapped global form IDs
        let magic_effects = magic_effects
            .into_iter()
            .map(|(_k, v)| (v.get_global_form_id(), v))
            .collect();

        Self {
            load_order,
            ingredients,
            magic_effects,
        }
    }

    pub fn from_vecs(
        load_order: Vec<String>,
        mut ingredients: Vec<Ingredient>,
        mut magic_effects: Vec<MagicEffect>,
    ) -> Self {
        let mut load_order = LoadOrder::new(load_order);

        // Remove unused entries from the load order
        let used_indexes = ingredients
            .iter()
            .map(|x| x.get_global_form_id())
            .chain(magic_effects.iter().map(|x| x.get_global_form_id()))
            .map(|x| x.load_order_index);
        let index_remap_data = load_order.drain_unused(used_indexes);

        // Remap load order indexes in ingredient global form IDs
        for ingredient in ingredients.iter_mut() {
            let new_index = *index_remap_data
                .get(&ingredient.global_form_id.load_order_index)
                .unwrap();
            ingredient.global_form_id.set_load_order_index(new_index);
        }

        // Create new ingredients hashmap with remapped global form IDs
        let ingredients = ingredients
            .into_iter()
            .map(|ing| (ing.get_global_form_id(), ing))
            .collect();

        // Remap load order indexes in magic_effect global form IDs
        for magic_effect in magic_effects.iter_mut() {
            let new_index = *index_remap_data
                .get(&magic_effect.global_form_id.load_order_index)
                .unwrap();
            magic_effect.global_form_id.set_load_order_index(new_index);
        }

        // Create new magic_effects hashmap with remapped global form IDs
        let magic_effects = magic_effects
            .into_iter()
            .map(|mgef| (mgef.get_global_form_id(), mgef))
            .collect();

        Self {
            load_order,
            ingredients,
            magic_effects,
        }
    }

    pub fn get_ingredients(&self) -> &HashMap<GlobalFormId, Ingredient> {
        &self.ingredients
    }

    pub fn get_magic_effects(&self) -> &HashMap<GlobalFormId, MagicEffect> {
        &self.magic_effects
    }

    pub fn get_magic_effect(&self, global_form_id: &GlobalFormId) -> Option<&MagicEffect> {
        self.magic_effects.get(global_form_id)
    }

    pub fn get_ingredient(&self, global_form_id: &GlobalFormId) -> Option<&Ingredient> {
        self.ingredients.get(global_form_id)
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
                        if self.magic_effects.contains_key(&ingef.get_global_form_id()) {
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
                        ing.get_global_form_id()
                    }
                })
                .collect::<Vec<_>>()
        };

        for form_id in ingredients_form_ids_to_remove {
            self.ingredients.remove(&form_id);
        }
    }
}
