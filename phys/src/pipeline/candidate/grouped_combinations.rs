use crate::pipeline::candidate::combinations::Combinations;
use rayon::prelude::*;

// The following functions are laid out this way due to limitations of the rust type system
// (we can not nest a variable amount of parallel iterators).
// These combinations will always overflow on n - (n % group_size) elements, so it is recommended to only
// attempt on n % group_size == 0 cases.
pub fn grouped_combinations_2<const MAX_K: usize>(k: usize, n: usize) -> impl ParallelIterator<Item=[usize; MAX_K]> {
    const GROUP_SIZE: usize = 2;

    let inner_n = n / GROUP_SIZE;
    let inner_k = k / GROUP_SIZE;

    let first_iter = first_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n);
    let second_iter = offset_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n, 1);

    first_iter.chain(second_iter)
}

pub fn grouped_combinations_3<const MAX_K: usize>(k: usize, n: usize) -> impl ParallelIterator<Item=[usize; MAX_K]> {
    const GROUP_SIZE: usize = 3;

    let inner_n = n / GROUP_SIZE;
    let inner_k = k / GROUP_SIZE;

    let first_iter = first_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n);
    let second_iter = offset_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n, 1);
    let third_iter = offset_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n, 2);

    first_iter.chain(second_iter).chain(third_iter)
}

pub fn grouped_combinations_6<const MAX_K: usize>(k: usize, n: usize) -> impl ParallelIterator<Item=[usize; MAX_K]> {
    const GROUP_SIZE: usize = 6;

    let inner_n = n / GROUP_SIZE;
    let inner_k = k / GROUP_SIZE;

    let first_iter = first_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n);
    let second_iter = offset_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n, 1);
    let third_iter = offset_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n, 2);
    let fourth_iter = offset_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n, 3);
    let fifth_iter = offset_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n, 4);
    let sixth_iter = offset_iter::<MAX_K, GROUP_SIZE>(inner_k, inner_n, 5);

    first_iter.chain(second_iter).chain(third_iter).chain(fourth_iter).chain(fifth_iter).chain(sixth_iter)
}

fn first_iter<const MAX_K: usize, const GROUP_SIZE: usize>(inner_k: usize, inner_n: usize) -> impl ParallelIterator<Item=[usize; MAX_K]> {
    Combinations::<MAX_K>::new(inner_k, inner_n).into_par_iter().map(move |mut combination| {
        for i in (0..inner_k).rev() {
            let copy_to = i * GROUP_SIZE;
            combination[copy_to] = combination[i] * GROUP_SIZE;
            for j in 1..GROUP_SIZE {
                combination[copy_to + j] = combination[copy_to] + j;
            }
        }

        combination
    })
}

fn offset_iter<const MAX_K: usize, const GROUP_SIZE: usize>(inner_k: usize, inner_n: usize, offset: usize) -> impl ParallelIterator<Item=[usize; MAX_K]> {
    Combinations::<MAX_K>::new(inner_k, inner_n).into_par_iter().map(move |mut combination| {
        let ind_offs = if combination[inner_k - 1] == inner_n - 1 {
            // Overflow
            offset
        } else {
            0
        };
        for i in (0..inner_k).rev() {
            let copy_to = i * GROUP_SIZE + ind_offs;
            combination[copy_to] = combination[i] * GROUP_SIZE + 1;
            if i == inner_k - 1 && ind_offs != 0 {
                // Copy all but "offset" last elements
                for j in 1..GROUP_SIZE - offset {
                    combination[copy_to + j] = combination[copy_to] + j;
                }
            } else {
                for j in 1..GROUP_SIZE {
                    combination[copy_to + j] = combination[copy_to] + j;
                }
            }
        }
        // Initialize ind_offs amount of indexes in the front
        for j in 0..ind_offs {
            combination[j] = j;
        }

        combination
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::LinkedList;

    #[test]
    fn test_grouped_combinations() {
        let it = grouped_combinations_2::<6>(6, 9);
        assert_eq!(it.opt_len(), Some(8));

        let res = it.collect::<LinkedList<_>>();
        assert_eq!(res.iter().collect::<Vec<_>>(), vec![&[0, 1, 2, 3, 4, 5], &[0, 1, 2, 3, 6, 7], &[0, 1, 4, 5, 6, 7], &[2, 3, 4, 5, 6, 7], &[1, 2, 3, 4, 5, 6], &[0, 1, 2, 3, 4, 7], &[0, 1, 2, 5, 6, 7], &[0, 3, 4, 5, 6, 7]]);
    }
}