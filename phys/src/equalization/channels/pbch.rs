const DMRS_THREE: [[usize; 72]; 4] = [
    compute_dmrs_indexes_three(0),
    compute_dmrs_indexes_three(1),
    compute_dmrs_indexes_three(2),
    compute_dmrs_indexes_three(3)
];

const DMRS_FULL: [[usize; 144]; 4] = [
    compute_dmrs_indexes_full(0),
    compute_dmrs_indexes_full(1),
    compute_dmrs_indexes_full(2),
    compute_dmrs_indexes_full(3)
];

const DATA_THREE: [[usize; 72 * 3]; 4] = [
    compute_data_indexes_three(0),
    compute_data_indexes_three(1),
    compute_data_indexes_three(2),
    compute_data_indexes_three(3)
];

const DATA_FULL: [[usize; 144 * 3]; 4] = [
    compute_data_indexes_full(0),
    compute_data_indexes_full(1),
    compute_data_indexes_full(2),
    compute_data_indexes_full(3)
];

pub const PBCH_NUM_SUBCARRIERS: usize = 240;
pub const PBCH_NUM_SYMBOLS: usize = 3;

const fn compute_dmrs_indexes_three(v: usize) -> [usize; 72] {
    // Offset of 48
    // Per row add all indexes times 4 plus offset v
    // For second row (index / 36 = 1) add two rows of offset in time
    let mut ret: [usize; 72] = [0; 72];
    let mut i = 0;
    while i < 36 {
        ret[i] = 48 + i * 4 + v;
        i += 1;
    }
    i = 0;
    while i < 36 {
        ret[i + 36] = 240 * 2 + 48 + i * 4 + v;
        i += 1;
    }
    ret
}

const fn compute_dmrs_indexes_full(v: usize) -> [usize; 144] {
    let mut ret: [usize; 144] = [0; 144];
    let mut index = 0;
    while index < 60 {
        ret[index] = index * 4 + v;
        index += 1;
    }
    while index < 72 {
        ret[index] = 240 + (index - 60) * 4 + v;
        index += 1;
    }
    while index < 84 {
        ret[index] = 240 + 192 + (index - 72) * 4 + v;
        index += 1;
    }
    while index < 144 {
        ret[index] = 240 * 2 + (index - 84) * 4 + v;
        index += 1;
    }
    ret
}

pub fn get_pbch_dmrs_indexes(cell_id: u16, three_mhz: bool) -> &'static [usize] {
    let v: usize = (cell_id & 0x3) as usize;
    if three_mhz {
        &DMRS_THREE[v]
    } else {
        &DMRS_FULL[v]
    }
}

const fn compute_data_indexes_three(v: usize) -> [usize; 72 * 3] {
    let mut ret: [usize; 72 * 3] = [0; 72 * 3];
    let mut i = 0;
    let mut j = 0;
    while i < 36 * 3 {
        if j % 4 == v {
            j += 1;
            continue;
        }
        ret[i] = 48 + j;
        i += 1;
        j += 1;
    }
    i = 0;
    j = 0;
    while i < 36 * 3 {
        if j % 4 == v {
            j += 1;
            continue;
        }
        ret[i + 36 * 3] = 240 * 2 + 48 + j;
        i += 1;
        j += 1;
    }
    ret
}

const fn compute_data_indexes_full(v: usize) -> [usize; 144 * 3] {
    let mut ret: [usize; 144 * 3] = [0; 144 * 3];
    let mut i = 0;
    let mut j = 0;
    while i < 60 * 3 {
        if j % 4 == v {
            j += 1;
            continue;
        }
        ret[i] = j;
        i += 1;
        j += 1;
    }
    i = 0;
    j = 0;
    while i < 12 * 3 {
        if j % 4 == v {
            j += 1;
            continue;
        }
        ret[i + 60 * 3] = 240 + j;
        i += 1;
        j += 1;
    }
    i = 0;
    j = 0;
    while i < 12 * 3 {
        if j % 4 == v {
            j += 1;
            continue;
        }
        ret[i + 72 * 3] = 240 + 192 + j;
        i += 1;
        j += 1;
    }
    i = 0;
    j = 0;
    while i < 60 * 3 {
        if j % 4 == v {
            j += 1;
            continue;
        }
        ret[i + 84 * 3] = 240 * 2 + j;
        i += 1;
        j += 1;
    }
    ret
}

pub fn get_pbch_data_indexes(cell_id: u16, three_mhz: bool) -> &'static [usize] {
    let v: usize = (cell_id & 0x3) as usize;
    if three_mhz {
        &DATA_THREE[v]
    } else {
        &DATA_FULL[v]
    }
}

pub fn get_pbch_pilot_symbols(three_mhz: bool) -> &'static [usize] {
    if three_mhz {
        &[0, 2]
    } else {
        &[0, 1, 2]
    }
}