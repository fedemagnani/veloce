//! Instruction classes per instruction set: what an instruction does to control flow, memory and ordering

use crate::normalize::Instruction;
use crate::target::Arch;

/// What one instruction does, several classes possibly at once
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Class {
    /// Branches on a condition: the code the stats exist to spot
    pub cond_branch: bool,
    /// Jumps unconditionally to a local label, or indirectly through a register or table
    pub jump: bool,
    /// Calls a function, a tail call jumping to a global symbol included
    pub call: bool,
    /// Picks between values, or compares, on a condition without branching
    pub select: bool,
    pub load: bool,
    pub store: bool,
    /// Uses a SIMD or floating-point register
    pub simd: bool,
    /// Orders memory: acquire/release accesses, ordered atomics, fences, locked x86 instructions
    pub barrier: bool,
}

/// Classes of `instruction` on `arch`
pub fn classify(instruction: &Instruction, arch: Arch) -> Class {
    match arch {
        Arch::Aarch64 => aarch64::classify(instruction),
        Arch::X86_64 => x86_64::classify(instruction),
    }
}

/// AArch64, in the syntax LLVM prints for Mach-O and ELF alike
mod aarch64 {
    use super::{Class, Instruction};
    use crate::normalize::{Piece, pieces};

    /// Compare-and-branch and test-and-branch, besides the `b.<cond>` family
    const COND_BRANCHES: [&str; 4] = ["cbz", "cbnz", "tbz", "tbnz"];
    /// Conditional selects, sets and compares, the branch-free alternative to a conditional branch
    const SELECTS: [&str; 14] = [
        "csel", "csinc", "csinv", "csneg", "cset", "csetm", "cinc", "cinv", "cneg", "fcsel",
        "ccmp", "ccmn", "fccmp", "fccmpe",
    ];
    /// Read-modify-write atomics, longest roots first so `ldsmax` isn't read as `lds…`
    const ATOMIC_ROOTS: [&str; 18] = [
        "ldsmax", "ldsmin", "ldumax", "ldumin", "stsmax", "stsmin", "stumax", "stumin", "ldadd",
        "ldclr", "ldeor", "ldset", "stadd", "stclr", "steor", "stset", "swp", "cas",
    ];
    /// Acquire loads and release stores, plain and exclusive
    const ORDERED_ACCESSES: [&str; 9] = [
        "ldar", "ldapr", "ldapur", "ldaxr", "ldaxp", "stlr", "stlur", "stlxr", "stlxp",
    ];
    const FENCES: [&str; 3] = ["dmb", "dsb", "isb"];

    pub fn classify(instruction: &Instruction) -> Class {
        let mnemonic = instruction.mnemonic.as_str();
        let base = base(mnemonic);
        let to_symbol = !instruction.symbols.is_empty();
        let atomic = atomic_ordering(base);
        Class {
            cond_branch: is_cond_branch(mnemonic, base),
            jump: (mnemonic == "b" && !to_symbol) || base == "br",
            call: matches!(base, "bl" | "blr") || (mnemonic == "b" && to_symbol),
            select: SELECTS.contains(&base),
            load: base.starts_with("ld") || atomic.is_some(),
            store: base.starts_with("st") || atomic.is_some(),
            simd: has_arrangement(mnemonic) || uses_vector_register(&instruction.shape),
            barrier: is_barrier(base, atomic),
        }
    }

    /// Mnemonic without a Mach-O arrangement suffix such as `.16b`, or a condition such as `.ne`
    fn base(mnemonic: &str) -> &str {
        mnemonic.split('.').next().unwrap_or(mnemonic)
    }

    /// `b.<cond>`, `bc.<cond>`, or a compare- or test-and-branch
    fn is_cond_branch(mnemonic: &str, base: &str) -> bool {
        let conditional = mnemonic.starts_with("b.") || mnemonic.starts_with("bc.");
        conditional || COND_BRANCHES.contains(&base)
    }

    /// The ordering suffix of a read-modify-write atomic (`""`, `a`, `l` or `al`), `None` for other instructions
    fn atomic_ordering(base: &str) -> Option<&str> {
        let root = ATOMIC_ROOTS.iter().find(|root| base.starts_with(**root))?;
        let rest = &base[root.len()..];
        // `casp` is the pair form, `b` and `h` the byte and halfword sizes
        let rest = rest.strip_prefix('p').unwrap_or(rest);
        let ordering = rest.strip_suffix(['b', 'h']).unwrap_or(rest);
        matches!(ordering, "" | "a" | "l" | "al").then_some(ordering)
    }

