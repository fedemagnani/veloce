//! Attribution of probe instructions to watched functions, through the DWARF inline tree of the host object
//!
//! - each inlined instance whose origin is a watched function claims its address ranges, the outermost match only
//! - each listing instruction gets its address by decoding the object alongside the listing, padding included

use std::ops::Range;
use std::path::{Component, Path};
use std::rc::Rc;

use gimli::{
    AttributeValue, DebuggingInformationEntry, Dwarf, EndianSlice, EntriesTreeNode, Reader as _,
    RelocateReader, RunTimeEndian, SectionId, Unit, UnitOffset,
};
use iced_x86::{Decoder, DecoderOptions, Mnemonic};
use object::{Object, ObjectSection, ObjectSymbol, RelocationMap};

use crate::normalize::{Alignment, Slot};
use crate::registry::Watch;
use crate::target::{Arch, Target};

/// The AArch64 `nop`, the padding of aligned code
const AARCH64_NOP: [u8; 4] = [0x1f, 0x20, 0x03, 0xd5];

/// DWARF read through the object's relocations, which relocatable ELF needs for every address
type Reader<'a> = RelocateReader<EndianSlice<'a, RunTimeEndian>, Relocations>;

/// The host object, parsed once per target
pub struct Inlines<'a> {
    file: object::File<'a>,
    dwarf: Dwarf<Reader<'a>>,
}

/// A watched function inlined into a probe, with the offsets from the probe's start it covers
pub struct Watched {
    /// Index into the watches it was matched against
    pub watch: usize,
    /// The inlined function's path, demangled without its hash
    pub display: String,
    pub ranges: Vec<Range<u64>>,
}

