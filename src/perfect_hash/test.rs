// Port of ConstexprCore/perfect_hash by Daniel Lemire and Francisco Thiesen, MIT

use super::generator::test::Algorithm;
use super::{PerfectMap, PerfectSet, Phf, key_len_bound, padded_len};
use crate::{perfect_map, perfect_set};

mod set {
    use super::*;

    perfect_set!(static GREEK = ["alpha", "beta", "gamma"];);
    perfect_set!(static SINGLE = ["only"];);

    #[test]
    fn contains_and_index_of() {
        check_set(
            &GREEK,
            &["alpha", "beta", "gamma"],
            &["delta", "", "alph", "alphaa"],
        );
        check_set(&SINGLE, &["only"], &["other", "onl", "onlyy", ""]);
    }

    #[test]
    fn lookups_take_any_bytes() {
        let owned = String::from("beta");
        let bytes: &[u8] = b"gamma";
        assert!(GREEK.contains(&owned));
        assert!(GREEK.contains(b"alpha"));
        assert!(GREEK.contains(bytes));
        assert!(!GREEK.contains(b"alph\0"));
    }

    perfect_set!(static METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];);

    #[test]
    fn http_methods() {
        let keys = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
        check_set(&METHODS, &keys, &["CONNECT", "TRACE", "get", "", "GETS"]);
    }

    perfect_set!(static PROTOCOLS = ["http", "https", "ftp", "ws", "wss", "file"];);

    #[test]
    fn url_protocols() {
        let keys = ["http", "https", "ftp", "ws", "wss", "file"];
        check_set(
            &PROTOCOLS,
            &keys,
            &[
                "ssh", "telnet", "mailto", "data", "blob", "urn", "HTTP", "htt",
            ],
        );
    }
}

mod map {
    use super::*;

    perfect_map!(static STATUS: u16 = { "OK" => 200, "Not Found" => 404, "Internal Server Error" => 500 };);

    #[test]
    fn get_and_contains_key() {
        assert_eq!(STATUS.len(), 3);
        assert_eq!(STATUS.get("OK"), Some(&200));
        assert_eq!(STATUS.get("Not Found"), Some(&404));
        assert_eq!(STATUS.get("Internal Server Error"), Some(&500));
        assert_eq!(STATUS.get("Bad Request"), None);
        assert_eq!(STATUS.get(""), None);
        assert!(STATUS.contains_key("OK"));
        assert!(!STATUS.contains_key("Gone"));
    }

    perfect_map!(static WEEKDAYS: u8 = { "mon" => 1, "tue" => 2, "wed" => 3, "thu" => 4, "fri" => 5 };);

    #[test]
    fn same_length_keys() {
        assert_eq!(WEEKDAYS.get("mon"), Some(&1));
        assert_eq!(WEEKDAYS.get("fri"), Some(&5));
        assert_eq!(WEEKDAYS.get("xyz"), None);
    }

    perfect_map!(static LETTER_NAMES: &str = { "a" => "alpha", "b" => "bravo", "c" => "charlie" };);

    #[test]
    fn single_byte_keys_use_the_direct_table() {
        assert_eq!(LETTER_NAMES.get("b"), Some(&"bravo"));
        assert_eq!(LETTER_NAMES.get("d"), None);
        assert_eq!(LETTER_NAMES.get("ab"), None);
        assert_eq!(LETTER_NAMES.get(""), None);
    }

    #[test]
    fn owned_values_drop_once() {
        use std::rc::Rc;
        let counter = Rc::new(());
        let keys: [&[u8]; 3] = [b"x", b"yy", b"zzz"];
        let phf = Phf::new(&keys);
        let values = [
            Rc::clone(&counter),
            Rc::clone(&counter),
            Rc::clone(&counter),
        ];
        assert_eq!(phf.table_size(), 4);
        let map = PerfectMap::<Rc<()>, 3, 4, 3, 16>::new(&keys, values, &phf);
        assert_eq!(Rc::strong_count(&counter), 4);
        assert!(map.get(b"yy").is_some());
        drop(map);
        assert_eq!(Rc::strong_count(&counter), 1);
    }
}

