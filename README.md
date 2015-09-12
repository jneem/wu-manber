wu-manber
=========

A crate for fast multi-string searching (after an initial pre-processing step
involving the strings to search for).  This crate implements the Wu-Manber
algorithm, which is particularly fast when all of the search strings are long.
Otherwise, the
[Aho-Corasick](http://doc.rust-lang.org/regex/aho_corasick/index.html) crate
may be faster.

[![Build status](https://travis-ci.org/jneem/wu-manber.svg)](https://travis-ci.org/jneem/wu-manber)

[Documentation](http://jneem.github.io/wu-manber/wu_manber/index.html)

