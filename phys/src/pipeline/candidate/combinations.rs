use core::iter::ExactSizeIterator;
use rayon::iter::plumbing::*;
use rayon::prelude::*;

pub const MAX_REALISTIC_N: usize = 120;
pub const MAX_REALISTIC_K: usize = 16 * 6;

pub const BINOMINAL_COEFFICIENTS: BinomTable<MAX_REALISTIC_N, { MAX_REALISTIC_N + 1 }, MAX_REALISTIC_K, { MAX_REALISTIC_K + 1 }> = BinomTable::new();

pub struct BinomTable<const N: usize, const N_PLUS_1: usize, const K: usize, const K_PLUS_1: usize> {
    table: [[u128; K_PLUS_1]; N_PLUS_1],
}

impl<const N: usize, const N_PLUS_1: usize, const K: usize, const K_PLUS_1: usize> Default for BinomTable<N, N_PLUS_1, K, K_PLUS_1> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, const N_PLUS_1: usize, const K: usize, const K_PLUS_1: usize> BinomTable<N, N_PLUS_1, K, K_PLUS_1> {
    pub const fn new() -> Self {
        let mut table = [[0u128; K_PLUS_1]; N_PLUS_1];

        let mut n = 0;
        while n <= N {
            table[n][0] = 1;

            let max_k = if n < K { n } else { K };
            let mut k = 1;
            while k <= max_k {
                if k == n {
                    table[n][k] = 1;
                } else {
                    table[n][k] = table[n - 1][k - 1] + table[n - 1][k];
                }
                k += 1;
            }

            n += 1;
        }

        Self { table }
    }

    #[inline(always)]
    pub fn get(&self, n: usize, k: usize) -> u128 {
        self.table[n][k]
    }
}

#[inline(always)]
fn unrank<const K: usize>(
    mut rank: u128,
    n: usize,
    k: usize,
    out: &mut [usize; K],
) {
    let mut x = 0;
    let mut remaining = k;

    for i in 0..k {
        while x < n {
            let c = BINOMINAL_COEFFICIENTS.get(n - x - 1, remaining - 1);
            if rank < c {
                break;
            }
            rank -= c;
            x += 1;
        }
        out[i] = x;
        x += 1;
        remaining -= 1;
    }
}

#[derive(Clone)]
pub struct Combinations<const MAX_K: usize> {
    k: usize,
    n: usize,
    current: u128,
    end: u128,
}

impl<const MAX_K: usize> Combinations<MAX_K> {
    pub fn new(k: usize, mut n: usize) -> Self {
        assert!(k <= MAX_K);
        assert!(k <= MAX_REALISTIC_K);
        if n > MAX_REALISTIC_N {
            // We have an unlikely big n (that is impossible to brute force combination wise anyway)
            n = MAX_REALISTIC_N;
        }
        if k > n {
            Self {
                k,
                n,
                current: 0,
                end: 0,
            }
        } else {
            let total = BINOMINAL_COEFFICIENTS.get(n, k);
            Self {
                k,
                n,
                current: 0,
                end: total,
            }
        }
    }

    pub fn reset(&mut self) {
        self.current = 0;
    }
}

impl<const MAX_K: usize> Iterator for Combinations<MAX_K> {
    type Item = [usize; MAX_K];

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.end {
            return None;
        }

        let mut result = [0; MAX_K];
        unrank(self.current, self.n, self.k, &mut result);
        self.current += 1;
        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.end - self.current) as usize;
        (len, Some(len))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.current = self.current.saturating_add(n as u128);
        self.next()
    }
}

impl<const MAX_K: usize> ExactSizeIterator for Combinations<MAX_K> {
    fn len(&self) -> usize {
        (self.end - self.current) as usize
    }
}


pub struct ParCombinations<const MAX_K: usize> {
    k: usize,
    n: usize,
    start: u128,
    end: u128,
}

impl<const MAX_K: usize> IntoParallelIterator for Combinations<MAX_K> {
    type Iter = ParCombinations<MAX_K>;
    type Item = [usize; MAX_K];

    fn into_par_iter(self) -> Self::Iter {
        ParCombinations {
            k: self.k,
            n: self.n,
            start: self.current,
            end: self.end,
        }
    }
}


impl<const MAX_K: usize> ParallelIterator for ParCombinations<MAX_K> {
    type Item = [usize; MAX_K];

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        let producer = ParCombinations {
            k: self.k,
            n: self.n,
            start: self.start,
            end: self.end,
        };
        bridge_unindexed(producer, consumer)
    }

    fn opt_len(&self) -> Option<usize> {
        Some((self.end - self.start) as usize)
    }
}

impl<const MAX_K: usize> UnindexedProducer for ParCombinations<MAX_K> {
    type Item = [usize; MAX_K];

    fn split(self) -> (Self, Option<Self>) {
        let len = self.end - self.start;
        if len <= 1 {
            return (self, None);
        }

        let mid = self.start + len / 2;

        (
            ParCombinations {
                k: self.k,
                n: self.n,
                start: self.start,
                end: mid,
            },
            Some(ParCombinations {
                k: self.k,
                n: self.n,
                start: mid,
                end: self.end,
            }),
        )
    }

    fn fold_with<F>(self, mut folder: F) -> F
    where
        F: Folder<Self::Item>,
    {
        let mut buffer = [0usize; MAX_K];
        let mut rank = self.start;

        while rank < self.end && !folder.full() {
            unrank(rank, self.n, self.k, &mut buffer);
            folder = folder.consume(buffer);
            rank += 1;
        }

        folder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::LinkedList;

    #[test]
    pub fn test_combinations() {
        let mut it = Combinations::<3>::new(3, 5);

        assert_eq!(it.len(), 10);

        assert_eq!(it.next(), Some([0, 1, 2]));
        assert_eq!(it.next(), Some([0, 1, 3]));
        assert_eq!(it.next(), Some([0, 1, 4]));
        assert_eq!(it.next(), Some([0, 2, 3]));
        assert_eq!(it.next(), Some([0, 2, 4]));
        assert_eq!(it.next(), Some([0, 3, 4]));

        assert_eq!(it.next(), Some([1, 2, 3]));
        assert_eq!(it.next(), Some([1, 2, 4]));
        assert_eq!(it.next(), Some([1, 3, 4]));

        assert_eq!(it.next(), Some([2, 3, 4]));

        assert_eq!(it.next(), None);

        it.reset();

        assert_eq!(it.next(), Some([0, 1, 2]));
        assert_eq!(it.nth(3), Some([0, 2, 4]));
    }

    #[test]
    pub fn test_par_combinations() {
        let it = Combinations::<3>::new(3, 5).into_par_iter();
        let col = it.collect::<LinkedList<_>>();
        let mut it = col.iter();

        assert_eq!(it.len(), 10);

        assert_eq!(it.next(), Some(&[0, 1, 2]));
        assert_eq!(it.next(), Some(&[0, 1, 3]));
        assert_eq!(it.next(), Some(&[0, 1, 4]));
        assert_eq!(it.next(), Some(&[0, 2, 3]));
        assert_eq!(it.next(), Some(&[0, 2, 4]));
        assert_eq!(it.next(), Some(&[0, 3, 4]));

        assert_eq!(it.next(), Some(&[1, 2, 3]));
        assert_eq!(it.next(), Some(&[1, 2, 4]));
        assert_eq!(it.next(), Some(&[1, 3, 4]));

        assert_eq!(it.next(), Some(&[2, 3, 4]));

        assert_eq!(it.next(), None);
    }
}
