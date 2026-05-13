# Fix recursive template literal widening (#6312)

Status: claim
PR: TBD

## Scope

Investigate and fix recursive template literal evaluation with string intrinsics where a concrete literal result widens to `string`.

## Verification plan

- Reproduce #6312 with a focused CLI case.
- Add focused regression coverage for recursive CamelCase template literal evaluation.
- Run the targeted test and direct CLI repro.
