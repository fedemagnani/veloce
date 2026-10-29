//! A target triple and the assembly conventions that follow from it

/// Instruction set of a target, deciding how instructions are classified
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Aarch64,
    X86_64,
}

/// Object format of a target, deciding how symbols and local labels are spelled
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Apple targets: C symbols prefixed with `_`, local labels `L…`, private data `l_…`
    MachO,
    /// Everything else LLVM emits for: no symbol prefix, local labels and private data `.L…`
    Elf,
}

/// A target triple with its instruction set and object format
pub struct Target {
    pub triple: String,
    pub arch: Arch,
    pub format: Format,
}

impl Target {
    /// Parses `triple`, failing on an instruction set the classifier has no tables for
    pub fn parse(triple: &str) -> Result<Self, String> {
        let arch = if triple.starts_with("aarch64") || triple.starts_with("arm64") {
            Arch::Aarch64
        } else if triple.starts_with("x86_64") {
            Arch::X86_64
        } else {
            return Err(format!(
                "unsupported target {triple}: only aarch64 and x86_64"
            ));
        };
        let format = if triple.contains("-apple-") {
            Format::MachO
        } else {
            Format::Elf
        };
        Ok(Self {
            triple: triple.to_string(),
            arch,
            format,
        })
    }

    /// Prefix the object format puts before C symbol names
    pub fn symbol_prefix(&self) -> &'static str {
        match self.format {
            Format::MachO => "_",
            Format::Elf => "",
        }
    }

    /// Markers starting a comment that runs to the end of the line
    pub fn comment_markers(&self) -> &'static [&'static str] {
        match self.arch {
            Arch::Aarch64 => &[";", "//"],
            Arch::X86_64 => &["#"],
        }
    }

    /// Whether `token` names a private data constant, such as a panic location
    pub fn is_local_data(&self, token: &str) -> bool {
        match self.format {
            Format::MachO => token.starts_with("l_") || token.starts_with("l___"),
            Format::Elf => token.starts_with(".Lanon.") || token.starts_with(".L__unnamed_"),
        }
    }

    /// Whether `token` names a local code label, a branch target or an assembler temporary
    pub fn is_local_label(&self, token: &str) -> bool {
        let local = match self.format {
            Format::MachO => token.starts_with('L'),
            Format::Elf => token.starts_with(".L"),
        };
        local && !self.is_local_data(token)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn triples_map_to_arch_and_format() {
        let apple = Target::parse("aarch64-apple-darwin").unwrap();
        let linux = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        let wasm = Target::parse("wasm32-unknown-unknown");
        assert_eq!((apple.arch, apple.format), (Arch::Aarch64, Format::MachO));
        assert_eq!((linux.arch, linux.format), (Arch::X86_64, Format::Elf));
        assert!(wasm.is_err());
    }

    #[test]
    fn local_labels_and_data_are_told_apart() {
        let apple = Target::parse("aarch64-apple-darwin").unwrap();
        let linux = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        assert!(apple.is_local_label("LBB0_3"));
        assert!(!apple.is_local_label("l_anon.0123.1"));
        assert!(apple.is_local_data("l_anon.0123.1"));
        assert!(linux.is_local_label(".LBB1_5"));
        assert!(linux.is_local_data(".Lanon.0123.2"));
        assert!(!linux.is_local_label("LBB1_5"));
    }
}