mod edge_cases {
    use super::*;

    perfect_set!(static PREFIXES = ["a", "ab", "abc", "abcd"];);
    perfect_set!(static SIMILAR = ["foo", "fop", "foq", "goo", "boo"];);
    perfect_set!(static LETTERS = [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m",
        "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
    ];);
    perfect_set!(static XYZ = ["x", "y", "z"];);
    perfect_set!(static BOOLS = ["true", "false"];);

    #[test]
    fn small_sets() {
        check_set(&PREFIXES, &["a", "ab", "abc", "abcd"], &["abcde", "", "b"]);
        check_set(
            &SIMILAR,
            &["foo", "fop", "foq", "goo", "boo"],
            &["for", "zoo"],
        );
        check_set(&XYZ, &["x", "y", "z"], &["w", "", "xy"]);
        check_set(&BOOLS, &["true", "false"], &["maybe"]);
    }

    #[test]
    fn single_byte_keys() {
        let letters: Vec<String> = ('a'..='z').map(String::from).collect();
        let keys: Vec<&str> = letters.iter().map(String::as_str).collect();
        check_set(&LETTERS, &keys, &["A", "1", "", "aa", "zz", "!"]);
    }

    perfect_set!(static ANIMALS = ["cat", "dog", "bat", "rat", "hat"];);
    perfect_set!(static LAST_DIFFERS = ["abp", "abq", "abr", "abs", "abt"];);
    perfect_set!(static FIRST_DIFFERS = ["a_end", "b_end", "c_end", "d_end", "e_end"];);
    perfect_set!(static INTERIOR = ["xax", "xbx", "xcx", "xdx"];);
    perfect_set!(static TWO_POSITIONS = ["ax", "ay", "bx", "by"];);
    perfect_set!(static BACKTRACK = ["cat", "cot", "bat", "bot"];);
    perfect_set!(static BACKTRACK5 = ["xap", "xbp", "xcp", "yaq", "ybq"];);
    perfect_set!(static MIXED_LENGTHS = ["a", "ab", "ba", "abc", "bca"];);

    #[test]
    fn position_selection_strategies() {
        check_set(
            &ANIMALS,
            &["cat", "dog", "bat", "rat", "hat"],
            &["car", "mat"],
        );
        check_set(
            &LAST_DIFFERS,
            &["abp", "abq", "abr", "abs", "abt"],
            &["abx", "ab"],
        );
        check_set(
            &FIRST_DIFFERS,
            &["a_end", "b_end", "c_end", "d_end", "e_end"],
            &["z_end"],
        );
        check_set(&INTERIOR, &["xax", "xbx", "xcx", "xdx"], &["xzx", "xxx"]);
        check_set(&TWO_POSITIONS, &["ax", "ay", "bx", "by"], &["cx", "az"]);
        check_set(
            &BACKTRACK,
            &["cat", "cot", "bat", "bot"],
            &["cut", "bit", "ca"],
        );
        check_set(
            &BACKTRACK5,
            &["xap", "xbp", "xcp", "yaq", "ybq"],
            &["xdp", "yap"],
        );
        check_set(
            &MIXED_LENGTHS,
            &["a", "ab", "ba", "abc", "bca"],
            &["b", "cab"],
        );
    }

    const ALPHA_BETA_GAMMA: [&[u8]; 3] = [b"alpha", b"beta", b"gamma"];
    static NON_MINIMAL: PerfectSet<3, 5, 5, 16> =
        PerfectSet::new(&ALPHA_BETA_GAMMA, &Phf::gperf(&ALPHA_BETA_GAMMA, 5));

    #[test]
    fn non_minimal_table() {
        assert_eq!(NON_MINIMAL.table_size(), 5);
        check_set(&NON_MINIMAL, &["alpha", "beta", "gamma"], &["delta", ""]);
    }

    perfect_set!(static KEYWORDS = [
        "auto", "bool", "break", "case", "char", "class", "const", "do",
        "double", "else", "enum", "float", "for", "goto", "if",
    ];);

