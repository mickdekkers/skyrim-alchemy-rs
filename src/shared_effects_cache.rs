use std::{cmp::Ordering, collections::HashMap};

use crate::plugin_parser::{
    form_id::{FormIdContainer, FormIdPairRef},
    ingredient::Ingredient,
};

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormIdCombo<'a>(FormIdPairRef<'a>, FormIdPairRef<'a>);

impl<'a> FormIdCombo<'a> {
    pub fn new(a: FormIdPairRef<'a>, b: FormIdPairRef<'a>) -> Self {
        // Make sure elements are ordered (ascending)
        let (a, b) = match a.cmp(&b) {
            Ordering::Less | Ordering::Equal => (a, b),
            Ordering::Greater => (b, a),
        };
        Self(a, b)
    }
}

// TODO: maybe use DashMap instead? Allows concurrent use
pub struct SharedEffectsCache<'a>(HashMap<FormIdCombo<'a>, bool>);

impl<'a> SharedEffectsCache<'a> {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn cached_shares_effects_with(&mut self, a: &'a Ingredient, b: &'a Ingredient) -> bool {
        let key = FormIdCombo::new(a.get_form_id_pair_ref(), b.get_form_id_pair_ref());
        *self.entry(key).or_insert_with(|| a.shares_effects_with(b))
    }
}

impl<'a> core::ops::Deref for SharedEffectsCache<'a> {
    type Target = HashMap<FormIdCombo<'a>, bool>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> core::ops::DerefMut for SharedEffectsCache<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