impl<'a> Inlines<'a> {
    /// Parses the object and its DWARF
    pub fn parse(object: &'a [u8]) -> Result<Self, String> {
        let file =
            object::File::parse(object).map_err(|error| format!("parsing the object: {error}"))?;
        let endian = if file.is_little_endian() {
            RunTimeEndian::Little
        } else {
            RunTimeEndian::Big
        };
        let load = |id: SectionId| -> Result<Reader<'a>, String> {
            let section = file.section_by_name(id.name());
            let data = section
                .as_ref()
                .and_then(|section| section.data().ok())
                .unwrap_or(&[]);
            // Mach-O keeps the addresses in place, so a map it can't build changes nothing
            let map = section
                .and_then(|section| section.relocation_map().ok())
                .unwrap_or_default();
            let reader = EndianSlice::new(data, endian);
            Ok(RelocateReader::new(reader, Relocations(Rc::new(map))))
        };
        let dwarf = Dwarf::load(load)?;
        Ok(Self { file, dwarf })
    }

    /// Every watched function inlined into the probe `symbol`, offsets relative to its start
    pub fn watched(&self, symbol: &str, watches: &[Watch]) -> Result<Vec<Watched>, String> {
        let (unit, offset, low_pc) = self.subprogram(symbol)?;
        let mut tree = unit.entries_tree(Some(offset)).map_err(dwarf_error)?;
        let root = tree.root().map_err(dwarf_error)?;
        let mut found = Vec::new();
        self.walk(&unit, root, watches, low_pc, &mut found)?;
        Ok(found)
    }

    /// Offset from the probe's start of each instruction of its `layout`, padding skipped where the listing aligns
    pub fn offsets(
        &self,
        symbol: &str,
        layout: &[Slot],
        target: &Target,
    ) -> Result<Vec<u64>, String> {
        let name = format!("{}{symbol}", target.symbol_prefix());
        let found = self
            .file
            .symbols()
            .find(|candidate| candidate.name() == Ok(name.as_str()));
        let symbol_entry = found.ok_or_else(|| format!("no symbol {name} in the object"))?;
        let index = symbol_entry
            .section_index()
            .ok_or_else(|| format!("symbol {name} is in no section"))?;
        let section = self.file.section_by_index(index).map_err(object_error)?;
        let data = section.data().map_err(object_error)?;
        let start = symbol_entry.address() - section.address();
        let mut cursor = Cursor {
            data,
            section_address: section.address(),
            position: usize::try_from(start)
                .map_err(|_| "symbol past the address space".to_string())?,
            arch: target.arch,
        };
        let mut offsets = Vec::new();
        for slot in layout {
            match slot {
                Slot::Align(alignment) => cursor.skip_padding(*alignment)?,
                Slot::Instruction => {
                    let offset = cursor.position as u64 - start;
                    cursor.next_instruction()?;
                    offsets.push(offset);
                }
            }
        }
        let extent = self.extent(symbol)?;
        let decoded = cursor.position as u64 - start;
        if decoded != extent {
            return Err(format!("{decoded} bytes decoded, the DWARF spans {extent}"));
        }
        Ok(offsets)
    }

    /// Unit, entry and start address of the subprogram of `symbol`
    fn subprogram(&self, symbol: &str) -> Result<(Unit<Reader<'a>>, UnitOffset, u64), String> {
        let mut headers = self.dwarf.units();
        while let Some(header) = headers.next().map_err(dwarf_error)? {
            let unit = self.dwarf.unit(header).map_err(dwarf_error)?;
            let mut entries = unit.entries();
            while let Some(entry) = entries.next_dfs().map_err(dwarf_error)? {
                if entry.tag() != gimli::DW_TAG_subprogram {
                    continue;
                }
                let Some(low_pc) = low_pc(entry) else {
                    continue;
                };
                if self.name(&unit, entry).as_deref() == Some(symbol) {
                    let offset = entry.offset();
                    return Ok((unit, offset, low_pc));
                }
            }
        }
        Err(format!("no DWARF subprogram for {symbol}"))
    }

    /// Bytes the subprogram of `symbol` spans, from its first range's start to its last range's end
    fn extent(&self, symbol: &str) -> Result<u64, String> {
        let (unit, offset, low_pc) = self.subprogram(symbol)?;
        let entry = unit.entry(offset).map_err(dwarf_error)?;
        let ranges = self.ranges(&unit, &entry)?;
        let end = ranges.iter().map(|range| range.end).max().unwrap_or(low_pc);
        Ok(end - low_pc)
    }

    /// Records the outermost watched instances under `node`, then looks no deeper in them
    fn walk(
        &self,
        unit: &Unit<Reader<'a>>,
        node: EntriesTreeNode<'_, '_, Reader<'a>>,
        watches: &[Watch],
        low_pc: u64,
        found: &mut Vec<Watched>,
    ) -> Result<(), String> {
        let entry = node.entry();
        if entry.tag() == gimli::DW_TAG_inlined_subroutine
            && let Some((watch, display)) = self.matching_watch(unit, entry, watches)?
        {
            let ranges = self.ranges(unit, entry)?;
            let relative = ranges
                .into_iter()
                .map(|range| range.start - low_pc..range.end - low_pc);
            found.push(Watched {
                watch,
                display,
                ranges: relative.collect(),
            });
            return Ok(());
        }
        let mut children = node.children();
        while let Some(child) = children.next().map_err(dwarf_error)? {
            self.walk(unit, child, watches, low_pc, found)?;
        }
        Ok(())
    }

    /// The watch an inlined instance's origin matches: same name, file, and declaration within the watched lines
    fn matching_watch(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
        watches: &[Watch],
    ) -> Result<Option<(usize, String)>, String> {
        let Some(origin) = self.origin(unit, entry)? else {
            return Ok(None);
        };
        let declaration = self.declaration(unit, &origin)?;
        let Some(declaration) = declaration else {
            return Ok(None);
        };
        let matched = watches.iter().position(|watch| declaration.matches(watch));
        let display = declaration.display;
        Ok(matched.map(|index| (index, display)))
    }

    /// The entry an inlined instance's `DW_AT_abstract_origin` points to, within the same unit
    fn origin(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Result<Option<DebuggingInformationEntry<Reader<'a>>>, String> {
        let value = entry.attr_value(gimli::DW_AT_abstract_origin);
        let offset = match value {
            Some(AttributeValue::UnitRef(offset)) => offset,
            Some(AttributeValue::DebugInfoRef(offset)) => {
                let Some(offset) = offset.to_unit_offset(&unit.header) else {
                    return Ok(None);
                };
                offset
            }
            _ => return Ok(None),
        };
        let origin = unit.entry(offset).map_err(dwarf_error)?;
        Ok(Some(origin))
    }

    /// Name, file and line a function is declared at, following `DW_AT_specification` for what's missing
    fn declaration(
        &self,
        unit: &Unit<Reader<'a>>,
        origin: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Result<Option<Declaration>, String> {
        let mut entry = origin.clone();
        // a definition may leave its name and position to the declaration it specifies
        for _ in 0..2 {
            let name = self.name(unit, &entry);
            let line = entry
                .attr_value(gimli::DW_AT_decl_line)
                .and_then(|value| value.udata_value());
            let file = entry.attr_value(gimli::DW_AT_decl_file);
            if let (Some(name), Some(line), Some(file)) = (name, line, file) {
                let file = self.file_path(unit, file);
                let display = self.display(unit, &entry).unwrap_or_else(|| name.clone());
                return Ok(file.map(|file| Declaration {
                    name,
                    line,
                    file,
                    display,
                }));
            }
            let Some(AttributeValue::UnitRef(offset)) =
                entry.attr_value(gimli::DW_AT_specification)
            else {
                return Ok(None);
            };
            entry = unit.entry(offset).map_err(dwarf_error)?;
        }
        Ok(None)
    }

    /// `DW_AT_name` of an entry
    fn name(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<String> {
        let value = entry.attr_value(gimli::DW_AT_name)?;
        self.string(unit, value)
    }

    /// The linkage name, demangled without its hash and shortened, for the reader of the stats
    fn display(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Option<String> {
        let value = entry.attr_value(gimli::DW_AT_linkage_name)?;
        let mangled = self.string(unit, value)?;
        let demangled = rustc_demangle::try_demangle(&mangled).ok()?;
        let path = format!("{demangled:#}");
        Some(short(&path))
    }

    /// The path a `DW_AT_decl_file` index names, joined to its directory and the unit's compilation directory
    fn file_path(
        &self,
        unit: &Unit<Reader<'a>>,
        value: AttributeValue<Reader<'a>>,
    ) -> Option<String> {
        let index = match value {
            AttributeValue::FileIndex(index) => index,
            other => other.udata_value()?,
        };
        let program = unit.line_program.as_ref()?;
        let header = program.header();
        let file = header.file(index)?;
        let name = self.string(unit, file.path_name())?;
        let directory = file
            .directory(header)
            .and_then(|directory| self.string(unit, directory))
            .unwrap_or_default();
        let compilation = unit
            .comp_dir
            .as_ref()
            .and_then(|dir| dir.to_string_lossy().ok().map(|dir| dir.into_owned()))
            .unwrap_or_default();
        let joined = Path::new(&compilation).join(directory).join(name);
        Some(joined.to_string_lossy().into_owned())
    }

    /// An attribute's string, wherever the DWARF version keeps it
    fn string(&self, unit: &Unit<Reader<'a>>, value: AttributeValue<Reader<'a>>) -> Option<String> {
        let reader = self.dwarf.attr_string(unit, value).ok()?;
        let text = reader.to_string_lossy().ok()?;
        Some(text.into_owned())
    }

    /// Address ranges of an entry, `DW_AT_low_pc`/`DW_AT_high_pc` or `DW_AT_ranges`
    fn ranges(
        &self,
        unit: &Unit<Reader<'a>>,
        entry: &DebuggingInformationEntry<Reader<'a>>,
    ) -> Result<Vec<Range<u64>>, String> {
        let mut iter = self.dwarf.die_ranges(unit, entry).map_err(dwarf_error)?;
        let mut ranges = Vec::new();
        while let Some(range) = iter.next().map_err(dwarf_error)? {
            ranges.push(range.begin..range.end);
        }
        Ok(ranges)
    }
}

/// Where and as what a function is declared
struct Declaration {
    name: String,
    line: u64,
    file: String,
    display: String,
}

impl Declaration {
    /// Same name, generics aside; declared inside the watched lines of the same file
    fn matches(&self, watch: &Watch) -> bool {
        let base_name = self.name.split('<').next().unwrap_or(&self.name);
        let in_lines = (watch.first_line..=watch.last_line).contains(&self.line);
        base_name == watch.name && in_lines && same_file(&self.file, &watch.file)
    }
}

/// `<veloce::swmr::writer::Writer<u64>>::stage::<{closure}>` as `Writer<u64>::stage`: modules, turbofish and the
/// brackets of an inherent impl dropped
fn short(path: &str) -> String {
    let unqualified = drop_qualifiers(path);
    let without_turbofish = drop_turbofish(&unqualified);
    drop_inherent_brackets(&without_turbofish)
}

/// Every `name::` followed by another name, keeping the last segment of each path
fn drop_qualifiers(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(start) = rest.find(|c: char| c.is_alphanumeric() || c == '_') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let (word, after) = rest.split_at(end);
        let qualifies = after
            .strip_prefix("::")
            .is_some_and(|next| next.starts_with(|c: char| c.is_alphanumeric() || c == '_'));
        if qualifies {
            rest = &after[2..];
        } else {
            out.push_str(word);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// A trailing `::<..>` of the function's own generics
fn drop_turbofish(path: &str) -> String {
    let mut depth = 0usize;
    let mut last_separator = None;
    let bytes = path.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'<' => depth += 1,
            b'>' => depth = depth.saturating_sub(1),
            b':' if depth == 0 && bytes.get(index + 1) == Some(&b':') => {
                last_separator = Some(index)
            }
            _ => {}
        }
    }
    match last_separator {
        Some(index) if path[index + 2..].starts_with('<') => path[..index].to_string(),
        _ => path.to_string(),
    }
}

/// `<Type>::method` as `Type::method`, a trait impl's `<Type as Trait>` kept
fn drop_inherent_brackets(path: &str) -> String {
    if !path.starts_with('<') {
        return path.to_string();
    }
    let mut depth = 0usize;
    for (index, character) in path.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    let inner = &path[1..index];
                    let is_inherent = !inner.contains(" as ");
                    if is_inherent && path[index + 1..].starts_with("::") {
                        return format!("{inner}{}", &path[index + 1..]);
                    }
                    return path.to_string();
                }
            }
            _ => {}
        }
    }
    path.to_string()
}

