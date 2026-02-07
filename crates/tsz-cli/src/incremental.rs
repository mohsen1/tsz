//! Incremental Compilation Support
//!
//! This module implements TypeScript's incremental compilation feature, which enables:
//! - Faster rebuilds by caching compilation results
//! - .tsbuildinfo file persistence for cross-session caching
//! - Smart dependency tracking for minimal recompilation
//!
//! # Build Info Format
//!
//! The .tsbuildinfo file stores:
//! - Version information for cache invalidation
//! - File hashes for change detection
//! - Dependency graphs between files
//! - Emitted file signatures for output caching

#![allow(clippy::print_stderr)]

use anyhow::{Context, Result};
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Version of the build info format
pub const BUILD_INFO_VERSION: &str = "0.1.0";

/// Build information persisted between compilations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    /// Version of the build info format
    pub version: String,
    /// Compiler version that created this build info
    pub compiler_version: String,
    /// Root files that were compiled
    pub root_files: Vec<String>,
    /// Information about each compiled file
    pub file_infos: BTreeMap<String, FileInfo>,
    /// Dependency graph: file -> files it imports
    pub dependencies: BTreeMap<String, Vec<String>>,
    /// Semantic diagnostics for files (cached from previous builds)
    #[serde(default)]
    pub semantic_diagnostics_per_file: BTreeMap<String, Vec<CachedDiagnostic>>,
    /// Emit output signatures (for output file caching)
    pub emit_signatures: BTreeMap<String, EmitSignature>,
    /// Path to the most recently changed .d.ts file
    /// Used by project references for fast invalidation checking
    #[serde(
        rename = "latestChangedDtsFile",
        skip_serializing_if = "Option::is_none"
    )]
    pub latest_changed_dts_file: Option<String>,
    /// Options that affect compilation
    #[serde(default)]
    pub options: BuildInfoOptions,
    /// Timestamp of when the build was completed
    pub build_time: u64,
}

/// Information about a single compiled file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    /// File version (content hash or modification time)
    pub version: String,
    /// Signature of the file's exports (for dependency tracking)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Whether this file has changed since last build
    #[serde(default)]
    pub affected_files_pending_emit: bool,
    /// The file's import dependencies
    #[serde(default)]
    pub implied_format: Option<String>,
}

/// Emit output signature for caching
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmitSignature {
    /// Hash of the emitted JavaScript
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<String>,
    /// Hash of the emitted declaration file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dts: Option<String>,
    /// Hash of the emitted source map
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
}

/// Compiler options that affect build caching
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfoOptions {
    /// Target ECMAScript version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Module system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Whether to emit declarations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration: Option<bool>,
    /// Strict mode enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Cached diagnostic information for incremental builds
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedDiagnostic {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: u8,
    pub code: u32,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related_information: Vec<CachedRelatedInformation>,
}

/// Cached related information for diagnostics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedRelatedInformation {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: u8,
    pub code: u32,
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self {
            version: BUILD_INFO_VERSION.to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            root_files: Vec::new(),
            file_infos: BTreeMap::new(),
            dependencies: BTreeMap::new(),
            semantic_diagnostics_per_file: BTreeMap::new(),
            emit_signatures: BTreeMap::new(),
            latest_changed_dts_file: None,
            options: BuildInfoOptions::default(),
            build_time: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }
}

impl BuildInfo {
    /// Create a new empty build info
    pub fn new() -> Self {
        Self::default()
    }

    /// Load build info from a file
    /// Returns Ok(None) if the file exists but is incompatible (version mismatch)
    /// Returns Ok(Some(build_info)) if the file is valid and compatible
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read build info: {}", path.display()))?;

        let build_info: BuildInfo = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse build info: {}", path.display()))?;

        // Validate version compatibility (Format version)
        if build_info.version != BUILD_INFO_VERSION {
            return Ok(None);
        }

        // Validate compiler version compatibility
        // This ensures changes in hashing algorithms or internal logic trigger a rebuild
        if build_info.compiler_version != env!("CARGO_PKG_VERSION") {
            return Ok(None);
        }

