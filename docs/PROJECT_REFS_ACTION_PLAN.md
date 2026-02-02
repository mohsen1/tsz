# Project References Implementation Plan for tsz

## Overview

This document provides a concrete implementation plan for TypeScript Project References support in tsz, based on:
1. Comprehensive analysis of tsz's current codebase infrastructure
2. Detailed examination of tsc's project references behavior
3. Minimal viable implementation (MVP) approach

**Status**: Foundation exists in `src/cli/project_refs.rs`, needs integration and completion

## Current State Analysis

### What Already Exists (✅ Implemented)

1. **Project Reference Graph** (`src/cli/project_refs.rs`)
   - `ProjectReference` struct with `path`, `prepend`, `circular` fields
   - `ProjectReferenceGraph::load` - recursively loads all tsconfig.json files
   - `build_order()` - Kahn's algorithm for topological sorting
   - `detect_cycles()` - DFS-based cycle detection
   - `validate_composite_project()` - validates composite project requirements
   - `resolve_cross_project_import()` - resolves imports to .d.ts files in output directories

2. **BuildInfo Infrastructure** (`src/cli/incremental.rs`)
   - `BuildInfo` struct with file hashes, dependencies, signatures
   - `emit_signatures` map for tracking .d.ts content changes
   - Per-project `.tsbuildinfo` files (one per tsconfig.json)
   - Change detection via content hashing (not timestamps)
   - **Note**: `latestChangedDtsFile` field is **NOT** implemented; using `emit_signatures` instead

3. **Declaration Emitter** (`src/declaration_emitter.rs`)
   - Fully functional .d.ts generation
   - Handles type-only imports correctly
   - Emits proper export signatures

4. **Multi-File Type Checking** (`src/checker/context.rs`, `src/checker/symbol_resolver.rs`)
   - `CheckerContext` with `all_arenas`, `all_binders`, `resolved_module_paths`
   - Cross-file symbol resolution via `resolve_cross_file_export`
   - Context switching for resolving types from other files

### What's Missing (❌ Not Implemented)

1. **Build Mode Orchestrator**
   - No `--build` flag handler
   - No loop over sorted projects
   - No cross-project build status coordination

2. **Module Resolution Integration**
   - `resolve_module_specifier` in `driver_resolution.rs` doesn't use project refs
   - Cross-project imports not wired into main resolver

3. **`latestChangedDtsFile` Field**
   - Not in `BuildInfo` struct (using `emit_signatures` instead)
   - May need to add for tsc compatibility

4. **Build Info Coordination Across Projects**
   - No mechanism to check if upstream project output changed
   - No invalidation propagation between projects

## tsc's Behavior (Observed)

### Build Order Execution
```
1. Load dependency graph from root tsconfig.json
2. Topological sort (Kahn's algorithm)
3. For each project in order:
   a. Check if .tsbuildinfo exists and is valid
   b. Check if any inputs changed (source files or referenced .d.ts)
   c. If dirty: compile, emit .d.ts, write .tsbuildinfo
   d. If clean: skip compilation
4. Stop on first error (unless --force)
```

### Module Resolution with Project References
- Import `import { X } from "../core"` in source
- Resolved to `../core/dist/index.d.ts` (not `../core/src/index.ts`)
- Uses `latestChangedDtsFile` field in .tsbuildinfo for fast invalidation
- The "redirect" happens automatically: source import → output .d.ts

### Key BuildInfo Fields
```json
{
  "latestChangedDtsFile": "./dist/index.d.ts",  // Most recently changed .d.ts
  "fileInfos": [...],
  "version": "5.9.3"
}
```

## Minimal Viable Implementation (MVP)

### Phase 1: Build Mode Orchestrator (Week 1)

**File**: `src/cli/driver.rs` - New function

