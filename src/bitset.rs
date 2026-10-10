//! A fixed-size set of small integers, used for per-execution rule state.

#[derive(Debug, Clone)]
pub(crate) struct BitSet {
    words: Vec<u64>,
    len: usize,
}

impl BitSet {
    /// A set over `0..len`, initially containing every value if `full`.
    pub fn new(len: usize, full: bool) -> Self {
        let mut words = vec![if full { u64::MAX } else { 0 }; len.div_ceil(64)];
        if full && !len.is_multiple_of(64) {
            *words.last_mut().unwrap() = (1u64 << (len % 64)) - 1;
        }
        BitSet { words, len }
    }

    pub fn contains(&self, i: usize) -> bool {
        self.words[i / 64] & (1 << (i % 64)) != 0
    }

    pub fn insert(&mut self, i: usize) {
        self.words[i / 64] |= 1 << (i % 64);
    }

    pub fn remove(&mut self, i: usize) {
        self.words[i / 64] &= !(1 << (i % 64));
    }

    /// The smallest member that is `>= start`.
    pub fn next_from(&self, start: usize) -> Option<usize> {
        if start >= self.len {
            return None;
        }
        let mut w = start / 64;
        let mut word = self.words[w] & (u64::MAX << (start % 64));
        loop {
            if word != 0 {
                return Some(w * 64 + word.trailing_zeros() as usize);
            }
            w += 1;
            if w == self.words.len() {
                return None;
            }
            word = self.words[w];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_and_search() {
        let mut set = BitSet::new(130, false);
        assert_eq!(set.next_from(0), None);
        for i in [3, 64, 129] {
            set.insert(i);
        }
        assert_eq!(set.next_from(0), Some(3));
        assert_eq!(set.next_from(4), Some(64));
        assert_eq!(set.next_from(65), Some(129));
        assert_eq!(set.next_from(130), None);
        set.remove(64);
        assert!(!set.contains(64) && set.contains(129));
        assert_eq!(set.next_from(4), Some(129));

        let full = BitSet::new(130, true);
        assert!((0..130).all(|i| full.contains(i)));
        assert_eq!(full.next_from(129), Some(129));
        assert_eq!(BitSet::new(0, true).next_from(0), None);
    }
}
