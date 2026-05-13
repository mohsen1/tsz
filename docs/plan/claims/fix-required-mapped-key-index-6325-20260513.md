# Fix Required<T> mapped key indexed access (#6325)

Status: claim
PR: TBD

## Scope

Investigate and fix the false positive TS2536 where `K in keyof T` is rejected as an index into `Required<T>` inside a mapped type.

## Verification plan

- Reproduce the issue with the minimal CLI case from #6325.
- Add focused regression coverage for `Required<T>[K]` inside a mapped type.
- Run the targeted test and direct CLI repro.
