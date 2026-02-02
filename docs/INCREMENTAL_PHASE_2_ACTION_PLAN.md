# Incremental Compilation - Phase 2: Smart Invalidation & Diagnostics Caching

## Overview

This document provides a concrete, prioritized action plan for implementing the remaining incremental compilation features in tsz, based on comprehensive analysis of TypeScript's behavior and tsz's current architecture.

## Priority Matrix

| Priority | Feature | Value | Complexity | Dependencies |
|----------|---------|-------|------------|--------------|
| **P1** | Smart Invalidation (Export Hash Comparison) | 🔴 Critical | High | None |
| **P2** | Semantic Diagnostics Caching | 🔴 Critical | Medium | P1 |
| **P3** | BuildInfo Version Compatibility | 🟡 Important | Low | None |
| **P4** | Compression (fileIdsList) | 🟢 Nice-to-have | Low | None |
| **P5** | Composite Projects | 🔵 Strategic | High | P1, P2 |

## Phase 1: Smart Invalidation (P1)

### Problem Statement
Currently, tsz parses, binds, and type-checks ALL files on every build, even when loading BuildInfo. The only optimization is avoiding re-emitting unchanged output files.

### Goal
Skip the expensive Type Checking phase for files that:
1. Haven't changed (source hash matches)
2. Don't depend on files whose export signatures changed

### Implementation Steps

#### Step 1.1: Implement Export Hash Comparison Logic

**File:** `src/cli/driver.rs`

**Current Code:**
```rust
// collect_diagnostics type-checks ALL files
for (file_idx, file) in program.files.iter().enumerate() {
    // Always checks, even if unchanged
}
```

**New Logic:**
```rust
// Add to compile_inner or collect_diagnostics

// 1. After binding, compute which files need type checking
let mut files_to_check: Vec<(usize, BoundFile)> = Vec::new();
let mut files_to_skip: Vec<PathBuf> = Vec::new();

for (file_idx, file) in program.files.iter().enumerate() {
    let file_path = PathBuf::from(&file.file_name);

    // Check if file is in the "affected" set
    let must_check = if let Some(ref c) = effective_cache {
        // File was changed or has no cached export hash
        !c.export_hashes.contains_key(&file_path) ||
        change_tracker.affected_files().contains(&file_path)
    } else {
        true // No cache, must check everything
    };

    if must_check {
        files_to_check.push((file_idx, file));
    } else {
        files_to_skip.push(file_path);
    }
}
```

#### Step 1.2: Implement Cascading Invalidation

**File:** `src/cli/driver.rs`

Add a new function to determine the invalidation set:

```rust
/// Determine which files need type checking based on export hash changes
fn compute_invalidation_set(
    cache: &CompilationCache,
    changed_files: &HashSet<PathBuf>,
    program: &MergedProgram,
    base_dir: &Path,
) -> HashSet<PathBuf> {
    let mut invalidation_set = HashSet::new();
    let mut work_queue: VecDeque<PathBuf> = changed_files.iter().cloned().collect();
    let mut checked = HashSet::new();

    while let Some(path) = work_queue.pop_front() {
        if checked.contains(&path) {
            continue;
        }

        // Add to invalidation set
        invalidation_set.insert(path.clone());

        // Compute NEW export hash for this file
        // (requires running checker, which we're doing anyway)

        // Compare with old hash
        let old_hash = cache.export_hashes.get(&path);

        // Get dependents
        if let Some(dependents) = cache.reverse_dependencies.get(&path) {
            if old_hash.is_some() {
                // If we have an old hash and the new one differs,
                // dependents need re-checking (this will be determined
                // after we compute the new hash)
            }
        }

        checked.insert(path);
    }

    invalidation_set
}
```

**Key Insight:** We can't know if the export hash changed until we type-check the file. This creates the "chicken-and-egg" problem. The solution is:

1. **Always type-check changed files** (baseline cost)
2. **After checking, compare export hash**
3. **If different, add direct dependents to the check queue**
4. **Repeat until queue is empty**

#### Step 1.3: Update `collect_diagnostics` to Use Invalidation Set

**File:** `src/cli/driver.rs`

