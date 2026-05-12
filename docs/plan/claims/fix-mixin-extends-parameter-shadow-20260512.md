# fix(checker): resolve mixin extends parameter shadowing

- **Date**: 2026-05-12
- **Branch**: `fix-mixin-extends-parameter-shadow-20260512`
- **Base**: `main`
- **Issue**: [#6101](https://github.com/mohsen1/tsz/issues/6101)
- **PR**: draft pending
- **Status**: WIP
- **Workstream**: 1 (diagnostic conformance / false-positive binder/checker bug)

## Intent

Make `tsz` match `tsc` when a mixin function parameter shadows an outer
abstract class name and a returned class expression extends that parameter.
The heritage expression is in value position, so `extends Base` should resolve
to the function parameter `Base: TBase`, not to the outer abstract class `Base`.

## Initial Scope

- Add a focused regression for the #6101 repro.
- Fix the class-expression heritage/base-type path so function parameters in
  scope take precedence over outer class declarations in value position.
- Preserve genuine TS2653 diagnostics for classes that do inherit abstract
  members from an abstract base.

## Verification

Pending implementation.
