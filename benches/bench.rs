#![feature(test)]

extern crate aho_corasick;
extern crate test;
extern crate wu_manber;

use aho_corasick::{AcAutomaton, Automaton};
use wu_manber::TwoByteWM;
use test::Bencher;

const INPUT: &'static str = include_str!("regexdna_input.txt");

fn shootout_needles() -> Vec<&'static str> {
    vec![
        "cgggtaaa",
        "ggggtaaa",
        "tgggtaaa",
        "tttaccca",
        "tttacccc",
        "tttacccg",
    ]
}

#[bench]
fn shootout(b: &mut Bencher) {
    let wm = TwoByteWM::new(shootout_needles());
    b.bytes = INPUT.len() as u64;
    b.iter(|| assert_eq!(wm.find(INPUT).count(), 3));
}

#[bench]
fn shootout_ac(b: &mut Bencher) {
    let ac = AcAutomaton::new(shootout_needles());
    b.bytes = INPUT.len() as u64;
    b.iter(|| assert_eq!(ac.find(INPUT).count(), 3));
}

