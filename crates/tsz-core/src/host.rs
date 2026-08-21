//! Host abstraction for project discovery and source loading.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One immediate child returned by [`ProgramHost::read_directory`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    pub path: PathBuf,
    pub is_file: bool,
    pub is_directory: bool,
}

/// Filesystem surface required by configuration and project construction.
///
/// Keeping this boundary in `tsz-core` lets the CLI, language service, and
/// future virtual hosts share one project-selection implementation.
pub trait ProgramHost: Sync {
    fn current_directory(&self) -> &Path;

    fn use_case_sensitive_file_names(&self) -> bool {
        true
    }

    fn file_exists(&self, path: &Path) -> bool;

    fn directory_exists(&self, path: &Path) -> bool;

    fn read_file(&self, path: &Path) -> io::Result<String>;

    fn read_directory(&self, path: &Path) -> io::Result<Vec<HostEntry>>;

    fn realpath(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }
}

/// Native filesystem host rooted at a stable current directory.
#[derive(Debug, Clone)]
pub struct SystemHost {
    current_directory: PathBuf,
}

impl SystemHost {
    #[must_use]
    pub fn new(current_directory: impl Into<PathBuf>) -> Self {
        Self {
            current_directory: current_directory.into(),
        }
    }
}

impl Default for SystemHost {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

impl ProgramHost for SystemHost {
    fn current_directory(&self) -> &Path {
        &self.current_directory
    }

    fn use_case_sensitive_file_names(&self) -> bool {
        !cfg!(any(windows, target_os = "macos"))
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn directory_exists(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn read_directory(&self, path: &Path) -> io::Result<Vec<HostEntry>> {
        fs::read_dir(path)?
            .map(|entry| {
                let entry = entry?;
                let file_type = entry.file_type()?;
                Ok(HostEntry {
                    path: entry.path(),
                    is_file: file_type.is_file(),
                    is_directory: file_type.is_dir(),
                })
            })
            .collect()
    }

    fn realpath(&self, path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
}
