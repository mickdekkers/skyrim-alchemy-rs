use ahash::AHashSet;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use std::{collections::HashSet, time::Instant};

use arrayvec::ArrayVec;
use itertools::Itertools;
use permutator::LargeCombinationIterator;
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    slice::ParallelSliceMut,
};

use crate::{
    game_data::GameData,
    plugin_parser::{
        form_id::FormIdContainer,
        ingredient::{Ingredient, IngredientEffect},
    },
    potion::Potion,
};

pub struct PotionsList<'a> {
    game_data: &'a GameData,
    potions_2: Vec<Potion<'a>>,
    potions_3: Vec<Potion<'a>>,
}

// impl<'a> Serialize for PotionsList<'a> {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         let mut pl = serializer.serialize_struct("PotionsList", 2)?;
//         pl.serialize_field("potions_2", &self.potions_2)?;
//         pl.serialize_field("potions_3", &self.potions_3)?;
//         pl.end()
//     }
// }

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
        let potions_2 = PotionsList::build_potions_2(self.game_data);
        let potions_3 = PotionsList::build_potions_3(self.game_data);

        self.potions_2 = potions_2;
        self.potions_3 = potions_3;
    }

    /// Compute the Vec of potions with 2 ingredients
    fn build_potions_2(game_data: &GameData) -> Vec<Potion> {
        // TODO: recheck this note
        // Note: temporarily storing the combinations and then using par_iter is about twice as
        // fast as using par_bridge directly on the combinations iterator (at the cost of some ram)
        let start = Instant::now();
        let ingredients = game_data
            .get_ingredients()
            .values()
            .sorted_by_key(|ig| &ig.name)
            .collect::<Vec<_>>();
        let combos_2: Vec<_> = LargeCombinationIterator::new(&ingredients, 2).collect::<Vec<_>>();
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
                let ingredients = ArrayVec::<_, 3>::from_iter(combo.iter().copied().copied());
                Potion::from_ingredients(&ingredients, game_data)
                    .expect("ingredients combo should be valid Potion")
            })
            .collect();
        log::debug!(
            "Created {} Potion instances (in {:?})",
            potions_2.len(),
            start.elapsed()
        );
        let start = Instant::now();
        // Sort (unstably) in parallel by gold value descending
        potions_2.par_sort_unstable_by(|a, b| a.gold_value.cmp(&b.gold_value).reverse());
        log::debug!(
            "Sorted {} Potion instances (in {:?})",
            potions_2.len(),
            start.elapsed()
        );

        potions_2
    }

    // Compute the Vec of potions with 3 ingredients
    fn build_potions_3(game_data: &GameData) -> Vec<Potion> {
        // TODO: see if it might be possible to generate the combinations in parallel somehow
        // TODO: recheck this note
        // Note: temporarily storing the combinations and then using par_iter is about twice as
        // fast as using par_bridge directly on the combinations iterator (at the cost of some ram)
        let start = Instant::now();
        let ingredients = game_data
            .get_ingredients()
            .values()
            .sorted_by_key(|ig| &ig.name)
            .collect::<Vec<_>>();
        let combos_3: Vec<_> = LargeCombinationIterator::new(&ingredients, 3).collect::<Vec<_>>();
        log::debug!(
            "Found {} possible 3-ingredient combos (in {:?})",
            combos_3.len(),
            start.elapsed()
        );

        let start = Instant::now();
        let valid_combos_3: Vec<_> = combos_3
            .par_iter()
            .filter(|combo| {
                let a = *combo.get(0).unwrap();
                let b = *combo.get(1).unwrap();
                let c = *combo.get(2).unwrap();

                let mut a_b_effects = a.effects_shared_with(b);
                let mut b_c_effects = b.effects_shared_with(c);
                let mut c_a_effects = c.effects_shared_with(a);

                let a_shares_effects_with_b = a_b_effects.peek().is_some();
                let b_shares_effects_with_c = b_c_effects.peek().is_some();
                let c_shares_effects_with_a = c_a_effects.peek().is_some();

                // We require at least two edges that contribute a unique effect (otherwise one of
                // the ingredients is used for no reason and goes to waste)
                //      a
                //    /   \
                //   c --- b
                fn edges_are_not_the_same<'a, T>(edge_1: T, edge_2: T, edge_3: Option<T>) -> bool
                where
                    T: Iterator<Item = &'a IngredientEffect>,
                {
                    // Note: this function assumes the iterators are not empty
                    let edge_1 = edge_1
                        .map(|eff| eff.get_global_form_id())
                        .collect::<AHashSet<_>>();
                    let edge_2 = edge_2
                        .map(|eff| eff.get_global_form_id())
                        .collect::<AHashSet<_>>();
                    let edge_3 = edge_3.map(|edge_3| {
                        edge_3
                            .map(|eff| eff.get_global_form_id())
                            .collect::<AHashSet<_>>()
                    });

                    // Each ingredient must contribute at least one unique effect when combined
                    // with the others
                    if let Some(edge_3) = edge_3 {
                        let edges_1_2_have_diff =
                            edge_1.symmetric_difference(&edge_2).next().is_some();
                        let edges_2_3_have_diff =
                            edge_2.symmetric_difference(&edge_3).next().is_some();
                        let edges_3_1_have_diff =
                            edge_3.symmetric_difference(&edge_1).next().is_some();

                        (edges_1_2_have_diff && (edges_3_1_have_diff || edges_2_3_have_diff))
                            || (edges_3_1_have_diff && edges_2_3_have_diff)
                    } else {
                        edge_1.symmetric_difference(&edge_2).next().is_some()
                    }
                }

                match (
                    a_shares_effects_with_b,
                    b_shares_effects_with_c,
                    c_shares_effects_with_a,
                ) {
                    (true, true, false) => edges_are_not_the_same(a_b_effects, b_c_effects, None),
                    (true, false, true) => edges_are_not_the_same(a_b_effects, c_a_effects, None),
                    (false, true, true) => edges_are_not_the_same(b_c_effects, c_a_effects, None),
                    (true, true, true) => {
                        edges_are_not_the_same(a_b_effects, b_c_effects, Some(c_a_effects))
                    }
                    // Anything else does not have at least 2 edges
                    (_, _, _) => false,
                }
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
                let ingredients = ArrayVec::<_, 3>::from_iter(combo.iter().copied().copied());
                Potion::from_ingredients(&ingredients, game_data)
                    .expect("ingredients combo should be valid Potion")
            })
            .collect();
        log::debug!(
            "Created {} Potion instances (in {:?})",
            potions_3.len(),
            start.elapsed()
        );
        let start = Instant::now();
        // Sort (unstably) in parallel by gold value descending
        potions_3.par_sort_unstable_by(|a, b| a.gold_value.cmp(&b.gold_value).reverse());
        log::debug!(
            "Sorted {} Potion instances (in {:?})",
            potions_3.len(),
            start.elapsed()
        );

        potions_3
    }

    pub fn get_potions(&self) -> impl Iterator<Item = &Potion> + '_ {
        // Return an iterator over the two potions vecs merged in order of gold value descending
        self.potions_3
            .iter()
            .merge_by(self.potions_2.iter(), |a, b| a.gold_value > b.gold_value)
    }
}
