use ahash::AHashMap;
use itertools::{EitherOrBoth::*, Itertools};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadOrder {
    load_order: Vec<String>,
}

impl LoadOrder {
    pub fn new(load_order: Vec<String>) -> Self {
        Self {
            load_order: load_order.into_iter().collect::<Vec<_>>(),
        }
    }

    pub fn find_index(&self, mod_name: &str) -> Option<u16> {
        self.load_order
            .iter()
            .enumerate()
            .find_map(|(index, name)| {
                if matches!(cmp_ignore_case_ascii(name, mod_name), Ordering::Equal) {
                    Some(index as u16)
                } else {
                    None
                }
            })
    }

    pub fn get(&self, index: u16) -> Option<&str> {
        self.load_order.get(index as usize).map(|x| x.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.load_order.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> + '_ {
        self.load_order.iter()
    }

    /// Removes unused entries from the LoadOrder based on the used indexes returned by the iterator
    /// that is passed in. If nothing was removed, returns None. Otherwise returns Some(HashMap) of
    /// old index to new index which must be used to update any existing indexes into the LoadOrder.
    #[must_use]
    pub fn drain_unused(
        &mut self,
        used_indexes: impl Iterator<Item = u16>,
    ) -> Option<AHashMap<u16, u16>> {
        let used_entries_with_old_indexes = used_indexes
            .sorted_unstable()
            .dedup()
            .map(|index| (self.get(index).unwrap().to_string(), index))
            .collect::<AHashMap<String, u16>>();

        let num_removed = self
            .load_order
            .drain_filter(|entry| !used_entries_with_old_indexes.contains_key(entry))
            .count();

        if num_removed == 0 {
            return None;
        }

        log::debug!("Removed {} unused entries from load order", num_removed);
        Some(
            used_entries_with_old_indexes
                .iter()
                // Create map from old index to new index
                .map(|(entry, old_index)| (*old_index, self.find_index(entry).unwrap()))
                .collect(),
        )
    }
}

impl Display for LoadOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.load_order
                .iter()
                .enumerate()
                .map(|(index, entry)| format!("{:04}: {}", index, entry))
                .join("\n")
        )
    }
}

// https://stackoverflow.com/a/63871901/1233003
/// Efficient way to compare two string slices case-insensitively
fn cmp_ignore_case_ascii(a: &str, b: &str) -> Ordering {
    a.bytes()
        .zip_longest(b.bytes())
        .map(|ab| match ab {
            Left(_) => Ordering::Greater,
            Right(_) => Ordering::Less,
            Both(a, b) => a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()),
        })
        .find(|&ordering| ordering != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}
