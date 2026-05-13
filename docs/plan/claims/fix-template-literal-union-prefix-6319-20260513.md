# Fix template literal union prefix matching (#6319)

Status: ready
PR: #6328

## Scope

Investigate and fix conditional template literal matching where a substitution segment contains a union prefix such as `${' ' | '\t'}${infer Rest}`.

## Verification plan

- Reproduce #6319 with a focused CLI case.
- Add focused regression coverage for union-prefix template literal matching.
- Run the targeted test and direct CLI repro.

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false /tmp/issue6319.ts` - pass.
- `cargo test -p tsz-cli --test tsc_compat_tests template_literal_union_prefix_pattern_matches_before_infer -- --nocapture` - pass.
- `cargo fmt --all -- --check` - pass.
