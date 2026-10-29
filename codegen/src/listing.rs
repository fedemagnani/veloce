//! An emitted assembly file: the probes it exports and the body of each

use crate::target::Target;

/// Assembly of the probe library for one target
pub struct Listing<'a> {
    lines: Vec<&'a str>,
    triple: &'a str,
    // Mach-O prefixes C symbols with an underscore, ELF doesn't
    prefix: &'static str,
}

/// A probe and the instructions it compiles to
pub struct Probe<'a> {
    pub name: &'a str,
    /// The probe whose body this one shares, identical functions being merged into aliases
    pub merged_into: Option<&'a str>,
    /// Lines from the function's label to its `.cfi_endproc`
    pub body: Vec<&'a str>,
}

impl<'a> Listing<'a> {
    /// Splits `text`, emitted for `target`, into lines
    pub fn parse(text: &'a str, target: &'a Target) -> Self {
        let lines = text.lines().collect();
        Self {
            lines,
            triple: &target.triple,
            prefix: target.symbol_prefix(),
        }
    }

    /// Every exported unmangled symbol, in order of appearance, with its body
    pub fn probes(&self) -> Result<Vec<Probe<'a>>, String> {
        let names = self.exported_names();
        let probes = names.into_iter().map(|name| self.probe(name));
        probes.collect()
    }

    /// Unmangled `.globl` symbols: every Rust item is mangled, so these are exactly the `no_mangle` probes
    fn exported_names(&self) -> Vec<&'a str> {
        let mut names = Vec::new();
        for line in &self.lines {
            let Some(symbol) = line.trim().strip_prefix(".globl") else {
                continue;
            };
            let Some(name) = symbol.trim().strip_prefix(self.prefix) else {
                continue;
            };
            let mangled = name.starts_with("_ZN") || name.starts_with("_R");
            if !mangled && !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }

    /// The probe `name`, following an alias to the body it shares
    fn probe(&self, name: &'a str) -> Result<Probe<'a>, String> {
        let merged_into = self.alias_of(name);
        let source = merged_into.unwrap_or(name);
        let body = self
            .body_of(source)
            .ok_or_else(|| format!("no body for {name} in the {} listing", self.triple))?;
        Ok(Probe {
            name,
            merged_into,
            body,
        })
    }

    /// Target of `name = other` or `.set name, other`
    fn alias_of(&self, name: &str) -> Option<&'a str> {
        let symbol = format!("{}{name}", self.prefix);
        self.lines.iter().find_map(|line| {
            let line = line.trim();
            let assigned = line
                .strip_prefix(symbol.as_str())
                .and_then(|rest| rest.trim_start().strip_prefix('='));
            let set = line
                .strip_prefix(".set")
                .and_then(|rest| rest.trim().strip_prefix(symbol.as_str()));
            let set = set.and_then(|rest| rest.trim_start().strip_prefix(','));
            let other = assigned.or(set)?.trim();
            other.strip_prefix(self.prefix)
        })
    }

    /// Lines from `name:` to the next `.cfi_endproc`, both included
    fn body_of(&self, name: &str) -> Option<Vec<&'a str>> {
        let label = format!("{}{name}:", self.prefix);
        let start = self.lines.iter().position(|line| *line == label)?;
        let rest = &self.lines[start..];
        let end = rest.iter().position(|line| line.trim() == ".cfi_endproc")?;
        Some(rest[..=end].to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const MACH_O: &str = "\t.globl\t_probe_a
\t.p2align\t2
_probe_a:
\t.cfi_startproc
\tadd\tx0, x0, #1
\tret
\t.cfi_endproc

\t.globl\t__RNvCs1_7example6helper
__RNvCs1_7example6helper:
\t.cfi_startproc
\tret
\t.cfi_endproc

\t.globl\t_probe_b
_probe_b = _probe_a";

    const ELF: &str = "\t.globl\tprobe_c
\t.type\tprobe_c,@function
probe_c:
\t.cfi_startproc
\tleaq\t1(%rdi), %rax
\tretq
.Lfunc_end0:
\t.cfi_endproc
\t.globl\t_ZN7example6helper17h0123456789abcdefE
\t.globl\tprobe_d
\t.set probe_d, probe_c";

    #[test]
    fn mach_o_probes_skip_mangled_symbols_and_follow_aliases() {
        let target = Target::parse("aarch64-apple-darwin").unwrap();
        let listing = Listing::parse(MACH_O, &target);
        let probes = listing.probes().unwrap();
        let names: Vec<&str> = probes.iter().map(|probe| probe.name).collect();
        assert_eq!(names, ["probe_a", "probe_b"]);
        assert_eq!(probes[0].merged_into, None);
        assert_eq!(
            probes[0].body,
            [
                "_probe_a:",
                "\t.cfi_startproc",
                "\tadd\tx0, x0, #1",
                "\tret",
                "\t.cfi_endproc"
            ]
        );
        assert_eq!(probes[1].merged_into, Some("probe_a"));
        assert_eq!(probes[1].body, probes[0].body);
    }

    #[test]
    fn elf_probes_have_no_prefix_and_accept_set_aliases() {
        let target = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        let listing = Listing::parse(ELF, &target);
        let probes = listing.probes().unwrap();
        let names: Vec<&str> = probes.iter().map(|probe| probe.name).collect();
        assert_eq!(names, ["probe_c", "probe_d"]);
        assert_eq!(probes[0].body.len(), 6);
        assert_eq!(probes[1].merged_into, Some("probe_c"));
    }

    #[test]
    fn an_exported_symbol_without_a_body_is_an_error() {
        let target = Target::parse("aarch64-apple-darwin").unwrap();
        let listing = Listing::parse("\t.globl\t_lonely", &target);
        let error = listing.probes().err().unwrap();
        assert!(error.contains("no body for lonely"), "{error}");
    }
}
