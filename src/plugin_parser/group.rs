use std::io::{BufRead, Seek};

use encoding_rs::WINDOWS_1252;
use nom::bytes::complete::{tag, take};
use nom::combinator::{all_consuming, map, peek};
use nom::multi::length_data;
use nom::number::complete::le_u32;
use nom::sequence::{delimited, tuple};
use nom::IResult;

// use crate::error::Error;
use esplugin::record::Record;
use esplugin::record_id::RecordId;
use esplugin::GameId;

const GROUP_TYPE: &[u8] = b"GRUP";

/// Skyrim group header length. See https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format#File_Format
const GROUP_HEADER_LENGTH: u8 = 24;

/// Skyrim group header length to skip after the interesting bits
const GROUP_HEADER_LENGTH_TO_SKIP: u8 = 12;

const RECORD_TYPE_LENGTH: usize = 4;
pub type RecordType = [u8; 4];

#[derive(Clone, PartialEq, Eq, Debug, Hash, Default)]
pub struct GroupHeader {
    pub size_of_group_records: u32,
    pub label: RecordType,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum GroupRecord {
    Group(Group),
    Record(Record),
}

#[derive(Clone, PartialEq, Eq, Debug, Hash, Default)]
pub struct Group {
    pub header: GroupHeader,
    pub group_records: Vec<GroupRecord>,
}

impl Group {
    pub fn parse(
        input: &[u8],
        skip_group_records: fn(RecordType) -> bool,
    ) -> IResult<&[u8], Group> {
        group(input, skip_group_records)
    }
}

fn group(input: &[u8], skip_group_records: fn(RecordType) -> bool) -> IResult<&[u8], Group> {
    let (remaining_input, header) = group_header(input)?;
    let (remaining_input, group_records_data) =
        take(header.size_of_group_records)(remaining_input)?;

    let group_records: Vec<GroupRecord> = if !skip_group_records(header.label) {
        parse_group_records(group_records_data, skip_group_records)?.1
    } else {
        Vec::new()
    };

    Ok((
        remaining_input,
        Group {
            header,
            group_records,
        },
    ))
}

fn parse_group_records(
    input: &[u8],
    skip_group_records: fn(RecordType) -> bool,
) -> IResult<&[u8], Vec<GroupRecord>> {
    let mut input1 = input;

    // TODO: size this?
    let mut group_records: Vec<GroupRecord> = Vec::new();
    while !input1.is_empty() {
        group_records.push({
            let (_, next_type) = peek(take(GROUP_TYPE.len()))(input1)?;
            if next_type == GROUP_TYPE {
                let (input2, group) = group(input1, skip_group_records)?;
                input1 = input2;
                GroupRecord::Group(group)
            } else {
                let (input2, record) = Record::parse(input1, GameId::SkyrimSE, false)?;
                input1 = input2;
                GroupRecord::Record(record)
            }
        })
    }

    Ok((input1, group_records))
}

fn record_type(input: &[u8]) -> IResult<&[u8], RecordType> {
    map(take(RECORD_TYPE_LENGTH), |s: &[u8]| {
        s.try_into()
            .expect("record type slice should be the required length")
    })(input)
}

fn group_header(input: &[u8]) -> IResult<&[u8], GroupHeader> {
    map(
        tuple((
            tag(GROUP_TYPE),
            le_u32,
            record_type,
            take(GROUP_HEADER_LENGTH_TO_SKIP),
        )),
        |(_, group_size, group_label, _)| GroupHeader {
            size_of_group_records: group_size - u32::from(GROUP_HEADER_LENGTH),
            label: group_label,
        },
    )(input)
}
