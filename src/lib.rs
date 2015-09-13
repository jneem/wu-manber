// Copyright 2015 Joe Neeman.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms

//! This crate gives an implementation of Wu and Manber's algorithm for finding one of several
//! strings (which we will call "needles") in a much larger string (the "haystack").  This is not
//! to be confused with Wu and Manber's algorithm for fuzzy matching.
//!
//! The Wu-Manber algorithm is very efficient when all of the strings to be matched are long. It
//! requires a pre-processing step with a fair amount of memory overhead -- currently about 32kb in
//! this implementation, but future improvements may reduce that when there are not too many
//! needles.
//!
//! This implementation supports a maximum of 65536 needles, each of which can be at most 65536
//! bytes long. These requirements may be relaxed in the future.
//!
//! # Example
//! ```
//! use wu_manber::{Match, TwoByteWM};
//! let needles = vec!["quick", "brown", "lazy", "wombat"];
//! let haystack = "The quick brown fox jumps over the lazy dog.";
//! let searcher = TwoByteWM::new(&needles);
//! let mat = searcher.find(haystack).next().unwrap();
//! assert_eq!(mat, Match { start: 4, end: 9, pat_idx: 0 });
//! ```

use std::cmp::min;

#[cfg(test)]
extern crate aho_corasick;

/// This is the type for indexing into the bytes of the needles.  Its size determines the maximum
/// length of a needle.
type NByteIdx = u16;

/// This is the type for indexing into the list of needles.  Its size determines the maximum number
/// of needles.
type NeedleIdx = u16;

/// `TwoByteWM` stores the precomputed tables needed for a two-byte-wide implementation of the
/// Wu-Manber algorithm.
///
/// "Two-byte-wide" means that the search phase in the Wu-Manber algorithm uses spans of two bytes
/// to look for potential matches.  This is suitable for moderately sized sets of needles; if there
/// are too many needles then it might be faster to use spans of three bytes (but that isn't yet
/// implemented by this crate).
#[derive(Debug)]
pub struct TwoByteWM {
    /// The needles that we are trying to match against, and their indices.
    ///
    /// Each of the needles has length (in bytes) at least 2.  They are sorted in increasing order
    /// of the hash value of their two critical bytes.
    needles: Vec<(usize, Vec<u8>)>,

    /// For each of the needles above, this contains the first two bytes, concatenated into a
    /// `u16`.
    ///
    /// This `Vec` is indexed in the same way as `needles`.
    prefix: Vec<u16>,

    /// The minimimum length of any needle.
    pat_len: NByteIdx,

    /// If `shift[HashFn(a, b)] = i` then no needle contains the two-byte string `ab` starting
    /// anywhere between positions `pat_len - 2 - i` and `pat_len - 2`.
    ///
    /// Note that because this `Vec` can be quite long, we might save a substantial amount of space
    /// by shrinking the size of `NByteIdx`.
    shift: Vec<NByteIdx>,

    /// If `hash[HashFn(a, b)] = i` then the needles whose critical bytes hash to `HashFn(a, b)`
    /// begin at `needles[i]`.
    ///
    /// Note that because this `Vec` can be quite long, we might save a substantial amount of space
    /// by shrinking the size of `NeedleIdx`.
    hash: Vec<NeedleIdx>,
}

#[derive(Debug, PartialEq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
    pub pat_idx: usize,
}

pub struct Matches<'a, 'b> {
    wm: &'a TwoByteWM,
    haystack: &'b [u8],
    cur_pos: usize,
}

impl<'a, 'b> Iterator for Matches<'a, 'b> {
    type Item = Match;
    fn next(&mut self) -> Option<Match> {
        self.wm.find_from(self.haystack, self.cur_pos).map(|m| { self.cur_pos = m.end; m })
    }
}

/// For now, we default to this hash function (which is the one from the original paper of Wu and
/// Manber). In the future, we may want to look for a better one depending on the needles.
fn hash_fn(a: u8, b: u8) -> NeedleIdx {
    ((a as NeedleIdx) << 5) + (b as NeedleIdx)
}

const HASH_MAX: usize = (0xFFusize << 5) + 0xFF;

impl TwoByteWM {
    fn pat(&self, p_idx: NeedleIdx) -> &[u8] {
        &self.needles[p_idx as usize].1
    }

    fn pat_idx(&self, p_idx: NeedleIdx) -> usize {
        self.needles[p_idx as usize].0
    }

