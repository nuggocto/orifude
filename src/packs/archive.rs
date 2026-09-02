use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use zip::{CompressionMethod, ZipArchive};

use super::{
    MAX_ARCHIVE_BYTES, MAX_EXTRACTED_BYTES, MAX_FILES, PackError, ValidatedPack, content_limit,
    validate_files, validate_relative_path,
};

const MAX_ARCHIVE_ENTRIES: usize = MAX_FILES + 2;

pub(super) fn validate_archive_bytes(bytes: &[u8]) -> Result<ValidatedPack, PackError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(PackError::one(
            "archive",
            "compressed size exceeds the limit",
        ));
    }
    preflight_entry_count(bytes)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| PackError::Archive)?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(PackError::one("archive", "entry count exceeds the limit"));
    }

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

fn preflight_entry_count(bytes: &[u8]) -> Result<(), PackError> {
    const END_HEADER_BYTES: usize = 22;
    const MAX_COMMENT_BYTES: usize = u16::MAX as usize;
    const SIGNATURE: &[u8; 4] = b"PK\x05\x06";

    if bytes.len() < END_HEADER_BYTES {
        return Err(PackError::Archive);
    }
    let start = bytes
        .len()
        .saturating_sub(END_HEADER_BYTES.saturating_add(MAX_COMMENT_BYTES));
    for offset in (start..=bytes.len() - END_HEADER_BYTES).rev() {
        if &bytes[offset..offset + SIGNATURE.len()] != SIGNATURE {
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
            || total_entries == u16::MAX
            || usize::from(total_entries) > MAX_ARCHIVE_ENTRIES
            || central_offset
                .checked_add(central_bytes)
                .is_none_or(|end| end > offset)
        {
            return Err(PackError::one(
                "archive",
                "archive entry table exceeds its supported bounds",
            ));
        }
        return Ok(());
    }
    Err(PackError::Archive)
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