    #[test]
    fn keywords() {
        let keys = [
            "auto", "bool", "break", "case", "char", "class", "const", "do", "double", "else",
            "enum", "float", "for", "goto", "if",
        ];
        check_set(&KEYWORDS, &keys, &["string", "vector", ""]);
    }

    perfect_set!(static LONG_KEYS = [
        "authentication_service",
        "database_connection_pool",
        "distributed_lock_manager",
        "event_processing_pipeline",
        "message_serialization_layer",
    ];);

    #[test]
    fn long_keys() {
        let keys = [
            "authentication_service",
            "database_connection_pool",
            "distributed_lock_manager",
            "event_processing_pipeline",
            "message_serialization_layer",
        ];
        check_set(
            &LONG_KEYS,
            &keys,
            &["authentication_services", "database_connection"],
        );
    }

    perfect_set!(static WITH_EMPTY = ["", "a", "ab"];);

    #[test]
    fn empty_key() {
        check_set(&WITH_EMPTY, &["", "a", "ab"], &["b", "ba"]);
    }

    perfect_set!(static OVERLAP = ["abXcd", "abYcd", "abZcd"];);

    #[test]
    fn overlap_encoding_middle_bytes() {
        check_set(&OVERLAP, &["abXcd", "abYcd", "abZcd"], &["abWcd", "ab_cd"]);
    }

    perfect_set!(static TICKERS20 = [
        "AAPL", "AMD", "AMZN", "BA", "C", "COST", "CVX", "DIS", "F", "GE",
        "GOOG", "GS", "HD", "IBM", "JPM", "MA", "META", "MSFT", "NVDA", "TSLA",
    ];);
    perfect_set!(static TWO_BYTE25 = [
        "AA", "AB", "AC", "AD", "AE", "AF", "AG", "AH", "AI", "AJ",
        "BA", "BB", "BC", "BD", "BE", "BF", "BG", "BH", "BI", "BJ",
        "CA", "CB", "CC", "CD", "CE",
    ];);

    #[test]
    fn larger_sets() {
        let tickers = [
            "AAPL", "AMD", "AMZN", "BA", "C", "COST", "CVX", "DIS", "F", "GE", "GOOG", "GS", "HD",
            "IBM", "JPM", "MA", "META", "MSFT", "NVDA", "TSLA",
        ];
        check_set(&TICKERS20, &tickers, &["PLTR", "UBER", "G"]);
        let two_byte = [
            "AA", "AB", "AC", "AD", "AE", "AF", "AG", "AH", "AI", "AJ", "BA", "BB", "BC", "BD",
            "BE", "BF", "BG", "BH", "BI", "BJ", "CA", "CB", "CC", "CD", "CE",
        ];
        check_set(&TWO_BYTE25, &two_byte, &["DA", "AK"]);
    }

    perfect_set!(static FIVE_BYTES = ["abcde", "abcdf", "bbcde"];);

    perfect_set!(static GREEK_ALL_MISSES = ["alpha", "beta", "gamma", "delta", "epsilon"];);

    #[test]
    fn length_255_probe_never_matches_an_empty_slot() {
        let nul_probe = [0u8; 255];
        let five_bytes_found = FIVE_BYTES.contains(&nul_probe);
        let greek_found = GREEK_ALL_MISSES.contains(&nul_probe);
        assert!(!five_bytes_found);
        assert!(!greek_found);
    }
}

mod review {
    use super::*;

    const INTERIOR_KEYS: [&[u8]; 8] = [
        b"baseaaa_z",
        b"baseaab_z",
        b"baseaba_z",
        b"baseabb_z",
        b"basebaa_z",
        b"basebab_z",
        b"basebba_z",
        b"basebbb_z",
    ];
    // a const, not a static, to size the table in the type below
    #[allow(clippy::large_const_arrays)]
    const INTERIOR_PHF: Phf = Phf::new(&INTERIOR_KEYS);
    static INTERIOR: PerfectSet<8, { INTERIOR_PHF.table_size() }, 9, 16> =
        PerfectSet::new(&INTERIOR_KEYS, &INTERIOR_PHF);

