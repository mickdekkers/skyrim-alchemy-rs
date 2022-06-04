use std::{fmt::Display, str::FromStr};

use serde_with::{DeserializeFromStr, SerializeDisplay};

#[derive(
    Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, DeserializeFromStr, SerializeDisplay,
)]
pub struct GlobalFormId {
    pub load_order_index: u16,
    pub id: u32,
}

impl GlobalFormId {
    pub fn new(load_order_index: u16, id: u32) -> Self {
        GlobalFormId {
            load_order_index,
            id,
        }
    }
}

impl Display for GlobalFormId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}:{:06x}", self.load_order_index, self.id)
    }
}

impl FromStr for GlobalFormId {
    type Err = String;

    /// Parse a value like `0004:3F0001`
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split(':');

        let load_order_index = {
            let part = parts
                .next()
                .ok_or_else(|| "Missing first value".to_string())?;
            part.parse::<u16>().map_err(|err| err.to_string())?
        };

        let id = {
            let part = parts
                .next()
                .ok_or_else(|| "Missing second value".to_string())?;
            u32::from_str_radix(part, 16).map_err(|err| err.to_string())?
        };

        Ok(Self {
            load_order_index,
            id,
        })
    }
}

pub trait FormIdContainer {
    fn get_global_form_id(&self) -> GlobalFormId;
}
