//! Lookup harness of the perfect_hash benches, mirroring the ConstexprCore/perfect_hash benchmarks

use std::collections::HashMap;
use std::hash::BuildHasher;

use test::Bencher;

use super::stream::{Mix, stream};

/// Times one lookup per iteration, cycling through the stream, after checking every answer
pub fn run(
    b: &mut Bencher,
    mix: Mix,
    hits: &[&'static str],
    misses: &[&'static str],
    lookup: impl Fn(&str) -> Option<i32>,
) {
    for (i, key) in hits.iter().enumerate() {
        let value = lookup(key);
        assert_eq!(value, Some(i as i32), "{key:?}");
    }
    for key in misses {
        let value = lookup(key);
        assert_eq!(value, None, "{key:?}");
    }
    let stream = stream(mix, hits, misses);
    let mut i = 0;
    b.iter(|| {
        let key = stream[i];
        i += 1;
        if i == stream.len() {
            i = 0;
        }
        lookup(key)
    });
}

/// Map from each key to its declaration index, the values every method returns
pub fn index_map<S: BuildHasher>(
    hits: &[&'static str],
    hasher: S,
) -> HashMap<&'static str, i32, S> {
    let mut map = HashMap::with_hasher(hasher);
    map.extend(hits.iter().enumerate().map(|(i, key)| (*key, i as i32)));
    map
}

/// Same as [`index_map`], for hashbrown
pub fn index_hashbrown<S: BuildHasher>(
    hits: &[&'static str],
    hasher: S,
) -> hashbrown::HashMap<&'static str, i32, S> {
    let mut map = hashbrown::HashMap::with_hasher(hasher);
    map.extend(hits.iter().enumerate().map(|(i, key)| (*key, i as i32)));
    map
}
