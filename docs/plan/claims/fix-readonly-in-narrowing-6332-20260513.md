# Fix readonly loss after `in` narrowing (#6332)

Status: ready
PR: #6334

## Scope

Investigate and fix missing TS2540 where readonly properties on union members become assignable after narrowing with an `in` check.

## Verification plan

- Reproduce #6332 with a focused CLI case.
- Add focused regression coverage for readonly preservation after `in` narrowing.
- Run the targeted test and direct CLI repro.

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false /tmp/issue6332.ts` - pass, emits TS2540 at the assignment.
- `cargo test -p tsz-cli --test tsc_compat_tests readonly_property_remains_readonly_after_in_narrowing -- --nocapture` - pass.
- `cargo fmt --all -- --check` - pass.