        Ok(Some(build_info))
    }

    /// Save build info to a file
    pub fn save(&self, path: &Path) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        let content =
            serde_json::to_string_pretty(self).context("failed to serialize build info")?;

        std::fs::write(path, content)
            .with_context(|| format!("failed to write build info: {}", path.display()))?;

        Ok(())
    }

    /// Add or update file info
    pub fn set_file_info(&mut self, path: &str, info: FileInfo) {
        self.file_infos.insert(path.to_string(), info);
    }

    /// Get file info
    pub fn get_file_info(&self, path: &str) -> Option<&FileInfo> {
        self.file_infos.get(path)
    }

    /// Set dependencies for a file
    pub fn set_dependencies(&mut self, path: &str, deps: Vec<String>) {
        self.dependencies.insert(path.to_string(), deps);
    }

    /// Get dependencies for a file
    pub fn get_dependencies(&self, path: &str) -> Option<&[String]> {
        self.dependencies.get(path).map(|v| v.as_slice())
    }

    /// Set emit signature for a file
    pub fn set_emit_signature(&mut self, path: &str, signature: EmitSignature) {
        self.emit_signatures.insert(path.to_string(), signature);
    }

    /// Check if a file has changed since last build
    pub fn has_file_changed(&self, path: &str, current_version: &str) -> bool {
        match self.file_infos.get(path) {
            Some(info) => info.version != current_version,
            None => true, // New file
        }
    }

    /// Get all files that depend on a given file
    pub fn get_dependents(&self, path: &str) -> Vec<String> {
        self.dependencies
            .iter()
            .filter(|(_, deps)| deps.iter().any(|d| d == path))
            .map(|(file, _)| file.clone())
            .collect()
    }
}

/// Tracks changes between builds
#[derive(Debug, Default)]
pub struct ChangeTracker {
    /// Files that have been modified
    changed_files: FxHashSet<PathBuf>,
    /// Files that need to be recompiled (changed + dependents)
    affected_files: FxHashSet<PathBuf>,
    /// Files that are new since last build
    new_files: FxHashSet<PathBuf>,
    /// Files that have been deleted
    deleted_files: FxHashSet<PathBuf>,
}

impl ChangeTracker {
    /// Create a new change tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute changes by comparing current files with build info
    pub fn compute_changes(
        &mut self,
        build_info: &BuildInfo,
        current_files: &[PathBuf],
    ) -> Result<()> {
        let current_set: FxHashSet<_> = current_files.iter().collect();
        let _previous_set: FxHashSet<_> = build_info.file_infos.keys().map(PathBuf::from).collect();

        // Find new files
        for file in current_files {
            let path_str = file.to_string_lossy();
            if !build_info.file_infos.contains_key(path_str.as_ref()) {
                self.new_files.insert(file.clone());
                self.affected_files.insert(file.clone());
            }
        }

        // Find deleted files
        for path_str in build_info.file_infos.keys() {
            let path = PathBuf::from(path_str);
            if !current_set.contains(&path) {
                self.deleted_files.insert(path);
            }
        }

        // Check for modified files
        for file in current_files {
            if self.new_files.contains(file) {
                continue;
            }

            let current_version = compute_file_version(file)?;
            let path_str = file.to_string_lossy();

            if build_info.has_file_changed(&path_str, &current_version) {
                self.changed_files.insert(file.clone());
                self.affected_files.insert(file.clone());
            }
        }

        // Add dependents of changed files
        let mut dependents_to_add = Vec::new();
        for changed in &self.changed_files {
            let path_str = changed.to_string_lossy();
            for dep in build_info.get_dependents(&path_str) {
                dependents_to_add.push(PathBuf::from(dep));
            }
        }

        // Also handle deleted file dependents
        for deleted in &self.deleted_files {
            let path_str = deleted.to_string_lossy();
            for dep in build_info.get_dependents(&path_str) {
                dependents_to_add.push(PathBuf::from(dep));
            }
        }

        for dep in dependents_to_add {
            if current_set.contains(&dep) {
                self.affected_files.insert(dep);
            }
        }

        Ok(())
    }

