use crate::grid::numerology::{BASE_SCS, NUM_NUMEROLOGIES};
use std::fmt::{Debug, Formatter};

const NUMEROLOGY_ORDER: [(u64, bool); NUM_NUMEROLOGIES] = [
    (60_000, true),
    (15_000, false),
    (30_000, false),
    (60_000, false),
    (120_000, false),
    (240_000, false),
    (480_000, false),
    (960_000, false),
];

pub struct NumerologyKeyed<T> {
    len: usize,
    data: [Option<T>; NUM_NUMEROLOGIES],
}

impl<T> NumerologyKeyed<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            len: 0,
            data: [const { None }; NUM_NUMEROLOGIES],
        }
    }

    #[inline]
    pub fn get_by_scs(&self, scs: u64, extended: bool) -> Option<&T> {
        match &self.data[Self::get_index(scs, extended)] {
            Some(x) => Some(x),
            None => None
        }
    }

    #[inline]
    pub fn get_by_scs_mut(&mut self, scs: u64, extended: bool) -> Option<&mut T> {
        match &mut self.data[Self::get_index(scs, extended)] {
            Some(x) => Some(x),
            None => None
        }
    }

    #[inline]
    pub fn set_by_scs(&mut self, scs: u64, extended: bool, data: T) {
        let ind = Self::get_index(scs, extended);
        if self.data[ind].is_none() {
            self.len += 1;
        }
        self.data[ind] = Some(data);
    }

    #[inline]
    pub fn get_index(scs: u64, extended: bool) -> usize {
        match extended {
            true => 0,
            false => (scs / BASE_SCS) as usize
        }
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item=&T> {
        self.data.iter().filter(|x| x.is_some()).map(|x| unsafe {
            // Safety: This is safe, since the iterator is previously filtered for only Some(_) values
            x.as_ref().unwrap_unchecked()
        })
    }

    #[inline]
    pub fn iter_with_numerology(&self) -> impl Iterator<Item=(u64, bool, &T)> {
        self.data.iter().zip(NUMEROLOGY_ORDER).filter(|(x, _)| x.is_some()).map(|(x, (scs, ext))| (scs, ext, unsafe {
            // Safety: This is safe, since the iterator is previously filtered for only Some(_) values
            x.as_ref().unwrap_unchecked()
        }))
    }

    #[inline]
    pub fn iter_with_numerology_mut(&mut self) -> impl Iterator<Item=(u64, bool, &mut T)> {
        self.data.iter_mut().zip(NUMEROLOGY_ORDER).filter(|(x, _)| x.is_some()).map(|(x, (scs, ext))| (scs, ext, unsafe {
            // Safety: This is safe, since the iterator is previously filtered for only Some(_) values
            x.as_mut().unwrap_unchecked()
        }))
    }

    #[inline]
    pub fn iter_numerologies(&self) -> impl Iterator<Item=(u64, bool)> {
        self.data.iter().zip(NUMEROLOGY_ORDER).filter(|(x, _)| x.is_some()).map(|(_, (scs, ext))| (scs, ext))
    }

    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item=&mut T> {
        self.data.iter_mut().filter(|x| x.is_some()).map(|x| unsafe {
            // Safety: This is safe, since the iterator is previously filtered for only Some(_) values
            x.as_mut().unwrap_unchecked()
        })
    }

    #[inline]
    pub fn into_iter(self) -> impl Iterator<Item=T> {
        self.data.into_iter().filter(|x| x.is_some()).map(|x| unsafe {
            // Safety: This is safe, since the iterator is previously filtered for only Some(_) values
            x.unwrap_unchecked()
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, ind: usize) -> &T {
        unsafe {
            self.data.get_unchecked(ind).as_ref().unwrap_unchecked()
        }
    }
}

impl<T: Debug> Debug for NumerologyKeyed<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("NumerologyKeyed {{ {} }}", self.iter().map(|x| format!("{:?}", x)).collect::<Vec<String>>().join(", ")))
    }
}

impl<T> FromIterator<(u64, bool, T)> for NumerologyKeyed<T> {
    fn from_iter<A: IntoIterator<Item=(u64, bool, T)>>(iter: A) -> Self {
        let mut ret = Self::new();
        for (scs, ext, val) in iter {
            ret.set_by_scs(scs, ext, val);
        }
        ret
    }
}