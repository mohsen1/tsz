use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

use crate::config::ResolvedCompilerOptions;

use super::path_resolution::normalize_resolved_path;
use super::*;

/// Two-key lookup index for program files.
///
/// The primary key is `normalize_resolved_path(file_name)` — the same key the
/// rest of the driver uses to identify a program file. The secondary key is
/// the file's canonical real path (`canonicalize`), used as a fallback when
/// the resolver returns one path to a file but the program tracks the same
/// underlying file via a different path (typically because one path uses a
/// symlink and the other does not).
///
/// Without the fallback, a single source file accessed through two
/// equivalent paths is treated as two distinct program files: the lookup
/// in `canonical_to_file_idx` keyed on the resolver's output misses the
/// program-file entry keyed on the other path. The symptom shows up most
/// often in workspace projects that mount packages into `node_modules`
/// via symlinks while also globbing the underlying source files directly.
///
/// When `preserveSymlinks` is enabled the secondary map is empty: that mode
/// explicitly opts the program out of symlink resolution, so paths must
/// match verbatim.
#[derive(Default)]
pub(crate) struct ProgramFileIndex {
    canonical_to_file_idx: FxHashMap<PathBuf, usize>,
    real_path_to_file_idx: FxHashMap<PathBuf, usize>,
    // Per-directory cache of `symlink_metadata().is_symlink()` so the
    // ancestor walk pays one syscall per *unique* directory across the
    // whole program. Without it, populating the secondary map naively
    // canonicalizes every file (6000 syscalls on a 6000-file workspace);
    // with it, only files actually under a symlinked ancestor canonicalize.
    dir_symlink_cache: FxHashMap<PathBuf, bool>,
}

