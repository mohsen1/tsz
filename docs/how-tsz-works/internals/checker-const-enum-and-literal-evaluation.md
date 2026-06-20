# Const Enums and Compile-Time Literal Evaluation

## Orientation

The sibling doc [checker-jsx-properties-accessors-enums](checker-jsx-properties-accessors-enums.md)
covers enums at the *declaration-orchestration* boundary: which grammar checks
fire, where `TS2474`/`TS2567` are reported, and how an enum becomes a
`TypeData::Enum` symbol type. This doc drills down one level into the **kernel
that actually folds constant arithmetic**: the two thread-local evaluators
(`const_enum_eval.rs` and `enum_utils.rs`), the shared cycle/depth guards that
keep them from blowing the stack on `enum E { A = E.A }`, the ECMAScript
`ToInt32`/`ToUint32` numeric semantics that make `0x80000000 | 0` fold to
`-2147483648` exactly like `tsc`, and the auto-increment counter that turns a
gap-free numeric enum into a precise `0 | 1 | 2` literal union.

The single most important architectural fact about this subsystem is that it
is **syntax-directed constant folding, not type evaluation**. These evaluators
walk the raw parser `NodeArena` (numeric literals, unary/binary operator
tokens, property/element access chains) and produce an `f64`. They deliberately
do *not* ask the solver to evaluate a type, because the values they compute
must exist *before* the enum's `TypeData::Enum` literal-union is constructed —
the member literal types are downstream of these `f64` results, not the other
way around. This is the rare place where the checker is allowed to do its own
numeric computation rather than delegating to the solver: there is no "type" to
relate or instantiate, only a parser expression tree to fold per the
ECMAScript abstract operations. The solver still owns everything *after* a
member value becomes `factory.literal_number(v)`; see
[solver-types-intern-def](solver-types-intern-def.md) and
[checker-type-of-symbol-and-symbol-types](checker-type-of-symbol-and-symbol-types.md)
for how those literal types and the enclosing `TypeData::Enum` are interned and
keyed.

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Syntax-directed folding of enum member initializers to `f64` (`evaluate_const_enum_initializer`, `evaluate_constant_expression`). | Type *evaluation* of conditional/mapped/template types — that is the solver's `evaluate_*` family ([solver-evaluation](solver-evaluation.md)). |
| ECMAScript `ToInt32`/`ToUint32` operand coercion for bitwise/shift folding (via `tsz_common::numeric`). | Assignability of enum members to `number`/`string` — the `Enum(def_id, structural)` carries the structural side for [solver-relations](solver-relations.md). |
| Auto-increment counter (gap-free `0,1,2,…`; reset on explicit numeric initializer; broken by string initializer). | Building the interned `TypeData::Enum` / `literal_number` types — that is `computed/mod.rs` + the [solver-types-intern-def](solver-types-intern-def.md) factory. |
| Cycle detection across self/mutual recursion (`cycle_guard`, `CycleSetId::ConstEnum`/`NonConstEnum`). | Flow narrowing of enum-typed values — see [checker-flow-and-narrowing](checker-flow-and-narrowing.md) / [solver-narrowing](solver-narrowing.md). |
| Const-enum diagnostics `TS2474`/`TS2477`/`TS2478` (constancy/non-finite/NaN) and the forward-ref/self-ref grammar checks (`TS2651`/`TS2565`). | Const-enum *inlining* into emitted JS — the emitter has its own `i64` folder; see [emitter](emitter.md). |
| Memoization of folded member results (thread-local `CONST_EVAL_MEMO` / `EVAL_MEMO`). | Module-merge legality (`TS2567`) reporting *location* — that is `declarations.rs`; this layer only supplies the folded value. |

## Module map

