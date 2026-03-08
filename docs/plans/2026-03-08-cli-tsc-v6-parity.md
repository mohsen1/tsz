# CLI tsc v6 Full Parity Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `tsz` CLI output and behavior character-for-character identical to `tsc` v6 (`6.0.0-dev.20260306`).

**Architecture:** Replace clap's help/error rendering with custom tsc-matching renderers while keeping clap for argument parsing. Fix output streams, defaults, and flag coverage to match tsc v6 exactly.

**Tech Stack:** Rust, clap (parsing only), custom help/error rendering

**Reference:** Exact tsc v6 outputs captured in `/tmp/tsc-v6-reference.txt`

---

## Task 1: Version Output Parity

**Files:**
- Modify: `crates/tsz-cli/src/bin/tsz.rs`
- Modify: `crates/tsz-cli/src/args.rs`
- Modify: `crates/tsz-cli/Cargo.toml` (if build.rs needed)
- Test: `crates/tsz-cli/tests/tsc_compat_tests.rs`

**Goal:** `tsz --version` prints `Version 6.0.0-dev.20260306` (matching pinned TS version from `scripts/typescript-versions.json`).

**Step 1:** In `args.rs`, remove clap's `#[command(version)]` attribute or override it. The version string should come from the pinned TypeScript version, not Cargo.toml.

**Step 2:** In `preprocess_args()` in `tsz.rs`, intercept `--version` and `-v` (and `-V`) before clap processes them. Print `Version {pinned_version}\n` to stdout and exit 0.

**Step 3:** The pinned version should be read from `scripts/typescript-versions.json` at build time via a `build.rs` that sets a `TSZ_TSC_VERSION` env var, or hardcoded as a const that's updated when the pin changes.

**Step 4:** Add/update test in `tsc_compat_tests.rs`:
```rust
#[test]
fn version_output_matches_tsc() {
    // tsz --version should output "Version 6.0.0-dev.20260306\n"
}
```

**Step 5:** Commit: `fix(cli): match tsc v6 version output format`

---

## Task 2: Custom Help Renderer — `--help`

**Files:**
- Create: `crates/tsz-cli/src/help.rs`
- Modify: `crates/tsz-cli/src/bin/tsz.rs`
- Modify: `crates/tsz-cli/src/args.rs`
- Modify: `crates/tsz-cli/src/lib.rs`
- Test: `crates/tsz-cli/tests/tsc_compat_tests.rs`

**Goal:** `tsz --help` and `tsz -h` produce identical output to `tsc --help`.

**Step 1:** Create `help.rs` with a `render_help(version: &str) -> String` function that produces the exact tsc `--help` output:
```
tsc: The TypeScript Compiler - Version {version}

COMMON COMMANDS

  tsc
  Compiles the current project (tsconfig.json in the working directory.)

  tsc app.ts util.ts
  ...
```

The output has these sections:
1. Header line
2. COMMON COMMANDS (7 example commands)
3. COMMAND LINE FLAGS (--help/-h, --watch/-w, --all, --version/-v, --init, --project/-p, --showConfig, --ignoreConfig, --build/-b)
4. COMMON COMPILER OPTIONS (--pretty, --declaration/-d, --declarationMap, --emitDeclarationOnly, --sourceMap, --noEmit, --target/-t, --module/-m, --lib, --allowJs, --checkJs, --jsx, --outFile, --outDir, --removeComments, --strict, --types, --esModuleInterop)
5. Footer: `You can learn about all of the compiler options at https://aka.ms/tsc`

Each COMMON COMPILER OPTION includes `type:` and `default:` lines where applicable, and `one of:`/`one or more:` for enum options. Match exact wording from tsc v6 reference.

**Step 2:** In `preprocess_args()`, intercept `--help`, `-h`, and `-?` before clap. Print the help output and exit 0. Also make `-h` and `--help` produce the same output (clap differentiates them).

**Step 3:** Add `-?` as a recognized help alias in the preprocessor.

**Step 4:** Add test comparing help output character-for-character against expected.

**Step 5:** Commit: `feat(cli): custom help renderer matching tsc v6 format`

---

## Task 3: Custom Help Renderer — `--all`

