//! Normalized probe bodies: build-specific names replaced, so identical code reads identically across builds

use std::collections::{HashMap, HashSet};

use crate::target::{Arch, Format, Target};

/// Prefixes x86 prints as their own mnemonic before the instruction they modify
const X86_PREFIXES: [&str; 6] = ["lock", "rep", "repe", "repz", "repne", "repnz"];

/// A line of a normalized body
pub enum Line {
    /// The function's own name, or a renumbered branch target
    Label(String),
    Instruction(Instruction),
}

/// An instruction with its operands normalized
pub struct Instruction {
    /// Mnemonic, an x86 prefix such as `lock` included
    pub mnemonic: String,
    /// Operands as displayed: labels and constants renumbered, symbols demangled
    pub operands: String,
    /// Operands with every global symbol replaced by `sym`, for classification
    pub shape: String,
    /// Demangled global symbols the operands reference, in order
    pub symbols: Vec<String>,
}

/// Normalizes a probe body, from its label to its `.cfi_endproc`
pub fn normalize(body: &[&str], target: &Target) -> Vec<Line> {
    let raw_lines = parse_lines(body, target);
    let referenced = referenced_labels(&raw_lines, target);
    let mut names = Names::default();
    let mut lines = Vec::new();
    for raw in &raw_lines {
        match raw {
            RawLine::Label(label) if target.is_local_label(label) => {
                // assembler temporaries and linker hints nothing branches to carry no information
                if referenced.contains(*label) {
                    let renamed = names.label(label);
                    lines.push(Line::Label(renamed));
                }
            }
            RawLine::Label(label) if target.is_local_data(label) => {}
            RawLine::Label(label) => lines.push(Line::Label(display_symbol(label, target))),
            RawLine::Instruction { mnemonic, operands } => {
                let instruction = rewrite(mnemonic, operands, target, &mut names);
                lines.push(Line::Instruction(instruction));
            }
        }
    }
    lines
}

impl Line {
    /// The line as written to a listing
    pub fn render(&self) -> String {
        match self {
            Line::Label(name) => format!("{name}:"),
            Line::Instruction(instruction) if instruction.operands.is_empty() => {
                format!("\t{}", instruction.mnemonic)
            }
            Line::Instruction(instruction) => {
                format!("\t{}\t{}", instruction.mnemonic, instruction.operands)
            }
        }
    }
}

/// A body line before renaming, directives and comments already dropped
enum RawLine<'a> {
    Label(&'a str),
    Instruction { mnemonic: String, operands: String },
}

/// Drops comments, blank lines and directives, splitting the rest into labels and instructions
fn parse_lines<'a>(body: &[&'a str], target: &Target) -> Vec<RawLine<'a>> {
    let mut lines = Vec::new();
    for line in body {
        let code = strip_comment(line, target).trim();
        if code.is_empty() {
            continue;
        }
        if let Some(label) = code.strip_suffix(':')
            && !label.contains(char::is_whitespace)
        {
            lines.push(RawLine::Label(label));
            continue;
        }
        if code.starts_with('.') {
            continue;
        }
        let (mnemonic, operands) = split_mnemonic(code);
        lines.push(RawLine::Instruction { mnemonic, operands });
    }
    lines
}

/// `line` up to its first comment marker
fn strip_comment<'a>(line: &'a str, target: &Target) -> &'a str {
    let markers = target.comment_markers().iter();
    let start = markers.filter_map(|marker| line.find(marker)).min();
    match start {
        Some(start) => &line[..start],
        None => line,
    }
}

/// Mnemonic, an x86 prefix joined to the instruction it modifies, and operands with whitespace collapsed
fn split_mnemonic(code: &str) -> (String, String) {
    let mut words = code.split_whitespace();
    let first = words.next().unwrap_or_default();
    let mut mnemonic = first.to_string();
    if X86_PREFIXES.contains(&first)
        && let Some(modified) = words.next()
    {
        mnemonic = format!("{first} {modified}");
    }
    let operands: Vec<&str> = words.collect();
    (mnemonic, operands.join(" "))
}

/// Local labels some instruction refers to
fn referenced_labels<'a>(lines: &[RawLine<'a>], target: &Target) -> HashSet<String> {
    let mut referenced = HashSet::new();
    for line in lines {
        let RawLine::Instruction { operands, .. } = line else {
            continue;
        };
        for piece in pieces(operands) {
            if let Piece::Word(word) = piece
                && target.is_local_label(word)
            {
                referenced.insert(word.to_string());
            }
        }
    }
    referenced
}

