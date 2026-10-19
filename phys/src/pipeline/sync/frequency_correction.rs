/// In order to keep cyclic prefix vs symbol duration at integer positions, we need to ensure that the sampling
/// frequency uses a multiple of symbol sample size.
/// The reference symbol duration is 144 or 160 samples, or 512 samples for extended prefix respectively.
/// All are divisible by 16, same with the reference symbol duration of 2048 samples (see TS 38.211 5.3.1).
/// Calculate samples for 7 symbols:
/// Normal CP: 144 * 7 + 16 + 2048 * 7 = 15360 => all parts divisible by 16 => 960
/// Extended CP: 512 * 7 + 2048 * 7 = 17920 => all parts divisible by 16 => 1120
/// For FR1 we need to find the least common multiple of 960 and 30 KHz: 120000
/// For FR2 (normal CP) we need to find the least common multiple of 960 and 120 KHz: 120000
/// For FR2 (extended CP) we need to find the least common multiple of 1120 and 60 KHz: 420000
const NORMAL_CP_SAMPLING_COMPOUND: usize = 120000;
const EXTENDED_CP_SAMPLING_COMPOUND: usize = 420000;

pub fn to_native_frequency(input_freq: usize, extended_cp: bool) -> usize {
    // We simply round to the nearest multiple of scs
    input_freq - input_freq % match extended_cp {
        true => EXTENDED_CP_SAMPLING_COMPOUND,
        false => NORMAL_CP_SAMPLING_COMPOUND
    }
}