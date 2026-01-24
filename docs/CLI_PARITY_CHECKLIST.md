# CLI Parity Checklist

Date: 2026-01-24
Status: Draft
Scope: tsz CLI parity with tsc (entrypoint, args, watch, tests).

## High-impact gaps
- [ ] Build mode: implement --build with project references; support --clean and --dry
- [ ] --all: list compiler options with tsc-equivalent formatting
- [ ] --listFiles: list all files read, not only emitted files
- [ ] Response files: support @file argument expansion
- [ ] --traceDependencies: add to args and runtime wiring

## Parsed flags missing runtime wiring
- [ ] --diagnostics and --extendedDiagnostics
- [ ] --explainFiles
- [ ] --generateTrace <dir>
- [ ] --generateCpuProfile <file>
- [ ] --preserveWatchOutput
- [ ] --assumeChangesOnlyAffectDirectDependencies

## Behavioral parity risks to validate
- [ ] Exit codes: distinguish invalid args vs compile errors
- [ ] Stdout vs stderr separation for diagnostics and status
- [ ] Watch output: status banners and "Found N errors" formatting
- [ ] listFiles ordering matches tsc output

## Test coverage
- [ ] CLI parity tests for stdout/stderr and exit codes
- [ ] Build mode tests (clean, dry, project references)
- [ ] Response file parsing tests
- [ ] Watch mode output tests

## Notes
- Evidence locations: src/bin/tsz.rs, src/cli/args.rs, src/cli/args_tests.rs, watch runner