/// Whether two paths name the same file, a relative one matching the end of the other
fn same_file(dwarf: &str, watched: &str) -> bool {
    let dwarf: Vec<Component<'_>> = Path::new(dwarf).components().collect();
    let watched: Vec<Component<'_>> = Path::new(watched)
        .components()
        .filter(|component| *component != Component::CurDir)
        .collect();
    !watched.is_empty() && dwarf.ends_with(&watched)
}

/// Decoding position in a section's bytes
struct Cursor<'d> {
    data: &'d [u8],
    section_address: u64,
    position: usize,
    arch: Arch,
}

impl Cursor<'_> {
    /// Steps over one instruction, returning whether it is a `nop`
    fn next_instruction(&mut self) -> Result<bool, String> {
        let rest = self.data.get(self.position..).unwrap_or_default();
        let (length, is_nop) = match self.arch {
            Arch::Aarch64 => {
                let word = rest
                    .get(..4)
                    .ok_or("an instruction runs past its section")?;
                (4, word == AARCH64_NOP)
            }
            Arch::X86_64 => {
                let mut decoder = Decoder::new(64, rest, DecoderOptions::NONE);
                let instruction = decoder.decode();
                if instruction.is_invalid() {
                    return Err(format!("undecodable x86 code at {:#x}", self.position));
                }
                (instruction.len(), instruction.mnemonic() == Mnemonic::Nop)
            }
        };
        self.position += length;
        Ok(is_nop)
    }

    /// Steps over the `nop`s an alignment directive put here
    fn skip_padding(&mut self, alignment: Alignment) -> Result<(), String> {
        let address = self.section_address + self.position as u64;
        let mut padding = (alignment.bytes - address % alignment.bytes) % alignment.bytes;
        if alignment.max_skip.is_some_and(|max| padding > max) {
            padding = 0;
        }
        let target = self.position as u64 + padding;
        while (self.position as u64) < target {
            let is_nop = self.next_instruction()?;
            if !is_nop {
                return Err(format!("expected padding at {:#x}", self.position));
            }
        }
        if self.position as u64 != target {
            return Err(format!(
                "padding overran its alignment at {:#x}",
                self.position
            ));
        }
        Ok(())
    }
}

