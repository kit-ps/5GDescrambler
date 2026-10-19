use rayon::iter::plumbing::{bridge_unindexed, Folder, UnindexedConsumer, UnindexedProducer};
use rayon::iter::ParallelIterator;

pub struct Sequences<const MAX_K: usize> {
    start: usize,
    end: usize,
    k: usize,
}

impl<const MAX_K: usize> Sequences<MAX_K> {
    pub fn new(k: usize, n: usize) -> Self {
        if k > n {
            Self {
                start: 0,
                end: 0,
                k,
            }
        } else {
            Self {
                start: 0,
                end: n - k + 1,
                k,
            }
        }
    }
}

impl<const MAX_K: usize> Iterator for Sequences<MAX_K> {
    type Item = [usize; MAX_K];

    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            None
        } else {
            let mut data = [0; MAX_K];
            for i in 0..self.k {
                data[i] = i + self.start;
            }
            self.start += 1;
            Some(data)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.end, Some(self.end))
    }
}

impl<const MAX_K: usize> ExactSizeIterator for Sequences<MAX_K> {
    fn len(&self) -> usize {
        self.end
    }
}

impl<const MAX_K: usize> ParallelIterator for Sequences<MAX_K> {
    type Item = [usize; MAX_K];

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        bridge_unindexed(self, consumer)
    }

    fn opt_len(&self) -> Option<usize> {
        Some(self.end - self.start)
    }
}

impl<const MAX_K: usize> UnindexedProducer for Sequences<MAX_K> {
    type Item = [usize; MAX_K];

    fn split(self) -> (Self, Option<Self>) {
        let len = self.end - self.start;
        if len <= 1 {
            return (self, None);
        }

        let mid = self.start + len / 2;
        (Sequences {
            start: self.start,
            end: mid,
            k: self.k,
        }, Some(Sequences {
            start: mid,
            end: self.end,
            k: self.k,
        }))
    }

    fn fold_with<F>(self, mut folder: F) -> F
    where
        F: Folder<Self::Item>,
    {
        let mut buffer = [0usize; MAX_K];
        let mut rank = self.start;

        while rank < self.end && !folder.full() {
            for i in 0..self.k {
                buffer[i] = i + self.start;
            }
            folder = folder.consume(buffer);
            rank += 1;
        }

        folder
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequences() {
        let mut seq = Sequences::<6>::new(6, 10);

        assert_eq!(seq.next(), Some([0, 1, 2, 3, 4, 5]));
        assert_eq!(seq.next(), Some([1, 2, 3, 4, 5, 6]));
        assert_eq!(seq.next(), Some([2, 3, 4, 5, 6, 7]));
        assert_eq!(seq.next(), Some([3, 4, 5, 6, 7, 8]));
        assert_eq!(seq.next(), Some([4, 5, 6, 7, 8, 9]));
        assert_eq!(seq.next(), None);
    }
}