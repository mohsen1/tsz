# Fix Required<T> mapped key indexed access (#6325)

Status: ready
PR: #6326

## Scope

Investigate and fix the false positive TS2536 where `K in keyof T` is rejected as an index into `Required<T>` inside a mapped type.

## Verification plan

- Reproduce the issue with the minimal CLI case from #6325.
- Add focused regression coverage for `Required<T>[K]` inside a mapped type.
- Run the targeted test and direct CLI repro.

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false /tmp/issue6325.ts` - pass, no TS2536.
- `cargo test -p tsz-cli --test tsc_compat_tests required_mapped_keyof_index_access_does_not_report_ts2536 -- --nocapture` - pass.
- `cargo test -p tsz-checker --test required_constraint_local_alias_tests` - pass.
- `cargo fmt --all -- --check` - pass.

## Result

- Fixed TS2536 for `Required<T>[K]` when `K` comes from `[K in keyof T]`.
- Added a guard that local non-mapped `Required` aliases do not receive the lib mapped-key shortcut.