**Files:**
- Modify: `crates/tsz-cli/src/help.rs`
- Modify: `crates/tsz-cli/src/bin/tsz.rs`
- Test: `crates/tsz-cli/tests/tsc_compat_tests.rs`

**Goal:** `tsz --help --all` and `tsz --all` produce identical output to `tsc --help --all`.

**Step 1:** Add `render_help_all(version: &str) -> String` to `help.rs`. This produces the full option listing organized by tsc v6's exact categories:
```
tsc: The TypeScript Compiler - Version {version}

ALL COMPILER OPTIONS

### Command-line Options
...
### Modules
...
### JavaScript Support
...
### Interop Constraints
...
### Type Checking
...
### Watch and Build Modes
...
### Backwards Compatibility
...
### Projects
...
### Emit
...
### Compiler Diagnostics
...
### Editor Support
...
### Language and Environment
...
### Output Formatting
...
### Completeness
...

You can learn about all of the compiler options at https://aka.ms/tsc

WATCH OPTIONS
...

BUILD OPTIONS
...
```

Each option shows `type:`, `default:`, `one of:`, or `one or more:` metadata matching tsc v6 exactly.

**Step 2:** Intercept `--all` in preprocessor (with or without `--help`). Print the all-options output and exit 0.

**Step 3:** Add test comparing --all output character-for-character.

**Step 4:** Commit: `feat(cli): --all help output matching tsc v6 format`

---

## Task 4: CLI Error Handling Parity

**Files:**
- Modify: `crates/tsz-cli/src/bin/tsz.rs`
- Modify: `crates/tsz-cli/src/args.rs`
- Test: `crates/tsz-cli/tests/tsc_compat_tests.rs`

**Goal:** Unknown flags produce `error TS5023`/`TS5025` format. Exit code 1 for CLI errors.

**Step 1:** Wrap clap's parse with error handling. When clap returns `ErrorKind::UnknownArgument` or similar, suppress clap's output and instead print:
```
error TS5023: Unknown compiler option '{flag}'.
```

**Step 2:** For near-matches (clap provides suggestions), format as:
```
error TS5025: Unknown compiler option '{flag}'. Did you mean '{suggestion}'?
```

**Step 3:** Exit with code 1 (not 2) for CLI-level errors.

**Step 4:** For no-input-files with no tsconfig: print `Version {version}\n` then the full help text (same as `--help` output), exit 1.

**Step 5:** Enforce `--build` must be first argument (TS6369) — check in preprocessor before clap.

**Step 6:** Add tests:
```rust
#[test]
fn unknown_flag_produces_ts5023() { ... }
#[test]
fn unknown_flag_with_suggestion_produces_ts5025() { ... }
#[test]
fn unknown_flag_exit_code_1() { ... }
#[test]
fn no_input_shows_help_exit_1() { ... }
```

**Step 7:** Commit: `fix(cli): match tsc v6 error format and exit codes`

---

## Task 5: Diagnostics to stderr

**Files:**
- Modify: `crates/tsz-cli/src/reporter.rs`
- Modify: `crates/tsz-cli/src/bin/tsz.rs` (any direct print! calls for diagnostics)
- Test: `crates/tsz-cli/tests/reporter_tests.rs`

**Goal:** All diagnostic output goes to stderr. Non-diagnostic output (--version, --help, --listFiles, --showConfig, --listEmittedFiles) stays on stdout.

**Step 1:** In `reporter.rs`, change all `print!`/`println!` calls in diagnostic rendering to `eprint!`/`eprintln!`. The `render()`, `format_diagnostic_*()`, `format_summary()` methods should write to stderr.

**Step 2:** In `tsz.rs`, change any `print!` calls that emit diagnostic-related output to `eprint!`. Keep stdout for: version, help, file listings, showConfig, emitted file listings.

**Step 3:** Update reporter tests to capture stderr instead of stdout.

**Step 4:** Commit: `fix(cli): emit diagnostics to stderr matching tsc`

---

## Task 6: `--showConfig` Parity

**Files:**
- Modify: `crates/tsz-cli/src/bin/tsz.rs` (handle_show_config)
- Test: `crates/tsz-cli/tests/tsc_compat_tests.rs`

**Goal:** `tsz --showConfig` output matches tsc v6 exactly.

