use crate::gold_sequence::GOLD_SEQUENCE_INPUT_SIZE;
use crate::polar::DCI_CRC_BITS;
use bit_types::bit_vector::BitVector;

// Syndrome mismatch is sadly highly dense, so we can only say whether there is at least an error, or no error at all.
pub fn syndrome_match<const LANES: usize, const LEN: usize, const A: usize>(
    vec: &BitVector<LANES, LEN>,
) -> bool {
    let relevant = vec.and(&get_syndrome_relevant::<LANES, LEN, A>());
    let errors = relevant.xor(&get_syndrome_ones::<LANES, LEN, A>());
    errors.is_zero()
}

/**
Returns a vector with 1 in every position that is relevant for the syndrome
*/
pub const fn get_syndrome_relevant<const LANES: usize, const LEN: usize, const A: usize>()
    -> BitVector<LANES, LEN> {
    let mut ret = BitVector::new();
    let mut i = 0;
    while i < DCI_CRC_BITS {
        ret.set_bit(i, 1);
        i += 1;
    }
    ret.set_bit(DCI_CRC_BITS + A, 1);
    // -1 for one eliminated (free) bit in syndrome
    i = DCI_CRC_BITS + A + GOLD_SEQUENCE_INPUT_SIZE - 1;
    while i < LEN {
        ret.set_bit(i, 1);
        i += 1;
    }
    ret
}

pub const fn get_syndrome_ones<const LANES: usize, const LEN: usize, const A: usize>()
    -> BitVector<LANES, LEN> {
    let mut ret = BitVector::new();
    let mut i = 0;
    while i < DCI_CRC_BITS {
        ret.set_bit(i, 1);
        i += 1;
    }
    ret.set_bit(DCI_CRC_BITS + A, 1);
    ret
}
