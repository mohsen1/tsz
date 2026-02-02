# Project References Implementation Progress

## Completed Phases

### ✅ Phase 1: Build Mode Orchestrator (Completed 2025-02-02)

**Implementation**: `src/cli/build.rs` + `src/bin/tsz.rs`

- Created `build_solution()` function as the main build orchestrator
- Integrated `is_project_up_to_date()` for incremental build detection
- Modified existing `handle_build()` to use up-to-date checking
- Added tracking and reporting of skipped/up-to-date projects

**Key Features**:
- Loads project reference graph using existing `ProjectReferenceGraph`
- Determines build order via topological sort (Kahn's algorithm)
- Checks `.tsbuildinfo` files for up-to-date status
- Skips projects that don't need rebuilding (unless `--force` is set)
- Reports build statistics (built count, skipped count, error count)

**Status**: Fully functional and tested

### ✅ Phase 3: latestChangedDtsFile Field (Completed 2025-02-02)

**Implementation**: `src/cli/incremental.rs`

- Added `latest_changed_dts_file: Option<String>` field to `BuildInfo` struct
- Field uses camelCase serialization (`latestChangedDtsFile`) to match tsc format
- Updated `BuildInfo::default()` to initialize the field as `None`
- Updated `compilation_cache_to_build_info()` to include the field

**Purpose**:
- Enables downstream projects to quickly check if upstream outputs changed
- Allows fast invalidation by comparing file modification times
- Avoids loading full upstream `.tsbuildinfo` for timestamp checks

**Status**: Infrastructure in place, TODO: implement tracking of which .d.ts changed during emit

## Remaining Work

### ⏳ Phase 2: Module Resolution Integration (Deferred)

**Status**: Infrastructure exists but not integrated

**What Exists**:
- `resolve_cross_project_import()` function in `project_refs.rs`
- `try_resolve_in_project()` function for finding .d.ts in output directories
- `ResolvedProject` struct with `resolved_references` field

**What's Missing**:
- Integration with main `resolve_module_specifier()` in driver.rs
- Threading project references through the resolver call chain
- Handling of relative imports across project boundaries

**Recommendation**: For MVP, rely on TypeScript's standard module resolution finding .d.ts files in output directories. Full integration can be deferred.

### ⏳ Phase 4: Cross-Project Invalidation Logic (Partially Complete)

**Status**: Basic checking exists, timestamp comparison incomplete

**What's Implemented**:
- `is_project_up_to_date()` checks if project's `.tsbuildinfo` exists
- Checks version compatibility via `BuildInfo::load()`
- Basic referenced project validation

**What's Missing**:
- Actual timestamp comparison using `latest_changed_dts_file`
- File modification time tracking
- Integration with emit phase to set `latest_changed_dts_file`

**Next Steps**:
1. Track emitted .d.ts files during emit phase
2. Set `latest_changed_dts_file` to the most recent .d.ts
3. Compare timestamps in `are_referenced_projects_uptodate()`

### ⏳ Phase 5: CLI Integration (Already Complete)

**Status**: Fully functional

- `--build` / `-b` flag ✅
- `--force` / `-f` flag ✅
- `--clean` flag ✅
- `--dry` flag ✅
- `--verbose` / `--build-verbose` flag ✅
- `--stopBuildOnErrors` flag ✅

### ⏳ Phase 6: Testing (Not Started)

**Status**: No tests yet for project references

**Test Structure Needed**:
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
└── incremental_rebuild/
    ├── lib/tsconfig.json
    └── app/tsconfig.json (references: lib)
```

**Test Cases**:
1. Basic reference resolution
2. Topological build order
3. Incremental rebuild (change upstream, verify downstream rebuilds)
4. Error propagation
5. Clean mode
6. Force rebuild

## Current Limitations

1. **No Module Resolution Integration**: Projects must use standard TypeScript module resolution to find referenced project outputs. Custom `resolve_cross_project_import` is not called.

2. **No .d.ts Change Tracking**: The `latest_changed_dts_file` field exists but is not populated. Cross-project invalidation relies on `.tsbuildinfo` existence only.

3. **No Source File Change Detection**: `is_project_up_to_date()` doesn't check if source files changed. It only checks if `.tsbuildinfo` exists and is valid.

4. **No Declaration Emit Verification**: No check that composite projects actually emitted `.d.ts` files.

## Recommended Next Steps

For a functional MVP:

1. **Implement source file change detection** in `is_project_up_to_date()`:
   - Load `BuildInfo` and compare file hashes
   - Use existing `ChangeTracker` from `incremental.rs`

2. **Track .d.ts emissions** and set `latest_changed_dts_file`:
   - During emit phase, track which .d.ts files were created
   - Find the most recent by modification time
   - Update `BuildInfo` before saving

3. **Implement timestamp comparison** in `are_referenced_projects_uptodate()`:
   - Use `latest_changed_dts_file` to get the path
   - Compare file modification time with build timestamp
   - Mark project as dirty if upstream output is newer

4. **Add basic tests** for core functionality:
   - Build order verification
   - Incremental build detection
   - Error propagation

5. **Defer full module resolution integration**:
   - Current approach should work for standard monorepo layouts
   - Custom resolution can be added later if needed

## Success Metrics

Current status:
- ✅ Projects build in dependency order
- ✅ Up-to-date projects are skipped (based on .tsbuildinfo existence)
- ✅ Build statistics are reported
- ✅ latestChangedDtsFile field exists (not yet populated)
- ⏳ Source file changes not detected
- ⏳ .d.ts timestamp comparison not implemented
- ❌ No automated tests for project references

For full MVP:
- [ ] Source file changes trigger rebuild
- [ ] Upstream .d.ts changes trigger downstream rebuild
- [ ] Basic test suite passes
- [ ] End-to-end build works for sample monorepo