    #[test]
    fn three_interior_positions() {
        let keys = [
            "baseaaa_z",
            "baseaab_z",
            "baseaba_z",
            "baseabb_z",
            "basebaa_z",
            "basebab_z",
            "basebba_z",
            "basebbb_z",
        ];
        assert_eq!(INTERIOR.algorithm(), Algorithm::Gperf);
        check_set(&INTERIOR, &keys, &["baseaac_z", "baseaaa_x"]);
    }

    const METHOD_KEYS: [&[u8]; 6] = [b"GET", b"PUT", b"POST", b"PATCH", b"DELETE", b"HEAD"];
    static HD_METHODS: PerfectSet<6, 16, 6, 16> =
        PerfectSet::new(&METHOD_KEYS, &Phf::hash_and_displace(&METHOD_KEYS, 16));

    #[test]
    fn hash_and_displace() {
        assert_eq!(HD_METHODS.algorithm(), Algorithm::HashAndDisplace);
        check_set(
            &HD_METHODS,
            &["GET", "PUT", "POST", "PATCH", "DELETE", "HEAD"],
            &["OPTIONS", "BAD"],
        );
    }

    perfect_set!(static EMPTY_LONG = ["", "abcdefghijklmnopqa", "abcdefghijklmnopqb"];);
    perfect_set!(static EMPTY_SHORT = ["", "ab", "ac"];);
    perfect_set!(static EMPTY_MEDIUM = ["", "abcde", "abcdf"];);
    perfect_map!(static EMPTY_LONG_MAP: i32 = { "" => 11, "abcdefghijklmnopqa" => 22, "abcdefghijklmnopqb" => 33 };);

    #[test]
    fn empty_probes() {
        let empty: &[u8] = &[];
        assert_eq!(EMPTY_LONG.index_of(empty), Some(0));
        assert!(EMPTY_SHORT.contains(empty));
        assert!(EMPTY_MEDIUM.contains(empty));
        assert_eq!(EMPTY_LONG_MAP.get(empty), Some(&11));
        assert!(!EMPTY_LONG.contains("abcdefghijklmnopqc"));
        assert_eq!(EMPTY_LONG_MAP.get("abcdefghijklmnopqc"), None);
    }

    #[test]
    fn helpers() {
        let keys: [&[u8]; 2] = [b"", b"abc"];
        assert_eq!(key_len_bound(&keys), 3);
        assert_eq!(key_len_bound(&[b"".as_slice()]), 1);
        assert_eq!(padded_len(1), 16);
        assert_eq!(padded_len(16), 16);
        assert_eq!(padded_len(17), 32);
        assert_eq!(padded_len(254), 256);
    }

    #[test]
    fn longest_keys() {
        check_long_pair::<33, 48>();
        check_long_pair::<64, 64>();
        check_long_pair::<200, 208>();
        check_long_pair::<254, 256>();
    }

    /// Builds a two-key set of `L`-byte keys at runtime and probes it with near misses
    fn check_long_pair<const L: usize, const W: usize>() {
        let [first, second] = long_key_pair(L);
        let keys: [&[u8]; 2] = [first.as_bytes(), second.as_bytes()];
        let phf = Phf::new(&keys);
        assert_eq!(phf.table_size(), 2);
        let set = PerfectSet::<2, 2, L, W>::new(&keys, &phf);
        check_set(&set, &[&first, &second], &[]);

        let mut probe = first.clone().into_bytes();
        probe[L / 2] = b'!';
        let flipped_found = set.contains(&probe);
        assert!(!flipped_found);
        probe[L / 2] = first.as_bytes()[L / 2];
        probe.pop();
        let truncated_found = set.contains(&probe);
        assert!(!truncated_found);
    }

    /// Two keys of `len` bytes cycling through the alphabet, differing only in the last byte
    fn long_key_pair(len: usize) -> [String; 2] {
        let body: String = (0..len - 1)
            .map(|i| (b'a' + (i % 26) as u8) as char)
            .collect();
        [format!("{body}1"), format!("{body}2")]
    }
}

/// Runtime fuzzing of the const generator: random sets over a small alphabet, every tier
mod generated {
    use super::*;

    /// Sets per configuration, fewer under Miri
    const SETS: usize = if cfg!(miri) { 2 } else { 40 };