```rust
fn collect_diagnostics(
    program: &MergedProgram,
    options: &ResolvedCompilerOptions,
    base_dir: &Path,
    cache: Option<&mut CompilationCache>,
    lib_contexts: &[LibContext],
    // NEW: Pass the set of files that need checking
    invalidation_set: Option<&HashSet<PathBuf>>,
) -> Vec<Diagnostic> {
    // ... existing setup ...

    for (file_idx, file) in program.files.iter().enumerate() {
        let file_path = PathBuf::from(&file.file_name);

        // NEW: Skip if not in invalidation set AND we have cached diagnostics
        if let Some(set) = invalidation_set {
            if !set.contains(&file_path) {
                if let Some(cached) = cache
                    .as_deref()
                    .and_then(|c| c.diagnostics.get(&file_path))
                {
                    diagnostics.extend(cached.clone());
                    continue;
                }
            }
        }

        // ... rest of type checking ...
    }
}
```

### Expected Performance Impact

- **First build:** Same performance (baseline)
- **Second build (no changes):** Minor improvement (skip binding/cache creation)
- **Third build (implementation change):** Major improvement (skip type checking of dependents)

## Phase 2: Semantic Diagnostics Caching (P2)

### Problem Statement
If we implement Phase 1 (skipping type checking), files that are skipped will have ZERO diagnostics reported. This is the "ghost diagnostic" problem - errors disappear.

### Goal
Persist diagnostics to BuildInfo and replay them for skipped files.

### Implementation Steps

#### Step 2.1: Create CachedDiagnostic Struct

**File:** `src/cli/incremental.rs`

```rust
use crate::checker::types::diagnostics::{Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation};

/// A lightweight representation of a diagnostic for storage in .tsbuildinfo
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CachedDiagnostic {
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related_information: Vec<CachedRelatedInformation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CachedRelatedInformation {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
}

// Conversion traits
impl From<&Diagnostic> for CachedDiagnostic {
    fn from(diag: &Diagnostic) -> Self {
        Self {
            start: diag.start,
            length: diag.length,
            message_text: diag.message_text.clone(),
            category: diag.category,
            code: diag.code,
            related_information: diag.related_information
                .iter()
                .map(|info| CachedRelatedInformation {
                    file: info.file.clone(),
                    start: info.start,
                    length: info.length,
                    message_text: info.message_text.clone(),
                    category: info.category,
                    code: info.code,
                })
                .collect(),
        }
    }
}

impl CachedDiagnostic {
    pub fn to_diagnostic(&self, file: String) -> Diagnostic {
        Diagnostic {
            file,
            start: self.start,
            length: self.length,
            message_text: self.message_text.clone(),
            category: self.category,
            code: self.code,
            related_information: self.related_information
                .iter()
                .map(|r| DiagnosticRelatedInformation {
                    file: r.file.clone(),
                    start: r.start,
                    length: r.length,
                    message_text: r.message_text.clone(),
                    category: r.category,
                    code: r.code,
                })
                .collect(),
        }
    }
}
```

#### Step 2.2: Update BuildInfo Struct

**File:** `src/cli/incremental.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    // ... existing fields ...

    /// Semantic diagnostics cached from the previous run.
    /// Key is the relative file path.
    #[serde(default)]
    pub semantic_diagnostics_per_file: BTreeMap<String, Vec<CachedDiagnostic>>,

    // ... rest of fields ...
}
```

#### Step 2.3: Update Save/Load Functions

**File:** `src/cli/driver.rs`

**In `compilation_cache_to_build_info`:**

```rust
// Convert cache.diagnostics to BuildInfo format
let mut semantic_diagnostics = BTreeMap::new();
for (path, diags) in &cache.diagnostics {
    let relative_path = path
        .strip_prefix(base_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    let cached: Vec<CachedDiagnostic> = diags
        .iter()
        .map(|d| CachedDiagnostic::from(d))
        .collect();

    semantic_diagnostics.insert(relative_path, cached);
}

// Assign to BuildInfo
build_info.semantic_diagnostics_per_file = semantic_diagnostics;
```

**In `build_info_to_compilation_cache`:**

```rust
// Load diagnostics from BuildInfo into cache
for (path_str, cached_diags) in &build_info.semantic_diagnostics_per_file {
    let full_path = base_dir.join(path_str);

    let diags: Vec<Diagnostic> = cached_diags
        .iter()
        .map(|cd| cd.to_diagnostic(full_path.to_string_lossy().into_owned()))
        .collect();

    cache.diagnostics.insert(full_path, diags);
}
```

