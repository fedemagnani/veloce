//! Criterion lookups over every upstream key set, for A/B comparisons with `--save-baseline` and `--baseline`
//!
//! `cargo bench --bench perfect_hash --features criterion`, on stable; kept out of CI, whose runners are too noisy.
//! With `--counters`, prints hardware counters per lookup instead, as root on macOS: `just counters`.

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Bencher, Criterion, criterion_group};
use stream::{Mix, stream};

#[cfg(target_os = "macos")]
mod counters;
#[macro_use]
#[path = "../../src/perfect_hash/keysets.rs"]
mod keysets;
mod stream;

/// Mixes benched per key set, with their names
const MIXES: [(Mix, &str); 3] = [
    (Mix::Hits, "hits"),
    (Mix::Misses, "misses"),
    (Mix::Mixed, "mixed"),
];

/// `lookups` and `count_lookups`, over every key set the `keysets!` list holds and the three mixes
macro_rules! lookup_benches {
    ($($tier:ident $name:ident { $($hit:literal => $value:literal),+ $(,)? } misses [$($miss:literal),+ $(,)?])*) => {
        /// A group per key set, a bench per mix
        fn lookups(c: &mut Criterion) {
            $({
                const HITS: &[&str] = &[$($hit),+];
                const MISSES: &[&str] = &[$($miss),+];
                veloce::perfect_map!(static MAP: i32 = { $($hit => $value),+ };);
                let mut group = c.benchmark_group(concat!("perfect_hash/", stringify!($name)));
                for (mix, label) in MIXES {
                    group.bench_function(label, |b| bench_lookups(b, mix, HITS, MISSES, |key| MAP.get(key).copied()));
                }
                group.finish();
            })*
        }

        /// A row of counters per key set and mix
        #[cfg(target_os = "macos")]
        fn count_lookups(counter: &counters::Counter) {
            $({
                const HITS: &[&str] = &[$($hit),+];
                const MISSES: &[&str] = &[$($miss),+];
                veloce::perfect_map!(static MAP: i32 = { $($hit => $value),+ };);
                for (mix, label) in MIXES {
                    let bench = format!("{}/{label}", stringify!($name));
                    counter.count(&bench, mix, HITS, MISSES, |key| MAP.get(key).copied());
                }
            })*
        }
    };
}

// after the macro it expands to, `macro_rules!` being scoped textually
keysets!(lookup_benches);

criterion_group! {
    name = benches;
    config = config();
    targets = lookups
}

/// Criterion's timings, or hardware counters with `--counters`
fn main() {
    if std::env::args().any(|arg| arg == "--counters") {
        count();
        return;
    }
    benches();
    Criterion::default().configure_from_args().final_summary();
}

/// Prints counters per lookup for every key set and mix, the bench thread on performance cores as when timed
fn count() {
    #[cfg(target_os = "macos")]
    {
        pin_to_performance_cores();
        counters::run(count_lookups);
    }
    #[cfg(not(target_os = "macos"))]
    panic!("--counters reads Apple's kperf counters, macOS only");
}

/// Criterion tuned for ~1 ns lookups, the bench thread kept off the efficiency cores
fn config() -> Criterion {
    pin_to_performance_cores();
    // drift between runs reaches a few percent on idle machines: smaller changes read as noise
    Criterion::default().noise_threshold(0.03)
}

/// Asks macOS to schedule this thread on performance cores, so samples don't mix core types
fn pin_to_performance_cores() {
    #[cfg(target_os = "macos")]
    {
        let class = libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE;
        // SAFETY: sets the calling thread's own QoS class, no pointers involved
        let status = unsafe { libc::pthread_set_qos_class_self_np(class, 0) };
        assert_eq!(status, 0, "pthread_set_qos_class_self_np");
    }
}

/// Times lookups cycling through the stream of `mix`, each hit's value stored as upstream does, after checking all
fn bench_lookups(
    b: &mut Bencher,
    mix: Mix,
    hits: &[&'static str],
    misses: &[&'static str],
    lookup: impl Fn(&str) -> Option<i32>,
) {
    check_answers(hits, misses, &lookup);
    let stream = stream(mix, hits, misses);
    let mut results = vec![0; stream.len()];
    let mut next = 0;
    b.iter_custom(|iters| {
        let start = Instant::now();
        next = lookup_run(&lookup, &stream, &mut results, next, iters);
        let elapsed: Duration = start.elapsed();
        black_box(&results);
        elapsed
    });
}

/// Asserts every hit maps to its declaration index and every miss to nothing
fn check_answers(
    hits: &[&'static str],
    misses: &[&'static str],
    lookup: &impl Fn(&str) -> Option<i32>,
) {
    for (i, key) in hits.iter().enumerate() {
        let value = lookup(key);
        assert_eq!(value, Some(i as i32), "{key:?}");
    }
    for key in misses {
        let value = lookup(key);
        assert_eq!(value, None, "{key:?}");
    }
}

/// `count` lookups from `next` on, wrapping around the stream, returning where the next run starts
fn lookup_run(
    lookup: &impl Fn(&str) -> Option<i32>,
    stream: &[&'static str],
    results: &mut [i32],
    mut next: usize,
    count: u64,
) -> usize {
    let mut left = count as usize;
    while left > 0 {
        let end = (next + left).min(stream.len());
        // slices, so the loop below carries no bounds check
        let keys = &stream[next..end];
        let slots = &mut results[next..end];
        for (key, slot) in keys.iter().zip(slots) {
            if let Some(value) = lookup(key) {
                *slot = value;
            }
        }
        left -= end - next;
        next = if end == stream.len() { 0 } else { end };
    }
    next
}
