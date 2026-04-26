**2026-04-26 23:13:34** -- fix(solver): allow source missing index signature when target index value is `any` and target has string index

Mirrors tsc's `indexSignaturesRelatedTo` short-circuit at checker.ts:24828. When
the target type has a string index signature and a particular target index info
maps to `any`, the source need not declare the matching index signature. This
fixes a family of false-positive TS2322 errors where named class/interface
sources without explicit string/number index sigs are assigned to targets like
`{ [x: string]: any }` or `{ [x: string]: any, [x: number]: any }`.

Test target: TypeScript/tests/cases/conformance/types/members/objectTypeWithStringAndNumberIndexSignatureToAny.ts
