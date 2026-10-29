//! The registry `probes!` and `#[codegen::watch]` leave in a `codegen` section of the probed crate's objects
//!
//! - `codegen-probes:<macro>\n` names a probe macro to expand
//! - `codegen-watch:<fn>:<first line>:<last line>:<file>\n` names a watched function and the lines it spans

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use object::read::archive::ArchiveFile;
use object::{Object, ObjectSection};

/// Name of the registry section, the segment of Mach-O's `__DATA,codegen` aside
const SECTION: &str = "codegen";
/// Prefix of a probe macro entry
const PROBES: &[u8] = b"codegen-probes:";
/// Prefix of a watched function entry
const WATCH: &[u8] = b"codegen-watch:";

/// What the probed crate registered, each list sorted and deduplicated
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Registry {
    pub probes: Vec<String>,
    pub watches: Vec<Watch>,
}

/// A watched function: its name and the lines its item spans in `file`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Watch {
    pub name: String,
    pub first_line: u64,
    pub last_line: u64,
    /// As rustc saw it: absolute, or relative to the directory cargo ran it in
    pub file: String,
}

/// The registry in the object files of `rlib`
pub fn read(rlib: &Path) -> Result<Registry, String> {
    let data = fs::read(rlib).map_err(|error| format!("reading {}: {error}", rlib.display()))?;
    let archive = ArchiveFile::parse(&*data)
        .map_err(|error| format!("parsing {}: {error}", rlib.display()))?;
    let mut probes = BTreeSet::new();
    let mut watches = BTreeSet::new();
    for member in archive.members() {
        let member = member.map_err(|error| format!("reading {}: {error}", rlib.display()))?;
        let Ok(bytes) = member.data(&*data) else {
            continue;
        };
        // the rlib's metadata members are no object files
        let Ok(file) = object::File::parse(bytes) else {
            continue;
        };
        for section in file.sections() {
            if section.name() != Ok(SECTION) {
                continue;
            }
            let contents = section
                .data()
                .map_err(|error| format!("reading a section: {error}"))?;
            probes.extend(entries(contents, PROBES));
            let parsed = entries(contents, WATCH)
                .into_iter()
                .filter_map(|entry| parse_watch(&entry));
            watches.extend(parsed);
        }
    }
    let probes = probes.into_iter().collect();
    let watches = watches.into_iter().collect();
    Ok(Registry { probes, watches })
}

/// `<fn>:<first line>:<last line>:<file>`, the file last as it may hold colons
fn parse_watch(entry: &str) -> Option<Watch> {
    let mut fields = entry.splitn(4, ':');
    let name = fields.next()?.to_string();
    let first_line = fields.next()?.parse().ok()?;
    let last_line = fields.next()?.parse().ok()?;
    let file = fields.next()?.to_string();
    Some(Watch {
        name,
        first_line,
        last_line,
        file,
    })
}

/// Text of the entries starting with `prefix`, whatever padding the linker put between them
fn entries(contents: &[u8], prefix: &[u8]) -> Vec<String> {
    let mut texts = Vec::new();
    let mut rest = contents;
    while let Some(start) = find(rest, prefix) {
        let after = &rest[start + prefix.len()..];
        let Some(end) = after.iter().position(|&byte| byte == b'\n') else {
            break;
        };
        let text = String::from_utf8_lossy(&after[..end]).into_owned();
        texts.push(text);
        rest = &after[end + 1..];
    }
    texts
}

/// Position of `needle` in `haystack`
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn entries_are_read_through_padding() {
        let contents = b"codegen-probes:swmr\n\0\0\0codegen-probes:perfect_hash_probes\n\0";
        let names = entries(contents, PROBES);
        assert_eq!(names, ["swmr", "perfect_hash_probes"]);
    }

    #[test]
    fn a_truncated_entry_is_dropped() {
        let contents = b"codegen-probes:whole\ncodegen-probes:cut";
        let names = entries(contents, PROBES);
        assert_eq!(names, ["whole"]);
    }

    #[test]
    fn watch_entries_keep_colons_in_the_file() {
        let contents = b"codegen-probes:p\n\0codegen-watch:push:7:16:C:\\work\\src\\ring.rs\n";
        let texts = entries(contents, WATCH);
        let watch = parse_watch(&texts[0]);
        let expected = Watch {
            name: "push".to_string(),
            first_line: 7,
            last_line: 16,
            file: "C:\\work\\src\\ring.rs".to_string(),
        };
        assert_eq!(watch, Some(expected));
    }
}