    /// An ordered access, an atomic with acquire or release semantics, or a fence
    fn is_barrier(base: &str, atomic: Option<&str>) -> bool {
        let ordered_access = ORDERED_ACCESSES
            .iter()
            .any(|access| base.starts_with(access));
        let ordered_atomic = atomic.is_some_and(|ordering| !ordering.is_empty());
        ordered_access || ordered_atomic || FENCES.contains(&base)
    }

    /// Whether Mach-O spells a vector arrangement on the mnemonic, as in `cmeq.16b`
    fn has_arrangement(mnemonic: &str) -> bool {
        let suffix = mnemonic.split_once('.').map(|(_, suffix)| suffix);
        suffix.is_some_and(|suffix| suffix.starts_with(|c: char| c.is_ascii_digit()))
    }

    /// Whether an operand names a SIMD or floating-point register: `v`, `q`, `d`, `s`, `h` or `b` and 0 to 31
    fn uses_vector_register(shape: &str) -> bool {
        pieces(shape).iter().any(|piece| {
            let Piece::Word(word) = piece else {
                return false;
            };
            let register = word.split('.').next().unwrap_or(word);
            let Some(number) = register.strip_prefix(['v', 'q', 'd', 's', 'h', 'b']) else {
                return false;
            };
            number.parse::<u8>().is_ok_and(|n| n <= 31)
        })
    }
}

/// x86-64, in AT&T syntax with LLVM's size suffixes
mod x86_64 {
    use super::{Class, Instruction};

    /// Instructions reading their memory operand without writing it
    const READ_ONLY: [&str; 9] = [
        "cmp", "test", "ucomis", "comis", "mul", "imul", "div", "idiv", "call",
    ];
    /// Instructions writing their memory destination without reading it
    const WRITE_ONLY: [&str; 4] = ["mov", "vmov", "set", "stos"];
    /// Instructions with a memory-looking operand that access no memory
    const NO_ACCESS: [&str; 3] = ["lea", "nop", "prefetch"];
    const FENCES: [&str; 3] = ["mfence", "lfence", "sfence"];
    const VECTOR_REGISTERS: [&str; 3] = ["%xmm", "%ymm", "%zmm"];

    pub fn classify(instruction: &Instruction) -> Class {
        let (prefix, base) = unprefixed(&instruction.mnemonic);
        let operands = split_operands(&instruction.shape);
        let memory: Vec<bool> = operands
            .iter()
            .map(|operand| operand.contains('('))
            .collect();
        let to_symbol = !instruction.symbols.is_empty();
        let jmp = base.starts_with("jmp");
        let (load, store) = memory_access(base, &memory);
        let locked_exchange = base.starts_with("xchg") && memory.contains(&true);
        let shape = &instruction.shape;
        Class {
            cond_branch: (base.starts_with('j') && !jmp) || base.starts_with("loop"),
            jump: jmp && !to_symbol,
            call: base.starts_with("call") || (jmp && to_symbol),
            select: base.starts_with("cmov") || is_setcc(base),
            load,
            store,
            simd: VECTOR_REGISTERS
                .iter()
                .any(|register| shape.contains(register)),
            barrier: prefix == Some("lock") || locked_exchange || FENCES.contains(&base),
        }
    }

    /// Prefix and the instruction it modifies, as `split_mnemonic` joined them
    fn unprefixed(mnemonic: &str) -> (Option<&str>, &str) {
        match mnemonic.split_once(' ') {
            Some((prefix, base)) => (Some(prefix), base),
            None => (None, mnemonic),
        }
    }

