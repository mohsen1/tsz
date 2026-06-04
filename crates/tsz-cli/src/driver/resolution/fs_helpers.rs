//! Small filesystem and environment helpers shared across the resolution
//! submodules. Kept in their own module so `resolution.rs` stays within its
//! architecture size ratchet.

use std::path::{Path, PathBuf};

pub(crate) fn is_declaration_file(path: &Path) -> bool {
    tsz::module_resolver::ModuleExtension::from_path(path).is_declaration()
}

pub(crate) fn canonicalize_with_missing_tail(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }

    let mut tail = Vec::new();
    let mut current = path;
    while !current.exists() {
        let Some(name) = current.file_name() else {
            return path.to_path_buf();
        };
        tail.push(name.to_os_string());
        let Some(parent) = current.parent() else {
            return path.to_path_buf();
        };
        current = parent;
    }

    let Ok(mut canonical) = std::fs::canonicalize(current) else {
        return path.to_path_buf();
    };
    for component in tail.iter().rev() {
        canonical.push(component);
    }
    canonical
}

pub(crate) fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
}