    /// Creates lookup tables to efficiently search for the given needles.
    ///
    /// The order of `needles` is significant, since all `Match`es returned from this `TwoByteWM`
    /// will include an index into `needles` saying which needle matched.
    pub fn new<I, P>(needles: I) -> TwoByteWM
            where P: AsRef<[u8]>, I: IntoIterator<Item=P> {
        let needles: Vec<_> = needles.into_iter().map(|s| s.as_ref().to_vec()).collect();
        if needles.is_empty() {
            panic!("cannot create TwoByteWM from an empty set of needles");
        } else if needles.len() > NeedleIdx::max_value() as usize {
            panic!("too many needles");
        }

        let pat_len = needles.iter().map(|p| p.len()).min().unwrap();
        if pat_len < 2 {
            panic!("all needles must have length (in bytes) at least 2");
        } else if pat_len > NByteIdx::max_value() as usize {
            panic!("these needles are too long");
        }
        let pat_len = pat_len as NByteIdx;

        let h = |p: &[u8]| hash_fn(p[(pat_len-2) as usize], p[(pat_len-1) as usize]);
        let mut needles: Vec<_> = needles.into_iter().enumerate().collect();
        needles.sort_by(|p, q| h(&p.1).cmp(&h(&q.1)));
        let needles = needles;
        let prefix: Vec<_> = needles.iter()
            .map(|p| ((p.1[0] as u16) << 8) + (p.1[1] as u16))
            .collect();

        let mut hash = vec![0; HASH_MAX + 2];
        for (p_idx, &(_, ref p)) in needles.iter().enumerate().rev() {
            let h_idx = h(&p) as usize;
            hash[h_idx] = p_idx as NeedleIdx;
            if hash[h_idx + 1] == 0 {
                hash[h_idx + 1] = p_idx as NeedleIdx + 1;
            }
        }

        let mut shift = vec![pat_len - 1; HASH_MAX + 1];
        for &(_, ref p) in &needles {
            for p_pos in 0..(pat_len - 1) {
                let h = hash_fn(p[p_pos as usize], p[(p_pos + 1) as usize]);
                shift[h as usize] = min(shift[h as usize], pat_len - p_pos - 2);
            }
        }

        TwoByteWM {
            needles: needles,
            prefix: prefix,
            pat_len: pat_len,
            shift: shift,
            hash: hash,
        }
    }

    /// Searches for a single match, starting from the given byte offset.
    pub fn find_from<P>(&self, haystack: P, offset: usize) -> Option<Match> where P: AsRef<[u8]> {
        // `pos` points to the index in `haystack` that we are trying to align against the index
        // `pat_len - 1` of the needles.
        let pat_len = self.pat_len as usize;
        let mut pos = offset + pat_len - 1;
        let haystack = haystack.as_ref();
        while pos <= haystack.len() - 1 {
            let h = hash_fn(haystack[pos - 1], haystack[pos]) as usize;
            let shift = self.shift[h] as usize;
            if shift == 0 {
                // We might have matched the end of some needle.  Iterate over all the needles
                // that we might have matched, and see if they match the beginning.
                let a = haystack[pos - pat_len + 1];
                let b = haystack[pos - pat_len + 2];
                let prefix = ((a as u16) << 8) + (b as u16);
                let mut found: Option<NeedleIdx> = None;
                for p_idx in self.hash[h]..self.hash[h+1] {
                    if self.prefix[p_idx as usize] == prefix {
                        // The prefix matches too, so now check for the full match.
                        let p = self.pat(p_idx);
                        if haystack[(pos - pat_len + 1)..].starts_with(&p) {
                            found = match found {
                                None => Some(p_idx),
                                Some(q_idx) => {
                                    let q = self.pat(q_idx);
                                    Some(if p.len() < q.len() { p_idx } else { q_idx })
                                }
                            }
                        }
                    }
                }
                if let Some(p_idx) = found {
                    return Some(Match {
                        start: pos - pat_len + 1,
                        end: pos - pat_len + 1 + self.pat(p_idx).len(),
                        pat_idx: self.pat_idx(p_idx),
                    })
                }

                pos += 1;
            } else {
                pos += shift;
            }
        }

        None
    }

    /// Returns an iterator over non-overlapping matches.
    pub fn find<'a, 'b>(&'a self, haystack: &'b str) -> Matches<'a, 'b> {
        Matches {
            wm: &self,
            haystack: haystack.as_bytes(),
            cur_pos: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use ::{Match, TwoByteWM};
    use aho_corasick::{AcAutomaton, Automaton};

    #[test]
    fn examples() {
        let needles = vec![
            "fox",
            "brown",
            "vwxyz",
            "yz",
            "ijk",
            "ijklm",
        ];
        let haystacks = vec![
            "The quick brown fox jumped over the lazy dog.",
            "abcdefghijklmnopqrstuvwxyz",
        ];

        let wm = TwoByteWM::new(&needles);
        let ac = AcAutomaton::new(&needles);
        for hay in &haystacks {
            let wm_answer: Vec<Match> = wm.find(hay).collect();
            let ac_answer: Vec<Match> = ac.find(hay)
                .map(|m| Match { start: m.start, end: m.end, pat_idx: m.pati })
                .collect();
            assert_eq!(wm_answer, ac_answer);
        }
    }
}


