use std::num::NonZeroU32;

use anyhow::anyhow;
use encoding_rs::WINDOWS_1252;

use super::strings_table::StringsTable;

pub fn parse_string(data: &[u8]) -> String {
    WINDOWS_1252
        .decode_without_bom_handling_and_without_replacement(data)
        .map(|s| Some(s.to_string()))
        // FIXME: may panic
        .unwrap()
        .unwrap()
}

pub fn parse_zstring(data: &[u8]) -> String {
    let len = data.len();
    if len < 2 {
        String::from("")
    } else {
        // zstrings are null terminated strings
        // See https://en.uesp.net/wiki/Skyrim_Mod:File_Format_Conventions#Data_Types

        // TODO: can probably avoid iterating over the string data twice
        let null_index = data
            .iter()
            .enumerate()
            .find_map(|(index, byte)| match byte == &b'\0' {
                true => Some(index),
                false => None,
            })
            .expect("expected null terminated string to contain null");
        parse_string(&data[..null_index])
    }
}

pub fn parse_lstring(
    data: &[u8],
    is_localized: bool,
    strings_table: &Option<StringsTable>,
) -> String {
    if is_localized {
        let strings_table = strings_table
            .as_ref()
            .expect("missing strings table for localized plugin");

        let id = le_slice_to_u32(data);
        return strings_table.get(id).unwrap_or_else(|| String::from(""));
    }

    // All lstrings are zstrings when not localized
    // See https://en.uesp.net/wiki/Skyrim_Mod:File_Format_Conventions#Data_Types
    parse_zstring(data)
}

pub fn le_slice_to_u32(input: &[u8]) -> u32 {
    let int_bytes = &input[..std::mem::size_of::<u32>()];
    u32::from_le_bytes(
        int_bytes
            .try_into()
            .expect("slice to contain enough bytes to read a u32"),
    )
}

pub fn nom_err_to_anyhow_err<E>(err: nom::Err<E>) -> anyhow::Error
where
    E: std::fmt::Debug,
{
    anyhow::anyhow!(err.to_string())
}

pub fn split_form_id<FnGetMaster>(
    id: NonZeroU32,
    get_master: FnGetMaster,
) -> Result<(String, u32), anyhow::Error>
where
    FnGetMaster: Fn(NonZeroU32) -> Option<String>,
{
    // See https://en.uesp.net/wiki/Skyrim:Form_ID
    let mod_name =
        get_master(id).ok_or_else(|| anyhow!("record has invalid master reference in form ID"))?;
    // The last six hex digits are the ID of the record itself
    let id = u32::from(id) & 0x00FFFFFF;

    Ok((mod_name, id))
}
