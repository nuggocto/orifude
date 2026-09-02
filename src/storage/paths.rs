use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    data: PathBuf,
    config: PathBuf,
    cache: PathBuf,
}

impl AppPaths {
    /// Resolves the operating system's per-user application directories.
    ///
    /// # Errors
    ///
    /// Returns when the platform cannot resolve a user home directory.
    pub fn platform() -> Result<Self, PathError> {
        let project = ProjectDirs::from("", "", "orifude").ok_or(PathError)?;
        Ok(Self::injected(
            project.data_dir(),
            project.config_dir(),
            project.cache_dir(),
        ))
    }

    /// Builds paths rooted in caller-owned directories, primarily for tests.
    #[must_use]
    pub fn injected(
        data: impl Into<PathBuf>,
        config: impl Into<PathBuf>,
        cache: impl Into<PathBuf>,
    ) -> Self {
        Self {
            data: data.into(),
            config: config.into(),
            cache: cache.into(),
        }
    }

    #[must_use]
    pub fn data(&self) -> &Path {
        &self.data
    }

    #[must_use]
    pub fn config(&self) -> &Path {
        &self.config
    }

    #[must_use]
    pub fn cache(&self) -> &Path {
        &self.cache
    }

    #[must_use]
    pub fn database(&self) -> PathBuf {
        self.data.join("orifude.sqlite3")
    }

    #[must_use]
    pub fn lock(&self) -> PathBuf {
        self.data.join("orifude.lock")
    }

    #[must_use]
    pub fn managed_packs(&self) -> PathBuf {
        self.data.join("packs")
    }

    #[must_use]
    pub fn pack_staging(&self) -> PathBuf {
        self.data.join("pack-staging")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathError;

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the operating system user directories are unavailable")
    }
}

impl Error for PathError {}
