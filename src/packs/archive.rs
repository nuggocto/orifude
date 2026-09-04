use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor, Read, Seek, SeekFrom};

use zip::read::{ArchiveOffset, Config};
use zip::{CompressionMethod, ZipArchive};

use super::{
    MAX_ARCHIVE_BYTES, MAX_EXTRACTED_BYTES, MAX_FILES, PackError, ValidatedPack, content_limit,
    validate_files, validate_relative_path,
};

const MAX_ARCHIVE_ENTRIES: usize = MAX_FILES + 2;
const END_HEADER_BYTES: usize = 22;
const END_SIGNATURE: &[u8; 4] = b"PK\x05\x06";

pub(super) fn validate_archive_bytes(bytes: &[u8]) -> Result<ValidatedPack, PackError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(PackError::one(
            "archive",
            "compressed size exceeds the limit",
        ));
    }
    let catalog = preflight_catalog(bytes)?;
    let reading_files = Cell::new(false);
    let reader = ArchiveReader {
        cursor: Cursor::new(&bytes[..catalog.footer + END_HEADER_BYTES]),
        catalog_start: catalog.start,
        footer: catalog.footer,
        reading_files: &reading_files,
    };
    let mut archive = ZipArchive::with_config(
        Config {
            archive_offset: ArchiveOffset::Known(0),
        },
        reader,
    )
    .map_err(|_| PackError::Archive)?;
    if archive.len() != catalog.entries
        || archive.central_directory_start() != catalog.start as u64
        || archive.offset() != 0
    {
        return Err(PackError::Archive);
    }
    reading_files.set(true);

    let mut files = BTreeMap::new();
    let mut folded_paths = BTreeSet::new();
    let mut extracted = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| PackError::Archive)?;
        let name = entry.name().to_owned();
        if entry.is_dir() {
            let directory = name.strip_suffix('/').unwrap_or(&name);
            validate_relative_path(directory)?;
            if entry.is_symlink() || is_special_directory(entry.unix_mode()) {
                return Err(PackError::one(
                    directory,
                    "archive entry type is not accepted",
                ));
            }
            if !folded_paths.insert(directory.to_ascii_lowercase()) {
                return Err(PackError::one(
                    directory,
                    "path duplicates another path by case",
                ));
            }
            if !matches!(directory, "puzzles" | "notes") {
                return Err(PackError::one(
                    directory,
                    "directory is not declared by the schema",
                ));
            }
            continue;
        }
        validate_relative_path(&name)?;
        if entry.enclosed_name().is_none() || entry.is_symlink() || is_special(entry.unix_mode()) {
            return Err(PackError::one(name, "archive entry type is not accepted"));
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(PackError::one(name, "compression method is not accepted"));
        }
        let folded = name.to_ascii_lowercase();
        if !folded_paths.insert(folded) {
            return Err(PackError::one(name, "path duplicates another path by case"));
        }
        if files.len() >= MAX_FILES {
            return Err(PackError::one("archive", "file count exceeds the limit"));
        }
        let limit = content_limit(&name)?;
        if entry.size() > limit {
            return Err(PackError::one(name, "file size exceeds its limit"));
        }
        let remaining = MAX_EXTRACTED_BYTES.saturating_sub(extracted);
        let read_limit = limit.min(remaining).saturating_add(1);
        let mut contents = Vec::new();
        entry
            .take(read_limit)
            .read_to_end(&mut contents)
            .map_err(|_| PackError::Archive)?;
        if contents.len() as u64 > limit || contents.len() as u64 > remaining {
            return Err(PackError::one(name, "extracted size exceeds the limit"));
        }
        extracted = extracted
            .checked_add(contents.len() as u64)
            .ok_or_else(|| PackError::one("archive", "extracted size exceeds the limit"))?;
        files.insert(name, contents);
    }
    validate_files(files)
}

struct Catalog {
    start: usize,
    footer: usize,
    entries: usize,
}

// During metadata parsing, expose only the checked catalog and its one footer.
// Earlier footers in payloads and signatures in the unused archive comment
// cannot become fallback catalogs. File reads then see the original bytes at
// unchanged offsets. No second archive-sized buffer is allocated.
struct ArchiveReader<'a> {
    cursor: Cursor<&'a [u8]>,
    catalog_start: usize,
    footer: usize,
    reading_files: &'a Cell<bool>,
}