/// The address of `DW_AT_low_pc`, present on a function with code
fn low_pc(entry: &DebuggingInformationEntry<Reader<'_>>) -> Option<u64> {
    match entry.attr_value(gimli::DW_AT_low_pc)? {
        AttributeValue::Addr(address) => Some(address),
        _ => None,
    }
}

/// `object`'s relocation map as gimli applies it
#[derive(Debug, Clone)]
struct Relocations(Rc<RelocationMap>);

impl gimli::Relocate for Relocations {
    fn relocate_address(&self, offset: usize, value: u64) -> gimli::Result<u64> {
        Ok(self.0.relocate(offset as u64, value))
    }

    fn relocate_offset(&self, offset: usize, value: usize) -> gimli::Result<usize> {
        let relocated = self.0.relocate(offset as u64, value as u64);
        usize::try_from(relocated).map_err(|_| gimli::Error::UnsupportedOffset)
    }
}

fn dwarf_error(error: gimli::Error) -> String {
    format!("reading DWARF: {error}")
}

fn object_error(error: object::Error) -> String {
    format!("reading the object: {error}")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn relative_watch_paths_match_the_end_of_absolute_ones() {
        let dwarf = "/work/veloce/src/swmr/writer.rs";
        let matched = same_file(dwarf, "src/swmr/writer.rs");
        let absolute = same_file(dwarf, "/work/veloce/src/swmr/writer.rs");
        let other = same_file(dwarf, "src/swmr/reader.rs");
        let partial = same_file(dwarf, "riter.rs");
        assert!(matched);
        assert!(absolute);
        assert!(!other);
        assert!(!partial);
    }

    #[test]
    fn declarations_match_by_name_line_range_and_file() {
        let watch = Watch {
            name: "get".to_string(),
            first_line: 38,
            last_line: 49,
            file: "src/perfect_hash/map.rs".to_string(),
        };
        let declared = |name: &str, line| Declaration {
            name: name.to_string(),
            line,
            file: "/work/veloce/src/perfect_hash/map.rs".to_string(),
            display: String::new(),
        };
        let generic = declared("get<str>", 40);
        let outside = declared("get", 50);
        let closure = declared("{closure#0}", 41);
        assert!(generic.matches(&watch));
        assert!(!outside.matches(&watch));
        assert!(!closure.matches(&watch));
    }

    #[test]
    fn padding_is_skipped_to_the_alignment() {
        let mut code = Vec::new();
        code.extend_from_slice(&[0x00, 0x00, 0x80, 0xd2]);
        code.extend_from_slice(&AARCH64_NOP);
        code.extend_from_slice(&AARCH64_NOP);
        code.extend_from_slice(&AARCH64_NOP);
        code.extend_from_slice(&[0xc0, 0x03, 0x5f, 0xd6]);
        let mut cursor = Cursor {
            data: &code,
            section_address: 0x1000,
            position: 0,
            arch: Arch::Aarch64,
        };
        let alignment = Alignment {
            bytes: 16,
            max_skip: None,
        };
        cursor.next_instruction().unwrap();
        cursor.skip_padding(alignment).unwrap();
        assert_eq!(cursor.position, 16);
    }

    #[test]
    fn displayed_paths_drop_modules_turbofish_and_inherent_brackets() {
        let staged = short(
            "<veloce::swmr::writer::Writer<u64, veloce::swmr::async::Async>>::stage_with_latest::<<veloce::swmr::writer::Writer<u64>>::publish::{closure#0}>",
        );
        let compare = short("<veloce::perfect_hash::set::PerfectSet<20, 32, 15, 16>>::compare_key");
        let free = short("ring::checksum");
        let trait_impl =
            short("<core::slice::iter::Iter<u64> as core::iter::traits::iterator::Iterator>::fold");
        assert_eq!(staged, "Writer<u64, Async>::stage_with_latest");
        assert_eq!(compare, "PerfectSet<20, 32, 15, 16>::compare_key");
        assert_eq!(free, "checksum");
        assert_eq!(trait_impl, "<Iter<u64> as Iterator>::fold");
    }
}