    /// Operands split at commas outside parentheses, `(%rdi,%rcx,8)` staying one operand
    fn split_operands(shape: &str) -> Vec<&str> {
        let mut operands = Vec::new();
        let mut depth = 0usize;
        let mut start = 0;
        for (index, c) in shape.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    operands.push(shape[start..index].trim());
                    start = index + 1;
                }
                _ => {}
            }
        }
        let last = shape[start..].trim();
        if !last.is_empty() {
            operands.push(last);
        }
        operands
    }

    /// Whether the instruction loads and stores, the AT&T destination being the last operand
    fn memory_access(base: &str, memory: &[bool]) -> (bool, bool) {
        let any_memory = memory.contains(&true);
        if NO_ACCESS.iter().any(|mnemonic| base.starts_with(mnemonic)) {
            return (false, false);
        }
        if base.starts_with("push") {
            return (any_memory, true);
        }
        if base.starts_with("pop") {
            return (true, any_memory);
        }
        let Some((&destination, sources)) = memory.split_last() else {
            return (false, false);
        };
        let reads_source = sources.contains(&true);
        if !destination {
            return (reads_source, false);
        }
        let compare_exchange = base.starts_with("cmpxchg");
        let bit_test =
            base.starts_with("bt") && !["bts", "btr", "btc"].iter().any(|b| base.starts_with(b));
        let read_only = READ_ONLY.iter().any(|mnemonic| base.starts_with(mnemonic));
        if (read_only && !compare_exchange) || bit_test || base.starts_with("jmp") {
            return (true, false);
        }
        if WRITE_ONLY.iter().any(|mnemonic| base.starts_with(mnemonic)) {
            return (reads_source, true);
        }
        (true, true)
    }

    /// `set<cc>`, at most six letters as in `setnae`
    fn is_setcc(base: &str) -> bool {
        base.starts_with("set") && base.len() <= 6
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Classes of one instruction, built the way `normalize` would
    fn class(arch: Arch, mnemonic: &str, shape: &str, symbols: &[&str]) -> Class {
        let symbols = symbols.iter().map(|symbol| symbol.to_string()).collect();
        let instruction = Instruction {
            mnemonic: mnemonic.to_string(),
            operands: shape.to_string(),
            shape: shape.to_string(),
            symbols,
        };
        classify(&instruction, arch)
    }

    #[test]
    fn aarch64_control_flow() {
        let cond = class(Arch::Aarch64, "b.ne", "L3", &[]);
        let test_branch = class(Arch::Aarch64, "tbnz", "w8, #0, L1", &[]);
        let jump = class(Arch::Aarch64, "b", "L1", &[]);
        let tail_call = class(Arch::Aarch64, "b", "sym", &["memcpy"]);
        let call = class(Arch::Aarch64, "bl", "sym", &["core::panicking::panic"]);
        let select = class(Arch::Aarch64, "csetm", "w12, lo", &[]);
        assert_eq!(
            cond,
            Class {
                cond_branch: true,
                ..Class::default()
            }
        );
        assert!(test_branch.cond_branch);
        assert_eq!(
            jump,
            Class {
                jump: true,
                ..Class::default()
            }
        );
        assert_eq!(
            tail_call,
            Class {
                call: true,
                ..Class::default()
            }
        );
        assert_eq!(
            call,
            Class {
                call: true,
                ..Class::default()
            }
        );
        assert_eq!(
            select,
            Class {
                select: true,
                ..Class::default()
            }
        );
    }

    #[test]
    fn aarch64_memory_and_ordering() {
        let load = class(Arch::Aarch64, "ldp", "x12, x10, [x11, #16]", &[]);
        let acquire = class(Arch::Aarch64, "ldar", "x14, [x13]", &[]);
        let release = class(Arch::Aarch64, "stlur", "x8, [x9, #-8]", &[]);
        let relaxed_rmw = class(Arch::Aarch64, "ldadd", "x1, x0, [x2]", &[]);
        let ordered_rmw = class(Arch::Aarch64, "casalb", "w1, w2, [x0]", &[]);
        let fence = class(Arch::Aarch64, "dmb", "ish", &[]);
        let prefetch = class(Arch::Aarch64, "prfm", "pldl1keep, [x0]", &[]);
        assert_eq!(
            load,
            Class {
                load: true,
                ..Class::default()
            }
        );
        assert_eq!(
            acquire,
            Class {
                load: true,
                barrier: true,
                ..Class::default()
            }
        );
        assert_eq!(
            release,
            Class {
                store: true,
                barrier: true,
                ..Class::default()
            }
        );
        assert_eq!(
            relaxed_rmw,
            Class {
                load: true,
                store: true,
                ..Class::default()
            }
        );
        assert_eq!(
            ordered_rmw,
            Class {
                load: true,
                store: true,
                barrier: true,
                ..Class::default()
            }
        );
        assert_eq!(
            fence,
            Class {
                barrier: true,
                ..Class::default()
            }
        );
        assert_eq!(prefetch, Class::default());
    }

    #[test]
    fn aarch64_simd_from_mnemonic_or_registers() {
        let mach_o = class(Arch::Aarch64, "cmeq.16b", "v0, v0, v1", &[]);
        let elf = class(Arch::Aarch64, "cmeq", "v0.16b, v0.16b, v1.16b", &[]);
        let vector_load = class(Arch::Aarch64, "ldr", "q0, [x13]", &[]);
        let to_general = class(Arch::Aarch64, "fmov", "x10, d0", &[]);
        let general = class(Arch::Aarch64, "add", "x9, x9, #1", &[]);
        assert!(mach_o.simd && elf.simd && to_general.simd);
        assert_eq!(
            vector_load,
            Class {
                load: true,
                simd: true,
                ..Class::default()
            }
        );
        assert!(!general.simd);
    }

    #[test]
    fn x86_control_flow_and_selects() {
        let cond = class(Arch::X86_64, "jne", "L5", &[]);
        let jump = class(Arch::X86_64, "jmp", "L7", &[]);
        let tail_call = class(Arch::X86_64, "jmpq", "*sym@GOTPCREL(%rip)", &["notify"]);
        let indirect_call = class(Arch::X86_64, "callq", "*%r14", &[]);
        let cmov = class(Arch::X86_64, "cmovneq", "%rcx, %rax", &[]);
        let setcc = class(Arch::X86_64, "sete", "%al", &[]);
        assert_eq!(
            cond,
            Class {
                cond_branch: true,
                ..Class::default()
            }
        );
        assert_eq!(
            jump,
            Class {
                jump: true,
                ..Class::default()
            }
        );
        assert_eq!(
            tail_call,
            Class {
                call: true,
                load: true,
                ..Class::default()
            }
        );
        assert_eq!(
            indirect_call,
            Class {
                call: true,
                ..Class::default()
            }
        );
        assert!(cmov.select && setcc.select);
    }

    #[test]
    fn x86_memory_follows_the_destination_operand() {
        let load = class(Arch::X86_64, "movq", "48(%rdx), %rcx", &[]);
        let store = class(Arch::X86_64, "movq", "$0, (%rdi)", &[]);
        let rmw = class(Arch::X86_64, "addq", "$1, 8(%rdi,%rcx,8)", &[]);
        let compare = class(Arch::X86_64, "cmpq", "$0, (%rdi)", &[]);
        let address = class(Arch::X86_64, "leaq", "anon.0(%rip), %rdx", &[]);
        let push = class(Arch::X86_64, "pushq", "%rbx", &[]);
        assert_eq!(
            load,
            Class {
                load: true,
                ..Class::default()
            }
        );
        assert_eq!(
            store,
            Class {
                store: true,
                ..Class::default()
            }
        );
        assert_eq!(
            rmw,
            Class {
                load: true,
                store: true,
                ..Class::default()
            }
        );
        assert_eq!(
            compare,
            Class {
                load: true,
                ..Class::default()
            }
        );
        assert_eq!(address, Class::default());
        assert_eq!(
            push,
            Class {
                store: true,
                ..Class::default()
            }
        );
    }

    #[test]
    fn x86_ordering() {
        let locked = class(Arch::X86_64, "lock cmpxchgq", "%rcx, (%rdi)", &[]);
        let exchange = class(Arch::X86_64, "xchgq", "%rcx, 48(%rdx)", &[]);
        let register_swap = class(Arch::X86_64, "xchgq", "%rcx, %rax", &[]);
        let fence = class(Arch::X86_64, "mfence", "", &[]);
        assert_eq!(
            locked,
            Class {
                load: true,
                store: true,
                barrier: true,
                ..Class::default()
            }
        );
        assert_eq!(
            exchange,
            Class {
                load: true,
                store: true,
                barrier: true,
                ..Class::default()
            }
        );
        assert!(!register_swap.barrier);
        assert!(fence.barrier);
    }
}
