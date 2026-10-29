//! Probes for assembly-aware analysis, inspected with `cargo codegen`
//!
//! - a probe is a `#[unsafe(no_mangle)]`, `#[inline(never)]` wrapper of a crate's API, easy to find in the assembly
//! - [`probes!`] defines them in the crate, but they compile only where a downstream crate expands them
//! - so they show what the crate's users get: the same cross-crate inlining, and the public API only
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
//! codegen::probes!(demo_probes {
//!     fn demo_tripled(x: u8) -> u8 {
//!         $crate::tripled(x)
//!     }
//! });
//! # fn main() {
//! #     demo_probes!();
//! #     assert_eq!(demo_tripled(2), 6);
//! # }
//! ```
//!
//! `cargo codegen` then builds a crate expanding `demo_probes!()`, listed in `[package.metadata.codegen]` as
//! `probes = ["demo_probes"]`.
//!
//! ## Writing probes
//!
//! - reach the crate through `$crate::`, which resolves to it wherever the probes are expanded
//! - `fn` items become probes; other items, such as statics or macro invocations, are expanded as written
//! - `#[cfg]` inside a block is evaluated in the expanding crate: gate the whole `probes!` invocation instead

/// Defines the exported macro `$name!()`, expanding the probes in the crate that invokes it
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
    };
}

/// Expands the items of a [`probes!`] block, marking each `fn` as a probe
#[doc(hidden)]
#[macro_export]
macro_rules! __probe_items {
    () => {};
    (
        $(#[$attr:meta])*
        fn $probe:ident ($($args:tt)*) $(-> $ret:ty)? $block:block
        $($rest:tt)*
    ) => {
        $(#[$attr])*
        #[unsafe(no_mangle)]
        #[inline(never)]
        pub fn $probe($($args)*) $(-> $ret)? $block

        $crate::__probe_items! { $($rest)* }
    };
    ($item:item $($rest:tt)*) => {
        $item

        $crate::__probe_items! { $($rest)* }
    };
}

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
}