#### Step 2.4: Update `collect_diagnostics` to Persist Diagnostics

**File:** `src/cli/driver.rs`

```rust
// At the end of checking a file
if let Some(c) = effective_cache.as_deref_mut() {
    let export_hash = compute_export_hash(program, file, file_idx, &mut checker);
    c.type_caches.insert(file_path.clone(), checker.extract_cache());

    // NEW: Also cache diagnostics
    c.diagnostics.insert(file_path.clone(), file_diagnostics.clone());

    c.export_hashes.insert(file_path, export_hash);
}
```

### Expected Impact

- Files that are type-checked will have diagnostics persisted to BuildInfo
- Files that are skipped (due to Phase 1) will replay cached diagnostics
- No more "ghost diagnostics" - errors persist across builds

## Phase 3: BuildInfo Version Compatibility (P3)

### Problem Statement
Currently, `BuildInfo::load` uses `bail!()` when version doesn't match, causing the build to fail. This differs from tsc's behavior.

### Goal
Gracefully handle version mismatches by treating them as cache misses.

### Implementation Steps

#### Step 3.1: Change Load Signature

**File:** `src/cli/incremental.rs`

**Current:**
```rust
pub fn load(path: &Path) -> Result<Self> {
    let content = std::fs::read_to_string(path)?;
    let build_info: BuildInfo = serde_json::from_str(&content)?;

    if build_info.version != BUILD_INFO_VERSION {
        bail!("Build info version mismatch: expected {}, got {}", ...);
    }

    Ok(build_info)
}
```

**New:**
```rust
pub fn load(path: &Path) -> Result<Option<Self>> {
    let content = std::fs::read_to_string(path)?;
    let build_info: BuildInfo = serde_json::from_str(&content)?;

    // Validate version compatibility
    if build_info.version != BUILD_INFO_VERSION {
        tracing::debug!(
            "Build info version mismatch: expected {}, got {}. Ignoring cache.",
            BUILD_INFO_VERSION,
            build_info.version
        );
        return Ok(None);
    }

    // Validate compiler version (optional but recommended)
    if build_info.compiler_version != env!("CARGO_PKG_VERSION") {
        tracing::debug!(
            "Compiler version mismatch: expected {}, got {}. Ignoring cache.",
            env!("CARGO_PKG_VERSION"),
            build_info.compiler_version
        );
        return Ok(None);
    }

    Ok(Some(build_info))
}
```

#### Step 3.2: Update Call Sites

**File:** `src/cli/driver.rs`

```rust
// In compile_inner
if let Some(build_info_path) = get_build_info_path(...) {
    if build_info_path.exists() {
        match BuildInfo::load(&build_info_path) {
            Ok(Some(build_info)) => {
                // Use the loaded BuildInfo
                local_cache = Some(build_info_to_compilation_cache(&build_info, &base_dir));
            }
            Ok(None) => {
                // Version mismatch - start fresh
                tracing::info!("BuildInfo cache invalidated, starting fresh build");
            }
            Err(e) => {
                tracing::warn!("Failed to load BuildInfo: {}, starting fresh", e);
            }
        }
    }
    should_save_build_info = true;
}
```

### Expected Impact

- Upgrading tsz versions won't break existing builds
- Graceful fallback to full build when cache is incompatible
- Matches tsc behavior exactly

## Phase 4: Compression - fileIdsList (P4)

### Problem Statement
Current BuildInfo uses full string paths everywhere, leading to large file sizes.

### Goal
Compress file paths using integer IDs, matching tsc's format.

### Implementation Steps

#### Step 4.1: Add Compression Helper

**File:** `src/cli/incremental.rs`

```rust
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

struct FileRegistry {
    paths: Vec<String>,
    lookup: FxHashMap<String, usize>,
}

impl FileRegistry {
    fn new() -> Self {
        Self {
            paths: Vec::new(),
            lookup: FxHashMap::default(),
        }
    }

    fn get_or_register(&mut self, path: &str) -> usize {
        if let Some(&id) = self.lookup.get(path) {
            return id;
        }

        let id = self.paths.len();
        self.paths.push(path.to_string());
        self.lookup.insert(path.to_string(), id);
        id
    }
}
```

