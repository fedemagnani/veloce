//! `--counters`: hardware counters per lookup through Apple's kperf, over the streams and loop the timings use
//!
//! Drives the private frameworks through `darwin-kperf-sys` the way `darwin-kperf` does, minus its table of known CPUs,
//! which predates macOS 27 naming the M4 Pro's database `as6`: events are looked up by name in whatever database loads.

use std::ffi::{CStr, c_int};
use std::hint::black_box;
use std::ptr;

use darwin_kperf_sys::kperf::{self, KPC_CLASS_CONFIGURABLE_MASK, KPC_MAX_COUNTERS, kpc_config_t};
use darwin_kperf_sys::kperfdata::{self, kpep_config, kpep_db};
use darwin_kperf_sys::load::LibraryHandle;

use super::{check_answers, lookup_run};
use crate::stream::{Mix, stream};

/// Events by their name in Apple's PMC databases, the same on every generation so far, with column names
const EVENTS: [(&CStr, &str); 4] = [
    (c"FIXED_CYCLES", "cycles"),
    (c"FIXED_INSTRUCTIONS", "instr"),
    (c"INST_BRANCH", "branches"),
    (c"BRANCH_MISPRED_NONSPEC", "mispred"),
];

/// Counted passes over each 2 000 000-key stream, so the two reads' own cost vanishes
const PASSES: u64 = 5;

/// Prints a row of counters per lookup for each bench `each` hands to the counter; needs root
pub fn run(each: impl FnOnce(&Counter)) {
    let counter = Counter::start();
    let names = EVENTS.map(|(_, name)| format!("{name:>9}"));
    println!("{:<30}{}{:>9}", "bench", names.join(""), "ipc");
    each(&counter);
}

/// The calling thread's counters, programmed for `EVENTS`; dropping it hands them back to the system
pub struct Counter {
    kpc: kperf::VTable,
    kpep: kperfdata::VTable,
    db: *mut kpep_db,
    config: *mut kpep_config,
    // where each event's count lands in the thread's counter array
    slots: [usize; EVENTS.len()],
    // the system's `force_all_ctrs` before we took the counters
    saved_force_all: c_int,
    // the frameworks stay loaded while their functions may be called
    _frameworks: [LibraryHandle; 2],
}

impl Counter {
    /// Loads the frameworks, takes the counters from the system and starts counting on this thread
    fn start() -> Self {
        let kperf_lib =
            LibraryHandle::open(c"/System/Library/PrivateFrameworks/kperf.framework/kperf")
                .expect("loading kperf.framework");
        let kpep_lib =
            LibraryHandle::open(c"/System/Library/PrivateFrameworks/kperfdata.framework/kperfdata")
                .expect("loading kperfdata.framework");
        let kpc = kperf::VTable::load(&kperf_lib).expect("resolving kperf.framework");
        let kpep = kperfdata::VTable::load(&kpep_lib).expect("resolving kperfdata.framework");
        let mut saved_force_all = 0;
        // SAFETY: reads a sysctl into a local; failing means no root
        let status = unsafe { (kpc.kpc_force_all_ctrs_get)(&raw mut saved_force_all) };
        assert_eq!(
            status, 0,
            "reading hardware counters needs root, see `just counters`"
        );
        let mut counter = Self {
            kpc,
            kpep,
            db: ptr::null_mut(),
            config: ptr::null_mut(),
            slots: [0; EVENTS.len()],
            saved_force_all,
            _frameworks: [kperf_lib, kpep_lib],
        };
        counter.configure();
        counter
    }

