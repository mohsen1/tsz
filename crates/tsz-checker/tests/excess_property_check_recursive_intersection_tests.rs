//! Regression tests for issue #8687: excess-property check must be suppressed
//! for nested object literals when the target is an intersection that contains
//! a recursive interface or type alias.
//!
//! **Structural rule (owner: `widen_nested_target_if_recursive`):**
//! When a recursive type (interface or alias) is a member of an intersection
//! `Rec & Extra`, a fresh object literal assigned to a recursive property of
//! `Rec` must be validated against the full intersection `Rec & Extra`, not
//! just `Rec`. This prevents false TS2353 errors for properties contributed by
//! the `Extra` member that are valid at that nesting level.
//!
//! Adjacent cases covered per CLAUDE.md §25/§26:
//! - Recursive type alias (not just interface) intersected with literal shape
//! - Recursive interface intersected with a homomorphic mapped type (`Readonly<T>`)
//! - `exactOptionalPropertyTypes` variant (optional properties remain optional)
//! - Three-level deep nesting
//! - Negative controls: truly excess properties still error at every depth

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_source, check_source_diagnostics, check_source_strict_messages,
};

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn codes_with_opts(source: &str, opts: CheckerOptions) -> Vec<u32> {
    check_source(source, "test.ts", opts)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn has_code(source: &str, code: u32) -> bool {
    codes(source).contains(&code)
}

// ---------------------------------------------------------------------------
// 1. Interface recursive member inside intersection — already covered by the
//    inline tests in property.rs but included here as the canonical repro for
//    issue #8687.
// ---------------------------------------------------------------------------

#[test]
fn canonical_repro_interface_recursive_intersection_no_false_ts2353() {
    // `parent?: User` resolves to `User | undefined`; nested literal must be
    // checked against `UserGroup` (= `User & { admin }`) not just `User`.
    let src = r#"
interface User { name: string; parent?: User; }
type UserGroup = User & { admin: boolean; }
const u: UserGroup = { name: "Alice", admin: true, parent: { name: "Bob", admin: false } };
"#;
    assert!(
        !has_code(src, 2353),
        "expected no TS2353 for nested literal with valid intersection props"
    );
}

#[test]
fn canonical_repro_renamed_interface_no_false_ts2353() {
    // Rename axis: same rule must hold for identifiers other than `User`.
    let src = r#"
interface Vertex { id: number; edge?: Vertex; }
type TaggedVertex = Vertex & { color: string; }
const v: TaggedVertex = { id: 1, color: "red", edge: { id: 2, color: "blue" } };
"#;
    assert!(
        !has_code(src, 2353),
        "expected no TS2353 for renamed recursive interface intersection"
    );
}

// ---------------------------------------------------------------------------
// 2. Recursive type alias (not an interface declaration) intersected with a
//    literal object type.
// ---------------------------------------------------------------------------

#[test]
fn recursive_type_alias_intersection_no_false_ts2353() {
    // `type Tree = { value: number; left?: Tree; right?: Tree; }` is a
    // self-referential type alias. `Tree & { tag: string }` should allow
    // `{ value, tag, left: { value, tag } }` without TS2353.
    let src = r#"
type Tree = { value: number; left?: Tree; right?: Tree; };
type TaggedTree = Tree & { tag: string; }
const t: TaggedTree = {
    value: 1,
    tag: "root",
    left: { value: 2, tag: "left" },
    right: { value: 3, tag: "right" },
};
"#;
    assert!(
        !has_code(src, 2353),
        "expected no TS2353 for recursive type alias intersection nested literals"
    );
}

#[test]
fn recursive_type_alias_intersection_renamed_no_false_ts2353() {
    // Rename axis: different alias name must not break the structural rule.
    let src = r#"
type Chain = { data: string; rest?: Chain; };
type MarkedChain = Chain & { marker: number; }
const c: MarkedChain = { data: "a", marker: 1, rest: { data: "b", marker: 2 } };
"#;
    let all_diags = check_source_diagnostics(src);
    let ts2353: Vec<_> = all_diags.iter().filter(|d| d.code == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "expected no TS2353 for renamed recursive type alias intersection, got: {:?}",
        ts2353.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn conditional_mapped_recursive_intersection_no_false_ts2353() {
    let src = r#"
type Request = { l1: { l2: boolean } };
type Example<T> = { ex?: T | null };

type Schema1<T> = (T extends boolean ? { type: 'boolean'; } : { props: { [P in keyof T]: Schema1<T[P]> }; }) & Example<T>;

export const schemaObj1: Schema1<Request> = {
  props: {
    l1: {
      props: {
        l2: { type: 'boolean' },
        invalid: false,
      },
    },
  },
}

type Schema2<T> = (T extends boolean ? { type: 'boolean'; } & Example<T> : { props: { [P in keyof T]: Schema2<T[P]> }; } & Example<T>);

export const schemaObj2: Schema2<Request> = {
  props: {
    l1: {
      props: {
        l2: { type: 'boolean' },
        invalid: false,
      },
    },
  },
}

type Schema3<T> = Example<T> & (T extends boolean ? { type: 'boolean'; } : { props: { [P in keyof T]: Schema3<T[P]> }; });

export const schemaObj3: Schema3<Request> = {
  props: {
    l1: {
      props: {
        l2: { type: 'boolean' },
        invalid: false,
      },
    },
  },
}

type Schema4<T> = (T extends boolean ? { type: 'boolean'; } & Example<T> : { props: Example<T> & { [P in keyof T]: Schema4<T[P]> }; });

export const schemaObj4: Schema4<Request> = {
  props: {
    l1: {
      props: {
        l2: { type: 'boolean' },
        invalid: false,
      },
    },
  },
}
"#;
    let ts2353: Vec<_> = check_source_strict_messages(src)
        .into_iter()
        .filter(|(code, _)| *code == 2353)
        .collect();
    assert!(
        ts2353.is_empty(),
        "expected no TS2353 for conditional mapped recursive intersection, got: {ts2353:?}"
    );
}

#[test]
fn conditional_mapped_recursive_intersection_top_level_excess_still_errors() {
    let src = r#"
type Request = { l1: { l2: boolean } };
type Example<T> = { ex?: T | null };
type Schema<T> = (T extends boolean ? { type: 'boolean'; } : { props: { [K in keyof T]: Schema<T[K]> }; }) & Example<T>;

const schemaObj: Schema<Request> = {
  props: {
    l1: {
      props: {
        l2: { type: 'boolean' },
        invalid: false,
      },
    },
  },
  extra: false,
}
"#;
    let ts2353: Vec<_> = check_source_strict_messages(src)
        .into_iter()
        .filter(|(code, _)| *code == 2353)
        .collect();
    assert!(
        ts2353
            .iter()
            .any(|(_, message)| message.contains("'extra'")),
        "expected top-level TS2353 for conditional mapped recursive intersection, got: {ts2353:?}"
    );
}

#[test]
fn explicit_nested_object_with_recursive_operation_property_still_checks_excess() {
    let src = r#"
type Request = { l1: { l2: boolean } };
type Example<T> = { ex?: T | null };
type Schema<T> = (T extends boolean ? { type: 'boolean'; } : { props: { [K in keyof T]: Schema<T[K]> }; }) & Example<T>;
type Box = { l2: Schema<boolean> };

const schemaObj: { outer: Box } = {
  outer: {
    l2: { type: 'boolean' },
    extra: false,
  },
}
"#;
    let ts2353: Vec<_> = check_source_strict_messages(src)
        .into_iter()
        .filter(|(code, _)| *code == 2353)
        .collect();
    assert!(
        ts2353
            .iter()
            .any(|(_, message)| message.contains("'extra'")),
        "expected TS2353 for explicit object with recursive operation property, got: {ts2353:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. Recursive interface intersected with a homomorphic mapped type
//    (`Readonly<Tree>`). The intersection still has the original `Tree` as a
//    direct `Lazy` member, so the widening gate fires correctly.
// ---------------------------------------------------------------------------

#[test]
fn recursive_interface_intersected_with_readonly_mapped_type_no_false_ts2353() {
    // `Tree & Readonly<Tree>` — the outer intersection still contains `Tree`
    // as a direct member, so a nested literal for `left` must be checked
    // against the full intersection, not just `Tree`.
    let src = r#"
interface Tree { value: string; left?: Tree; }
type ReadonlyTree = Tree & Readonly<Tree>;
const t: ReadonlyTree = { value: "root", left: { value: "leaf" } };
"#;
    // `Readonly<Tree>` needs lib; without lib just verify no spurious TS2353.
    let diags = check_source_diagnostics(src);
    let ts2353: Vec<_> = diags.iter().filter(|d| d.code == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "expected no TS2353 for recursive interface + Readonly mapped type, got: {ts2353:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. exactOptionalPropertyTypes variant.
//    The excess-property rule is unchanged by this flag: a nested literal that
//    omits optional properties is still valid; a truly extra key still errors.
// ---------------------------------------------------------------------------

#[test]
fn exact_optional_property_types_recursive_intersection_no_false_ts2353() {
    let opts = CheckerOptions {
        exact_optional_property_types: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    // With exactOptionalPropertyTypes the optional `parent` can only be
    // absent, not explicitly `undefined`.  A fresh nested literal that omits
    // `parent` must still be accepted without TS2353 for `admin`.
    let src = r#"
interface User { name: string; parent?: User; }
type UserGroup = User & { admin: boolean; }
const u: UserGroup = { name: "Alice", admin: true, parent: { name: "Bob", admin: false } };
"#;
    let diags = codes_with_opts(src, opts);
    assert!(
        !diags.contains(&2353),
        "expected no TS2353 under exactOptionalPropertyTypes, got: {diags:?}"
    );
}

#[test]
fn exact_optional_property_types_renamed_recursive_intersection_no_false_ts2353() {
    // Rename axis under exactOptionalPropertyTypes.
    let opts = CheckerOptions {
        exact_optional_property_types: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let src = r#"
interface Frame { label: string; prev?: Frame; }
type AnnotatedFrame = Frame & { seq: number; }
const f: AnnotatedFrame = {
    label: "first",
    seq: 1,
    prev: { label: "second", seq: 2 },
};
"#;
    let diags = codes_with_opts(src, opts);
    assert!(
        !diags.contains(&2353),
        "expected no TS2353 for renamed case under exactOptionalPropertyTypes, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. Three-level deep nesting: the widening must apply at every nested depth.
// ---------------------------------------------------------------------------

#[test]
fn three_levels_deep_nesting_no_false_ts2353() {
    let src = r#"
interface Node { value: number; child?: Node; }
type AnnotatedNode = Node & { label: string; }
const n: AnnotatedNode = {
    value: 1,
    label: "root",
    child: {
        value: 2,
        label: "mid",
        child: {
            value: 3,
            label: "leaf",
        },
    },
};
"#;
    assert!(
        !has_code(src, 2353),
        "expected no TS2353 at any nesting depth for valid three-level literal"
    );
}

#[test]
fn three_levels_deep_nesting_renamed_no_false_ts2353() {
    // Rename axis for the three-level case.
    let src = r#"
interface Scope { depth: number; inner?: Scope; }
type TracedScope = Scope & { name: string; }
const s: TracedScope = {
    depth: 0,
    name: "outer",
    inner: {
        depth: 1,
        name: "middle",
        inner: {
            depth: 2,
            name: "inner",
        },
    },
};
"#;
    assert!(
        !has_code(src, 2353),
        "expected no TS2353 for renamed three-level deep recursive intersection"
    );
}

// ---------------------------------------------------------------------------
// 6. Negative controls: truly excess properties still error regardless of
//    recursive intersection context.
// ---------------------------------------------------------------------------

#[test]
fn truly_excess_property_in_recursive_intersection_still_errors() {
    // `extra` is in neither `User` nor `{ admin }` → TS2353 must still fire.
    let src = r#"
interface User { name: string; parent?: User; }
type UserGroup = User & { admin: boolean; }
const u: UserGroup = {
    name: "Alice",
    admin: true,
    parent: { name: "Bob", admin: false, extra: 99 },
};
"#;
    assert!(
        has_code(src, 2353),
        "expected TS2353 for genuinely excess property 'extra'"
    );
}

#[test]
fn truly_excess_property_at_top_level_recursive_intersection_still_errors() {
    // At the top level (not nested), excess properties must also error.
    let src = r#"
interface User { name: string; parent?: User; }
type UserGroup = User & { admin: boolean; }
const u: UserGroup = { name: "Alice", admin: true, ghost: "boo" };
"#;
    assert!(
        has_code(src, 2353),
        "expected TS2353 for top-level excess property 'ghost'"
    );
}

#[test]
fn truly_excess_property_renamed_recursive_intersection_still_errors() {
    // Rename axis for the negative case.
    let src = r#"
interface Vertex { id: number; edge?: Vertex; }
type TaggedVertex = Vertex & { color: string; }
const v: TaggedVertex = {
    id: 1,
    color: "red",
    edge: { id: 2, color: "blue", orphan: true },
};
"#;
    assert!(
        has_code(src, 2353),
        "expected TS2353 for excess property 'orphan' in renamed recursive intersection"
    );
}