    #[test]
    fn random_short_sets() {
        check_random_sets::<6, 4, 16>(1, SETS);
        check_random_sets::<8, 7, 16>(2, SETS);
    }

    #[test]
    fn random_medium_sets() {
        check_random_sets::<12, 12, 16>(3, SETS);
        check_random_sets::<20, 24, 32>(4, SETS);
    }

    #[test]
    fn random_long_sets() {
        check_random_sets::<10, 40, 48>(5, SETS);
    }

    #[test]
    #[cfg_attr(miri, ignore = "dense sets reach the slow tiers, too slow under Miri")]
    fn random_dense_sets_reach_every_tier() {
        // 60 short keys over 4 letters exhaust gperf positions often enough to need the later tiers
        let algorithms = check_random_sets::<60, 5, 16>(6, 20);
        let gperf = algorithms[Algorithm::Gperf as usize];
        assert!(gperf > 0, "{algorithms:?}");
    }

    fn check_random_sets<const N: usize, const L: usize, const W: usize>(
        seed: u64,
        sets: usize,
    ) -> [usize; 3] {
        let mut rng = Rng(seed);
        let mut algorithms = [0; 3];
        for _ in 0..sets {
            let owned = random_keys::<N, L>(&mut rng);
            let keys: [&[u8]; N] = core::array::from_fn(|i| owned[i].as_slice());
            let phf = Phf::new(&keys);
            algorithms[phf.algorithm() as usize] += 1;
            match phf.table_size() {
                8 => check_table::<N, 8, L, W>(&keys, &phf, &mut rng),
                16 => check_table::<N, 16, L, W>(&keys, &phf, &mut rng),
                32 => check_table::<N, 32, L, W>(&keys, &phf, &mut rng),
                64 => check_table::<N, 64, L, W>(&keys, &phf, &mut rng),
                128 => check_table::<N, 128, L, W>(&keys, &phf, &mut rng),
                256 => check_table::<N, 256, L, W>(&keys, &phf, &mut rng),
                other => panic!("unexpected table size {other}"),
            }
        }
        algorithms
    }

    /// `N` distinct random keys of at most `L` bytes, one of exactly `L`
    fn random_keys<const N: usize, const L: usize>(rng: &mut Rng) -> Vec<Vec<u8>> {
        let mut keys: Vec<Vec<u8>> = vec![vec![b'a'; L]];
        while keys.len() < N {
            let key = random_key(rng, L);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    /// Random key of 1 to `max_len` bytes from a 4-letter alphabet, so keys share many bytes
    fn random_key(rng: &mut Rng, max_len: usize) -> Vec<u8> {
        let len = 1 + rng.below(max_len);
        (0..len).map(|_| b"abcd"[rng.below(4)]).collect()
    }

    /// Checks one set built for `T` slots against linear search over keys and random probes
    fn check_table<const N: usize, const T: usize, const L: usize, const W: usize>(
        keys: &[&[u8]; N],
        phf: &Phf,
        rng: &mut Rng,
    ) {
        let set = PerfectSet::<N, T, L, W>::new(keys, phf);
        for (i, key) in keys.iter().enumerate() {
            let index = set.index_of(key);
            assert_eq!(index, Some(i));
        }
        for _ in 0..64 {
            let probe = random_key(rng, L + 2);
            let expected = keys.iter().position(|key| *key == probe.as_slice());
            let index = set.index_of(&probe);
            let found = set.contains(&probe);
            assert_eq!(index, expected, "probe {probe:?}");
            assert_eq!(found, expected.is_some(), "probe {probe:?}");
        }
    }

    /// xorshift64*, deterministic
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }
}

/// Loads near the end of a 4 KiB block, the case the page-safe NEON loads exist for
#[cfg(all(target_arch = "aarch64", target_feature = "neon", not(miri)))]
mod neon_loads {
    use super::super::neon::compare_chunks;
    use super::super::neon::test::load_chunk_16;

