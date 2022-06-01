use std::{borrow::Cow, fmt::Display};

// TODO: remove Cow, not really used anymore
// Use Cow to allow use as key in hashmap without temporary allocations https://stackoverflow.com/a/36486921/1233003
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct GlobalFormId<'a> {
    pub plugin: Cow<'a, str>,
    pub id: u32,
}

impl<'a> GlobalFormId<'a> {
    pub fn new<S: Into<Cow<'a, str>>>(plugin: S, id: u32) -> Self {
        GlobalFormId {
            plugin: plugin.into(),
            id,
        }
    }

    // TODO: hopefully deprecate this when it's no longer needed
    pub fn to_owned_pair(&self) -> (String, u32) {
        (self.plugin.to_string(), self.id)
    }
}

impl<'a> Display for GlobalFormId<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.plugin, self.id)
    }
}

pub trait FormIdContainer {
    // TODO: deprecate get_local_form_id, poorly named and not useful
    fn get_local_form_id(&self) -> u32;
    fn get_global_form_id(&self) -> GlobalFormId;
}