**Step 1:** Fix indentation to 4 spaces (tsc uses 4-space JSON indent).

**Step 2:** Only serialize options that are explicitly set in tsconfig OR implied by other options (e.g., `strict: true` does NOT expand sub-options in showConfig — only `strict` itself is shown).

**Step 3:** Include resolved `"files"` array with relative paths (prefixed with `./`).

**Step 4:** Include `"include"` array if present in tsconfig.

**Step 5:** When no input files are found, emit TS18003 error and exit 1 (don't output config).

**Step 6:** Test against tsc reference: `{"compilerOptions":{"strict":true},"include":["*.ts"]}` with a `test.ts` file should produce:
```json
{
    "compilerOptions": {
        "strict": true
    },
    "files": [
        "./test.ts"
    ],
    "include": [
        "*.ts"
    ]
}
```

**Step 7:** Commit: `fix(cli): --showConfig output matches tsc v6 exactly`

---

## Task 7: `--init` Content Parity

**Files:**
- Modify: `crates/tsz-cli/src/bin/tsz.rs` (handle_init)
- Test: `crates/tsz-cli/tests/tsc_compat_tests.rs`

**Goal:** `tsz --init` generates the exact same tsconfig.json as tsc v6 and prints the same message.

**Step 1:** Update the generated tsconfig template to match tsc v6 output exactly (captured in reference). Key v6 defaults:
- `"module": "nodenext"`
- `"target": "esnext"`
- `"types": []`
- `"sourceMap": true`, `"declaration": true`, `"declarationMap": true`
- `"noUncheckedIndexedAccess": true`, `"exactOptionalPropertyTypes": true`
- `"strict": true`, `"jsx": "react-jsx"`, `"verbatimModuleSyntax": true`
- `"isolatedModules": true`, `"noUncheckedSideEffectImports": true`
- `"moduleDetection": "force"`, `"skipLibCheck": true`
- With exact comment structure and trailing comma style

**Step 2:** Console message: `"Created a new tsconfig.json\n\nYou can learn more at https://aka.ms/tsconfig\n"`

**Step 3:** Test init output matches reference.

**Step 4:** Commit: `fix(cli): --init generates tsc v6 default tsconfig`

---

## Task 8: Missing Flags & v6 Defaults

**Files:**
- Modify: `crates/tsz-cli/src/args.rs`
- Modify: `crates/tsz-cli/src/driver/core.rs` (apply_cli_overrides for new defaults)
- Test: `crates/tsz-cli/tests/args_tests.rs`

**Goal:** Add missing flags, update defaults to match tsc v6.

**Step 1:** Add `--ignoreConfig` flag (new in tsc v6 — ignores tsconfig.json, builds with CLI options and files only).

**Step 2:** Add `--libReplacement` flag (boolean, default false).

**Step 3:** Update default values to match tsc v6:
- `target` default: `es2025` (was es5)
- `strict` default: `true` (was false)
- `esModuleInterop` default: `true` (was false)
- `allowSyntheticDefaultImports` default: `true` (was false)
- `forceConsistentCasingInFileNames` default: `true` (was false)
- `noUncheckedSideEffectImports` default: `true` (was false)

**Step 4:** Remove `es3` from Target enum (dropped in tsc v6). Remove `es5` from target (still valid but not default). Keep `es5` as a valid target since tsc v6 still accepts it — just not listed in `--help --all` target options.

Wait — tsc v6 `--help --all` shows targets as: `es6/es2015, es2016, ..., es2025, esnext` — no `es5` listed. But es5 is still accepted as a value. So keep it parseable but remove from help display.

**Step 5:** Remove `es3` entirely (tsc v6 does not accept es3).

**Step 6:** Hide tsz-specific flags from help: `--sound`, `--traceDependencies`, `--typesVersions`, `--batch` should have `hide = true`.

**Step 7:** Update `moduleResolution` default description to match tsc v6: `nodenext` if module is `nodenext`; `node16` if module is `node16` or `node18`; otherwise `bundler`.

**Step 8:** Implement `--ignoreConfig` behavior in `tsz.rs` — skip tsconfig discovery/loading when set.

**Step 9:** Tests for new flags and defaults.

**Step 10:** Commit: `feat(cli): add --ignoreConfig, --libReplacement, update v6 defaults`

---

## Task 9: Build Mode Flag Remapping

**Files:**
- Modify: `crates/tsz-cli/src/bin/tsz.rs` (preprocess_args)
- Test: `crates/tsz-cli/tests/tsc_compat_tests.rs`

**Goal:** When `--build`/`-b` is the first argument, remap short flags to match tsc build mode behavior.

**Step 1:** In `preprocess_args()`, detect if the first non-flag argument is `--build` or `-b`.

**Step 2:** When in build mode, remap:
- `-v` → `--build-verbose` (not `--version`)
- `-d` → `--dry` (not `--declaration`)
- `-f` → `--force`

**Step 3:** Still allow `--version` long form to work in build mode.

**Step 4:** Tests:
```rust
#[test]
fn build_mode_v_means_verbose() { ... }
#[test]
fn build_mode_d_means_dry() { ... }
```

**Step 5:** Commit: `fix(cli): contextual flag remapping in build mode`

---

## Task 10: Watch Mode Status Messages

**Files:**
- Modify: `crates/tsz-cli/src/watch.rs`
- Modify: `crates/tsz-cli/src/reporter.rs` (if needed for message formatting)
- Test: `crates/tsz-cli/tests/watch_tests.rs`

**Goal:** Watch mode prints tsc-standard status messages.

**Step 1:** At watch start, print to stderr: `Starting compilation in watch mode...\n` (TS6031 format — with timestamp if pretty mode)

**Step 2:** On file change detection, print: `File change detected. Starting incremental compilation...\n` (TS6032)

**Step 3:** After compilation completes in watch, print: `Found N error(s). Watching for file changes.\n` (TS6194) or `Found 0 errors. Watching for file changes.\n`

**Step 4:** Wire `--excludeDirectories` and `--excludeFiles` from CLI args into the `WatchFilter`.

**Step 5:** Tests for watch message output.

**Step 6:** Commit: `feat(cli): tsc-compatible watch mode status messages`

---

## Task 11: tsconfig `extends` Array & Package Resolution

**Files:**
- Modify: `crates/tsz-cli/src/config.rs` or wherever tsconfig parsing happens
- Test: `crates/tsz-cli/tests/config_tests.rs`

**Goal:** Support `"extends": ["./base1.json", "./base2.json"]` (array form) and package-name extends like `"extends": "@tsconfig/node20/tsconfig.json"`.

**Step 1:** Change `extends` field type from `Option<String>` to `Option<StringOrArray>` where `StringOrArray` is `#[serde(untagged)] enum { Single(String), Array(Vec<String>) }`.

**Step 2:** For array extends, apply configs in order (later entries override earlier ones), matching tsc behavior.

**Step 3:** For package-name extends, resolve through `node_modules` using the same resolution logic as module resolution.

**Step 4:** Tests for array extends and package extends.

**Step 5:** Commit: `feat(cli): support tsconfig extends array and package resolution`

---

## Parallelization Strategy (Teams)

These tasks are independent and can be assigned to parallel agents:

| Team | Tasks | Rationale |
|------|-------|-----------|
| **Team A: Help System** | Tasks 1, 2, 3 | All in help.rs + preprocessor, tightly coupled |
| **Team B: Error & Build** | Tasks 4, 9 | Both modify preprocessor error path, related |
| **Team C: Output Parity** | Tasks 5, 6, 7 | Reporter + showConfig + init, output formatting |
| **Team D: Flags & Config** | Tasks 8, 11 | Args + config changes, independent |
| **Team E: Watch** | Task 10 | Fully independent, watch.rs only |

Teams A-E can all work in parallel via git worktrees.

---

## Verification

After all teams complete:
1. Run `tsz --version` and compare to `tsc --version`
2. Run `tsz --help` and diff against `tsc --help`
3. Run `tsz --help --all` and diff against `tsc --help --all`
4. Run `tsz --init` and diff generated tsconfig against tsc's
5. Run `tsz --showConfig -p <test>` and diff against tsc's
6. Run `tsz --badFlag` and compare error format + exit code
7. Run `tsz` with no args/tsconfig and compare behavior
8. Run full test suite: `cargo test -p tsz-cli`
9. Run conformance suite to check no regressions
