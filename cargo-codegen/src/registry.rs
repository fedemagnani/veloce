//! The probe registry: `codegen-probes:<macro>\n` entries `probes!` leaves in a `codegen` section of the probed crate

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use object::read::archive::ArchiveFile;
use object::{Object, ObjectSection};

/// Name of the registry section, the segment of Mach-O's `__DATA,codegen` aside
const SECTION: &str = "codegen";
/// Prefix of a registry entry, which `probes!` writes
const PREFIX: &[u8] = b"codegen-probes:";

/// Probe macros registered in the object files of `rlib`, sorted and deduplicated
pub fn read(rlib: &Path) -> Result<Vec<String>, String> {
    let data = fs::read(rlib).map_err(|error| format!("reading {}: {error}", rlib.display()))?;
    let archive = ArchiveFile::parse(&*data)
        .map_err(|error| format!("parsing {}: {error}", rlib.display()))?;
    let mut macros = BTreeSet::new();
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
            macros.extend(entries(contents));
        }
    }
    Ok(macros.into_iter().collect())
}

/// Macro names of the entries in a section's bytes, whatever padding the linker put between them
fn entries(contents: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = contents;
    while let Some(start) = find(rest, PREFIX) {
        let after = &rest[start + PREFIX.len()..];
        let Some(end) = after.iter().position(|&byte| byte == b'\n') else {
            break;
        };
        let name = String::from_utf8_lossy(&after[..end]).into_owned();
        names.push(name);
        rest = &after[end + 1..];
    }
    names
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
        let names = entries(contents);
        assert_eq!(names, ["swmr", "perfect_hash_probes"]);
    }

    #[test]
    fn a_truncated_entry_is_dropped() {
        let contents = b"codegen-probes:whole\ncodegen-probes:cut";
        let names = entries(contents);
        assert_eq!(names, ["whole"]);
    }
}
