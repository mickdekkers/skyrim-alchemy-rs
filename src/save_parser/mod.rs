use anyhow::{anyhow, Context};
use itertools::Itertools;
use lazy_static::lazy_static;
use log_err::{LogErrOption, LogErrResult};
use nom::IResult;
use skyrim_savegame::{read_vsval_to_u32, ChangeForm, FormIdType, RefId, SaveFile, VSVal};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::game_data::GameData;
use crate::plugin_parser::form_id::GlobalFormId;
use crate::plugin_parser::utils::nom_err_to_anyhow_err;

lazy_static! {
    static ref DEFAULT_SAVES_PATH: PathBuf = dirs::document_dir()
        .unwrap()
        .join("My Games/Skyrim Special Edition/Saves");
}

fn get_latest_save_data<PSaves>(saves_path: Option<PSaves>) -> Result<Vec<u8>, anyhow::Error>
where
    PSaves: AsRef<Path>,
{
    let saves_path = saves_path
        .as_ref()
        .map(AsRef::as_ref)
        .unwrap_or(DEFAULT_SAVES_PATH.as_path());

    let mut saves: Vec<(OsString, SystemTime)> = vec![];
    for entry in fs::read_dir(saves_path).with_context(|| "failed to read saves directory")? {
        let entry = entry.with_context(|| "failed to read saves directory entry")?;
        let path = entry.path();
        match path.extension() {
            Some(ext) if ext != "ess" => continue,
            None => continue,
            _ => (),
        };
        let metadata = entry
            .metadata()
            .with_context(|| "failed to read save file metadata")?;
        let modified = metadata
            .modified()
            .with_context(|| "failed to read save file modification time")?;
        saves.push((entry.file_name(), modified));
    }

    // Sort by last modified time descending
    saves.sort_by(|a, b| a.1.cmp(&b.1).reverse());

    log::debug!(
        "Found {} save files in directory {}",
        saves.len(),
        saves_path.display()
    );

    let latest_save_path = saves_path.join(
        saves
            .first()
            .map(|x| {
                log::debug!(
                    "Latest save: {} (last modified {})",
                    x.0.to_string_lossy(),
                    x.1.elapsed()
                        .map(|dur| format!(
                            "{} ago",
                            // Probably suboptimal way to round Duration to seconds, then format it
                            humantime::format_duration(Duration::from_secs(dur.as_secs()))
                        ))
                        .unwrap_or_else(|_| "<in the future> ????".to_string())
                );
                &x.0
            })
            .ok_or_else(|| anyhow!("no save file found in directory {}", saves_path.display()))?,
    );

    fs::read(latest_save_path).with_context(|| "failed to read save file")
}

pub type InventoryEntry = (GlobalFormId, u32);
pub type Inventory = Vec<InventoryEntry>;

