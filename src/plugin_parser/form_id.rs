use std::{fmt::Display, str::FromStr};

use serde_with::{DeserializeFromStr, SerializeDisplay};

use crate::load_order::LoadOrder;

#[derive(
    Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, DeserializeFromStr, SerializeDisplay,
)]
pub struct GlobalFormId(u32);

impl GlobalFormId {
    pub fn new(form_id: u32) -> Self {
        GlobalFormId(form_id)
    }
}

impl Display for GlobalFormId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08x}", self.0)
    }
}

impl FromStr for GlobalFormId {
    type Err = String;

    /// Parse a value like `043F0001`
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let form_id = u32::from_str_radix(s, 16).map_err(|err| err.to_string())?;

        Ok(Self(form_id))
    }
}

pub trait FormIdContainer {
    fn get_global_form_id(&self) -> GlobalFormId;
}
