# fix(emitter): rename TC39 decorator temporaries that collide with user bindings (#3091)

- **Date**: 2026-05-08
- **Branch**: `fix/decorator-temp-hygiene-3091`
- **PR**: TBD
- **Status**: claim
- **Workstream**: emit hygiene

## Intent

The TC39 decorator transform allocates fixed temporary names
(`_classDescriptor`, `_classExtraInitializers`, `_classThis`,
`_classDecorators`, `_classSuper`, `_metadata`, `_instanceExtraInitializers`,
`_staticExtraInitializers`) inside the transformed IIFE. If the source
class body or its surrounding scope already binds any of those names,
the generated `let _classDescriptor;` shadows the user binding and
silently changes runtime behaviour.

tsc avoids this by suffixing only the colliding name (e.g.
`_classDescriptor_1`) and leaving the rest at their default. This PR
adopts the same per-name collision policy: scan the class span's source
text for identifier-shaped tokens matching each candidate base name and
suffix `_1`, `_2`, … until unique.

## Files Touched

- `crates/tsz-emitter/src/transforms/es_decorators.rs` — replace 44
  hardcoded uses of the temporaries with field references; add a
  per-class hygiene scan and suffix loop.
- `crates/tsz-emitter/src/declaration_emitter/tests/...` or a new
  `tsz-emitter` unit test that runs the failing repro from #3091 and
  asserts the user binding survives.

## Verification

- New unit test for the collision case.
- `cargo nextest run -p tsz-emitter`.
- Local emit suite: confirm pass rate does not regress.
