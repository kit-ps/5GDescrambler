use bit_types::bit_matrix::BitMatrix;

// Bubble sort implementation taken from https://www.reddit.com/r/rust/comments/qw18oa/can_i_sort_an_array_at_compile_time/
pub const fn sort_arr<const N: usize>(mut arr: [usize; N]) -> [usize; N] {
    loop {
        let mut swapped = false;
        let mut i = 1;
        while i < arr.len() {
            if arr[i - 1] > arr[i] {
                let left = arr[i - 1];
                let right = arr[i];
                arr[i - 1] = right;
                arr[i] = left;
                swapped = true;
            }
            i += 1;
        }
        if !swapped {
            break;
        }
    }
    arr
}

pub const fn get_unity_matrix<const LANES: usize, const N: usize>() -> BitMatrix<LANES, LANES, N, N>
{
    let mut ret = BitMatrix::new();

    let mut n = 0;
    while n < N {
        ret.set_bit(n, n, 1);
        n += 1;
    }

    ret
}
