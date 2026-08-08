//! Boundary tests for a `keyof T[K]` conditional *check* inside a generic
//! mapped-type body — the `Kysely`-style dependent-operand family.
//!
//! Structural rule: when a generic mapped type
//! `{ [P in Keys]: Lit extends keyof Obj[P] ? Obj[P][Lit] : never }` is
//! instantiated (`P := "row"`), tsc substitutes the mapped key into the
//! conditional's *check* type and evaluates
//! `Lit extends keyof Obj["row"]` against the fully materialised
//! `keyof Obj["row"]`. The true branch is taken and the member type resolves
//! to `Obj["row"][Lit]`.
//!
//! tsz currently diverges only in this one shape: the conditional's extends
//! type `keyof Obj[P]` (after `P := "row"`, a `KeyOf(IndexAccess(Lazy(Obj),
//! "row"))`) is *not* materialised before the branch relation runs, so
//! `Lit extends keyof Obj["row"]` is judged `false` and the whole member
//! collapses to `never`. Every valid value is then rejected with a spurious
//! `TS2322`/`TS2345`. This is the root cause of the still-failing driver row
//! `tsz-cli::driver_tests::cross_file_dependent_operand_aliases_accept_scalars_and_literal_arrays`
//! (#15983): the widened `string[]` reported there is a downstream symptom —
//! the `DependentOperand<…>` target itself accepts *nothing* because its
//! nested `FieldLookup` mapped/conditional collapses to `never`.
//!
//! It belongs to the unmaterialised-`Lazy`/`Application`-operand family
//! (#15396): a decision site (the conditional branch relation) judges an
//! operand — `keyof Obj[P]` — that a resolver-backed materialisation would
//! have expanded.
//!
//! The passing controls below fence off the exact working neighbours so a
//! future fix (or refactor of the mapped/conditional evaluation path) cannot
//! silently regress them, and pin the single failing shape as an ignored,
//! oracle-annotated red signal that is far cheaper to iterate on than the
//! six-file driver fixture. Binder names deliberately differ from the driver
//! fixture (`Registry`/`entries`) — see `.claude/CLAUDE.md` §25.

use tsz_checker::test_utils::check_source_strict;

/// Assert `source` produces no assignability diagnostics (TS2322 / TS2345) at
/// the annotation seams the repros exercise — i.e. the annotated type resolved
/// to a shape that accepts the assigned value.
fn assert_resolves(source: &str, msg: &str) {
    let codes: Vec<u32> = check_source_strict(source)
        .iter()
        .map(|d| d.code)
        .filter(|&c| c == 2322 || c == 2345)
        .collect();
    assert_eq!(codes, Vec::<u32>::new(), "{msg}");
}

// ──────────────────────────────────────────────────────────────────────────
// Passing controls: the working neighbours of the failing shape.
// ──────────────────────────────────────────────────────────────────────────

/// `keyof Obj[K]` in a conditional *check* with an ordinary type parameter `K`
/// (not a mapped key) materialises correctly: `Store<"row">` is `"yes"`.
#[test]
fn keyof_indexed_access_check_with_plain_type_param_resolves() {
    assert_resolves(
        r#"
interface StoreRow { title: string; kind: 'alpha' | 'beta' }
interface Store { row: StoreRow }
type Probe<K extends keyof Store> = 'kind' extends keyof Store[K] ? 'yes' : 'no'
const ok: Probe<'row'> = 'yes'
"#,
        "keyof Store[K] in a conditional check with a plain type param must resolve to the true branch",
    );
}

/// `keyof Obj[P]` used directly as a mapped-type *value* (no surrounding
/// conditional) materialises correctly: `Cols<"row">` is `keyof StoreRow`.
#[test]
fn keyof_indexed_access_as_mapped_value_resolves() {
    assert_resolves(
        r#"
interface StoreRow { title: string; kind: 'alpha' | 'beta' }
interface Store { row: StoreRow }
type Cols<Tb extends keyof Store> = { [P in Tb]: keyof Store[P] }[Tb]
const ok: Cols<'row'> = 'title'
const ok2: Cols<'row'> = 'kind'
"#,
        "keyof Store[P] as a mapped value must resolve to keyof StoreRow",
    );
}

/// A conditional *check* inside a mapped body that does NOT read the mapped key
/// (`RowShape extends Store[P]` uses `Store[P]` only on the extends side, and
/// the check `Lit extends keyof RowShape` reads a concrete object) resolves.
#[test]
fn mapped_conditional_check_without_mapped_key_keyof_resolves() {
    assert_resolves(
        r#"
interface StoreRow { title: string; kind: 'alpha' | 'beta' }
interface Store { row: StoreRow }
type Pick1<Tb extends keyof Store> =
  { [P in Tb]: 'kind' extends keyof StoreRow ? StoreRow['kind'] : never }[Tb]
const ok: Pick1<'row'> = 'alpha'
"#,
        "a mapped conditional whose check reads a concrete object (not keyof of the mapped key) resolves",
    );
}

