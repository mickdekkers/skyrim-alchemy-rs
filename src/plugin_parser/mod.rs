// TODO: should return easily mergeable/updatable struct of all ingredients and magic effects. See https://github.com/cguebert/SkyrimAlchemyHelper/tree/master/libs/modParser
// See https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format

use encoding_rs::WINDOWS_1252;
use esplugin::record::Record;
use nom::IResult;

mod group;

fn record_type_to_string(data: &[u8; 4]) -> String {
    WINDOWS_1252
        .decode_without_bom_handling_and_without_replacement(data)
        .map(|s| Some(s.to_string()))
        // FIXME: may panic
        .unwrap()
        .unwrap()
}

fn le_slice_to_u32(input: &[u8]) -> u32 {
    let int_bytes = &input[..std::mem::size_of::<u32>()];
    u32::from_le_bytes(
        int_bytes
            .try_into()
            .expect("slice to contain enough bytes to read a u32"),
    )
}

fn _parse_plugin<'a>(input: &'a [u8]) -> IResult<&[u8], ()> {
    let (remaining_input, header_record) =
        Record::parse(&input, esplugin::GameId::SkyrimSE, false)?;

    println!("header_record: {:#?}", header_record);

    const COUNT_OFFSET: usize = 4;
    let record_and_group_count = header_record
        .subrecords()
        .iter()
        .find(|s| s.subrecord_type() == b"HEDR" && s.data().len() > COUNT_OFFSET)
        .map(|s| le_slice_to_u32(&s.data()[COUNT_OFFSET..]));

    println!("record_and_group_count: {:#?}", record_and_group_count);
    // let (input2, record_ids) = parse_record_ids(input1, game_id, &header_record, filename)?;

    let skip_group_records = |label| match &label {
        // We're only interested in ingredients and magic effects.
        b"INGR" | b"MGEF" => false,
        _ => true,
    };

    let mut interesting_groups = Vec::new();
    let mut input1 = remaining_input;
    while !input1.is_empty() {
        let (input2, group) = group::Group::parse(input1, skip_group_records)?;
        if group.group_records.len() > 0 {
            interesting_groups.push(group);
        }
        input1 = input2;
    }

    println!("interesting_groups: {:#?}", interesting_groups);
    println!("interesting_groups length: {:#?}", interesting_groups.len());

    // TODO: convert to more useful representation

    // println!(
    //     "first group label: {:#?}",
    //     record_type_to_string(&first_group.header.label)
    // );
    // first_group.header.label
    // Ok((
    //     input2,
    //     PluginData {
    //         header_record,
    //         record_ids,
    //     },
    // ))
    Ok((remaining_input, ()))
}

pub fn parse_plugin<'a>(input: &'a [u8]) -> Result<(), anyhow::Error> {
    Ok(_parse_plugin(input)
        .map_err(|err| anyhow::anyhow!(err.to_string()))?
        .1)
}