    /// Opens this CPU's database, programs `EVENTS` into the counters and starts them; `drop` undoes each step
    fn configure(&mut self) {
        let (kpc, kpep) = (&self.kpc, &self.kpep);
        // SAFETY: each call gets live pointers from the steps before it and buffers of the size it is told
        unsafe {
            check(
                (kpep.kpep_db_create)(ptr::null(), &raw mut self.db),
                "opening the PMC database",
            );
            check(
                (kpep.kpep_config_create)(self.db, &raw mut self.config),
                "creating a config",
            );
            check(
                (kpep.kpep_config_force_counters)(self.config),
                "forcing the counters",
            );
            for (name, _) in EVENTS {
                let mut event = ptr::null_mut();
                check(
                    (kpep.kpep_db_event)(self.db, name.as_ptr(), &raw mut event),
                    "finding an event",
                );
                let added =
                    (kpep.kpep_config_add_event)(self.config, &raw mut event, 0, ptr::null_mut());
                check(added, "adding an event");
            }
            let mut classes = 0;
            let mut registers = [0 as kpc_config_t; KPC_MAX_COUNTERS];
            let slots_size = size_of_val(&self.slots);
            check(
                (kpep.kpep_config_kpc_classes)(self.config, &raw mut classes),
                "reading classes",
            );
            check(
                (kpep.kpep_config_kpc_map)(self.config, self.slots.as_mut_ptr(), slots_size),
                "mapping",
            );
            let registers_size = size_of_val(&registers);
            check(
                (kpep.kpep_config_kpc)(self.config, registers.as_mut_ptr(), registers_size),
                "registers",
            );
            check(
                (kpc.kpc_force_all_ctrs_set)(1),
                "taking the counters from the system",
            );
            if classes & KPC_CLASS_CONFIGURABLE_MASK != 0 {
                check(
                    (kpc.kpc_set_config)(classes, registers.as_mut_ptr()),
                    "programming the counters",
                );
            }
            check((kpc.kpc_set_counting)(classes), "starting the counters");
            check(
                (kpc.kpc_set_thread_counting)(classes),
                "starting this thread's counters",
            );
        }
    }

    /// Counts `PASSES` passes over the stream of `mix` after a warm-up pass, printing the counts per lookup
    pub fn count(
        &self,
        bench: &str,
        mix: Mix,
        hits: &[&'static str],
        misses: &[&'static str],
        lookup: impl Fn(&str) -> Option<i32>,
    ) {
        check_answers(hits, misses, &lookup);
        let stream = stream(mix, hits, misses);
        let mut results = vec![0; stream.len()];
        let len = stream.len() as u64;
        // a warm-up pass, so caches and predictors start as warm as in a timed run
        lookup_run(&lookup, &stream, &mut results, 0, len);
        let before = self.sample();
        lookup_run(&lookup, &stream, &mut results, 0, len * PASSES);
        let after = self.sample();
        black_box(&results);
        let lookups = (len * PASSES) as f64;
        let per_lookup =
            std::array::from_fn::<f64, 4, _>(|i| (after[i] - before[i]) as f64 / lookups);
        let cells = per_lookup.map(|count| format!("{count:>9.2}"));
        let ipc = per_lookup[1] / per_lookup[0];
        println!("{bench:<30}{}{ipc:>9.2}", cells.join(""));
    }

    /// The thread's running count of each event
    fn sample(&self) -> [u64; EVENTS.len()] {
        let mut counters = [0; KPC_MAX_COUNTERS];
        // SAFETY: the buffer holds the KPC_MAX_COUNTERS values asked for; tid 0 is the calling thread
        let status = unsafe {
            (self.kpc.kpc_get_thread_counters)(0, KPC_MAX_COUNTERS as u32, counters.as_mut_ptr())
        };
        check(status, "reading the counters");
        self.slots.map(|slot| counters[slot])
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        // SAFETY: stops counting, restores the system's setting and frees what `configure` allocated, each once
        unsafe {
            (self.kpc.kpc_set_thread_counting)(0);
            (self.kpc.kpc_set_counting)(0);
            (self.kpc.kpc_force_all_ctrs_set)(self.saved_force_all);
            if !self.config.is_null() {
                (self.kpep.kpep_config_free)(self.config);
            }
            if !self.db.is_null() {
                (self.kpep.kpep_db_free)(self.db);
            }
        }
    }
}

/// Panics naming the step when a kperf call returns non-zero
fn check(status: c_int, step: &str) {
    assert_eq!(status, 0, "kperf failed {step}, status {status}");
}