| Path | Role |
| --- | --- |
| `crates/tsz-checker/src/types/utilities/const_enum_eval.rs` | Free-function const-enum initializer folder. Used during *declaration checking* to compute `TS2474`/`TS2477`/`TS2478`. Standalone (not a `CheckerState` method) so both `CheckerState` and `DeclarationChecker` can call it. |
| `crates/tsz-checker/src/types/utilities/enum_utils.rs` | `CheckerState`-method folder (`evaluate_constant_expression`, `evaluate_enum_member_access`, `compute_auto_increment_value`) used during *type checking* to build member literal types and `EnumKind`. Also holds `enum_member_type_from_decl`, `enum_kind`, `is_const_enum_symbol`. |
| `crates/tsz-checker/src/types/utilities/cycle_guard.rs` | Shared RAII cycle detector. Two thread-local visited sets keyed by `CycleSetId::{ConstEnum, NonConstEnum}`; `try_enter` returns `Some(CycleGuard)` or `None` (cycle). |
| `crates/tsz-checker/src/declarations/declarations.rs` | Caller for const-enum checks: clears the memo, iterates members, calls `evaluate_const_enum_initializer`, classifies `None`/`NaN`/`±∞` into `TS2474`/`TS2478`/`TS2477`. |
| `crates/tsz-checker/src/declarations/declarations_enum_helpers.rs` | Syntax-only `check_enum_member_self_reference` (`TS2565`) and `enum_has_forward_reference` (gating `TS2651`). |
| `crates/tsz-checker/src/state/type_analysis/computed/mod.rs` | Builds `TypeData::Enum(def_id, structural_union)`; runs the *type-checking* auto-increment pass and pre-caches each member's `enum_type(member_def_id, literal)`. |
| `crates/tsz-checker/src/checkers/enum_checker.rs` | `is_enum_type` / `is_enum_like_type` / `is_boxed_primitive_type` — symbol-flag-based fallbacks used by binary-operator arithmetic checks. |
| `crates/tsz-common/src/primitives/numeric.rs` | `parse_numeric_literal_value`, `to_int32`, `to_uint32` — the ECMAScript-exact numeric kernel both folders route through. |
| `crates/tsz-checker/src/types/computation/access.rs` | `TS2476` (a const-enum member can only be accessed via a string literal in element access). |
| `crates/tsz-checker/src/types/computation/identifier/resolved.rs` | `TS2475` (`const` enum used outside a property/index/import/type-query position). |
| `crates/tsz-checker/src/types/property_access_type/enum_namespace_access.rs` | `TS2450` (enum used before declaration) and `TS2748` (ambient const enum under `isolatedModules`/`verbatimModuleSyntax`). |

## Two folders, two phases

There are **two** constant folders in the checker, intentionally distinct
because they run in different compiler phases against different inputs and
report different diagnostics.

```
                          parser NodeArena (syntax only)
                                     |
        ┌────────────────────────────┴────────────────────────────┐
        │                                                          │
  DECLARATION-CHECK PHASE                               TYPE-CHECK PHASE
  (DeclarationChecker)                                  (CheckerState)
        │                                                          │
  const_enum_eval::evaluate_const_enum_initializer        enum_utils::evaluate_constant_expression
        │  (free fn, &NodeArena)                                  │  (&self method)
        │  CycleSetId::ConstEnum                                  │  CycleSetId::NonConstEnum
        │  CONST_EVAL_MEMO                                        │  EVAL_MEMO + EVAL_DEPTH
        ▼                                                          ▼
  Option<f64> ──► classify None/NaN/±∞                     Option<f64> ──► factory.literal_number(v)
  ► TS2474 / TS2478 / TS2477                               ► member literal type
                                                           ► TypeData::Enum(def_id, structural_union)
```

