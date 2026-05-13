# Fix template literal union prefix matching (#6319)

Status: claim
PR: TBD

## Scope

Investigate and fix conditional template literal matching where a substitution segment contains a union prefix such as `${' ' | '\t'}${infer Rest}`.

## Verification plan

- Reproduce #6319 with a focused CLI case.
- Add focused regression coverage for union-prefix template literal matching.
- Run the targeted test and direct CLI repro.