    #[test]
    fn chunk_lanes_are_exact_around_a_block_boundary() {
        let mut pages = Box::new(Pages([0; 8192]));
        for (i, byte) in pages.0.iter_mut().enumerate() {
            *byte = ((i * 151 + 7) & 255) as u8;
        }
        for len in 0..=16 {
            for offset in 4064..=4096 {
                let key = &pages.0[offset..offset + len];
                let (lo, hi) = load_chunk_16(key);
                assert_eq!(lo, key_lane(key, 0), "len {len} offset {offset}");
                assert_eq!(hi, key_lane(key, 1), "len {len} offset {offset}");
            }
        }
    }

    #[test]
    fn chunk_comparisons_cover_short_tails_and_block_offsets() {
        check_chunk_comparisons::<32>(17);
        check_chunk_comparisons::<32>(32);
        check_chunk_comparisons::<48>(33);
        check_chunk_comparisons::<64>(64);
        check_chunk_comparisons::<256>(254);
    }

    fn check_chunk_comparisons<const W: usize>(max_len: usize) {
        let mut pages = Box::new(Pages([0; 8192]));
        let offsets = [
            0, 1, 7, 15, 31, 4048, 4063, 4079, 4080, 4081, 4094, 4095, 4096,
        ];
        for len in 0..=max_len {
            let mut stored = [0u8; W];
            for (i, byte) in stored.iter_mut().take(len).enumerate() {
                *byte = ((i * 73 + len * 11) & 255) as u8;
            }
            for offset in offsets {
                pages.0[offset..offset + W].fill(0xA5);
                pages.0[offset..offset + len].copy_from_slice(&stored[..len]);
                let equal = compare_chunks(&pages.0[offset..offset + len], &stored);
                assert!(equal, "W {W} len {len} offset {offset}");
                // every chunk boundary and the final byte must count, even where the two windows overlap
                for pos in 0..len {
                    if pos % 16 != 0 && pos % 16 != 15 && pos + 1 != len {
                        continue;
                    }
                    pages.0[offset + pos] ^= 0x55;
                    let equal = compare_chunks(&pages.0[offset..offset + len], &stored);
                    assert!(!equal, "W {W} len {len} offset {offset} pos {pos}");
                    pages.0[offset + pos] ^= 0x55;
                }
            }
        }
    }

    /// Little-endian lane `lane` of the first `len` bytes at `p`, zero-padded
    fn key_lane(bytes: &[u8], lane: usize) -> u64 {
        let mut v = 0;
        for b in 0..8 {
            if let Some(&byte) = bytes.get(8 * lane + b) {
                v |= (byte as u64) << (8 * b);
            }
        }
        v
    }

    #[repr(C, align(4096))]
    struct Pages([u8; 8192]);
}

/// Probes ending right before a `PROT_NONE` page: any read past the key faults
#[cfg(all(unix, not(miri)))]
mod guard_page {
    use super::*;

    perfect_set!(static PROTOCOLS = ["http", "https", "ftp", "ws", "wss", "file"];);
    perfect_set!(static HEADERS = [
        "Accept", "Accept-Encoding", "Authorization", "Cache-Control", "Connection",
        "Content-Encoding", "Content-Length", "Content-Type", "Cookie", "Date",
        "Host", "If-None-Match", "Location", "Origin", "Referer", "Server",
        "Set-Cookie", "User-Agent", "Vary", "Via",
    ];);
    perfect_set!(static COUNTERS = [
        "total_items", "successful_operations", "failed_operations", "warnings", "errors",
        "retries", "bytes_processed", "records_read", "records_matched", "records_written",
    ];);
    perfect_set!(static CLASSES = [
        "java.util.concurrent.ConcurrentHashMap",
        "java.util.concurrent.atomic.AtomicInteger",
        "java.util.concurrent.locks.ReentrantReadWriteLock",
        "org.springframework.web.bind.annotation.RequestMapping",
        "io.netty.channel.socket.nio.NioSocketChannel",
    ];);

