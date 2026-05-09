---
status: WIP
issue: 3591
agent: claude (auto-loop)
started: 2026-05-07 19:00:33 UTC
---

# Top-level config validation gaps (#3591)

The issue lists three repros where tsc emits a config diagnostic and tsz exits 0:

- A. `watchOptions.watchFile: "bad"` → tsc TS6046 (enum value).
- B. `typeAcquisition.bogus: true` → tsc TS17010 (unknown option).
- C. `compileOnSave: "yes"` → tsc TS5024 (wrong scalar type).

## Scope of this PR

Fix repros B and C. Both fit the same top-level validator pattern that
already lives in `crates/tsz-core/src/config/mod.rs`
(`validate_top_level_object_option` for #3882, scalar `compilerOptions`).

- C: add `validate_top_level_boolean_option` and call it on `compileOnSave`.
- B: validate `typeAcquisition` is an object, then emit TS17010 for any key
  outside the known set
  (`enable`, `include`, `exclude`, `disableFilenameBasedTypeAcquisition`).

Repro A (watchOptions enum validation) is left for a follow-up — it needs
per-key enum tables for `watchFile`, `watchDirectory`, `fallbackPolling`,
`synchronousWatchDirectory`, etc. and is meaningfully larger.
