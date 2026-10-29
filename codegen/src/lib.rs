//! Probes for assembly-aware analysis, inspected with `cargo codegen`
//!
//! - a probe is a `#[unsafe(no_mangle)]`, `#[inline(never)]` wrapper of a crate's API, easy to find in the assembly
//! - [`probe`] and [`probes!`] define them in the crate, but they compile only where a downstream crate expands them
//! - so they show what the crate's users get: the same cross-crate inlining, and the public API only
//! - each definition registers itself in the compiled crate, where `cargo codegen` finds it
//!
//! ## Example
//!
//! In the probed crate, behind a feature of its own, `codegen = ["dep:codegen"]`:
//!
//! ```rust
//! pub fn tripled(x: u8) -> u8 {
//!     x.wrapping_mul(3)
//! }
//!
//! #[codegen::probe]
//! fn demo_tripled(x: u8) -> u8 {
//!     crate::tripled(x)
//! }
//!
//! // a block, sharing helper items between probes
//! #[codegen::probe]
//! mod demo {
//!     type Byte = u8;
//!
//!     fn demo_sixfold(x: Byte) -> Byte {
//!         crate::tripled(x).wrapping_mul(2)
//!     }
//! }
//! # fn main() {
//! #     __codegen_probe_demo_tripled!();
//! #     __codegen_probe_demo!();
//! #     assert_eq!(demo_tripled(2), 6);
//! #     assert_eq!(demo_sixfold(2), 12);
//! # }
//! ```
//!
//! ## Writing probes
//!
//! - reach the crate through `crate::` paths: the attribute makes them `$crate::`, resolving wherever probes expand
//! - `fn` items become `pub` probes, `unsafe` and `extern` ones included; generic ones fail to compile
//! - other items of a `mod`, such as `use`, statics or macro invocations, are expanded as written
//! - [`probes!`] is the same as a probe `mod`, for macros generating probes: write `$crate::` there
//! - keep the dependency named `codegen`: the attribute expands to `::codegen::probes!`
//!
//! ```compile_fail
//! codegen::probes!(generic_probes {
//!     fn identity<T>(x: T) -> T {
//!         x
//!     }
//! });
//! generic_probes!();
//! # fn main() {}
//! ```
//! - `#[cfg]` inside a `mod` or block is evaluated in the expanding crate: gate the whole `mod` or `probes!` instead

pub use codegen_macros::probe;

/// Defines the exported macro `$name!()`, expanding the probes in the crate that invokes it, and registers it
#[macro_export]
macro_rules! probes {
    ($name:ident { $($body:tt)* }) => {
        #[doc(hidden)]
        #[macro_export]
        macro_rules! $name {
            () => {
                $crate::__probe_items! { $($body)* }
            };
        }

        // `codegen-probes:<name>\n` in the object file, so `cargo codegen` finds the macro without being told
        const _: () = {
            const ENTRY: &str = ::core::concat!("codegen-probes:", ::core::stringify!($name), "\n");
            #[used]
            #[cfg_attr(target_vendor = "apple", unsafe(link_section = "__DATA,codegen"))]
            #[cfg_attr(not(target_vendor = "apple"), unsafe(link_section = "codegen"))]
            static REGISTRY: [u8; ENTRY.len()] = $crate::__registry_entry(ENTRY);
        };
    };
}

/// The bytes of a registry entry, as an array a `#[used]` static can hold in place
#[doc(hidden)]
pub const fn __registry_entry<const N: usize>(entry: &str) -> [u8; N] {
    let bytes = entry.as_bytes();
    let mut array = [0; N];
    let mut i = 0;
    while i < N {
        array[i] = bytes[i];
        i += 1;
    }
    array
}

