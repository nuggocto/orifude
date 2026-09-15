//! Managed pack installation, registry validation, and restart recovery.

use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::domain::puzzle::PuzzleIdentity;
use crate::packs::{
    MAX_INSTALLED_PACKS, MAX_MANAGED_BYTES, PackError, ValidatedPack, fingerprint_hex,
    validate_directory, validate_source,
};
use crate::storage::{
    MAX_INSTALLED_PACKS_DB, MAX_MANAGED_ENTRIES, RESERVED_PACK_IDS, Storage, StorageError,
    create_private_directory, i64_to_u64, is_fingerprint_name, set_private_file, sync_directory,
    u64_to_i64,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredPack {
    pub id: Box<str>,
    pub title: Box<str>,
    pub description: Option<Box<str>>,
    pub authors: Box<str>,
    pub license: Box<str>,
    pub fingerprint: [u8; 32],
    pub extracted_bytes: u64,
    pub installed_at_unix_seconds: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    Installed(RegisteredPack),
    AlreadyPresent(RegisteredPack),
}

impl Storage {
    /// Returns the bounded installed-pack catalog without parsing pack files.
    ///
    /// # Errors
    ///
    /// Returns when registry data is corrupt or cannot be read.
    pub fn registered_packs(&self) -> Result<Vec<RegisteredPack>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT pack_id, title, description, authors, license, fingerprint,
                    extracted_bytes, installed_at, managed_name
             FROM pack_registry ORDER BY pack_id LIMIT 33",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((registered_pack_from_row(row)?, row.get::<_, String>(8)?))
        })?;
        let registered = rows.collect::<Result<Vec<_>, _>>()?;
        let mut packs = Vec::with_capacity(registered.len());
        for (pack, managed_name) in registered {
            validate_registered_pack(&pack, &managed_name)?;
            packs.push(pack);
        }
        if packs.len() > MAX_INSTALLED_PACKS {
            return Err(StorageError::Corrupt);
        }
        packs.shrink_to_fit();
        Ok(packs)
    }

    /// Validates and atomically installs one local directory or ZIP pack.
    ///
    /// # Errors
    ///
    /// Returns a bounded source, conflict, capacity, recovery, filesystem, or
    /// database error. A committed pending operation is recovered on restart.
    pub fn install_pack(
        &mut self,
        source: &Path,
        installed_at_unix_seconds: i64,
    ) -> Result<InstallOutcome, StorageError> {
        let pack = validate_source(source)?;
        self.install_validated(&pack, installed_at_unix_seconds)
    }

    /// Verifies and loads one selected community pack, replacing any previous
    /// in-memory community pack.
    ///
    /// # Errors
    ///
    /// A missing registry row, invalid managed name, content validation error,
    /// or fingerprint mismatch disables loading without changing the registry.
    pub fn load_pack(&mut self, pack_id: &str) -> Result<Option<&ValidatedPack>, StorageError> {
        let registered = self.registered_pack_row(pack_id)?;
        let Some((summary, managed_name)) = registered else {
            self.loaded_pack = None;
            return Ok(None);
        };
        if !is_fingerprint_name(&managed_name) {
            return Err(StorageError::Corrupt);
        }
        let loaded = self.read_managed_pack(&summary, &managed_name)?;
        self.loaded_pack = Some(loaded);
        Ok(self.loaded_pack.as_ref())
    }

    /// Discards the validated pack cache after a caller has copied the bounded
    /// playable projection it needs.
    pub(crate) fn clear_loaded_pack(&mut self) {
        self.loaded_pack = None;
    }

    /// Removes a pack from play before deleting its managed files. Puzzle
    /// progress and replay records deliberately have no registry foreign key.
    ///
    /// # Errors
    ///
    /// Returns a database error before logical removal, or a cleanup error
    /// after logical removal. Startup retries the latter once.
    pub fn remove_pack(&mut self, pack_id: &str) -> Result<bool, StorageError> {
        let managed_name: Option<String> = self
            .connection
            .query_row(
                "SELECT managed_name FROM pack_registry WHERE pack_id = ?1",
                [pack_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(managed_name) = managed_name else {
            return Ok(false);
        };
        if !is_fingerprint_name(&managed_name) {
            return Err(StorageError::Corrupt);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM pack_registry WHERE pack_id = ?1", [pack_id])?;
        transaction.commit()?;
        self.loaded_pack = None;
        let path = self.paths.managed_packs().join(managed_name);
        if path.exists() {
            fs::remove_dir_all(path).map_err(|_| StorageError::PackCleanup)?;
        }
        Ok(true)
    }

    pub(crate) fn install_validated(
        &mut self,
        pack: &ValidatedPack,
        installed_at_unix_seconds: i64,
    ) -> Result<InstallOutcome, StorageError> {
        if RESERVED_PACK_IDS.contains(&pack.metadata().id()) {
            return Err(StorageError::PackConflict);
        }
        if let Some((existing, managed_name)) = self.registered_pack_row(pack.metadata().id())? {
            if existing.fingerprint == pack.fingerprint() {
                self.read_managed_pack(&existing, &managed_name)?;
                return Ok(InstallOutcome::AlreadyPresent(existing));
            }
            return Err(StorageError::PackConflict);
        }
        let (count, bytes): (i64, i64) = self.connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(extracted_bytes), 0) FROM pack_registry",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let count = i64_to_u64(count)?;
        let bytes = i64_to_u64(bytes)?;
        if count >= MAX_INSTALLED_PACKS_DB
            || bytes
                .checked_add(pack.extracted_bytes())
                .is_none_or(|total| total > MAX_MANAGED_BYTES)
        {
            return Err(StorageError::ResourceLimit);
        }
        let final_name = pack.fingerprint_hex();
        let final_path = self.paths.managed_packs().join(&final_name);
        if final_path.exists() {
            return Err(StorageError::PackConflict);
        }
        self.verify_footprint()?;
        let staging = self.paths.pack_staging();
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|_| StorageError::PackCleanup)?;
        }
        let prepared = (|| {
            create_private_directory(&staging)?;
            write_pack_files(&staging, pack)?;
            sync_directory(&staging)
        })();
        if let Err(error) = prepared {
            if staging.exists() && fs::remove_dir_all(&staging).is_err() {
                return Err(StorageError::PackCleanup);
            }
            return Err(error);
        }

        let pending_result = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO pending_install(
                     singleton, pack_id, fingerprint, final_name, installed_at
                 ) VALUES (1, ?1, ?2, ?3, ?4)",
                params![
                    pack.metadata().id(),
                    pack.fingerprint().as_slice(),
                    final_name,
                    installed_at_unix_seconds
                ],
            )?;
            transaction.commit().map_err(StorageError::from)
        })();
        if let Err(error) = pending_result {
            if fs::remove_dir_all(&staging).is_err() {
                return Err(StorageError::PackCleanup);
            }
            return Err(error);
        }

        fs::rename(&staging, &final_path)?;
        sync_directory(&self.paths.managed_packs())?;
        sync_directory(self.paths.data())?;
        let summary = register_pack(
            &mut self.connection,
            pack,
            &final_name,
            installed_at_unix_seconds,
        )?;
        Ok(InstallOutcome::Installed(summary))
    }

    fn registered_pack_row(
        &self,
        pack_id: &str,
    ) -> Result<Option<(RegisteredPack, String)>, StorageError> {
        let registered = self
            .connection
            .query_row(
                "SELECT pack_id, title, description, authors, license, fingerprint,
                        extracted_bytes, installed_at, managed_name
                 FROM pack_registry WHERE pack_id = ?1",
                [pack_id],
                |row| Ok((registered_pack_from_row(row)?, row.get::<_, String>(8)?)),
            )
            .optional()?;
        if let Some((summary, managed_name)) = &registered {
            validate_registered_pack(summary, managed_name)?;
        }
        Ok(registered)
    }

    fn read_managed_pack(
        &self,
        summary: &RegisteredPack,
        managed_name: &str,
    ) -> Result<ValidatedPack, StorageError> {
        validate_registered_pack(summary, managed_name)?;
        let path = self.paths.managed_packs().join(managed_name);
        let loaded = validate_directory(&path).map_err(|error| match error {
            PackError::Io(source) if source.kind() != io::ErrorKind::NotFound => {
                StorageError::from(source)
            }
            PackError::Io(_) | PackError::Invalid { .. } | PackError::SourceType => {
                StorageError::PackFingerprint
            }
            PackError::Archive => StorageError::PackFingerprint,
        })?;
        if loaded.metadata().id() != summary.id.as_ref()
            || loaded.fingerprint() != summary.fingerprint
        {
            return Err(StorageError::PackFingerprint);
        }
        Ok(loaded)
    }

    pub(super) fn reconcile_pack_install(&mut self) -> Result<(), StorageError> {
        let pending: Option<(String, Vec<u8>, String, i64)> = self
            .connection
            .query_row(
                "SELECT pack_id, fingerprint, final_name, installed_at
                 FROM pending_install WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let staging = self.paths.pack_staging();
        let Some((pack_id, fingerprint, final_name, installed_at)) = pending else {
            if staging.exists() {
                fs::remove_dir_all(staging).map_err(|_| StorageError::PackCleanup)?;
            }
            return Ok(());
        };
        let expected = fingerprint_array(&fingerprint)?;
        if PuzzleIdentity::new(&pack_id, "probe").is_err()
            || !is_fingerprint_name(&final_name)
            || final_name != fingerprint_hex(expected)
        {
            return Err(StorageError::Corrupt);
        }
        if RESERVED_PACK_IDS.contains(&pack_id.as_str()) {
            if staging.exists() {
                fs::remove_dir_all(&staging).map_err(|_| StorageError::PackCleanup)?;
            }
            self.connection
                .execute("DELETE FROM pending_install WHERE singleton = 1", [])?;
            return Ok(());
        }
        let final_path = self.paths.managed_packs().join(&final_name);
        if final_path.is_dir() {
            let pack = validate_directory(&final_path)?;
            if pack.metadata().id() != pack_id || pack.fingerprint() != expected {
                return Err(StorageError::PackFingerprint);
            }
            register_pack(&mut self.connection, &pack, &final_name, installed_at)?;
            if staging.exists() {
                fs::remove_dir_all(staging).map_err(|_| StorageError::PackCleanup)?;
            }
            return Ok(());
        }
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|_| StorageError::PackCleanup)?;
        }
        self.connection
            .execute("DELETE FROM pending_install WHERE singleton = 1", [])?;
        Ok(())
    }

    pub(super) fn recover_pack_licenses(&mut self) -> Result<(), StorageError> {
        let registered = {
            let mut statement = self
                .connection
                .prepare("SELECT pack_id, license FROM pack_registry ORDER BY pack_id LIMIT 33")?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        if registered.len() > MAX_INSTALLED_PACKS {
            return Err(StorageError::Corrupt);
        }
        let mut repairs = Vec::new();
        for (pack_id, license) in registered {
            if !crate::packs::license_is_valid(&license) {
                return Err(StorageError::Corrupt);
            }
            if license.chars().any(char::is_control) {
                repairs.push((pack_id, crate::packs::normalize_license(&license)));
            }
        }
        if repairs.is_empty() {
            return Ok(());
        }
        // Older releases accepted SPDX whitespace but rejected it on restart.
        // Repair only registry text; pack bytes and saved play remain unchanged.
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (pack_id, license) in repairs {
            let changed = transaction.execute(
                "UPDATE pack_registry SET license = ?1 WHERE pack_id = ?2",
                params![license, pack_id],
            )?;
            if changed != 1 {
                return Err(StorageError::Corrupt);
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub(super) fn reconcile_registered_pack_paths(&mut self) -> Result<(), StorageError> {
        let registered = {
            let mut statement = self.connection.prepare(
                "SELECT pack_id, managed_name FROM pack_registry ORDER BY pack_id LIMIT 37",
            )?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut missing = Vec::new();
        let mut active_count = 0_usize;
        for (pack_id, managed_name) in registered {
            if PuzzleIdentity::new(&pack_id, "probe").is_err() {
                return Err(StorageError::Corrupt);
            }
            if RESERVED_PACK_IDS.contains(&pack_id.as_str()) {
                missing.push(pack_id);
                continue;
            }
            active_count = active_count
                .checked_add(1)
                .ok_or(StorageError::ResourceLimit)?;
            if active_count > MAX_INSTALLED_PACKS || !is_fingerprint_name(&managed_name) {
                return Err(StorageError::Corrupt);
            }
            let path = self.paths.managed_packs().join(managed_name);
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => missing.push(pack_id),
                Err(error) if error.kind() == io::ErrorKind::NotFound => missing.push(pack_id),
                Err(error) => return Err(error.into()),
            }
        }
        if missing.is_empty() {
            return Ok(());
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for pack_id in missing {
            let removed =
                transaction.execute("DELETE FROM pack_registry WHERE pack_id = ?1", [pack_id])?;
            if removed != 1 {
                return Err(StorageError::Corrupt);
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub(super) fn cleanup_orphan_packs(&self) -> Result<(), StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT managed_name FROM pack_registry ORDER BY managed_name LIMIT 33")?;
        let live = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        if live.len() > MAX_INSTALLED_PACKS {
            return Err(StorageError::Corrupt);
        }
        for managed_name in &live {
            if !is_fingerprint_name(managed_name) {
                return Err(StorageError::Corrupt);
            }
        }
        let mut entry_count = 0_usize;
        for entry in fs::read_dir(self.paths.managed_packs())? {
            entry_count = entry_count
                .checked_add(1)
                .ok_or(StorageError::ResourceLimit)?;
            if entry_count > MAX_MANAGED_ENTRIES {
                return Err(StorageError::ResourceLimit);
            }
            let entry = entry?;
            let name = entry.file_name();
            let is_live = name
                .to_str()
                .filter(|name| is_fingerprint_name(name))
                .is_some_and(|name| live.contains(name));
            if !is_live {
                let file_type = entry.file_type()?;
                if file_type.is_dir() && !file_type.is_symlink() {
                    fs::remove_dir_all(entry.path()).map_err(|_| StorageError::PackCleanup)?;
                } else {
                    fs::remove_file(entry.path()).map_err(|_| StorageError::PackCleanup)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn verify_registry_bounds(&self) -> Result<(), StorageError> {
        let (count, bytes): (i64, i64) = self.connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(extracted_bytes), 0) FROM pack_registry",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if i64_to_u64(count)? > MAX_INSTALLED_PACKS_DB || i64_to_u64(bytes)? > MAX_MANAGED_BYTES {
            return Err(StorageError::ResourceLimit);
        }
        Ok(())
    }
}

fn register_pack(
    connection: &mut Connection,
    pack: &ValidatedPack,
    final_name: &str,
    installed_at_unix_seconds: i64,
) -> Result<RegisteredPack, StorageError> {
    let metadata = pack.metadata();
    let authors = metadata
        .authors()
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<&str>>()
        .join(", ");
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "INSERT INTO pack_registry(
             pack_id, title, description, authors, license, fingerprint,
             managed_name, extracted_bytes, installed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            metadata.id(),
            metadata.title(),
            metadata.description(),
            authors,
            metadata.license(),
            pack.fingerprint().as_slice(),
            final_name,
            u64_to_i64(pack.extracted_bytes())?,
            installed_at_unix_seconds
        ],
    )?;
    let cleared = transaction.execute(
        "DELETE FROM pending_install
         WHERE singleton = 1 AND pack_id = ?1 AND fingerprint = ?2
           AND final_name = ?3 AND installed_at = ?4",
        params![
            metadata.id(),
            pack.fingerprint().as_slice(),
            final_name,
            installed_at_unix_seconds
        ],
    )?;
    if cleared != 1 {
        return Err(StorageError::Corrupt);
    }
    transaction.commit()?;
    Ok(RegisteredPack {
        id: metadata.id().into(),
        title: metadata.title().into(),
        description: metadata.description().map(Into::into),
        authors: authors.into_boxed_str(),
        license: metadata.license().into(),
        fingerprint: pack.fingerprint(),
        extracted_bytes: pack.extracted_bytes(),
        installed_at_unix_seconds,
    })
}

fn registered_pack_from_row(row: &rusqlite::Row<'_>) -> Result<RegisteredPack, rusqlite::Error> {
    let fingerprint: Vec<u8> = row.get(5)?;
    let fingerprint = fingerprint_array(&fingerprint).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Blob,
            Box::new(InvalidFingerprint),
        )
    })?;
    Ok(RegisteredPack {
        id: row.get::<_, String>(0)?.into_boxed_str(),
        title: row.get::<_, String>(1)?.into_boxed_str(),
        description: row.get::<_, Option<String>>(2)?.map(String::into_boxed_str),
        authors: row.get::<_, String>(3)?.into_boxed_str(),
        license: row.get::<_, String>(4)?.into_boxed_str(),
        fingerprint,
        extracted_bytes: u64::try_from(row.get::<_, i64>(6)?).map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Integer,
                Box::new(InvalidFingerprint),
            )
        })?,
        installed_at_unix_seconds: row.get(7)?,
    })
}