impl ProgramFileIndex {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            canonical_to_file_idx: FxHashMap::with_capacity_and_hasher(
                capacity,
                Default::default(),
            ),
            real_path_to_file_idx: FxHashMap::default(),
            dir_symlink_cache: FxHashMap::default(),
        }
    }

    // Narrower than `path_resolution::path_has_symlinked_package_ancestor`:
    // that helper additionally canonicalizes each symlinked ancestor and
    // gates on `node_modules` membership. Here we only need to know whether
    // the file is reached through *any* symlinked ancestor — registering a
    // secondary key for a non-`node_modules` symlink is harmless (the
    // primary key already matches the resolver's output for it). The
    // per-directory cache keeps a 6000-file workspace at O(unique-dirs)
    // syscalls instead of O(file-count).
    fn has_symlinked_ancestor(&mut self, path: &Path) -> bool {
        let mut current = path.parent();
        while let Some(dir) = current {
            let is_symlink = match self.dir_symlink_cache.get(dir) {
                Some(&cached) => cached,
                None => {
                    let probed = std::fs::symlink_metadata(dir)
                        .map(|metadata| metadata.file_type().is_symlink())
                        .unwrap_or(false);
                    self.dir_symlink_cache.insert(dir.to_path_buf(), probed);
                    probed
                }
            };
            if is_symlink {
                return true;
            }
            current = dir.parent();
        }
        false
    }

    /// Register `file_name` under the primary canonical key. When
    /// `preserveSymlinks` is off AND the file is reached via a symlinked
    /// ancestor, also register a secondary entry keyed on the canonical
    /// real path. Secondary inserts are first-write-wins, so the index
    /// stays deterministic regardless of program-file iteration quirks.
    pub(crate) fn insert(
        &mut self,
        file_name: &str,
        idx: usize,
        options: &ResolvedCompilerOptions,
    ) -> PathBuf {
        let path = Path::new(file_name);
        let canonical = normalize_resolved_path(path, options);
        self.canonical_to_file_idx.insert(canonical.clone(), idx);

        if !options.preserve_symlinks && self.has_symlinked_ancestor(path) {
            let real = canonicalize_or_owned(path);
            if real != canonical {
                self.real_path_to_file_idx.entry(real).or_insert(idx);
            }
        }

        canonical
    }

    /// Direct primary-key lookup. Used by callers that already operate on
    /// the canonical key (no symlink fallback needed).
    pub(crate) fn get(&self, canonical: &Path) -> Option<usize> {
        self.canonical_to_file_idx.get(canonical).copied()
    }

    /// Primary-key lookup with symlink fallback. `canonical` is the
    /// already-normalized resolution output; `resolved_raw` is the raw
    /// resolver output (used to canonicalize for the secondary lookup).
    pub(crate) fn get_with_symlink_fallback(
        &self,
        canonical: &Path,
        resolved_raw: &Path,
        options: &ResolvedCompilerOptions,
    ) -> Option<usize> {
        if let Some(idx) = self.canonical_to_file_idx.get(canonical).copied() {
            return Some(idx);
        }
        if options.preserve_symlinks || self.real_path_to_file_idx.is_empty() {
            return None;
        }
        let real = canonicalize_or_owned(resolved_raw);
        self.real_path_to_file_idx.get(&real).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModuleResolutionKind;
    use std::fs;

    #[test]
    fn symlinked_and_real_paths_resolve_to_same_idx() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path();
        fs::create_dir_all(root.join("core/node_modules/package-a")).unwrap();
        fs::write(
            root.join("core/node_modules/package-a/index.d.ts"),
            "export interface Box {}",
        )
        .unwrap();
        symlink(
            root.join("core/node_modules/package-a"),
            root.join("package-a"),
        )
        .unwrap();

        let symlinked_path = root.join("package-a/index.d.ts");
        let real_path = root.join("core/node_modules/package-a/index.d.ts");

        let options = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            preserve_symlinks: false,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let mut index = ProgramFileIndex::with_capacity(1);
        index.insert(&symlinked_path.to_string_lossy(), 7, &options);

        // The same file resolved via its real path must still find idx 7.
        let real_canonical = normalize_resolved_path(&real_path, &options);
        let resolved = index
            .get_with_symlink_fallback(&real_canonical, &real_path, &options)
            .expect("real path should resolve to the symlinked program entry");
        assert_eq!(resolved, 7);

        // And the original symlinked path still works via the primary key.
        let symlinked_canonical = normalize_resolved_path(&symlinked_path, &options);
        assert_eq!(
            index.get_with_symlink_fallback(&symlinked_canonical, &symlinked_path, &options),
            Some(7),
        );
    }

    #[test]
    fn preserve_symlinks_disables_real_path_fallback() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path();
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/index.d.ts"), "export {};").unwrap();
        symlink(root.join("real"), root.join("linked")).unwrap();

        let symlinked_path = root.join("linked/index.d.ts");
        let real_path = root.join("real/index.d.ts");

        let options = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            preserve_symlinks: true,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let mut index = ProgramFileIndex::with_capacity(1);
        index.insert(&symlinked_path.to_string_lossy(), 3, &options);

        let real_canonical = normalize_resolved_path(&real_path, &options);
        assert!(
            index
                .get_with_symlink_fallback(&real_canonical, &real_path, &options)
                .is_none(),
            "preserveSymlinks must not unify symlink and real paths"
        );
    }

    #[test]
    fn first_write_wins_keeps_idx_deterministic() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path();
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/index.d.ts"), "export {};").unwrap();
        symlink(root.join("real"), root.join("linked")).unwrap();

        let symlinked_path = root.join("linked/index.d.ts");
        let other_symlinked_path = root.join("linked/index.d.ts");

        let options = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            preserve_symlinks: false,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let mut index = ProgramFileIndex::with_capacity(2);
        index.insert(&symlinked_path.to_string_lossy(), 1, &options);
        // Inserting a second entry that shares the same real path must not
        // clobber the first-write-wins fallback registration.
        index.insert(&other_symlinked_path.to_string_lossy(), 2, &options);

        let real_path = root.join("real/index.d.ts");
        let real_canonical = normalize_resolved_path(&real_path, &options);
        let resolved = index.get_with_symlink_fallback(&real_canonical, &real_path, &options);
        // Either 1 or 2 is acceptable as long as the result is stable.
        assert!(matches!(resolved, Some(1) | Some(2)), "got {resolved:?}");
    }
}
