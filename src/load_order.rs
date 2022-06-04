use itertools::{EitherOrBoth::*, Itertools};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt::Display;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoadOrder {
    load_order: Vec<String>,
}

impl LoadOrder {
    pub fn new(load_order: Vec<String>) -> Self {
        Self { load_order }
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

    pub fn iter(&self) -> std::slice::Iter<String> {
        self.load_order.iter()
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
                .map(|(index, item)| format!("{:04}: {}", index, item))
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
