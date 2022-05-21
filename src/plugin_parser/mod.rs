// TODO: should return easily mergeable/updatable struct of all ingredients and magic effects. See https://github.com/cguebert/SkyrimAlchemyHelper/tree/master/libs/modParser
// See https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format

use esplugin::record::Record;

mod group;

pub fn parse_plugin<'a>(input: &'a [u8]) -> Result<(), anyhow::Error> {
    let (remaining_input, header_record) = Record::parse(&input, esplugin::GameId::SkyrimSE, false)
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    println!("{:#?}", header_record);

    // if load_header_only {
    //     return Ok((
    //         input1,
    //         PluginData {
    //             header_record,
    //             record_ids: RecordIds::None,
    //         },
    //     ));
    // }

    // let (input2, record_ids) = parse_record_ids(input1, game_id, &header_record, filename)?;

    // Ok((
    //     input2,
    //     PluginData {
    //         header_record,
    //         record_ids,
    //     },
    // ))
    Ok(())
}