    /// Compute changes with absolute file paths
    /// Automatically normalizes paths relative to base_dir for comparison with BuildInfo
    pub fn compute_changes_with_base(
        &mut self,
        build_info: &BuildInfo,
        current_files: &[PathBuf],
        base_dir: &Path,
    ) -> Result<()> {
        // Normalize absolute paths to relative paths for BuildInfo comparison
        let current_files_relative: Vec<PathBuf> = current_files
            .iter()
            .filter_map(|path| path.strip_prefix(base_dir).ok().map(|p| p.to_path_buf()))
            .collect();

        // Compute changes using relative paths, but store absolute paths in results
        let current_set: FxHashSet<_> = current_files_relative.iter().collect();
        let _previous_set: FxHashSet<_> = build_info.file_infos.keys().map(PathBuf::from).collect();

        // Find new files
        for (i, file_rel) in current_files_relative.iter().enumerate() {
            let path_str = file_rel.to_string_lossy();
            if !build_info.file_infos.contains_key(path_str.as_ref()) {
                let abs_path = &current_files[i];
                self.new_files.insert(abs_path.clone());
                self.affected_files.insert(abs_path.clone());
            }
        }

        // Find deleted files
        for path_str in build_info.file_infos.keys() {
            let path = PathBuf::from(path_str);
            if !current_set.contains(&path) {
                self.deleted_files.insert(path);
            }
        }

        // Check for modified files
        for (i, file_rel) in current_files_relative.iter().enumerate() {
            let abs_path = &current_files[i];
            if self.new_files.contains(abs_path) {
                continue;
            }

            let current_version = compute_file_version(abs_path)?;
            let path_str = file_rel.to_string_lossy();

            if build_info.has_file_changed(&path_str, &current_version) {
                self.changed_files.insert(abs_path.clone());
                self.affected_files.insert(abs_path.clone());
            }
        }

        Ok(())
    }

    /// Get files that have changed
    pub fn changed_files(&self) -> &FxHashSet<PathBuf> {
        &self.changed_files
    }

    /// Get all files that need to be recompiled
    pub fn affected_files(&self) -> &FxHashSet<PathBuf> {
        &self.affected_files
    }

    /// Get new files
    pub fn new_files(&self) -> &FxHashSet<PathBuf> {
        &self.new_files
    }

    /// Get deleted files
    pub fn deleted_files(&self) -> &FxHashSet<PathBuf> {
        &self.deleted_files
    }

    /// Check if any files have changed
    pub fn has_changes(&self) -> bool {
        !self.changed_files.is_empty()
            || !self.new_files.is_empty()
            || !self.deleted_files.is_empty()
    }

    /// Get total number of affected files
    pub fn affected_count(&self) -> usize {
        self.affected_files.len()
    }
}

/// Compute a version string for a file (content hash)
pub fn compute_file_version(path: &Path) -> Result<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let content =
        std::fs::read(path).with_context(|| format!("failed to read file: {}", path.display()))?;

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();

    Ok(format!("{:016x}", hash))
}

/// Compute a signature for a file's exports (for dependency tracking)
pub fn compute_export_signature(exports: &[String]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    for export in exports {
        export.hash(&mut hasher);
    }

    format!("{:016x}", hasher.finish())
}

/// Builder for creating build info incrementally
pub struct BuildInfoBuilder {
    build_info: BuildInfo,
    base_dir: PathBuf,
}