Why two? The const-enum check in `declarations.rs` runs while the
`DeclarationChecker` is walking declaration nodes; it has only a `&NodeArena`
and an `EnumData`, no `CheckerState` self. Resolving a *cross-enum* reference
like `const enum A { X = B.Y }` therefore cannot go through the binder symbol
table — instead `const_enum_eval.rs` searches the AST for an enum named `B` in
the *same namespace path* and folds `Y` in that enum's context
(`resolve_external_enum_member`, see [Cross-enum resolution](#cross-enum-resolution)).
The type-check folder in `enum_utils.rs` *does* hold a `CheckerState`, so it
uses the binder's scope-aware `resolve_name_with_filter` and walks
`symbol.exports` / `symbol.members` for `E.A` chains
(`evaluate_enum_member_access`).

Both folders share the *same arithmetic*: identical operator tables, identical
`ToInt32`/`ToUint32` masking, identical depth ceiling (100). They differ only
in how they *resolve references* and which memo/cycle set they use.

## The arithmetic kernel

Both folders implement the same operator table. The numeric-literal leaf reads
`lit.value` (the scanner's pre-parsed value) and falls back to
`tsz_common::numeric::parse_numeric_literal_value(&lit.text)`, which understands
`0x`/`0b`/`0o` prefixes and `_` separators (`numeric.rs`,
`parse_numeric_literal_value`). The identifier leaf recognizes the two global
numeric constants `NaN` and `Infinity` before attempting member resolution
(`const_enum_eval.rs` lines 65–68; `enum_utils.rs` lines 434–438).

Operators (both `evaluate_const_enum_initializer` and
`evaluate_constant_expression`):

| Token | Folding |
| --- | --- |
| `+x` / `-x` | identity / negation on `f64`. |
| `~x` | `f64::from(!to_int32(operand))` — bitwise NOT after `ToInt32`. |
| `+ - * / %` | plain `f64` arithmetic. |
| `**` (`AsteriskAsteriskToken`) | `left.powf(right)`. |
| `\| & ^` | `f64::from(to_int32(left) OP to_int32(right))`. |
| `<<` | `f64::from(to_int32(left) << (to_uint32(right) & 0x1f))`. |
| `>>` | `f64::from(to_int32(left) >> (to_uint32(right) & 0x1f))` (sign-extending). |
| `>>>` | `f64::from(to_uint32(left) >> (to_uint32(right) & 0x1f))` (zero-fill). |

The crucial parity detail lives in `tsz_common::numeric` (`numeric.rs`). A naive
`value as i32` Rust cast **saturates** (`3e9_f64 as u32 == u32::MAX`,
`-1.0_f64 as u32 == 0`), whereas ECMAScript's `ToInt32`/`ToUint32` **wrap**
modulo `2^32`. `to_uint32` truncates toward zero then takes `rem_euclid(2^32)`;
`to_int32` reinterprets the `u32` bit pattern as `i32`. So:

- `0x80000000 | 0` → `to_int32(2147483648) == -2147483648`, not `i32::MAX`.
- `0xFFFFFFFF & 0xFFFFFFFF` → `-1`.
- `~0 == -1`, `~0x7FFFFFFF == -2147483648`.
- `1 << 31 == -2147483648`; `0x80000000 >>> 31 == 1`.

These exact witnesses are pinned by the unit tests at the bottom of
`const_enum_eval.rs` (`bitwise_or_wraps_like_ecmascript_toint32`,
`shifts_use_toint32_touint32_operands`). The shift count is masked with `& 0x1f`
exactly as the spec requires (only the low 5 bits matter).

The shift-count operand uses `to_uint32(right)`, but the **shifted** operand of
`<<`/`>>` uses `to_int32(left)` while `>>>` uses `to_uint32(left)`. That split
is what produces JS-faithful sign behavior: signed `>>` sign-extends
(`(0x80000000 | 0) >> 31 == -1`), unsigned `>>>` zero-fills.

> The emitter carries a **third**, independent constant folder
> (`crates/tsz-emitter/src/transforms/enum_es5.rs`, `evaluate_constant_expression`
> on `i64`) used to inline const-enum reads into emitted JS. It mirrors the same
> `& 0x1f` masking and `as i32`/`as u32` reinterpretation but produces `i64` for
> source-text generation, not `f64` for typing. It belongs to the emit layer;
> see [emitter](emitter.md). The checker folders never read or feed it.

## Walk-through A: const-enum constancy (`TS2474`/`TS2478`/`TS2477`)

Snippet:

```ts
const enum E {
  A = 1 << 4,     // 16, fine
  B = "x".length, // not constant → TS2474
  C = 0 / 0,      // NaN → TS2478
  D = 1 / 0,      // Infinity → TS2477
}
```

1. `DeclarationChecker::check_enum_declaration` (in `declarations.rs`) sees the
   `ConstKeyword` modifier (`has_modifier(..., SyntaxKind::ConstKeyword)`) and
   enters the const-enum branch (line ~927).

2. Before the member loop it calls
   `const_enum_eval::clear_const_eval_memo()` (line 953). This wipes
   `CONST_EVAL_MEMO` so memoization is *per enum declaration* — members within
   the same enum share folded results, but a later enum never reads stale
   entries keyed by a now-reused `NodeIndex`.

3. For each member with an initializer, string-literal initializers are skipped
   (`is_string_initializer`, lines 969–979) — they are always valid const-enum
   initializers and need no folding. A forward-reference guard runs first
   (`enum_has_forward_reference`); if the initializer points at a *later*
   member, `TS2651` was already reported and `TS2474` is suppressed (lines
   992–1000) to avoid a double diagnostic.

4. `evaluate_const_enum_initializer(arena, init, enum_data, enum_name, 0)` folds:
   - `A`: `1 << 4` → `to_int32(1) << (to_uint32(4) & 0x1f)` = `16.0` → `Some(16.0)`.
   - `B`: `"x".length` is a `PROPERTY_ACCESS_EXPRESSION` whose object is a string
     literal, not an enum reference. `expression_ends_with_identifier` fails (the
     object is not an identifier matching `enum_name`), and
     `resolve_cross_enum_property_access` requires the object to be a bare
     identifier — so it returns `None`.
   - `C`: `0 / 0` → `Some(f64::NAN)`.
   - `D`: `1 / 0` → `Some(f64::INFINITY)`.

5. `declarations.rs` classifies the result (lines 1012–1071):
   - `None` → `TS2474` *unless* the initializer is a bare identifier that
     resolves to a `const` `BLOCK_SCOPED_VARIABLE` (`is_const_var_ref`,
     lines 1018–1036). tsc treats a reference to a `const` variable as a valid
     constant expression even though the syntax-only folder cannot resolve the
     value; tsz mirrors this by *suppressing* `TS2474` in that case rather than
     inventing a value.
   - `NaN` → `TS2478` (`'const' enum member initializer was evaluated to
     disallowed value 'NaN'.`).
   - `±∞` → `TS2477` (`'const' enum member initializer was evaluated to a
     non-finite value.`).
   - anything finite → valid; no diagnostic.

The exact code↔message bindings:

| Code | Constant | Message |
| --- | --- | --- |
| `TS2474` | `CONST_ENUM_MEMBER_INITIALIZERS_MUST_BE_CONSTANT_EXPRESSIONS` | const enum member initializers must be constant expressions. |
| `TS2476` | `A_CONST_ENUM_MEMBER_CAN_ONLY_BE_ACCESSED_USING_A_STRING_LITERAL` | A const enum member can only be accessed using a string literal. |
| `TS2477` | `CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_A_NON_FINITE_VALUE` | 'const' enum member initializer was evaluated to a non-finite value. |
| `TS2478` | `CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_DISALLOWED_VALUE_NAN` | 'const' enum member initializer was evaluated to disallowed value 'NaN'. |
| `TS2475` | `CONST_ENUMS_CAN_ONLY_BE_USED_IN_PROPERTY_OR_INDEX_ACCESS_…` | 'const' enums can only be used in property or index access expressions or the right hand side of an import declaration or export assignment or type query. |
| `TS2450` | `ENUM_USED_BEFORE_ITS_DECLARATION` | Enum '{0}' used before its declaration. |
| `TS2567` | `ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS` | Enum declarations can only merge with namespace or other enum declarations. |
| `TS2651` | `A_MEMBER_INITIALIZER_IN_A_ENUM_DECLARATION_CANNOT_REFERENCE_MEMBERS_DECLARED_AFT` | A member initializer in a enum declaration cannot reference members declared after it… |
| `TS2565` | `PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED` | Property '{0}' is used before being assigned. |
| `TS2748` | `CANNOT_ACCESS_AMBIENT_CONST_ENUMS_WHEN_IS_ENABLED` | Cannot access ambient const enums when '{0}' is enabled. |

(Codes verified against `crates/tsz-common/src/diagnostics/data/parts/part_000.rs`
and `part_001.rs`.)

## Walk-through B: building a numeric enum's literal union

Snippet:

```ts
enum Dir { Up = 1, Down, Left, Right }
```

This runs in the **type-check** phase, in
`computed/mod.rs` when `get_type_of_symbol` resolves a symbol whose flags
include `symbol_flags::ENUM` (line 1034).

1. A stable `def_id` is minted: `self.ctx.get_or_create_def_id(sym_id)`
   (line 1036). The checker *stabilizes* the `DefId`; the
   `TypeEnvironment` later resolves it to a `TypeId` — see
   [solver-types-intern-def](solver-types-intern-def.md).

2. All enum declaration nodes are collected (line 1041). Merged enums (multiple
   `enum Dir { … }` blocks, including `enum`/`namespace` merges) contribute from
   every declaration.

3. The auto-increment pass (lines 1079–1113) walks members with a local
   `auto_value: Option<f64>` starting at `Some(0.0)`, **reset per declaration
   block**:
   - `Up = 1`: `enum_member_type_from_decl` reads the numeric literal `1` →
     `literal_number(1)`. Because it has an initializer,
     `evaluate_constant_expression(init)` is folded to `1.0`, so
     `auto_value = Some(2.0)`.
   - `Down`: no initializer, `enum_member_type_from_decl` returns `NUMBER` as a
     placeholder; the auto-increment branch (`else if member_type == NUMBER`)
     replaces it with `literal_number(2)` and bumps `auto_value` to `Some(3.0)`.
   - `Left` → `literal_number(3)`, `Right` → `literal_number(4)`.

   The comment at line 1097 explains *why* auto-increment is rebuilt here rather
   than left as `number`: a mapped type `{ [k in Dir]?: string }` needs the
   individual keys `"1" | "2" | "3" | "4"`, which only exist if each member is a
   distinct number literal, not the widened `number`. See
   [solver-mapped-and-tuple-shards](solver-mapped-and-tuple-shards.md) for the
   downstream mapped-type consumer.

4. A second pass (lines 1123–1159) builds, for each named member, a per-member
   `factory.enum_type(member_def_id, member_literal)` and caches it in
   `ctx.symbol_types`, `ctx.type_env`, and `ctx.type_environment`. It also calls
   `env.register_enum_parent(member_def_id, def_id)` so the solver can widen a
   member literal `Dir.Up` back to the enum `Dir` when needed.

5. The structural side is `factory.union(member_types)` (lines 1162–1171) —
   here `1 | 2 | 3 | 4`. The returned type is
   `factory.enum_type(def_id, structural_union)` (line 1183), i.e.
   `TypeData::Enum(def_id, 1|2|3|4)`. The comment at lines 1177–1182 is the
   canonical statement of the invariant: `Lazy(def_id)` alone would recurse
   forever in `ensure_refs_resolved`; the bare structural union alone would lose
   nominal identity (`Dir1` would equal `0|1`); `Enum(def_id, structural)`
   preserves *both* nominal identity for equality and structural shape for
   `Dir <: number` assignability.

6. Finally `merge_namespace_exports_into_object` builds the
   `typeof Dir` / `keyof typeof Dir` namespace object and stores it in
   `ctx.enum_namespace_types` and both `TypeEnvironment` instances (lines
   1185–1193).

`enum_kind` (lines 243–283) classifies an enum as `Numeric`, `String`, or
`Mixed` purely from member *initializer syntax kinds* (string literal →
`saw_string`; numeric literal or no initializer → `saw_numeric`). That
classification drives `apparent_enum_instance_type` (`number`, `string`, or
`number | string`) and gates the numeric-enum-subset assignability override in
`enum_assignability_override`.

## Auto-increment semantics

The auto-increment counter appears in three places, all with identical rules:

- `computed/mod.rs` (the type-build pass, above).
- `enum_utils::compute_auto_increment_value` (lines 640–667): given the parent
  enum symbol and a target member decl, it walks each declaration block resetting
  `auto_value` to `0.0`, adding `1.0` per member, and *folding* any explicit
  initializer (`auto_value = val + 1.0`). If an initializer cannot be folded
  (e.g. a string literal), `evaluate_constant_expression` returns `None` and the
  whole walk returns `None` — auto-increment is "broken" and the member has no
  numeric value.
- `enum_utils::enum_member_compat_map` (lines 194–238): builds a
  name→`EnumCompatValue` map (used by `enum_assignability_override`) tracking
  `next_numeric_value`; a string member produces `EnumCompatValue::String`, a
  foldable numeric member sets `next_numeric_value = value + 1.0`.

The rules the three implementations agree on:

1. The first member with no initializer is `0`.
2. A member after `X = n` (foldable) is `n + 1`.
3. A member after a string-valued or unfoldable member has **no** auto value
   (the chain is broken; in `tsc` this is the `enum E { A = "x", B }` error —
   tsz returns `None`, letting the member type fall back per the caller).
4. The counter resets at the start of each declaration block of a merged enum.

`enum_member_type_from_decl` (`enum_utils.rs`, lines 289–348) is the per-member
type resolver: string-literal initializer → `literal_string`; numeric-literal
initializer → `literal_number`; any other foldable initializer →
`literal_number(value)`; otherwise it recovers the auto-increment value via
`compute_auto_increment_value` and falls back to `NUMBER` only when nothing can
be computed.

## Cross-enum resolution

A const-enum initializer may reference a member of a *different* enum:

```ts
const enum A { X = B.Y }
const enum B { Y = 5 }
```

During declaration checking there is no `CheckerState`, so
`const_enum_eval.rs` resolves this purely over the AST:

1. `evaluate_const_enum_initializer` sees a `PROPERTY_ACCESS_EXPRESSION`. The
   object `B` does not match the *current* enum name `A`
   (`expression_ends_with_identifier` is false), so it falls to
   `resolve_cross_enum_property_access`.

2. That confirms the object is a bare `Identifier` (`B`) and calls
   `resolve_external_enum_member(arena, current_enum_data, "B", "Y", depth)`.

3. `resolve_external_enum_member` computes the *namespace path* of the current
   enum (`enum_namespace_path`, walking `MODULE_DECLARATION` ancestors) and the
   enclosing `SOURCE_FILE` (`source_file_ancestor`). It then DFS-walks the AST
   from the source file looking for an enum named `B` **in the same namespace
   path** (so `M.B` and `N.B` are not confused). When found, it locates member
   `Y` (`enum_member_index`), folds its initializer, and memoizes the result in
   `CONST_EVAL_MEMO` keyed by the member's `NodeIndex`.

Element-access form `B["Y"]` is handled symmetrically by
`resolve_cross_enum_element_access` (string-literal or
no-substitution-template argument only).

The type-check folder solves the same problem differently:
`evaluate_enum_member_access` uses `collect_access_chain` to gather a chain like
`A.B.C.E` → `(["A","B","C","E"], "V1")`, then walks the binder symbol table —
`file_locals[root]`, then `exports`/`members` per segment — to land on the enum
symbol and its member symbol (lines 495–573). Both numeric auto-increment and
explicit-initializer members are folded, guarded by `CycleSetId::NonConstEnum`.

## Cycle and depth guards

Two failure modes must be prevented: infinite recursion through a reference
cycle, and stack overflow through a deep-but-acyclic expression.

**Cycle detection** is owned by `cycle_guard.rs`. Each phase has its own
thread-local visited set (`CONST_ENUM_VISITED`, `NON_CONST_ENUM_VISITED`)
selected by `CycleSetId`. `try_enter(node, set_id)` inserts the member's
`NodeIndex`; if it was already present it returns `None` (cycle detected),
otherwise it returns a `CycleGuard` whose `Drop` removes the node — RAII
cleanup even on panic. This catches:

- direct self-reference: `const enum E { A = E.A }` → `try_enter` on `A`'s
  decl fails the second time.
- mutual recursion: `const enum E { A = F.B }; const enum F { B = E.A }`.
- auto-increment cycles: `enum E { A = F.C }; enum F { B = E.A, C }`, where
  computing `F.C`'s auto value walks back into `E.A` (guarded at
  `enum_utils.rs` line 561 before `compute_auto_increment_value`).

```
const_enum_eval::resolve_enum_member_value(name="A")
    cycle_guard::try_enter(A_decl, ConstEnum)  -> Some(guard)
        evaluate_const_enum_initializer(E.A)
            resolve_enum_member_value(name="A")
                cycle_guard::try_enter(A_decl, ConstEnum) -> None  ← cycle!
                return None                       (no value)
    drop(guard) removes A_decl from CONST_ENUM_VISITED
```

When the const-enum folder returns `None` for a self-reference, the *grammar*
diagnostic (`TS2565`, "used before being assigned") comes from a separate
syntax-only pass: `declarations_enum_helpers::check_enum_member_self_reference`,
which structurally matches the initializer for an identifier / `E.A` /
`E["A"]` equal to the member's own name and reports `TS2565` at the offending
node. The forward-reference grammar check `enum_has_forward_reference`
(same file) gates `TS2651`.

**Depth limiting**: both folders cap recursion at 100.
`const_enum_eval::evaluate_const_enum_initializer` takes an explicit `depth: u32`
argument and returns `None` above 100 (lines 51–53). `enum_utils` instead uses a
thread-local `EVAL_DEPTH` `Cell<u32>` and a `DepthGuard` RAII type that
decrements on drop; `evaluate_constant_expression` increments on entry and bails
above `MAX_EVAL_DEPTH = 100` (lines 356–367).

## Caches and invariants

| Cache | Where | Key | Invalidation |
| --- | --- | --- | --- |
| `CONST_EVAL_MEMO` | `const_enum_eval.rs` (thread-local `FxHashMap<NodeIndex, Option<f64>>`) | member `NodeIndex` | `clear_const_eval_memo()` — called in `declarations.rs` *before each const-enum declaration's* member loop, and in `reset_per_file_resolution_guards` (`checkers/mod.rs`). |
| `EVAL_MEMO` | `enum_utils.rs` (thread-local `FxHashMap<NodeIndex, Option<f64>>`) | member value-decl `NodeIndex` | cleared by the `DepthGuard` when `EVAL_DEPTH` returns to 0 (end of a top-level fold), and by `clear_enum_eval_memo()` per file. |
| `EVAL_DEPTH` | `enum_utils.rs` (thread-local `Cell<u32>`) | n/a | reset to 0 by `clear_enum_eval_memo`; decremented by `DepthGuard::drop`. |
| `CONST_ENUM_VISITED` / `NON_CONST_ENUM_VISITED` | `cycle_guard.rs` (thread-local `FxHashSet<NodeIndex>`) | member `NodeIndex` | `CycleGuard::drop` removes its own node; `clear_visited_sets()` wipes both per file. |
| `ctx.symbol_types` / `ctx.type_env` / `ctx.type_environment` | `computed/mod.rs` | `SymbolId` / `SymbolRef` / `DefId` | symbol-keyed, retained across files (stable identity); see [checker-context-and-state](checker-context-and-state.md). |
| `ctx.enum_namespace_types` | `computed/mod.rs` | enum `SymbolId` | the `typeof Enum` namespace object cache. |

**Critical invariant — `NodeIndex` keys are arena-local.** Every memo and
visited set in this subsystem is keyed by `NodeIndex`, which is only meaningful
within one file's `NodeArena`. When a worker thread is reused across files,
`reset_per_file_resolution_guards` (`checkers/mod.rs`, lines 152–161) **must**
clear `enum_utils::clear_enum_eval_memo`, `const_enum_eval::clear_const_eval_memo`,
and `cycle_guard::clear_visited_sets` together, or a stale `NodeIndex` from the
previous file could return a wrong folded value or spuriously trip cycle
detection. This is documented in `context/file_session_reset.rs` (lines 68–71),
which lists these three module memos as a class that *must* be cleared at the
file boundary precisely because they are `NodeIndex`-keyed. By contrast, the
`SymbolId`-keyed caches (`symbol_types` and friends) are deliberately *retained*
across files — symbol identity is stable, so retained entries are correct.

**Memo scoping for const enums.** The const-enum memo is cleared *per enum
declaration* (not just per file). This narrows correctness risk: a folded result
for `B.Y` is reusable across the members of one const enum, but a later const
enum starts from an empty memo, so there is no chance of a `NodeIndex` collision
leaking a value between unrelated enums even within the same file.

## Edge cases and tsc parity

- **`0x80000000 | 0` is `-2147483648`, not `i32::MAX`.** The whole reason
  `to_int32`/`to_uint32` exist instead of `as i32`. Saturating casts diverge
  from `tsc`; wrapping matches. Pinned by `const_enum_eval` unit tests.

- **Shift counts mask to 5 bits.** `1 << 32 == 1` (count `32 & 0x1f == 0`),
  matching JS, because the shift count is `to_uint32(right) & 0x1f`.

- **`NaN` vs `Infinity` are distinct const-enum errors.** `0/0` → `TS2478`,
  `1/0` → `TS2477`. tsc separates these; tsz classifies on `f64::is_nan` first,
  then `f64::is_infinite`.

- **String-valued const-enum members never fold.** They are recognized
  syntactically (`is_string_initializer`) and skipped before folding, because a
  string member is always a valid const-enum initializer and folding it to a
  number would be wrong. Auto-increment after a string member breaks (returns
  `None`) — matching tsc's "Enum member must have initializer" behavior for the
  *following* member.

- **`const` variable references suppress `TS2474`.** `const k = 4; const enum
  E { A = k }` is accepted by tsc even though the syntax-only folder can't
  resolve `k`. tsz checks the binder symbol for `BLOCK_SCOPED_VARIABLE` +
  `is_const_variable_declaration` and suppresses the diagnostic rather than
  fabricating a value (`declarations.rs`, `is_const_var_ref`). This is a
  deliberate *non*-fold: tsz emits no member value, only declines to error.

- **Forward references are `TS2651`, not `TS2474`.** `enum E { A = B, B = 1 }`
  reports the forward-reference error and *suppresses* the constancy error so
  the two don't stack. The forward-ref detection is syntactic
  (`enum_has_forward_reference`) and runs before folding.

- **Self-reference is `TS2565` (grammar), plus a `None` fold.** The "used
  before being assigned" message comes from
  `check_enum_member_self_reference`, independent of the fold returning `None`.

- **`E.Member` and `E["Member"]` both fold; `E[expr]` does not.** Element access
  with a non-string-literal argument is not a constant member reference;
  `collect_access_chain` returns `None`, so `E[someVar]` is unfoldable.

- **`TS2476` — const-enum element access requires a string literal.** A const
  enum read via `E[someVar]` (non-string-literal index) is rejected at
  `access.rs` (lines 698–737): the receiver is checked with
  `is_const_enum_symbol`, and a non-string-literal argument yields `TS2476` and
  `TypeId::ERROR`. This is the *read-site* analogue of the initializer rule.

- **`TS2475` — const enum used as a value.** `let x = E;` (a bare const-enum
  reference outside a property/index/import/type-query position) is rejected in
  `identifier/resolved.rs` (line 432+). A const enum has no runtime object, so
  only member access is legal.

- **`TS2450` / `TS2748` — TDZ and ambient/isolated access.** Using an enum
  before its declaration is `TS2450` (`enum_namespace_access.rs`,
  `check_tdz_violation`). Accessing an *ambient* const enum under
  `isolatedModules` (or `verbatimModuleSyntax`) is `TS2748`; the option name in
  the message is chosen from `compiler_options.verbatim_module_syntax`. These are
  consumption-side rules, distinct from the initializer-folding path but part of
  the same const-enum surface.

- **`TS2567` — const enum can't merge with a namespace.** A `const enum` that
  shares a symbol with a `MODULE_DECLARATION` reports `TS2567` at the enum name
  (`declarations.rs`, lines 928–946). The folder is unaffected; this is a
  merge-legality check.

- **Mixed enums and `apparent_enum_instance_type`.** When a numeric enum member
  is used where a primitive is expected, `apparent_enum_instance_type` maps
  `EnumKind::Numeric → number`, `String → string`, `Mixed → number | string`.
  This is what lets `Dir.Up` participate in arithmetic while
  `StringEnum.Member` does not.

## Where the boundary sits

- The folders **stop** at `Option<f64>`. Turning that into a `literal_number`
  type, interning it, building the `union`, and wrapping it in
  `TypeData::Enum(def_id, …)` is all `computed/mod.rs` + the solver factory
  (see [solver-types-intern-def](solver-types-intern-def.md)).

- Relating an enum to `number`/`string`, or one enum to another, is the
  solver's job through the structural side of `TypeData::Enum`
  ([solver-relations](solver-relations.md)). The checker's
  `enum_assignability_override` only supplies a *numeric-subset* hint for
  same-named numeric enums and refuses to equate const enums across symbols.

- The arithmetic-operand fallback `is_enum_like_type`
  (`checkers/enum_checker.rs`) is a *symbol-flag* check, used **only** when the
  resolved type is still `Lazy(DefId)` and the solver's `NumberLikeVisitor`
  couldn't see through it; when the type is fully resolved,
  `BinaryOpEvaluator::is_arithmetic_operand` in the solver is authoritative.
  This respects the architecture rule that the checker asks the solver for
  semantic answers — the fallback exists strictly for the unresolved-`Lazy`
  case.

- Inlining const-enum reads into emitted JavaScript is the emitter's
  independent `i64` folder; it does not consume these `f64` results. See
  [emitter](emitter.md).

## Related reading

- [checker-jsx-properties-accessors-enums](checker-jsx-properties-accessors-enums.md)
  — enum declaration orchestration and the boundary-level view of these
  evaluators.
- [checker-type-of-symbol-and-symbol-types](checker-type-of-symbol-and-symbol-types.md)
  — how `get_type_of_symbol` produces the enum's `TypeData::Enum`.
- [checker-declarations-modules](checker-declarations-modules.md) — enum/namespace
  merging context for `TS2567`.
- [solver-types-intern-def](solver-types-intern-def.md) — `DefId`/`TypeId`
  interning and the `Enum(def_id, structural)` representation.
- [solver-relations](solver-relations.md) — enum assignability.
- [solver-mapped-and-tuple-shards](solver-mapped-and-tuple-shards.md) — why
  per-member number literals matter for mapped types over enums.
- [emitter](emitter.md) — const-enum erasure/inlining and `preserveConstEnums`.
- [end-to-end-timeline](end-to-end-timeline.md) — where declaration-check vs
  type-check phases sit in the overall pass order.