pub fn read_saves<PSaves>(
    saves_path: Option<PSaves>,
    game_data: &GameData,
) -> Result<Inventory, anyhow::Error>
where
    PSaves: AsRef<Path>,
{
    let save_data = get_latest_save_data(saves_path)?;
    // TODO: this may panic. Catch somehow?
    let start = Instant::now();
    let save_file = skyrim_savegame::parse_save_file(save_data);
    log::debug!("Rudimentarily parsed save file (in {:?})", start.elapsed());
    log::info!("{:#?}", save_file);

    let start = Instant::now();
    let player_change_form = save_file
        .change_forms
        .iter()
        .find(|cf| {
            matches!(
                get_change_form_data_type(cf),
                Some(ChangeFormDataType::Actor)
            ) && ({
                let form_id = get_real_form_id(&cf.form_id, &save_file).log_unwrap();

                // Is player change form
                form_id == 0x14
            })
        })
        .log_expect("save game contains no player data");
    log::debug!("Found player change form (in {:?})", start.elapsed());

    let start = Instant::now();
    // See https://en.uesp.net/wiki/Skyrim_Mod:ChangeFlags#Initial_type
    // Note: assumes ACHR change form type
    let initial_type: u32 = {
        if matches!(player_change_form.form_id, FormIdType::Created(_)) {
            5
            // CHANGE_REFR_PROMOTED or CHANGE_REFR_CELL_CHANGED flags
        } else if player_change_form.change_flags & 0x02000000 != 0
            || player_change_form.change_flags & 0x00000008 != 0
        {
            6
            // CHANGE_REFR_HAVOK_MOVE or CHANGE_REFR_MOVE flags
        } else if player_change_form.change_flags & 0x00000004 != 0
            || player_change_form.change_flags & 0x00000002 != 0
        {
            4
        } else {
            0
        }
    };
    let initial_type_size: u32 = match initial_type {
        0 => 0,
        1 => 8,
        2 => 10,
        3 => 4,
        4 => 27,
        5 => 31,
        6 => 34,
        other => panic!("unknown initial type {}", other),
    };

    let (remaining_data, _) = nom::sequence::tuple((
        nom::combinator::cond(
            initial_type_size != 0,
            // Skip initial data
            nom::bytes::complete::take(initial_type_size),
        ),
        nom::combinator::cond(
            // CHANGE_REFR_HAVOK_MOVE flag
            player_change_form.change_flags & 0x00000004 != 0,
            // Skip havok data
            nom::multi::length_count(read_vsval, nom::number::complete::le_u8),
        ),
        // Skip unknown integer + unknown data
        nom::bytes::complete::take(std::mem::size_of::<u32>() + std::mem::size_of::<u8>() * 4),
        nom::combinator::cond(
            // CHANGE_FORM_FLAGS flag
            player_change_form.change_flags & 0x00000001 != 0,
            // Skip flag + unknown
            nom::bytes::complete::take(std::mem::size_of::<u32>() + std::mem::size_of::<u16>()),
        ),
        nom::combinator::cond(
            // CHANGE_REFR_BASEOBJECT flag
            player_change_form.change_flags & 0x00000080 != 0,
            // Skip base object ref ID
            nom::bytes::complete::take(3usize),
        ),
        nom::combinator::cond(
            // CHANGE_REFR_SCALE flag
            player_change_form.change_flags & 0x00000010 != 0,
            // Skip scale float
            nom::number::complete::le_f32,
        ),
    ))(player_change_form.data.as_ref())
    .map_err(nom_err_to_anyhow_err)?;
    log::debug!(
        "Skipped irrelevant data in player change form (in {:?})",
        start.elapsed()
    );

    // Now comes the extra data (probably), which we don't have enough information on to skip

    // TODO: scan the remaining changeform data for known refIDs to find the inventory
    // Construct skyrim_savegame::RefId out of 3 consecutive bytes, then convert that to a form ID and see if that is in a map of known ingredient form IDs
    // if it is, parse the next four bytes as an i32 (or u32?), which would indicate the count
    // probably need to use iter().windows() https://doc.rust-lang.org/stable/std/primitive.slice.html#method.windows
    // also see if we can skip the next n bytes if an ingredient is found
    // can do a sanity check on the count to see if that's in a reasonable range too
    // would be cool if we could use rayon, but probably not needed

    // TODO: somehow prevent / filter out false positives in case some random bytes happen to match a known form ID. Perhaps consider index where found and eliminate outliers at start and end? Inventory entries should be fairly close together, though each entry can also have zero or more extra datas (I'm guessing these will be rather small?)
    // TODO: need to somehow translate form ID in save to GlobalFormId... How does runtime form ID map to form ID in data? Read wiki.

    log::debug!(
        "Will try to parse inventory items from remaining {} bytes of player data",
        remaining_data.len()
    );

    let start = Instant::now();

    // TODO: the same ingredient (probably) won't appear multiple times. Pick one with lowest item count?
    let mut remaining_data = remaining_data;
    let mut inventory_items = vec![];
    while !remaining_data.is_empty() {
        match partial_inventory_item(remaining_data, &save_file, game_data) {
            Ok((remaining_input, inventory_item)) => {
                inventory_items.push(inventory_item);
                // Move cursor by length of successfully consumed data
                remaining_data = remaining_input;
            }
            Err(_) => {
                // Move cursor one byte, try again next iteration
                remaining_data = &remaining_data[1..];
            }
        }
    }

    log::debug!(
        "Parsed {} inventory items (in {:?})",
        inventory_items.len(),
        start.elapsed()
    );
    log::debug!(
        "Inventory:\n{}",
        inventory_items
            .iter()
            .map(|(form_id, count)| format!(
                "{} ({}): {}",
                form_id,
                game_data
                    .get_ingredient(form_id)
                    .unwrap()
                    .name
                    .as_ref()
                    .unwrap(),
                count
            ))
            .join("\n")
    );

    todo!();
    // Ok(())
}