impl BuildInfoBuilder {
    /// Create a new builder
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            build_info: BuildInfo::new(),
            base_dir,
        }
    }

    /// Create a builder from existing build info
    pub fn from_existing(build_info: BuildInfo, base_dir: PathBuf) -> Self {
        Self {
            build_info,
            base_dir,
        }
    }

    /// Set root files
    pub fn set_root_files(&mut self, files: Vec<String>) -> &mut Self {
        self.build_info.root_files = files;
        self
    }

    /// Add a file to the build info
    pub fn add_file(&mut self, path: &Path, exports: &[String]) -> Result<&mut Self> {
        let relative_path = self.relative_path(path);
        let version = compute_file_version(path)?;
        let signature = if exports.is_empty() {
            None
        } else {
            Some(compute_export_signature(exports))
        };

        self.build_info.set_file_info(
            &relative_path,
            FileInfo {
                version,
                signature,
                affected_files_pending_emit: false,
                implied_format: None,
            },
        );

        Ok(self)
    }

    /// Set dependencies for a file
    pub fn set_file_dependencies(&mut self, path: &Path, deps: Vec<PathBuf>) -> &mut Self {
        let relative_path = self.relative_path(path);
        let relative_deps: Vec<String> = deps.iter().map(|d| self.relative_path(d)).collect();

        self.build_info
            .set_dependencies(&relative_path, relative_deps);
        self
    }

    /// Set emit signature for a file
    pub fn set_file_emit(
        &mut self,
        path: &Path,
        js_hash: Option<&str>,
        dts_hash: Option<&str>,
    ) -> &mut Self {
        let relative_path = self.relative_path(path);
        self.build_info.set_emit_signature(
            &relative_path,
            EmitSignature {
                js: js_hash.map(String::from),
                dts: dts_hash.map(String::from),
                map: None,
            },
        );
        self
    }

    /// Set compiler options
    pub fn set_options(&mut self, options: BuildInfoOptions) -> &mut Self {
        self.build_info.options = options;
        self
    }

    /// Build the final build info
    pub fn build(mut self) -> BuildInfo {
        self.build_info.build_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.build_info
    }

    /// Get a relative path from the base directory
    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.base_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

