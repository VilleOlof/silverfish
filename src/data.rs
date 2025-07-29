//! `data` contains functions to decode & encode the packed i64 arrays that is used for blocks/biomes data.  

use simdnbt::owned::{NbtCompound, NbtTag};

/// Takes in an data and some other metadata and writes the changes to the state's "data" field.  
pub(crate) fn encode_data<const N: usize>(
    bit_count: u32,
    data: &[i64; N],
    data_len: usize,
    state: &mut NbtCompound,
) {
    let mut new_blockdata: Vec<i64> = Vec::with_capacity(N);

    let mut offset = 0;
    let mut currrent_long: i64 = 0;
    for i in 0..data_len {
        currrent_long |= data[i] << (offset * bit_count);
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

// note: we return how many bytes we wrote since i dont know if data is always 4096 no matter the context
// or if the Some() path can be less than 4096, and since data is now a slice
// we instead keep track of how many bytes we are using and only operate on those.
/// Takes in the raw packed long array from the NBT and transforms it into a vec of palette indexes.  
pub(crate) fn decode_data<const N: usize>(
    indexes: &mut [i64; N],
    bit_count: u32,
    data: Option<&[i64]>,
) -> usize {
    // if no data found we directly skip to a pre-defined zeroed vec
    match data {
        Some(data) => {
            let mut offset: u32 = 0;
            let mut index = 0;

            let mask = (1 << bit_count) - 1;
            for data_block in data.iter() {
                while (offset * bit_count) + bit_count <= 64 && index < N {
                    let block = (data_block >> (offset * bit_count)) & mask;

                    indexes[index] = block;

                    offset += 1;
                    index += 1;
                }
                offset = 0;
            }

            index
        }
        None => {
            for i in 0..N {
                indexes[i] = 0;
            }

            N
        }
    }
}
