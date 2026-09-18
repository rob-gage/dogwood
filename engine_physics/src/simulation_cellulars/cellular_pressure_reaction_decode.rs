// Copyright Rob Gage 2026

pub(crate) fn decode_rigid_reaction_fixed_point_record(
    rigid_reaction_record_bytes: &[u8],
    record_offset: usize,
) -> [f32; 4] {
    [
        i32::from_le_bytes(
            rigid_reaction_record_bytes[record_offset..record_offset + 4]
                .try_into()
                .unwrap(),
        ) as f32
            / 65536.0,
        i32::from_le_bytes(
            rigid_reaction_record_bytes[record_offset + 4..record_offset + 8]
                .try_into()
                .unwrap(),
        ) as f32
            / 65536.0,
        i32::from_le_bytes(
            rigid_reaction_record_bytes[record_offset + 8..record_offset + 12]
                .try_into()
                .unwrap(),
        ) as f32
            / 65536.0,
        u32::from_le_bytes(
            rigid_reaction_record_bytes[record_offset + 12..record_offset + 16]
                .try_into()
                .unwrap(),
        ) as f32
            / 256.0,
    ]
}

pub(crate) fn decode_rigid_fractured_slots(
    mapped_bytes: &[u8],
    fractures_offset: u64,
    mapped_size: u64,
) -> Box<[u32]> {
    if mapped_bytes[fractures_offset as usize..mapped_size as usize]
        .as_chunks::<4>()
        .0
        .is_empty()
    {
        return Box::new([]);
    }
    mapped_bytes[fractures_offset as usize..mapped_size as usize]
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .flat_map(|(word_index, fracture_word_bytes)| {
            let fracture_word: u32 = u32::from_le_bytes(*fracture_word_bytes);
            (0..32).filter_map(move |bit_index| {
                ((fracture_word & (1 << bit_index)) != 0)
                    .then_some((word_index as u32) * 32 + bit_index)
            })
        })
        .collect()
}