impl Read for ArchiveReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let start = self.cursor.position();
        let count = self.cursor.read(output)?;
        if !self.reading_files.get() {
            for (index, byte) in output[..count].iter_mut().enumerate() {
                let position = start + index as u64;
                if position < self.catalog_start as u64
                    || position >= (self.footer + END_HEADER_BYTES - 2) as u64
                {
                    *byte = 0;
                }
            }
        }
        Ok(count)
    }
}

impl Seek for ArchiveReader<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(position)
    }
}

fn preflight_catalog(bytes: &[u8]) -> Result<Catalog, PackError> {
    const MAX_COMMENT_BYTES: usize = u16::MAX as usize;

    if bytes.len() < END_HEADER_BYTES {
        return Err(PackError::Archive);
    }
    let start = bytes
        .len()
        .saturating_sub(END_HEADER_BYTES.saturating_add(MAX_COMMENT_BYTES));
    for offset in (start..=bytes.len() - END_HEADER_BYTES).rev() {
        if &bytes[offset..offset + END_SIGNATURE.len()] != END_SIGNATURE {
            continue;
        }
        let comment_bytes = usize::from(read_u16(bytes, offset + 20));
        if offset
            .checked_add(END_HEADER_BYTES)
            .and_then(|end| end.checked_add(comment_bytes))
            != Some(bytes.len())
        {
            continue;
        }
        let disk = read_u16(bytes, offset + 4);
        let central_disk = read_u16(bytes, offset + 6);
        let disk_entries = read_u16(bytes, offset + 8);
        let total_entries = read_u16(bytes, offset + 10);
        let central_bytes =
            usize::try_from(read_u32(bytes, offset + 12)).map_err(|_| PackError::Archive)?;
        let central_offset =
            usize::try_from(read_u32(bytes, offset + 16)).map_err(|_| PackError::Archive)?;
        if disk != 0
            || central_disk != 0
            || disk_entries != total_entries
            || total_entries == 0
            || usize::from(total_entries) > MAX_ARCHIVE_ENTRIES
            || central_offset.checked_add(central_bytes) != Some(offset)
        {
            return Err(PackError::one(
                "archive",
                "archive entry table exceeds its supported bounds",
            ));
        }
        let catalog = Catalog {
            start: central_offset,
            footer: offset,
            entries: usize::from(total_entries),
        };
        validate_catalog_entries(bytes, &catalog)?;
        return Ok(catalog);
    }
    Err(PackError::Archive)
}

fn validate_catalog_entries(bytes: &[u8], catalog: &Catalog) -> Result<(), PackError> {
    const CENTRAL_HEADER_BYTES: usize = 46;
    let directory = &bytes[catalog.start..catalog.footer];
    // Reject ambiguous metadata even if a later library version starts looking
    // for fallback footers in extra fields or per-entry comments.
    if directory
        .windows(END_SIGNATURE.len())
        .any(|bytes| bytes == END_SIGNATURE)
    {
        return Err(PackError::one(
            "archive",
            "catalog contains an embedded footer",
        ));
    }
    let mut remaining = directory;
    let mut names = BTreeSet::new();
    for _ in 0..catalog.entries {
        let header = remaining
            .get(..CENTRAL_HEADER_BYTES)
            .ok_or(PackError::Archive)?;
        if &header[..4] != b"PK\x01\x02" || read_u16(header, 34) != 0 {
            return Err(PackError::Archive);
        }
        let name_bytes = usize::from(read_u16(header, 28));
        let extra_bytes = usize::from(read_u16(header, 30));
        let comment_bytes = usize::from(read_u16(header, 32));
        let name_end = CENTRAL_HEADER_BYTES + name_bytes;
        let entry_end = name_end + extra_bytes + comment_bytes;
        let name = std::str::from_utf8(
            remaining
                .get(CENTRAL_HEADER_BYTES..name_end)
                .ok_or(PackError::Archive)?,
        )
        .map_err(|_| PackError::Archive)?;
        let path = name.strip_suffix('/').unwrap_or(name);
        validate_relative_path(path)?;
        if !names.insert(path.to_ascii_lowercase()) {
            return Err(PackError::one(path, "path duplicates another path by case"));
        }
        remaining = remaining.get(entry_end..).ok_or(PackError::Archive)?;
    }
    if !remaining.is_empty() {
        return Err(PackError::one(
            "archive",
            "catalog size or entry count is inconsistent",
        ));
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn is_special(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| {
        let kind = mode & 0o170_000;
        kind != 0 && kind != 0o100_000
    })
}

fn is_special_directory(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| {
        let kind = mode & 0o170_000;
        kind != 0 && kind != 0o040_000
    })
}
