use std::cmp::min;
use std::u16;

#[cfg(test)]
extern crate aho_corasick;

#[derive(Debug)]
pub struct Tables {
    /// The patterns that we are trying to match against, and their indices.
    ///
    /// Each of these has length (in bytes) at least 2.  They are sorted in increasing order of the
    /// hash value of their two critical bytes.
    patterns: Vec<(usize, Vec<u8>)>,

    /// For each of the patterns above, this contains the first two bytes, concatenated into a
    /// `u16`.
    ///
    /// This `Vec` is indexed in the same way as `patterns`.
    prefix: Vec<u16>,

    /// The minimimum length of any pattern.
    pat_len: u16,

    /// If `shift[HashFn(a, b)] = i` then no pattern contains the two-byte string `ab` starting
    /// anywhere between positions `pat_len - 2 - i` and `pat_len - 2`.
    shift: Vec<u16>,

    /// If `hash[HashFn(a, b)] = i` then the patterns whose critical bytes hash to `HashFn(a, b)`
    /// begin at `patterns[i]`.
    hash: Vec<u16>,
}

#[derive(Debug, PartialEq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
    pub pat_idx: usize,
}

pub struct Matches<'a, 'b> {
    tables: &'a Tables,
    haystack: &'b [u8],
    cur_pos: usize,
}

impl<'a, 'b> Iterator for Matches<'a, 'b> {
    type Item = Match;
    fn next(&mut self) -> Option<Match> {
        self.tables.find_from(self.haystack, self.cur_pos).map(|m| { self.cur_pos = m.end; m })
    }
}

/// For now, we default to this hash function (which is the one from the original paper of Wu and
/// Manber). In the future, we may want to look for a better one depending on the patterns.
fn hash_fn(a: u8, b: u8) -> u16 {
    ((a as u16) << 5) + (b as u16)
}

const HASH_MAX: usize = (0xFFusize << 5) + 0xFF;

impl Tables {
    fn pat(&self, p_idx: u16) -> &[u8] {
        &self.patterns[p_idx as usize].1
    }

    fn pat_idx(&self, p_idx: u16) -> usize {
        self.patterns[p_idx as usize].0
    }

    pub fn new<I, P>(strings: I) -> Tables
            where P: AsRef<[u8]>, I: IntoIterator<Item=P> {
        let patterns: Vec<_> = strings.into_iter().map(|s| s.as_ref().to_vec()).collect();
        if patterns.is_empty() {
            panic!("cannot create Tables from an empty set of patterns");
        } else if patterns.len() > u16::MAX as usize {
            panic!("we only support up to u16::MAX patterns");
        }

        let pat_len = patterns.iter().map(|p| p.len()).min().unwrap();
        if pat_len < 2 {
            panic!("all patterns must have length (in bytes) at least 2");
        } else if pat_len > u16::MAX as usize {
            panic!("we only support pattern lengths up to u16::MAX");
        }
        let pat_len = pat_len as u16;

        let h = |p: &[u8]| hash_fn(p[(pat_len-2) as usize], p[(pat_len-1) as usize]);
        let mut patterns: Vec<_> = patterns.into_iter().enumerate().collect();
        patterns.sort_by(|p, q| h(&p.1).cmp(&h(&q.1)));
        let patterns = patterns;
        let prefix: Vec<_> = patterns.iter()
            .map(|p| ((p.1[0] as u16) << 8) + (p.1[1] as u16))
            .collect();

        let mut hash = vec![0u16; HASH_MAX + 2];
        for (p_idx, &(_, ref p)) in patterns.iter().enumerate().rev() {
            let h_idx = h(&p) as usize;
            hash[h_idx] = p_idx as u16;
            if hash[h_idx + 1] == 0 {
                hash[h_idx + 1] = p_idx as u16 + 1;
            }
        }

        let mut shift = vec![pat_len - 1; HASH_MAX + 1];
        for &(_, ref p) in &patterns {
            for p_pos in 0..(pat_len - 1) {
                let h = hash_fn(p[p_pos as usize], p[(p_pos + 1) as usize]);
                shift[h as usize] = min(shift[h as usize], pat_len - p_pos - 2);
            }
        }

        Tables {
            patterns: patterns,
            prefix: prefix,
            pat_len: pat_len,
            shift: shift,
            hash: hash,
        }
    }

    pub fn find_from<P>(&self, haystack: P, offset: usize) -> Option<Match> where P: AsRef<[u8]> {
        // `pos` points to the index in `haystack` that we are trying to align against the index
        // `pat_len - 1` of the patterns.
        let pat_len = self.pat_len as usize;
        let mut pos = offset + pat_len - 1;
        let haystack = haystack.as_ref();
        while pos <= haystack.len() - 1 {
            let h = hash_fn(haystack[pos - 1], haystack[pos]) as usize;
            let shift = self.shift[h] as usize;
            if shift == 0 {
                // We might have matched the end of some pattern.  Iterate over all the patterns
                // that we might have matched, and see if they match the beginning.
                let a = haystack[pos - pat_len + 1];
                let b = haystack[pos - pat_len + 2];
                let prefix = ((a as u16) << 8) + (b as u16);
                let mut found: Option<u16> = None;
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

    pub fn find<'a, 'b>(&'a self, haystack: &'b [u8]) -> Matches<'a, 'b> {
        Matches {
            tables: &self,
            haystack: haystack,
            cur_pos: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use ::{Match, Tables};
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

        let tables = Tables::new(&needles);
        let ac = AcAutomaton::new(&needles);
        for hay in &haystacks {
            let wm_answer: Vec<Match> = tables.find(hay.as_bytes()).collect();
            let ac_answer: Vec<Match> = ac.find(hay)
                .map(|m| Match { start: m.start, end: m.end, pat_idx: m.pati })
                .collect();
            assert_eq!(wm_answer, ac_answer);
        }
    }
}


