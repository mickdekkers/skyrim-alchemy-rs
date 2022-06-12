use ahash::AHashMap;
use anyhow::anyhow;
use itertools::{EitherOrBoth::*, Itertools};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;

// FIXME: make case insensitive and instead use Skyrim.ccc to determine what is or is not CC content
fn is_creation_club_light_master<S: AsRef<str>>(mod_name: S) -> bool {
    let mod_name = mod_name.as_ref();
    mod_name.starts_with("cc") && mod_name.ends_with(".esl")
}

// FIXME: need to read esl/esm status from plugin directly, not base on extension. See Arthmoor post

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadOrder {
    // TODO: masters != plugins
    masters: Vec<String>,
    light_masters: Vec<String>,
}

impl LoadOrder {
    pub fn new(load_order: Vec<String>) -> Self {
        let (creation_club_light_masters, masters): (Vec<_>, Vec<_>) = load_order
            .into_iter()
            .partition(|entry| is_creation_club_light_master(entry));

        // FIXME: save games contain lists of loaded plugins. I might(?) need to use these to correctly parse the form IDs. Are these lists in order?
        // FIXME: the wiki is wrong. Arthmoor and mod organizer are right https://www.afkmods.com/index.php?/topic/5079-plugin-files-and-you-esmeslesp/ -- it's not alphabetical, but determined by the Skyrim.ccc file
        // FIXME: read and impl this https://www.afkmods.com/index.php?/topic/5079-plugin-files-and-you-esmeslesp/page/5/#comment-174631
        // Creation Club .esl files are sorted alphabetically to determine their load order
        // See https://en.uesp.net/wiki/Skyrim:Form_ID#Creation_ClubCC
        creation_club_light_masters.sort();

        Self {
            // Note: additional light masters may be identified as they are parsed
            light_masters: creation_club_light_masters,
            masters,
        }
    }

    // FIXME: handle "ESL flagged ESPs". They maintain the load order but are loaded in ESL space
    // FIXME: mod organizer seems to maintain that (generally) file time is the actual load order https://github.com/ModOrganizer2/modorganizer-game_gamebryo/blob/3abd56d555c65c9b1d0d547a33cdc0e66d54b61a/src/gamebryo/gamebryogameplugins.cpp#L182

    /// Marks the plugin with the specified `mod_name` as a light master
    ///
    /// **Note**: assumes plugins are iterated in original load order!
    pub fn plugin_is_esl_flagged(&mut self, mod_name: &str) -> Result<(), anyhow::Error> {
        if is_esl(mod_name) {
            // We don't need to do anything for .esl files, they were handled in Self::new
            return Ok(());
        }

        let index = self.find_masters_index(mod_name).ok_or_else(|| {
            anyhow!(
                "failed to mark plugin \"{}\" as light master: not found in masters",
                mod_name
            )
        })?;
        // Move from masters to light_masters
        let entry = self.masters.remove(index);
        self.light_masters.push(entry);
        Ok(())
    }

    /// Finds the index of `mod_name` in the `masters` Vec
    fn find_masters_index(&self, mod_name: &str) -> Option<usize> {
        self.masters.iter().enumerate().find_map(|(index, name)| {
            if matches!(cmp_ignore_case_ascii(name, mod_name), Ordering::Equal) {
                Some(index)
            } else {
                None
            }
        })
    }

    pub fn get_form_id_prefix(&self, mod_name: &str) -> Option<u32> {
        self.masters.iter().enumerate().find_map(|(index, name)| {
            if matches!(cmp_ignore_case_ascii(name, mod_name), Ordering::Equal) {
                Some(index as u32)
            } else {
                None
            }
        })
    }

    pub fn get(&self, index: u16) -> Option<&str> {
        self.masters.get(index as usize).map(|x| x.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.masters.is_empty()
    }

    // FIXME: need to clone this data
    pub fn iter(&self) -> impl Iterator<Item = &String> + '_ {
        self.masters.iter()
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
            .masters
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
                .map(|(entry, old_index)| (*old_index, self.get_form_id_prefix(entry).unwrap()))
                .collect(),
        )
    }
}

impl Display for LoadOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.masters
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