/// Renames the words of `operands`: labels and constants renumbered, global symbols demangled and masked in the shape
fn rewrite(mnemonic: &str, operands: &str, target: &Target, names: &mut Names) -> Instruction {
    let pieces = pieces(operands);
    let mut display = String::new();
    let mut shape = String::new();
    let mut symbols = Vec::new();
    for (i, piece) in pieces.iter().enumerate() {
        let word = match piece {
            Piece::Word(word) => *word,
            Piece::Other(text) => {
                display.push_str(text);
                shape.push_str(text);
                continue;
            }
        };
        let before = neighbour_char(&pieces, i, Side::Before);
        let after = neighbour_char(&pieces, i, Side::After);
        if target.is_local_label(word) {
            let renamed = names.label(word);
            display.push_str(&renamed);
            shape.push_str(&renamed);
        } else if target.is_local_data(word) {
            let renamed = names.data(word);
            display.push_str(&renamed);
            shape.push_str(&renamed);
        } else if is_global_symbol(word, before, after, mnemonic, target) {
            let symbol = display_symbol(word, target);
            display.push_str(&symbol);
            shape.push_str("sym");
            symbols.push(symbol);
        } else {
            display.push_str(word);
            shape.push_str(word);
        }
    }
    Instruction {
        mnemonic: mnemonic.to_string(),
        operands: display,
        shape,
        symbols,
    }
}

/// A run of characters a symbol can contain, or the text between two runs
#[derive(Debug, PartialEq)]
pub enum Piece<'a> {
    Word(&'a str),
    Other(&'a str),
}

/// Splits `text` into words, the runs of characters a symbol, label or register can contain, and the text between
pub fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut in_word = false;
    for (index, c) in text.char_indices() {
        let is_word_char = c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$');
        if is_word_char != in_word && index > start {
            let piece = &text[start..index];
            pieces.push(if in_word {
                Piece::Word(piece)
            } else {
                Piece::Other(piece)
            });
            start = index;
        }
        in_word = is_word_char;
    }
    if start < text.len() {
        let piece = &text[start..];
        pieces.push(if in_word {
            Piece::Word(piece)
        } else {
            Piece::Other(piece)
        });
    }
    pieces
}

/// Which neighbour of a word to look at
enum Side {
    Before,
    After,
}

