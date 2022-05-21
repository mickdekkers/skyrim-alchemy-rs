use encoding_rs::WINDOWS_1252;

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
        // See https://en.uesp.net/wiki/Skyrim_Mod:File_Format_Conventions#Data_Types
        // zstrings are null terminated, so we exclude the null
        parse_string(&data[..len - 1])
    }
}

pub fn parse_lstring(data: &[u8], is_localized: bool) -> String {
    // FIXME: impl strings table lookups
    assert_eq!(is_localized, false);

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