/// Determine the default .tsbuildinfo path based on configuration
pub fn default_build_info_path(config_path: &Path, out_dir: Option<&Path>) -> PathBuf {
    let config_name = config_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tsconfig");

    let build_info_name = format!("{}.tsbuildinfo", config_name);

    if let Some(out) = out_dir {
        out.join(&build_info_name)
    } else {
        config_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&build_info_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_info_roundtrip() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.tsbuildinfo");

        let mut build_info = BuildInfo::new();
        build_info.root_files = vec!["src/index.ts".to_string()];
        build_info.set_file_info(
            "src/index.ts",
            FileInfo {
                version: "abc123".to_string(),
                signature: Some("sig456".to_string()),
                affected_files_pending_emit: false,
                implied_format: None,
            },
        );
        build_info.set_dependencies("src/index.ts", vec!["src/utils.ts".to_string()]);

        // Save
        build_info.save(&path).unwrap();

        // Load
        let loaded = BuildInfo::load(&path).unwrap();
        let loaded = loaded.expect("Should load valid build info");

        assert_eq!(loaded.root_files, build_info.root_files);
        assert_eq!(loaded.file_infos.len(), 1);
        assert!(loaded.file_infos.contains_key("src/index.ts"));
    }

    #[test]
    fn test_file_change_detection() {
        let mut build_info = BuildInfo::new();
        build_info.set_file_info(
            "src/index.ts",
            FileInfo {
                version: "v1".to_string(),
                signature: None,
                affected_files_pending_emit: false,
                implied_format: None,
            },
        );

        // Same version - not changed
        assert!(!build_info.has_file_changed("src/index.ts", "v1"));

        // Different version - changed
        assert!(build_info.has_file_changed("src/index.ts", "v2"));

        // New file - changed
        assert!(build_info.has_file_changed("src/new.ts", "v1"));
    }

    #[test]
    fn test_dependent_tracking() {
        let mut build_info = BuildInfo::new();

        // index.ts depends on utils.ts
        build_info.set_dependencies("src/index.ts", vec!["src/utils.ts".to_string()]);
        // main.ts also depends on utils.ts
        build_info.set_dependencies("src/main.ts", vec!["src/utils.ts".to_string()]);

        let dependents = build_info.get_dependents("src/utils.ts");
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains(&"src/index.ts".to_string()));
        assert!(dependents.contains(&"src/main.ts".to_string()));
    }

    #[test]
    fn test_change_tracker() {
        let temp = TempDir::new().unwrap();

        // Create test files
        let file1 = temp.path().join("file1.ts");
        let file2 = temp.path().join("file2.ts");
        std::fs::write(&file1, "content1").unwrap();
        std::fs::write(&file2, "content2").unwrap();

        // Build info with file1
        let mut build_info = BuildInfo::new();
        let version1 = compute_file_version(&file1).unwrap();
        build_info.set_file_info(
            "file1.ts",
            FileInfo {
                version: version1,
                signature: None,
                affected_files_pending_emit: false,
                implied_format: None,
            },
        );

        // Track changes - file2 is new
        let mut tracker = ChangeTracker::new();
        tracker
            .compute_changes(&build_info, &[file1.clone(), file2.clone()])
            .unwrap();

        assert!(tracker.new_files().contains(&file2));
        assert!(!tracker.changed_files().contains(&file1));
        assert!(tracker.has_changes());
    }

    #[test]
    fn test_build_info_builder() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.ts");
        std::fs::write(&file, "export const x = 1;").unwrap();

        let mut builder = BuildInfoBuilder::new(temp.path().to_path_buf());
        builder
            .set_root_files(vec!["test.ts".to_string()])
            .add_file(&file, &["x".to_string()])
            .unwrap()
            .set_file_dependencies(&file, vec![]);

        let build_info = builder.build();

        assert_eq!(build_info.root_files, vec!["test.ts"]);
        assert!(build_info.file_infos.contains_key("test.ts"));
        assert!(build_info.file_infos["test.ts"].signature.is_some());
    }

    #[test]
    fn test_default_build_info_path() {
        let config = Path::new("/project/tsconfig.json");

        // Without outDir
        let path = default_build_info_path(config, None);
        assert_eq!(path, PathBuf::from("/project/tsconfig.tsbuildinfo"));

        // With outDir
        let path = default_build_info_path(config, Some(Path::new("/project/dist")));
        assert_eq!(path, PathBuf::from("/project/dist/tsconfig.tsbuildinfo"));
    }

    #[test]
    fn test_build_info_version_mismatch_returns_none() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.tsbuildinfo");

        // Create a build info with wrong version
        let build_info = BuildInfo {
            version: "0.0.0-wrong".to_string(), // Wrong version
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        };
        build_info.save(&path).unwrap();

        // Loading should return Ok(None) for version mismatch
        let result = BuildInfo::load(&path).unwrap();
        assert!(result.is_none(), "Version mismatch should return None");
    }

    #[test]
    fn test_build_info_compiler_version_mismatch_returns_none() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.tsbuildinfo");

        // Create a build info with wrong compiler version
        let build_info = BuildInfo {
            version: BUILD_INFO_VERSION.to_string(),
            compiler_version: "0.0.0-wrong".to_string(), // Wrong version
            ..Default::default()
        };
        build_info.save(&path).unwrap();

        // Loading should return Ok(None) for compiler version mismatch
        let result = BuildInfo::load(&path).unwrap();
        assert!(
            result.is_none(),
            "Compiler version mismatch should return None"
        );
    }

    #[test]
    fn test_build_info_valid_versions_returns_some() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.tsbuildinfo");

        // Create a valid build info
        let build_info = BuildInfo {
            version: BUILD_INFO_VERSION.to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            root_files: vec!["src/index.ts".to_string()],
            ..Default::default()
        };
        build_info.save(&path).unwrap();

        // Loading should return Ok(Some(build_info))
        let result = BuildInfo::load(&path).unwrap();
        assert!(result.is_some(), "Valid build info should return Some");

        let loaded = result.unwrap();
        assert_eq!(loaded.version, BUILD_INFO_VERSION);
        assert_eq!(loaded.compiler_version, env!("CARGO_PKG_VERSION"));
    }
}