/// A conditional inside a mapped body whose *check* reads the mapped key
/// through a plain `extends` (no `keyof`) resolves — isolating that the
/// divergence is the `keyof` wrapper, not indexed access of the mapped key.
#[test]
fn mapped_conditional_check_indexed_by_mapped_key_without_keyof_resolves() {
    assert_resolves(
        r#"
interface StoreRow { title: string; kind: 'alpha' | 'beta' }
interface Store { row: StoreRow }
type Pick2<Tb extends keyof Store> =
  { [P in Tb]: StoreRow extends Store[P] ? StoreRow['kind'] : never }[Tb]
const ok: Pick2<'row'> = 'alpha'
"#,
        "a mapped conditional whose check indexes the mapped key without keyof resolves",
    );
}

// ──────────────────────────────────────────────────────────────────────────
// The formerly-failing shape (now fixed; #15983 / #15396).
// ──────────────────────────────────────────────────────────────────────────

/// A generic mapped type whose conditional *check* is
/// `Name extends keyof Obj[P]` (a `keyof` of the mapped-key-indexed access)
/// must, after instantiation, take the true branch and resolve the member to
/// `Obj[P][Name]`.
///
/// Oracle (`tsc` 7.0.2, `--strict`): `FieldLookup<Store, 'row', 'kind'>` is
/// `'alpha' | 'beta'`, so `const ok = 'alpha'` is accepted.
///
/// Fixed by the materialize-or-defer gateway in the solver's conditional
/// evaluator (`keyof_inner_is_unresolvable_lazy` now evaluates an `IndexAccess`
/// inner): `Store['row']` reduces to the nested-only interface `StoreRow`, which
/// that pure-solver evaluation context cannot materialise, so `keyof Store['row']`
/// stays a deferred `KeyOf`. The conditional now defers instead of judging the
/// branch against the un-materialised operand, so a later pass takes the true
/// branch (#15983 / #15396).
#[test]
fn mapped_conditional_keyof_mapped_key_check_resolves() {
    assert_resolves(
        r#"
interface StoreRow { title: string; kind: 'alpha' | 'beta' }
interface Store { row: StoreRow }
type FieldLookup<Db, Tb extends keyof Db, Name> =
  { [P in Tb]: Name extends keyof Db[P] ? Db[P][Name] : never }[Tb]
type Looked = FieldLookup<Store, 'row', 'kind'>
const ok: Looked = 'alpha'
"#,
        "FieldLookup<Store, 'row', 'kind'> must resolve to 'alpha' | 'beta', not never",
    );
}

/// The same defect reached through `keyof Db` as the second argument (as the
/// driver fixture spells its `TB`), confirming the collapse is not keyed on the
/// concrete-literal `Tb` form. Oracle (`tsc` 7.0.2, `--strict`):
/// `FieldLookup<Store, keyof Store, 'kind'>` is `'alpha' | 'beta'`.
#[test]
fn mapped_conditional_keyof_mapped_key_check_keyof_arg_resolves() {
    assert_resolves(
        r#"
interface StoreRow { title: string; kind: 'alpha' | 'beta' }
interface Store { row: StoreRow }
type FieldLookup<Db, Tb extends keyof Db, Name> =
  { [P in Tb]: Name extends keyof Db[P] ? Db[P][Name] : never }[Tb]
type Looked = FieldLookup<Store, keyof Store, 'kind'>
const ok: Looked = 'alpha'
"#,
        "FieldLookup<Store, keyof Store, 'kind'> must resolve to 'alpha' | 'beta', not never",
    );
}

/// Binder-name independence (`.claude/CLAUDE.md` §25): the fix must key on the
/// structural shape, not on the `Store`/`StoreRow`/`FieldLookup` identifiers.
/// Renaming every binder — and reordering the type parameters — still resolves.
#[test]
fn mapped_conditional_keyof_renamed_binders_resolves() {
    assert_resolves(
        r#"
interface Cell { label: string; tag: 'x' | 'y' }
interface Grid { cell: Cell }
type Pluck<Key, Tbl extends keyof Db2, Db2> =
  { [Q in Tbl]: Key extends keyof Db2[Q] ? Db2[Q][Key] : never }[Tbl]
type Got = Pluck<'tag', 'cell', Grid>
const ok: Got = 'x'
const ok2: Got = 'y'
"#,
        "renamed FieldLookup with reordered params must still resolve to 'x' | 'y'",
    );
}

/// Negative control: a genuinely-absent member must still take the *false*
/// branch and yield `never`, so the deferral fix does not over-accept. Oracle
/// (`tsc` 7.0.2, `--strict`): `FieldLookup<Store, 'row', 'nope'>` is `never`, so
/// `const bad: Looked = 'alpha'` is a `TS2322`.
#[test]
fn mapped_conditional_keyof_absent_member_stays_never() {
    let codes: Vec<u32> = check_source_strict(
        r#"
interface StoreRow { title: string; kind: 'alpha' | 'beta' }
interface Store { row: StoreRow }
type FieldLookup<Db, Tb extends keyof Db, Name> =
  { [P in Tb]: Name extends keyof Db[P] ? Db[P][Name] : never }[Tb]
type Looked = FieldLookup<Store, 'row', 'nope'>
const bad: Looked = 'alpha'
"#,
    )
    .iter()
    .map(|d| d.code)
    .filter(|&c| c == 2322)
    .collect();
    assert_eq!(
        codes,
        vec![2322],
        "an absent member must collapse to never and reject the assignment (TS2322)"
    );
}
