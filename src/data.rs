//! `data` contains functions to decode & encode the packed i64 arrays that is used for blocks/biomes data.  

use simdnbt::owned::{NbtCompound, NbtTag};

/// Takes in an data and some other metadata and writes the changes to the state's "data" field.  
pub(crate) fn encode_data(size: usize, bit_count: u32, data: Vec<i64>, state: &mut NbtCompound) {
    let mut new_blockdata: Vec<i64> = Vec::with_capacity(size);

    let mut offset = 0;
    let mut currrent_long: i64 = 0;
    for block in data.iter() {
        currrent_long |= block << (offset * bit_count);
        offset += 1;

        if (offset * bit_count) + bit_count > 64 {
            new_blockdata.push(currrent_long);
            currrent_long = 0;
            offset = 0;
        }
    }

    if offset > 0 {
        new_blockdata.push(currrent_long);
    }

    // store back the data, state is &mut to section
    if !state.contains("data") {
        state.insert("data", NbtTag::LongArray(new_blockdata));
    } else {
        // this unwrap is 100% to exist due to the above check
        *state.long_array_mut("data").unwrap() = new_blockdata;
    }
}

/// Takes in the raw packed long array from the NBT and transforms it into a vec of palette indexes.  
pub(crate) fn decode_data(size: usize, bit_count: u32, data: Option<&[i64]>) -> Vec<i64> {
    // if no data found we directly skip to a pre-defined zeroed vec
    match data {
        Some(data) => {
            let mut old_indexes: Vec<i64> = Vec::with_capacity(size);

            let mut offset: u32 = 0;

            let mask = (1 << bit_count) - 1;
            for data_block in data.iter() {
                while (offset * bit_count) + bit_count <= 64 {
                    let block = (data_block >> (offset * bit_count)) & mask;

                    old_indexes.push(block);

                    offset += 1
                }
                offset = 0;
            }
            old_indexes.truncate(size);

            old_indexes
        }
        None => vec![0; size],
    }
}
