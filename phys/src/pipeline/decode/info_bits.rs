use three_gpp::info_bit::get_info_bit_pattern;
use three_gpp::rate_matching::get_rate_matching_mode;
use three_gpp::sequence::get_sequence_pattern;

pub const fn get_frozen_bit_pattern<const K: usize, const N: usize, const E: usize>() -> [bool; N] {
    let mode = get_rate_matching_mode(K, N, E);
    let info = get_sequence_pattern::<N>();
    let info_bits = get_info_bit_pattern::<K, N, E>(mode, info);
    let mut frozen = [true; N];
    let mut i = 0;
    while i < K {
        frozen[info_bits[i]] = false;
        i += 1;
    }
    frozen
}
