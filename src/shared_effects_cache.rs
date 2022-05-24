use moka::sync::{Cache, CacheBuilder};
use std::{cmp::Ordering, collections::HashMap};

use crate::plugin_parser::{form_id::FormIdContainer, ingredient::Ingredient};

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderedPair<T>
where
    T: Ord,
{
    a: T,
    b: T,
}

impl<T> OrderedPair<T>
where
    T: Ord,
{
    pub fn new(a: T, b: T) -> Self {
        // Make sure elements are ordered (ascending)
        let (a, b) = match a.cmp(&b) {
            Ordering::Less | Ordering::Equal => (a, b),
            Ordering::Greater => (b, a),
        };
        Self { a, b }
    }
}

pub struct SharedEffectsCache {
    cache: Cache<OrderedPair<u32>, bool>,
}

// 500k entries is plenty for even 1000 different ingredients.
// A single entry is 9 bytes without overhead (two u32s and a bool),
// so that totals about 4.5 MB of cache space.
const CACHE_CAPACITY: u64 = 500_000;

impl SharedEffectsCache {
    pub fn new() -> Self {
        Self {
            // Allocate entire cache upfront to avoid many small allocations later on
            cache: CacheBuilder::new(CACHE_CAPACITY)
                .initial_capacity(CACHE_CAPACITY as usize)
                .build(),
        }
    }

    pub fn cached_shares_effects_with(&self, a: &Ingredient, b: &Ingredient) -> bool {
        let key = OrderedPair::new(a.get_form_id(), b.get_form_id());
        self.cache.get_with(key, || a.shares_effects_with(b))
    }

    pub fn clear(&self) {
        self.cache.invalidate_all()
    }
}

pub struct SharedEffectsCacheUnsync {
    cache: HashMap<OrderedPair<u32>, bool>,
}

impl SharedEffectsCacheUnsync {
    pub fn new() -> Self {
        Self {
            // Allocate entire cache upfront to avoid many small allocations later on
            cache: HashMap::with_capacity(CACHE_CAPACITY as usize),
        }
    }

    pub fn cached_shares_effects_with(&mut self, a: &Ingredient, b: &Ingredient) -> bool {
        let key = OrderedPair::new(a.get_form_id(), b.get_form_id());
        *self
            .cache
            .entry(key)
            .or_insert_with(|| a.shares_effects_with(b))
    }

    pub fn clear(&mut self) {
        self.cache.clear()
    }
}