    #[test]
    fn lookups_are_exact_for_inputs_ending_at_a_protected_page() {
        check_probes_before(&PROTOCOLS, &["http", "https", "ftp", "ws", "wss", "file"]);
        let headers = [
            "Accept",
            "Accept-Encoding",
            "Authorization",
            "Cache-Control",
            "Connection",
            "Content-Encoding",
            "Content-Length",
            "Content-Type",
            "Cookie",
            "Date",
            "Host",
            "If-None-Match",
            "Location",
            "Origin",
            "Referer",
            "Server",
            "Set-Cookie",
            "User-Agent",
            "Vary",
            "Via",
        ];
        check_probes_before(&HEADERS, &headers);
        let counters = [
            "total_items",
            "successful_operations",
            "failed_operations",
            "warnings",
            "errors",
            "retries",
            "bytes_processed",
            "records_read",
            "records_matched",
            "records_written",
        ];
        check_probes_before(&COUNTERS, &counters);
        let classes = [
            "java.util.concurrent.ConcurrentHashMap",
            "java.util.concurrent.atomic.AtomicInteger",
            "java.util.concurrent.locks.ReentrantReadWriteLock",
            "org.springframework.web.bind.annotation.RequestMapping",
            "io.netty.channel.socket.nio.NioSocketChannel",
        ];
        check_probes_before(&CLASSES, &classes);
    }

    /// Every prefix of every key, ending 0 to 16 bytes before the guard, and each with one byte flipped
    fn check_probes_before<const N: usize, const T: usize, const L: usize, const W: usize>(
        set: &PerfectSet<N, T, L, W>,
        keys: &[&str],
    ) {
        let mut page = GuardedPage::new();
        let readable = page.readable();
        let end = readable.len();
        for key in keys {
            for len in 1..=key.len() {
                for gap in 0..=16 {
                    let start = end - gap - len;
                    readable[start..start + len].copy_from_slice(&key.as_bytes()[..len]);
                    for flip in 0..=len {
                        if flip < len {
                            readable[start + flip] ^= 0x01;
                        }
                        let probe = &readable[start..start + len];
                        let expected = expected_index(keys, probe);
                        let index = set.index_of(probe);
                        let found = set.contains(probe);
                        assert_eq!(index, expected, "probe {probe:?} gap {gap}");
                        assert_eq!(found, expected.is_some(), "probe {probe:?} gap {gap}");
                        if flip < len {
                            readable[start + flip] ^= 0x01;
                        }
                    }
                }
            }
        }
    }

    /// Two pages, the second `PROT_NONE`, unmapped on drop
    struct GuardedPage {
        mapping: *mut libc::c_void,
        page_size: usize,
    }

    impl GuardedPage {
        fn new() -> Self {
            // SAFETY: plain libc calls on a fresh private anonymous mapping
            unsafe {
                let page_size = libc::sysconf(libc::_SC_PAGESIZE) as usize;
                let prot = libc::PROT_READ | libc::PROT_WRITE;
                let flags = libc::MAP_PRIVATE | libc::MAP_ANON;
                let mapping = libc::mmap(core::ptr::null_mut(), 2 * page_size, prot, flags, -1, 0);
                assert_ne!(mapping, libc::MAP_FAILED);
                let guard = mapping.cast::<u8>().add(page_size).cast();
                let protected = libc::mprotect(guard, page_size, libc::PROT_NONE);
                assert_eq!(protected, 0);
                Self { mapping, page_size }
            }
        }

        /// The readable page, ending at the guard
        fn readable(&mut self) -> &mut [u8] {
            // SAFETY: the first page is mapped read-write and owned by `self`
            unsafe { core::slice::from_raw_parts_mut(self.mapping.cast(), self.page_size) }
        }
    }

    impl Drop for GuardedPage {
        fn drop(&mut self) {
            // SAFETY: unmaps the mapping created in `new`
            unsafe { libc::munmap(self.mapping, 2 * self.page_size) };
        }
    }
}

/// Slot assignments pinned to upstream v0.4.0's output for the same keys, so the port stays faithful
mod upstream_slots {
    use super::*;

    perfect_set!(static PROTOCOLS = ["http", "https", "ftp", "ws", "wss", "file"];);
    perfect_set!(static KEYWORDS = [
        "auto", "bool", "break", "case", "char", "class", "const", "do",
        "double", "else", "enum", "float", "for", "goto", "if",
    ];);
    perfect_set!(static COUNTERS = [
        "total_items", "successful_operations", "failed_operations", "warnings", "errors",
        "retries", "bytes_processed", "records_read", "records_matched", "records_written",
    ];);