/// Character right before or after the word at `index`, from the separator next to it
fn neighbour_char(pieces: &[Piece<'_>], index: usize, side: Side) -> Option<char> {
    let neighbour = match side {
        Side::Before => index.checked_sub(1).and_then(|i| pieces.get(i)),
        Side::After => pieces.get(index + 1),
    };
    let Some(Piece::Other(text)) = neighbour else {
        return None;
    };
    match side {
        Side::Before => text.chars().next_back(),
        Side::After => text.chars().next(),
    }
}

/// Whether `word`, between `before` and `after`, names a global symbol rather than a register, number or modifier
fn is_global_symbol(
    word: &str,
    before: Option<char>,
    after: Option<char>,
    mnemonic: &str,
    target: &Target,
) -> bool {
    let starts_like_name = word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    // `@PAGE`, `@GOTPCREL` and friends name a relocation, not a symbol
    if !starts_like_name || before == Some('@') {
        return false;
    }
    if target.format == Format::MachO {
        return word.starts_with('_');
    }
    if rustc_demangle::try_demangle(word).is_ok() {
        return true;
    }
    match target.arch {
        // AT&T syntax prefixes registers with `%` and immediates with `$`, so a bare name is a symbol
        Arch::X86_64 => before != Some('%'),
        // registers are bare on aarch64: a symbol follows a relocation specifier like `:lo12:` or is a branch target
        Arch::Aarch64 => {
            let after_specifier = before == Some(':') && after != Some(':');
            after_specifier || matches!(mnemonic, "b" | "bl" | "adrp")
        }
    }
}

/// `symbol` without the object format's prefix, demangled without its hash when it is a Rust symbol
fn display_symbol(symbol: &str, target: &Target) -> String {
    let prefix = target.symbol_prefix();
    let name = symbol.strip_prefix(prefix).unwrap_or(symbol);
    match rustc_demangle::try_demangle(name) {
        Ok(demangled) => format!("{demangled:#}"),
        Err(_) => name.to_string(),
    }
}

/// Renumbering of local labels and constants, in order of first appearance
#[derive(Default)]
struct Names {
    labels: HashMap<String, String>,
    data: HashMap<String, String>,
}

impl Names {
    /// `L<n>` for the `n`th distinct label
    fn label(&mut self, label: &str) -> String {
        let next = self.labels.len();
        let renamed = self.labels.entry(label.to_string());
        renamed.or_insert_with(|| format!("L{next}")).clone()
    }

    /// `anon.<n>` for the `n`th distinct constant
    fn data(&mut self, constant: &str) -> String {
        let next = self.data.len();
        let renamed = self.data.entry(constant.to_string());
        renamed.or_insert_with(|| format!("anon.{next}")).clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const MACH_O: &str = "_probe:
\t.cfi_startproc
; %bb.0:
\tcbz\tx1, LBB0_3
Lloh0:
\tadrp\tx2, l_anon.9753f0dab2d62c682f6068fa09a93d9e.2@PAGE
\tcmeq.16b\tv0, v0, v1
LBB0_3:
\tbl\t__RNvNtCs3qDxio5cMcq_4core9panicking18panic_bounds_check
\tret
\t.loh AdrpAdd\tLloh0, Lloh1
\t.cfi_endproc";

    const ELF: &str = "probe:
\t.cfi_startproc
# %bb.0:
\tleaq\t.Lanon.7cd81fdc10400127e36de415da1903cf.1(%rip), %rdx
\tlock\t\tcmpxchgq\t%rcx, (%rdi)
\tjne\t.LBB1_18
.Ltmp0:
.LBB1_18:
\tcallq\t*_RNvNtCsgO42Vx8rMzK_4core9panicking18panic_bounds_check@GOTPCREL(%rip)
.Lfunc_end1:
\t.size\tprobe, .Lfunc_end1-probe
\t.cfi_endproc";

    /// Rendered normalized body of `text` for `triple`
    fn rendered(text: &str, triple: &str) -> Vec<String> {
        let target = Target::parse(triple).unwrap();
        let body: Vec<&str> = text.lines().collect();
        let lines = normalize(&body, &target);
        lines.iter().map(Line::render).collect()
    }

    #[test]
    fn mach_o_directives_hints_and_hashes_go_away() {
        let lines = rendered(MACH_O, "aarch64-apple-darwin");
        let expected = [
            "probe:",
            "\tcbz\tx1, L0",
            "\tadrp\tx2, anon.0@PAGE",
            "\tcmeq.16b\tv0, v0, v1",
            "L0:",
            "\tbl\tcore::panicking::panic_bounds_check",
            "\tret",
        ];
        assert_eq!(lines, expected);
    }

    #[test]
    fn elf_prefixes_join_and_unreferenced_labels_drop() {
        let lines = rendered(ELF, "x86_64-unknown-linux-gnu");
        let expected = [
            "probe:",
            "\tleaq\tanon.0(%rip), %rdx",
            "\tlock cmpxchgq\t%rcx, (%rdi)",
            "\tjne\tL0",
            "L0:",
            "\tcallq\t*core::panicking::panic_bounds_check@GOTPCREL(%rip)",
        ];
        assert_eq!(lines, expected);
    }

    #[test]
    fn build_specific_names_do_not_change_the_output() {
        let other_build = MACH_O
            .replace("LBB0_3", "LBB7_12")
            .replace(
                "9753f0dab2d62c682f6068fa09a93d9e.2",
                "0123456789abcdef0123456789abcdef.5",
            )
            .replace("Cs3qDxio5cMcq_", "CsgO42Vx8rMzK_");
        let lines = rendered(MACH_O, "aarch64-apple-darwin");
        let other_lines = rendered(&other_build, "aarch64-apple-darwin");
        assert_eq!(lines, other_lines);
    }

    #[test]
    fn shape_masks_symbols_and_keeps_registers() {
        let target = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        let body = ["\tmovq\tmemcpy@GOTPCREL(%rip), %r14"];
        let lines = normalize(&body, &target);
        let Line::Instruction(instruction) = &lines[0] else {
            panic!("expected an instruction");
        };
        assert_eq!(instruction.shape, "sym@GOTPCREL(%rip), %r14");
        assert_eq!(instruction.symbols, ["memcpy"]);
    }

    #[test]
    fn pieces_split_words_from_separators() {
        let split = pieces("[x8, #16]!");
        let expected = [
            Piece::Other("["),
            Piece::Word("x8"),
            Piece::Other(", #"),
            Piece::Word("16"),
            Piece::Other("]!"),
        ];
        assert_eq!(split, expected);
    }
}