fn partial_inventory_item<'a>(
    input: &'a [u8],
    save_file: &SaveFile,
    game_data: &GameData,
) -> Result<(&'a [u8], (GlobalFormId, i32)), anyhow::Error> {
    let (remaining_input, form_id) = parse_ref_id_to_form_id(input, save_file)?;

    // I don't believe we'll ever see an ingredient with a form ID of exactly 0x00000000
    if form_id == 0x00000000 {
        return Err(anyhow!("form ID is 0x00000000"));
    }

    // Form IDs starting with 0xFF are dynamically allocated, ingredients (probably) don't have this
    if form_id & 0xFF000000 != 0 {
        return Err(anyhow!("form ID starts with 0xFF"));
    }

    // FIXME: make work for non skyrim.esm form IDs
    let form_id = GlobalFormId::new((form_id & 0xFF000000) as u16, form_id & 0x00FFFFFF);

    if !game_data.has_ingredient(&form_id) {
        return Err(anyhow!("form ID is not a known ingredient"));
    }

    // TODO: mod organizer has it right! check the form ID prefixes against that

    // The item count i32 is followed by a vsval indicating the count of extra data for this item. We don't care about this value, but we can use it to improve parsing accuracy
    let (remaining_input, item_count) =
        nom::sequence::terminated(nom::number::complete::le_i32, read_vsval)(remaining_input)
            .map_err(nom_err_to_anyhow_err)?;

    if item_count < 1 {
        return Err(anyhow!("item count is less than 1"));
    }

    if item_count > 5000 {
        return Err(anyhow!("item count is improbably high"));
    }

    Ok((remaining_input, (form_id, item_count)))
}

fn parse_ref_id_to_form_id<'a>(
    input: &'a [u8],
    save_file: &SaveFile,
) -> Result<(&'a [u8], u32), anyhow::Error> {
    let (remaining_input, three_bytes) = nom::bytes::complete::take(3usize)(input)
        .map_err(|err: nom::Err<(_, nom::error::ErrorKind)>| nom_err_to_anyhow_err(err))?;
    let (byte0, byte1, byte2) = (three_bytes[0], three_bytes[1], three_bytes[2]);
    let ref_id = RefId {
        byte0,
        byte1,
        byte2,
    };
    let form_id = get_real_form_id(&ref_id.get_form_id(), save_file)?;

    Ok((remaining_input, form_id))
}

#[derive(Debug)]
enum ChangeFormDataType {
    Actor,
}

/// Returns `Some(ChangeFormDataType)` if it's a data type we care about
fn get_change_form_data_type(change_form: &ChangeForm) -> Option<ChangeFormDataType> {
    // Look at lower 6 bits
    match change_form.data_type & 0x3F {
        1 => Some(ChangeFormDataType::Actor),
        _ => None,
    }
}

fn get_real_form_id(raw_form_id: &FormIdType, save_file: &SaveFile) -> Result<u32, anyhow::Error> {
    match raw_form_id {
        FormIdType::Index(value) => Ok(*save_file
            .form_id_array
            .get(*value as usize)
            .ok_or_else(|| anyhow!("form ID index not in form ID array: {}", value))?),
        FormIdType::Default(value) => Ok(*value),
        FormIdType::Created(value) => Ok(0xFF000000 | *value),
        FormIdType::Unknown(_) => Err(anyhow!("encountered unknown form ID type")),
    }
}

#[derive(Debug, PartialEq)]
pub enum CustomError<I> {
    InvalidVsvalValueType,
    Nom(I, nom::error::ErrorKind),
}

impl<I> nom::error::ParseError<I> for CustomError<I> {
    fn from_error_kind(input: I, kind: nom::error::ErrorKind) -> Self {
        CustomError::Nom(input, kind)
    }

    // TODO: see if we need to impl this differently, + other methods on ParseError trait
    fn append(_: I, _: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

/// Reads a vsval to u32
pub fn read_vsval(input: &[u8]) -> IResult<&[u8], u32, CustomError<&[u8]>> {
    let (input, first_byte) = nom::number::complete::le_u8(input)?;
    let val_type_enc = first_byte & 0b00000011;
    match val_type_enc {
        0 => Ok((input, ((first_byte & 0b11111100) >> 2) as u32)),
        1 => {
            let first_byte = first_byte as u16;
            let (input, second_byte) = nom::number::complete::le_u8(input)?;
            Ok((
                input,
                // TODO: XOR, correct?
                (((second_byte as u16) << 8 ^ first_byte) >> 2) as u32,
            ))
        }
        2 => {
            let first_byte = first_byte as u32;
            let (input, second_byte) = nom::number::complete::le_u8(input)?;
            let (input, third_byte) = nom::number::complete::le_u8(input)?;
            Ok((
                input,
                (((third_byte as u32) << 16 ^ (second_byte as u32) << 8 ^ first_byte) >> 2),
            ))
        }
        _ => {
            // TODO: determine if should be unrecoverable (I'm guessing not)
            Err(nom::Err::Error(CustomError::InvalidVsvalValueType))
        }
    }
}