#### Step 4.2: Update BuildInfo Structure

**File:** `src/cli/incremental.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    // ... existing fields ...

    /// Compressed file path registry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_ids_list: Option<Vec<String>>,

    /// Dependencies using integer IDs instead of paths
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced_map: Option<Vec<(usize, Vec<usize>)>>,

    // ... keep legacy fields for compatibility during transition ...
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, Vec<String>>,
}
```

#### Step 4.3: Implement Compression in Save

**File:** `src/cli/incremental.rs`

```rust
impl BuildInfo {
    pub fn save(&self, path: &Path) -> Result<()> {
        // If already compressed, just save
        if self.file_ids_list.is_some() {
            return self.save_direct(path);
        }

        // Otherwise, compress before saving
        self.save_compressed(path)
    }

    fn save_compressed(&self, path: &Path) -> Result<()> {
        let mut registry = FileRegistry::new();

        // Register all unique paths
        for path in self.file_infos.keys() {
            registry.get_or_register(path);
        }

        // Convert dependencies to IDs
        let mut referenced_map = Vec::new();
        for (src, deps) in &self.dependencies {
            let src_id = registry.get_or_register(src);
            let dep_ids: Vec<usize> = deps
                .iter()
                .map(|d| registry.get_or_register(d))
                .collect();
            referenced_map.push((src_id, dep_ids));
        }

        // Create compressed version
        let compressed = Self {
            file_ids_list: Some(registry.paths),
            referenced_map: Some(referenced_map),
            dependencies: BTreeMap::new(), // Clear legacy field
            ..self.clone()
        };

        compressed.save_direct(path)
    }
}
```

### Expected Impact

- Reduced .tsbuildinfo file size (30-50% for large projects)
- Faster save/load due to smaller JSON size
- Better compatibility with tsc's format

## Testing Strategy

### Unit Tests

1. **Export Hash Test:**
   - Create two files: A.ts (exports `foo: number`), B.ts (imports from A)
   - Build and check export hash
   - Change A.ts implementation (not export signature)
   - Rebuild and verify B.ts is NOT re-checked

2. **Diagnostic Cache Test:**
   - File with error
   - Build (captures error)
   - Touch file (no change)
   - Rebuild (replays cached error)
   - Fix file
   - Rebuild (error is gone, new cache is clean)

3. **Version Compatibility Test:**
   - Build with version "0.1.0"
   - Update code to version "0.2.0"
   - Build should succeed (cache invalidated, not error)

4. **Compression Test:**
   - Project with 100 files
   - Build and check fileIdsList is populated
   - Verify all dependencies use integer IDs

### Integration Tests

1. **Real-World Scenario:**
   - Large project (monorepo)
   - Measure build times:
     - Cold build (no .tsbuildinfo)
     - Warm build (no changes)
     - Incremental build (change one file in deep dependency)

2. **Watch Mode Test:**
   - Start watch mode
   - Make change
   - Verify only changed file + dependents are re-checked

## Success Criteria

- [ ] Files with unchanged source are skipped (binding still happens)
- [ ] Files whose dependencies' API didn't change are skipped during type checking
- [ ] Diagnostics persist correctly across builds (no ghost errors)
- [ ] Version mismatches don't cause build failures
- [ ] .tsbuildinfo file size is reduced by ~30% for large projects
- [ ] All tests pass, including new incremental tests

## Open Questions

1. **Should we cache ASTs to disk?**
   - Pros: Skip parsing/binding entirely
   - Cons: Complex serialization, large files
   - Decision: Deferred - measure performance first

2. **Should emit_signatures be populated?**
   - Currently always None
   - Could avoid re-emitting unchanged files
   - Decision: Add in Phase 4 if time permits

3. **How to handle composite projects?**
   - Requires cross-project .d.ts tracking
   - Dependent on P1 and P2
   - Decision: Phase 5 (future work)

## References

- `src/cli/incremental.rs` - BuildInfo structures
- `src/cli/driver.rs` - CompilationCache and compilation logic
- `docs/INCREMENTAL_COMPILATION_ACTION_PLAN.md` - Original analysis
- Gemini Q1-Q10 responses - Technical details and recommendations
