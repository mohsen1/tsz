**2026-04-27 04:05:57**: TS2322 property-elaboration widening parity for cross-primitive-kind literal sources.

`fn1({ a: 1 })` against `(s: { a: true })` now reports `Type 'number' is not assignable to type 'true'.` to match tsc, while direct literal assignments (`let x: 1 = "abc"`, `let c: true = 1 satisfies number`) keep their AST literal display. Anchors the fix in the assignment-source formatter so the property-elaboration pre-widened `number` source no longer gets resurrected to `1` from the AST.

Owner: Mohsen
Branch: `fix/ts2322-property-elaboration-widen-cross-primitive-20260427-0405`