/// Expands the items of a [`probes!`] block, marking each `fn` as a probe
#[doc(hidden)]
#[macro_export]
macro_rules! __probe_items {
    () => {};
    // `fn` items, whatever their visibility, `unsafe` or ABI, so none silently stays a plain item
    ($(#[$attr:meta])* $vis:vis unsafe extern $abi:literal fn $($rest:tt)*) => {
        $crate::__probe_items! { @fn [$(#[$attr])*] [unsafe extern $abi] fn $($rest)* }
    };
    ($(#[$attr:meta])* $vis:vis unsafe fn $($rest:tt)*) => {
        $crate::__probe_items! { @fn [$(#[$attr])*] [unsafe] fn $($rest)* }
    };
    ($(#[$attr:meta])* $vis:vis extern $abi:literal fn $($rest:tt)*) => {
        $crate::__probe_items! { @fn [$(#[$attr])*] [extern $abi] fn $($rest)* }
    };
    ($(#[$attr:meta])* $vis:vis fn $($rest:tt)*) => {
        $crate::__probe_items! { @fn [$(#[$attr])*] [] fn $($rest)* }
    };
    (@fn [$($attr:tt)*] [$($qualifier:tt)*] fn $probe:ident < $($rest:tt)*) => {
        ::core::compile_error!(::core::concat!(
            "probe `",
            ::core::stringify!($probe),
            "` is generic, so it has no symbol: wrap one concrete instance instead",
        ));
    };
    (
        @fn [$($attr:tt)*] [$($qualifier:tt)*]
        fn $probe:ident ($($args:tt)*) $(-> $ret:ty)? $block:block
        $($rest:tt)*
    ) => {
        $($attr)*
        #[unsafe(no_mangle)]
        #[inline(never)]
        pub $($qualifier)* fn $probe($($args)*) $(-> $ret)? $block

        $crate::__probe_items! { $($rest)* }
    };
    ($item:item $($rest:tt)*) => {
        $item

        $crate::__probe_items! { $($rest)* }
    };
}

// lets the attribute's `::codegen::probes!` resolve in this crate's own tests
#[cfg(test)]
extern crate self as codegen;

#[cfg(test)]
mod test {
    pub const BASE: u64 = 40;

    pub fn add_base(x: u64) -> u64 {
        x + BASE
    }

    crate::probes!(test_probes {
        /// Doc comments are kept
        fn codegen_test_add_base(x: u64) -> u64 {
            $crate::test::add_base(x)
        }

        static OFFSET: u64 = 2;

        fn codegen_test_offset() -> u64 {
            OFFSET
        }

        fn codegen_test_unit() {}

        pub fn codegen_test_pub(x: u64) -> u64 {
            x + 1
        }

        unsafe fn codegen_test_unsafe(x: *const u64) -> u64 {
            // SAFETY: the caller passes a valid pointer
            unsafe { *x }
        }

        extern "C" fn codegen_test_extern(x: u64) -> u64 {
            x + 3
        }

        pub(crate) unsafe extern "C" fn codegen_test_unsafe_extern(x: *const u64) -> u64 {
            // SAFETY: the caller passes a valid pointer
            unsafe { *x + 4 }
        }
    });

    // expanded here, as a downstream crate would: textual scope, macro-expanded exports having no absolute path
    test_probes!();

    #[test]
    fn probes_expand_to_callable_functions() {
        let added = codegen_test_add_base(2);
        let offset = codegen_test_offset();
        codegen_test_unit();
        assert_eq!(added, 42);
        assert_eq!(offset, 2);
    }

    #[test]
    fn probes_are_exported_unmangled() {
        // a no_mangle function is reachable through its plain C symbol
        unsafe extern "Rust" {
            safe fn codegen_test_add_base(x: u64) -> u64;
        }
        let through_symbol = codegen_test_add_base(1);
        assert_eq!(through_symbol, 41);
    }

    #[test]
    fn qualified_fns_are_probes_too() {
        // each is reached through its plain C symbol, so each was marked `no_mangle`
        unsafe extern "Rust" {
            safe fn codegen_test_pub(x: u64) -> u64;
            fn codegen_test_unsafe(x: *const u64) -> u64;
        }
        unsafe extern "C" {
            safe fn codegen_test_extern(x: u64) -> u64;
            fn codegen_test_unsafe_extern(x: *const u64) -> u64;
        }
        let value = 10;
        let public = codegen_test_pub(1);
        // SAFETY: `value` outlives both calls
        let read = unsafe { codegen_test_unsafe(&value) };
        let external = codegen_test_extern(1);
        // SAFETY: `value` outlives the call
        let read_external = unsafe { codegen_test_unsafe_extern(&value) };
        assert_eq!(public, 2);
        assert_eq!(read, 10);
        assert_eq!(external, 4);
        assert_eq!(read_external, 14);
    }

    pub fn doubled(x: u64) -> u64 {
        x * 2
    }

    #[crate::probe]
    fn codegen_test_attribute_fn(x: u64) -> u64 {
        crate::test::doubled(x)
    }

    #[crate::probe]
    mod attribute_block {
        // `pub(crate)` stays as written, only `crate::` paths become `$crate::`
        pub(crate) use crate::test::doubled;

        const TWO: u64 = 2;

        fn codegen_test_attribute_mod(x: u64) -> u64 {
            doubled(x) + TWO
        }
    }

    __codegen_probe_codegen_test_attribute_fn!();

    mod attribute_block_expanded {
        __codegen_probe_attribute_block!();
    }

    #[test]
    fn attribute_probes_reach_the_crate_through_rewritten_paths() {
        let from_fn = codegen_test_attribute_fn(4);
        let from_mod = attribute_block_expanded::codegen_test_attribute_mod(4);
        assert_eq!(from_fn, 8);
        assert_eq!(from_mod, 10);
    }

    #[test]
    fn attribute_probes_are_exported_unmangled() {
        unsafe extern "Rust" {
            safe fn codegen_test_attribute_mod(x: u64) -> u64;
        }
        let through_symbol = codegen_test_attribute_mod(1);
        assert_eq!(through_symbol, 4);
    }
}