```rust
pub fn run_build_mode(args: &CliArgs, cwd: &Path) -> Result<CompilationResult> {
    // 1. Load root tsconfig.json
    let root_config = resolve_tsconfig_path(cwd, args.project.as_deref())?
        .ok_or_else(|| anyhow::anyhow!("No tsconfig found"))?;

    // 2. Load project reference graph
    let graph = ProjectReferenceGraph::load(&root_config)?;

    // 3. Get build order (topological sort)
    let build_order = graph.build_order()?;

    // 4. Build projects in dependency order
    let mut all_diagnostics = Vec::new();
    let mut build_failed = false;

    for project_id in build_order {
        let project = graph.get_project(project_id)
            .ok_or_else(|| anyhow::anyhow!("Project not found: {:?}", project_id))?;

        // Check if project is up-to-date
        if !args.force && is_project_up_to_date(project) {
            tracing::info!("Project {} is up to date, skipping", project.config_path.display());
            continue;
        }

        tracing::info!("Building project: {}", project.config_path.display());

        // Compile this project
        let result = compile_project(args, &project.root_dir, &project.config_path)?;

        if !result.diagnostics.is_empty() {
            all_diagnostics.extend(result.diagnostics.clone());
            build_failed = true;

            if args.no_emit_on_error {
                anyhow::bail!("Build failed for {}", project.config_path.display());
            }
        }
    }

    Ok(CompilationResult {
        diagnostics: all_diagnostics,
        ..Default::default()
    })
}

fn is_project_up_to_date(project: &ResolvedProject) -> bool {
    // Load .tsbuildinfo for this project
    let build_info_path = get_build_info_path_for_project(project);

    if !build_info_path.exists() {
        return false;
    }

    // Try to load build info
    match BuildInfo::load(&build_info_path) {
        Ok(Some(build_info)) => {
            // Check if referenced projects' outputs changed
            // This requires checking latestChangedDtsFile or emit_signatures
            check_referenced_projects_uptodate(project, &build_info)
        }
        Ok(None) => false, // Version mismatch
        Err(_) => false,   // Corrupted or missing
    }
}
```

### Phase 2: Module Resolution Integration (Week 1-2)

**File**: `src/cli/driver_resolution.rs` - Modify `resolve_module_specifier`

```rust
pub fn resolve_module_specifier(
    // ... existing parameters ...
    project_refs: Option<&ResolvedProject>,  // ADD THIS PARAMETER
) -> Option<PathBuf> {
    // ... existing resolution logic ...

    // NEW: Check project references
    if let Some(refs) = project_refs {
        // Try each referenced project
        for reference in &references.resolved_references {
            if let Some(resolved) = resolve_cross_project_import(reference, module_specifier) {
                return Some(resolved);
            }
        }
    }

    // ... fallback to existing logic ...
}
```

**File**: `src/cli/driver.rs` - Pass project refs to resolver

```rust
// In compile_inner or similar
let resolved_project = load_project_with_references(config_path)?;

// Pass to module resolution calls
let resolved = resolve_module_specifier(
    file_path,
    specifier,
    options,
    base_dir,
    &mut resolution_cache,
    &program_paths,
    Some(&resolved_project),  // NEW PARAMETER
)?;
```

### Phase 3: Add `latestChangedDtsFile` Field (Week 2)

**File**: `src/cli/incremental.rs` - Update BuildInfo struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInfo {
    // ... existing fields ...

    /// Path to the most recently changed .d.ts file
    /// Used by project references for fast invalidation checking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_changed_dts_file: Option<String>,
}
```

**Update save/load logic**:
- Set `latest_changed_dts_file` when saving BuildInfo after successful emit
- Read and use it when checking if downstream projects need rebuild

**File**: `src/cli/driver.rs` - Update emit logic

```rust
// After emitting declaration files, track the most recently changed one
let mut latest_dts: Option<String> = None;

for emitted_file in &emitted_files {
    if emitted_file.ends_with(".d.ts") {
        let relative_path = emitted_file.strip_prefix(base_dir)?.to_str()?;
        latest_dts = Some(relative_path.to_string());
    }
}