    #[test]
    fn gperf_tier() {
        let protocols = ["http", "https", "ftp", "ws", "wss", "file"];
        let protocol_slots = slots(&PROTOCOLS, &protocols);
        assert_eq!(PROTOCOLS.algorithm(), Algorithm::Gperf);
        assert_eq!(PROTOCOLS.table_size(), 8);
        assert_eq!(protocol_slots, [5, 6, 3, 7, 0, 4]);

        let keywords = [
            "auto", "bool", "break", "case", "char", "class", "const", "do", "double", "else",
            "enum", "float", "for", "goto", "if",
        ];
        let keyword_slots = slots(&KEYWORDS, &keywords);
        assert_eq!(KEYWORDS.table_size(), 16);
        assert_eq!(
            keyword_slots,
            [0, 12, 10, 4, 7, 15, 9, 2, 6, 5, 13, 11, 8, 14, 3]
        );

        let counters = [
            "total_items",
            "successful_operations",
            "failed_operations",
            "warnings",
            "errors",
            "retries",
            "bytes_processed",
            "records_read",
            "records_matched",
            "records_written",
        ];
        let counter_slots = slots(&COUNTERS, &counters);
        assert_eq!(COUNTERS.table_size(), 16);
        assert_eq!(counter_slots, [11, 5, 1, 8, 6, 7, 0, 12, 15, 2]);
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "the position search over 40-byte keys is too slow under Miri"
    )]
    fn seeded_fullhash_tier() {
        // 18 same-length keys, each differing at its own position, need more positions than gperf may read
        let owned: Vec<String> = (0..18)
            .map(|j| {
                let mut key = vec![b'x'; 40];
                key[2 * j + 1] = b'y';
                String::from_utf8(key).unwrap()
            })
            .collect();
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let bytes: [&[u8]; 18] = core::array::from_fn(|i| keys[i].as_bytes());
        // built at runtime: the same search in const evaluation takes about a minute
        let phf = Phf::new(&bytes);
        assert_eq!(phf.algorithm(), Algorithm::SeededFullHash);
        assert_eq!(phf.table_size(), 32);
        let set = PerfectSet::<18, 32, 40, 48>::new(&bytes, &phf);
        let fullhash_slots = slots(&set, &keys);
        assert_eq!(
            fullhash_slots,
            [
                15, 10, 6, 26, 11, 5, 8, 25, 31, 7, 13, 9, 12, 0, 27, 23, 3, 1
            ]
        );
        check_set(
            &set,
            &keys,
            &["x", "xyxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxy"],
        );
    }

    /// Slot of every key, in declaration order
    fn slots<const N: usize, const T: usize, const L: usize, const W: usize>(
        set: &PerfectSet<N, T, L, W>,
        keys: &[&str],
    ) -> Vec<usize> {
        keys.iter()
            .map(|key| set.compute_hash(key.as_bytes()))
            .collect()
    }
}

/// Checks every key is found at its declaration index and every miss is rejected
fn check_set<const N: usize, const T: usize, const L: usize, const W: usize>(
    set: &PerfectSet<N, T, L, W>,
    keys: &[&str],
    misses: &[&str],
) {
    assert_eq!(set.len(), keys.len());
    for (i, key) in keys.iter().enumerate() {
        let index = set.index_of(key);
        assert!(set.contains(key), "{key:?} missing");
        assert_eq!(index, Some(i), "{key:?} at the wrong index");
        assert_eq!(set.key_at(i), key.as_bytes());
    }
    for miss in misses {
        let index = set.index_of(miss);
        assert!(!set.contains(miss), "{miss:?} wrongly found");
        assert_eq!(index, None, "{miss:?} wrongly indexed");
    }
}

/// Declaration index of `probe` by linear search, the reference lookups are checked against
fn expected_index(keys: &[&str], probe: &[u8]) -> Option<usize> {
    keys.iter().position(|key| key.as_bytes() == probe)
}