fn validate_registered_pack(pack: &RegisteredPack, managed_name: &str) -> Result<(), StorageError> {
    let display_is_valid = |value: &str, maximum: usize| {
        !value.trim().is_empty()
            && value.chars().count() <= maximum
            && !value.chars().any(char::is_control)
    };
    if PuzzleIdentity::new(&pack.id, "probe").is_err()
        || RESERVED_PACK_IDS.contains(&pack.id.as_ref())
        || !display_is_valid(&pack.title, 80)
        || pack
            .description
            .as_deref()
            .is_some_and(|value| !display_is_valid(value, 512))
        || pack.authors.chars().count() > 1_310
        || pack.authors.chars().any(char::is_control)
        || pack.license.chars().any(char::is_control)
        || !crate::packs::license_is_valid(&pack.license)
        || pack.extracted_bytes > crate::packs::MAX_EXTRACTED_BYTES
        || !is_fingerprint_name(managed_name)
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

#[derive(Debug)]
struct InvalidFingerprint;

impl fmt::Display for InvalidFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid fingerprint length")
    }
}

impl Error for InvalidFingerprint {}

fn fingerprint_array(bytes: &[u8]) -> Result<[u8; 32], StorageError> {
    bytes.try_into().map_err(|_| StorageError::Corrupt)
}

fn write_pack_files(root: &Path, pack: &ValidatedPack) -> Result<(), StorageError> {
    let mut directories = std::collections::BTreeSet::new();
    for (relative, contents) in pack.files() {
        let destination = root.join(relative);
        if let Some(parent) = destination.parent() {
            create_private_directory(parent)?;
            directories.insert(parent.to_path_buf());
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        set_private_file(&file)?;
        let mut file = file;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    for directory in directories {
        sync_directory(&directory)?;
    }
    Ok(())
}