// Save to BuildInfo
if let Some(c) = cache.as_deref_mut() {
    // ... existing cache updates ...
    c.build_info.latest_changed_dts_file = latest_dts;
}
```

### Phase 4: Cross-Project Invalidation Logic (Week 2)

**File**: `src/cli/driver.rs` - New function

```rust
fn check_referenced_projects_uptodate(
    project: &ResolvedProject,
    build_info: &BuildInfo,
) -> bool {
    // For each referenced project
    for reference in &project.resolved_references {
        let ref_build_info_path = reference.project_dir.join("tsconfig.tsbuildinfo");

        if !ref_build_info_path.exists() {
            return false;  // Referenced project not built yet
        }

        match BuildInfo::load(&ref_build_info_path) {
            Ok(Some(ref_build_info)) => {
                // Check if referenced project's latestChangedDtsFile is newer
                // than our build time
                if let Some(latest_dts) = &ref_build_info.latest_changed_dts_file {
                    let full_dts_path = reference.project_dir.join(latest_dts);

                    // Check file modification time
                    if let Ok(ref_dts_mtime) = fs::metadata(&full_dts_path)
                        .and_then(|m| m.modified())
                    {
                        if let Ok(build_time) = get_build_info_timestamp(build_info) {
                            if ref_dts_mtime > build_time {
                                tracing::info!(
                                    "Referenced project {} has newer output",
                                    reference.project_name
                                );
                                return false;
                            }
                        }
                    }
                }
            }
            Ok(None) => return false,  // Version mismatch
            Err(_) => return false,    // Error loading
        }
    }

    true
}
```

### Phase 5: CLI Integration (Week 2)

**File**: `src/cli/args.rs` - Already has flags
- `--build` / `-b` ✅
- `--force` / `-f` ✅
- `--clean` ✅
- `--verbose` ✅

**File**: `src/bin/tsz.rs` - Update main function

```rust
fn main() -> Result<()> {
    let args = args::parse()?;
    let cwd = std::env::current_dir()?;

    if args.build {
        // NEW: Use build mode orchestrator
        driver::run_build_mode(&args, &cwd)?;
    } else {
        // Existing: single project compilation
        driver::compile(&args, &cwd)?;
    }

    Ok(())
}
```

### Phase 6: Testing (Week 2-3)

**Test Structure**:
```
tests/project_references/
├── basic_references/
│   ├── core/tsconfig.json
│   ├── core/src/index.ts
│   ├── utils/tsconfig.json (references: core)
│   └── utils/src/utils.ts
├── transitive_dependencies/
│   ├── core/tsconfig.json
│   ├── mid/tsconfig.json (references: core)
│   └── app/tsconfig.json (references: mid)
└── error_propagation/
    ├── lib/tsconfig.json
    └── app/tsconfig.json (references: lib)
```

**Test Cases**:
1. Basic reference resolution
2. Topological build order
3. Incremental rebuild (change upstream, verify downstream rebuilds)
4. Error propagation (upstream error stops build)
5. Clean mode (remove all outputs)
6. Force rebuild (ignore .tsbuildinfo)

## Deferred Features (Post-MVP)

1. **`prepend: true`**
   - Requires concatenating JS outputs
   - Complex source map merging
   - Use case: bundling scenarios

2. **Circular References**
   - Requires multi-pass compilation
   - Complex type checking semantics
   - `circular` flag exists but not implemented

3. **`disableSourceOfProjectReferenceRedirect`**
   - Allows source-to-source resolution
   - Defeats some performance optimizations
   - Use case: specific tooling scenarios

4. **Composite Project Mode Enhancements**
   - `outFile` with composite
   - `declarationDir` separate from `outDir`
   - Complex monorepo layouts

## Success Criteria

- [ ] `tsz --build` compiles projects in dependency order
- [ ] Upstream projects' .d.ts files are resolved correctly
- [ ] Changing an upstream project triggers downstream rebuild
- [ ] `.tsbuildinfo` files are used for incremental builds
- [ ] Errors in upstream projects stop the build
- [ ] `--force` rebuilds all projects
- [ ] `--clean` removes all outputs
- [ ] All test cases pass

## Open Questions

1. **Import Path Mapping**: Should we use `baseUrl` and `paths` mappings, or rely on automatic redirect to output directories?
   - **Recommendation**: Start with automatic redirect (tsc's approach)

2. **`latestChangedDtsFile` vs `emit_signatures`**: Should we add the field for tsc compatibility, or rely on our hash-based approach?
   - **Recommendation**: Add the field for compatibility, use it as a fast-path check

3. **Watch Mode**: How should project references work with `--watch`?
   - **Deferred**: Handle in Phase 2 of implementation

## Implementation Phases Summary

| Phase | Duration | Focus | Files |
|-------|----------|-------|-------|
| 1 | Week 1 | Build orchestrator | `driver.rs` |
| 2 | Week 1-2 | Module resolution integration | `driver_resolution.rs` |
| 3 | Week 2 | Add latestChangedDtsFile | `incremental.rs` |
| 4 | Week 2 | Cross-project invalidation | `driver.rs` |
| 5 | Week 2 | CLI integration | `bin/tsz.rs` |
| 6 | Week 2-3 | Testing | `tests/project_references/` |

## References

- `src/cli/project_refs.rs` - Project reference graph implementation
- `src/cli/incremental.rs` - BuildInfo structure and persistence
- `src/cli/driver_resolution.rs` - Module resolution logic
- `src/declaration_emitter.rs` - .d.ts generation
- `src/checker/context.rs` - Multi-file type checking context
- TypeScript Project References Documentation: https://www.typescriptlang.org/docs/handbook/project-references.html
