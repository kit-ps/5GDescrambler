use crate::bit_vector::BitVector;

pub const POSITIVE_DEFAULT_LLR: f32 = 20.0;

#[derive(Debug)]
pub struct LLRVector<'a, const LEN: usize> {
    pub data: &'a [f32; LEN],
}

impl<'a, const LEN: usize> LLRVector<'a, LEN> {
    pub fn from_data(data: &'a [f32; LEN]) -> Self {
        Self { data }
    }

    pub fn from_f32_slice(data: &'a [f32]) -> Option<Self> {
        let data: &[f32; LEN] = data[..LEN].try_into().ok()?;
        Some(Self {
            data
        })
    }

    pub fn to_bits<const LANES: usize>(&self) -> BitVector<LANES, LEN> {
        let mut ret = BitVector::new();
        for i in 0..LEN {
            if self.data[i] < 0f32 {
                ret.set_bit(i, 1);
            }
        }
        ret
    }

    pub fn get(&self, i: usize) -> f32 {
        self.data[i]
    }

    pub fn get_avg_signal_strength(&self) -> f32 {
        self.data.iter().map(|x: &f32| x.abs()).sum::<f32>() / LEN as f32
    }
}

/**
Maps the bits in the vector to default LLR values. Maps to -POSITIVE_DEFAULT_LLR on 1 and POSITIVE_DEFAULT_LLR on 0.
*/
pub fn from_bits<const LANES: usize, const LEN: usize>(vec: &BitVector<LANES, LEN>) -> [f32; LEN] {
    let mut data = [0f32; LEN];
    for i in 0..LEN {
        data[i] = if vec.get_bit(i) == 1 {
            -POSITIVE_DEFAULT_LLR
        } else {
            POSITIVE_DEFAULT_LLR
        };
    }
    data
}

pub fn from_bytes<const LEN: usize>(d: &[u8]) -> [f32; LEN] {
    let mut data = [0f32; LEN];
    for i in 0..LEN {
        data[i] = if d[i] == 1 {
            -POSITIVE_DEFAULT_LLR
        } else {
            POSITIVE_DEFAULT_LLR
        };
    }
    data
}


