# [WIP] fix(emit): order computed auto-accessor WeakMap storage

- **Date**: 2026-05-12
- **Branch**: `fix/emit-computed-auto-accessor-weakmap-storage-20260512`
- **Issue**: #6030
- **Status**: claim
- **Workstream**: 2 (JS emit pass-rate bug)

## Intent

Make `autoAccessor5` JS emit match TypeScript for ES2015/ES5 by ensuring
computed public auto-accessor WeakMap storage is initialized before generated
constructor code can use it.

## Files Touched

- JS emitter/lowering code for computed public auto-accessors as needed
- Focused emitter regression tests for `autoAccessor5`

## Verification

- Planned: focused `autoAccessor5` JS emit run
- Planned: relevant `tsz-emitter` unit tests
