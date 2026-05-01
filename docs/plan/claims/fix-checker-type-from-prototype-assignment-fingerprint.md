# [abandoned] fix(checker): align prototype assignment TS2339 fingerprint

- **Date**: 2026-05-01
- **Branch**: `fix/checker-type-from-prototype-assignment-fingerprint`
- **PR**: #1955 (closed as abandoned)
- **Status**: abandoned
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Original intent

Investigate and fix the (snapshot-claimed) "fingerprint-only" TS2339
mismatch in
`TypeScript/tests/cases/conformance/salsa/typeFromPrototypeAssignment.ts`.

## Why abandoned

The "fingerprint-only" categorization in the snapshot was wrong — the
actual diff is wrong-code, not fingerprint-only.

`tsc` baseline (`typeFromPrototypeAssignment.ts`):

```
a.js(27,20): error TS2339: Property 'addon' does not exist on type '{ set: () => void; get(): void; }'.
```

— **one** TS2339 at the `Multimap.prototype.addon = function () {` line,
where the receiver is the prototype-object-literal type and the missing
member is `addon` (which is being assigned, so it doesn't yet exist on
the previously-assigned `{ set, get }` shape).

`tsz` actual (verbose run on this branch's HEAD, against built
dist-fast `tsz-conformance`):

```
TS2339 a.js:16:14 Property 'set'   does not exist on type 'Multimap'.
TS2339 a.js:17:14 Property 'get'   does not exist on type 'Multimap'.
TS2339 a.js:18:14 Property 'addon' does not exist on type 'Multimap'.
TS2339 a.js:22:14 Property 'set'   does not exist on type 'Multimap'.
TS2339 a.js:23:14 Property 'get'   does not exist on type 'Multimap'.
TS2339 a.js:24:14 Property 'addon' does not exist on type 'Multimap'.
TS2339 a.js:30:10 Property 'set'   does not exist on type 'Multimap'.
TS2339 a.js:31:10 Property 'get'   does not exist on type 'Multimap'.
TS2339 a.js:32:10 Property 'addon' does not exist on type 'Multimap'.
```

— **nine** TS2339s, all of the form `Property 'X' does not exist on
type 'Multimap'`. tsz does not recognize the JS-on-checked-JS pattern

```js
/** @constructor */
var Multimap = function() { ... };
Multimap.prototype = { set: function() {...}, get() {...} };
Multimap.prototype.addon = function () {...};
```

as adding `set` / `get` / `addon` to the `Multimap` instance type. As a
result every `this._map` / `this.set` / `this.get` / `this.addon`
inside the constructor and the prototype methods, plus every
`mm.set` / `mm.get` / `mm.addon` after `var mm = new Multimap()`,
fails property resolution against the bare constructor's instance
type, while the **single** expected `addon`-on-`{ set, get }` error
that tsc emits is missing entirely.

## Root cause

This is a missing **JS prototype-assignment expando** feature, not a
diagnostic-display bug. Implementing it requires:

1. **Binder**: detect the prototype-assignment shape
   `<Ctor>.prototype = <ObjectLiteral>` and `<Ctor>.prototype.<name> = <expr>`
   on a `@constructor`-tagged JS variable (or function declaration)
   and bind each member symbol onto the constructor's instance-type
   member table.
2. **Symbol expansion**: extend the constructor symbol's instance type
   with the bound prototype members so checker `this`-typing inside
   constructor and prototype methods resolves them.
3. **Checker**: for `this.X` access inside prototype methods, resolve
   against the expanded instance type rather than the literal-shape
   right-hand side of the prototype assignment.
4. **Solver glue**: the receiver type for `this.<name>` reads must
   contain the expando members so subtype / property-access queries
   see them.

Each item is a multi-day workstream of its own; the prototype-assignment
pass in tsc's binder is a large special-cased feature, and tsz has no
equivalent today.

## What this PR shipped

A claim file plus this abandoned-status writeup. No code changes were
landed because the planned scope (printer / anchor alignment) is
orthogonal to the actual gap (missing prototype-expando feature).

## Follow-up

A correctly-scoped successor PR would target one of:

- The JS prototype-assignment expando feature end-to-end (binder +
  symbol expansion + checker + solver).
- A scoped suppression that prevents the 9 false-positive TS2339s
  (still leaves the `addon`-on-prototype-literal expected error
  un-emitted, so `typeFromPrototypeAssignment.ts` would not pass).

Neither is in scope for this PR's branch slug.
