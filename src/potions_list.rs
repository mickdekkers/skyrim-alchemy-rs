use serde::{ser::SerializeStruct, Serialize, Serializer};
use std::time::Instant;

use arrayvec::ArrayVec;
use itertools::Itertools;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{game_data::GameData, potion::Potion};

pub struct PotionsList<'a> {
    game_data: &'a GameData,
    potions_2: Vec<Potion<'a>>,
    potions_3: Vec<Potion<'a>>,
}

impl<'a> Serialize for PotionsList<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut pl = serializer.serialize_struct("PotionsList", 2)?;
        pl.serialize_field("potions_2", &self.potions_2)?;
        pl.serialize_field("potions_3", &self.potions_3)?;
        pl.end()
    }
}

impl<'a> PotionsList<'a> {
    /// Create a new `PotionsList` from the provided ingredients and magic effects.
    /// Note: the ingredients and magic effects hashmaps should include all those that exist in the
    /// game. Filtering the `PotionsList` can be done after construction.
    pub fn new(game_data: &'a GameData) -> Self {
        Self {
            game_data,
            potions_2: Vec::new(),
            potions_3: Vec::new(),
        }
    }

    /// Computes all possible potions
    pub fn build_potions(&mut self) {
        self.build_potions_2();
        self.build_potions_3();
    }

    /// Compute the Vec of potions with 2 ingredients
    fn build_potions_2(&mut self) {
        // Note: temporarily storing the combinations and then using par_iter is about twice as
        // fast as using par_bridge directly on the combinations iterator (at the cost of some ram)
        let start = Instant::now();
        let combos_2: Vec<_> = self
            .game_data
            .get_ingredients()
            .values()
            .sorted_by_key(|ig| &ig.name)
            .combinations(2)
            .collect();
        log::debug!(
            "Found {} possible 2-ingredient combos (in {:?})",
            combos_2.len(),
            start.elapsed()
        );

        let start = Instant::now();
        let valid_combos_2: Vec<_> = combos_2
            .par_iter()
            .filter(|combo| {
                let a = combo.get(0).unwrap();
                let b = combo.get(1).unwrap();

                // Ensure the two ingredients share an effect
                a.shares_effects_with(b)
            })
            .collect();
        log::debug!(
            "Found {} valid 2-ingredient combos (in {:?})",
            valid_combos_2.len(),
            start.elapsed()
        );

        let start = Instant::now();
        let mut potions_2: Vec<_> = valid_combos_2
            .par_iter()
            .map(|combo| {
                let ingredients = ArrayVec::<_, 3>::from_iter(combo.iter().copied());
                Potion::from_ingredients(&ingredients, self.game_data)
                    .expect("ingredients combo should be valid Potion")
            })
            .collect();
        potions_2.sort_by_key(|pot| pot.get_gold_value());
        potions_2.reverse();
        log::debug!(
            "Created {} Potion instances (in {:?})",
            potions_2.len(),
            start.elapsed()
        );

        self.potions_2 = potions_2;
    }

    // Compute the Vec of potions with 3 ingredients
    fn build_potions_3(&mut self) {
        //Note: temporarily storing the combinations and then using par_iter is about twice as
        //fast as using par_bridge directly on the combinations iterator (at the cost of some ram)
        let start = Instant::now();
        let combos_3: Vec<_> = self
            .game_data
            .get_ingredients()
            .values()
            .sorted_by_key(|ig| &ig.name)
            .combinations(3)
            .collect();
        log::debug!(
            "Found {} possible 3-ingredient combos (in {:?})",
            combos_3.len(),
            start.elapsed()
        );

        let start = Instant::now();
        let valid_combos_3: Vec<_> = combos_3
            .par_iter()
            .filter(|combo| {
                let a = combo.get(0).unwrap();
                let b = combo.get(1).unwrap();
                let c = combo.get(2).unwrap();

                // Ensure all three ingredients share an effect with at least one of the others
                (a.shares_effects_with(b) && (c.shares_effects_with(a) || c.shares_effects_with(b)))
                    || (a.shares_effects_with(c) && b.shares_effects_with(c))
            })
            .collect();
        log::debug!(
            "Found {} valid 3-ingredient combos (in {:?})",
            valid_combos_3.len(),
            start.elapsed()
        );

        let start = Instant::now();
        let mut potions_3: Vec<_> = valid_combos_3
            .par_iter()
            .map(|combo| {
                let ingredients = ArrayVec::<_, 3>::from_iter(combo.iter().copied());
                Potion::from_ingredients(&ingredients, self.game_data)
                    .expect("ingredients combo should be valid Potion")
            })
            .collect();
        potions_3.sort_by_key(|pot| pot.get_gold_value());
        potions_3.reverse();
        log::debug!(
            "Created {} Potion instances (in {:?})",
            potions_3.len(),
            start.elapsed()
        );

        self.potions_3 = potions_3;
    }

    pub fn get_potions(&self) -> impl Iterator<Item = &Potion> + '_ {
        // Return an iterator over the two potions vecs merged in order of gold value descending
        self.potions_3
            .iter()
            .merge_by(self.potions_2.iter(), |a, b| {
                a.get_gold_value() > b.get_gold_value()
            })
    }
}
