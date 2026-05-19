//! Structural coverage for `excessPropertyCheckIntersectionWithRecursiveType.ts`
//! (issue #7648).
//!
//! Structural rule: an object literal assigned to a type built from a
//! recursive type alias whose body is a conditional with intersections in
//! either branch must trigger TS2353 for unknown properties on the same
//! object-shape "slice" tsc would render, regardless of whether the
//! conditional is wrapped *outside* the intersection (the "Schema1" shape)
//! or *inside* both branches (the "Schema2" shape).
//!
//! The renamed identifiers in each test prove the rule is keyed on
//! structural shape — not on the literal alias names from the upstream
//! repro (`Schema1`, `Schema2`, `Request`, `BuildTree`, `User`).
//!
//! Adjacent cases exercised:
//!   * Outer intersection over conditional (Schema1 shape).
//!   * Conditional outside, mapped+recursive false branch (Schema3 shape).
//!   * Conditional outside, false-branch starts with `Example<T> & ...`
//!     (Schema4 shape).
//!   * Top-level literal against the recursive alias (single-level depth).
//!   * Depth-bounded recursive tree via index-signature termination
//!     (BuildTree shape) — the "no-more-children" termination at depth 2
//!     must reject `children` as excess on the leaf level (TS2353) and as
//!     missing on a `.children` *read* at the same leaf level (TS2339).

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostics_for(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

fn count_with_message_contains(diags: &[Diagnostic], code: u32, needle: &str) -> usize {
    diags
        .iter()
        .filter(|d| d.code == code && d.message_text.contains(needle))
        .count()
}

fn render_diag_codes(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn diagnostics_codes_count(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

/// Schema1 shape: outer intersection over conditional.
///
///   type Outer<T> = (T extends boolean ? A : B) & Side<T>;
///
/// The inner literal at `props.l1.props` is contextually typed by
/// `{ l2: Outer<boolean>; }`. The `invalid` member must surface as
/// TS2353 against that object-shape rendering — and the message must
/// preserve `Outer<boolean>` as the alias rendering (not the expanded
/// branch form), matching tsc's display policy for this pattern.
#[test]
fn schema1_outer_intersection_emits_ts2353_at_nested_property() {
    // Rename variables `P`, `T`, alias names — only the structural
    // pattern should drive the diagnostic.
    let source = r#"
type Side<X> = { ex?: X | null };
type Outer<X> = (X extends boolean ? { tag: 'boolean'; } : { props: { [K in keyof X]: Outer<X[K]> }; }) & Side<X>;

type Req = { l1: { l2: boolean } };

export const obj: Outer<Req> = {
  props: {
    l1: {
      props: {
        l2: { tag: 'boolean' },
        invalid: false,
      },
    },
  },
};
"#;

    let diags = diagnostics_for(source);
    let hits = count_with_message_contains(&diags, 2353, "'invalid'");
    assert_eq!(
        hits, 1,
        "Outer-intersection Schema1 shape must emit exactly one TS2353 for the unknown `invalid` key; got: {:?}",
        render_diag_codes(&diags)
    );
}

/// Schema3 shape: same as Schema1 with the intersection operands flipped
/// (`Side<T> & (T extends boolean ? A : B)`). The conditional sits in the
/// right operand instead of the left.
#[test]
fn schema3_flipped_intersection_emits_ts2353_at_nested_property() {
    let source = r#"
type Side<X> = { ex?: X | null };
type Outer<X> = Side<X> & (X extends boolean ? { tag: 'boolean'; } : { props: { [K in keyof X]: Outer<X[K]> }; });

type Req = { l1: { l2: boolean } };

export const obj: Outer<Req> = {
  props: {
    l1: {
      props: {
        l2: { tag: 'boolean' },
        invalid: false,
      },
    },
  },
};
"#;

    let diags = diagnostics_for(source);
    let hits = count_with_message_contains(&diags, 2353, "'invalid'");
    assert_eq!(
        hits, 1,
        "Outer-intersection (flipped) Schema3 shape must emit exactly one TS2353; got: {:?}",
        render_diag_codes(&diags)
    );
}

/// Schema4 shape: outer conditional, false branch is
/// `{ props: Side<T> & { [K in keyof T]: ... } }`. The `Side<T>` is now
/// inside the mapped-bearing object, not outside.
#[test]
fn schema4_intersection_inside_false_props_emits_ts2353_at_nested_property() {
    let source = r#"
type Side<X> = { ex?: X | null };
type Outer<X> = X extends boolean
    ? { tag: 'boolean'; } & Side<X>
    : { props: Side<X> & { [K in keyof X]: Outer<X[K]> }; };

type Req = { l1: { l2: boolean } };

export const obj: Outer<Req> = {
  props: {
    l1: {
      props: {
        l2: { tag: 'boolean' },
        invalid: false,
      },
    },
  },
};
"#;

    let diags = diagnostics_for(source);
    let hits = count_with_message_contains(&diags, 2353, "'invalid'");
    assert_eq!(
        hits, 1,
        "Outer-conditional / Side-inside-props Schema4 shape must emit TS2353; got: {:?}",
        render_diag_codes(&diags)
    );
}

/// Top-level direct assignment against the Schema2-shape recursive alias
/// (single-level depth). This pins the *single-level* contract: the inner
/// literal at `props.l2` must trip TS2353 when contextually typed by the
/// recursive alias's per-key value type.
#[test]
fn schema2_single_level_direct_assignment_emits_ts2353() {
    let source = r#"
type Side<X> = { ex?: X | null };
type Outer<X> = X extends boolean
    ? { tag: 'boolean'; } & Side<X>
    : { props: { [K in keyof X]: Outer<X[K]> }; } & Side<X>;

export const obj: Outer<{ l2: boolean }> = {
  props: {
    l2: { tag: 'boolean' },
    invalid: false,
  },
};
"#;

    let diags = diagnostics_for(source);
    let hits = count_with_message_contains(&diags, 2353, "'invalid'");
    assert_eq!(
        hits, 1,
        "Schema2-shape single-level assignment must emit exactly one TS2353 for `invalid`; got: {:?}",
        render_diag_codes(&diags)
    );
}

/// Indexed-access into the Schema2-shape recursive alias must yield the
/// substituted shape (and thus also fire TS2353).
#[test]
fn schema2_indexed_access_evaluates_to_object_shape_for_excess_check() {
    let source = r#"
type Side<X> = { ex?: X | null };
type Outer<X> = X extends boolean
    ? { tag: 'boolean'; } & Side<X>
    : { props: { [K in keyof X]: Outer<X[K]> }; } & Side<X>;

type Req = { l1: { l2: boolean } };

// Equivalent to `Outer<{l2:boolean}>` after substitution.
type InnerByIndex = Outer<Req>['props']['l1'];

export const obj: InnerByIndex = {
  props: {
    l2: { tag: 'boolean' },
    invalid: false,
  },
};
"#;

    let diags = diagnostics_for(source);
    let hits = count_with_message_contains(&diags, 2353, "'invalid'");
    assert_eq!(
        hits, 1,
        "Indexed access through the Schema2-shape recursive alias must reveal the inner object-shape so EPC fires; got: {:?}",
        render_diag_codes(&diags)
    );
}

/// BuildTree-shape termination: `Build<T, N>` indexed by a depth-counted
/// tuple decides "leaf" vs "branch". At the leaf the type collapses to
/// plain `T` so an object literal supplying `children` must produce
/// TS2353, and a `.children` *read* on a leaf must produce TS2339.
#[test]
fn buildtree_leaf_rejects_children_via_ts2353_and_ts2339() {
    let source = r#"
type Len<L extends any[]> = L["length"];
type Cons<V, L extends any[]> = ((h: V, ...t: L) => void) extends (...args: infer R) => void ? R : any;

type Build<Leaf, Limit extends number = -1, Acc extends any[] = []> = {
  1: Leaf;
  0: Leaf & { children: Build<Leaf, Limit, Cons<any, Acc>>[] };
}[Len<Acc> extends Limit ? 1 : 0];

interface Leaf {
  name: string;
}

type Tree = Build<Leaf, 2>;

const tree: Tree = {
  name: "root",
  children: [
    {
      name: "lvl1",
      children: [
        {
          name: "lvl2",
          children: [
            { name: "lvl3", children: [{ name: "lvl4-extra" }] },
          ],
        },
      ],
    },
  ],
};

tree.children[0].children[0].children[0];
"#;

    let diags = diagnostics_for(source);

    let excess_children = count_with_message_contains(&diags, 2353, "'children'");
    assert!(
        excess_children >= 1,
        "depth-bounded recursion must reject the literal `children` past the leaf via TS2353; got: {:?}",
        render_diag_codes(&diags)
    );

    let missing_children = count_with_message_contains(&diags, 2339, "'children'");
    assert!(
        missing_children >= 1,
        "depth-bounded recursion must reject the `.children` read past the leaf via TS2339; got: {:?}",
        render_diag_codes(&diags)
    );

    let unexpected_ts2322 = diagnostics_codes_count(&diags, 2322);
    assert_eq!(
        unexpected_ts2322, 0,
        "BuildTree-shape termination must not produce TS2322; got: {:?}",
        render_diag_codes(&diags)
    );
}

/// Schema2 shape, **two-level nested literal**. This is the exact pattern
/// that the upstream conformance test
/// `excessPropertyCheckIntersectionWithRecursiveType.ts` exercises for
/// `schemaObj2`. tsc emits TS2353 for `invalid` at the deeply-nested
/// position; tsz currently does not surface this diagnostic because the
/// contextual type fed into the inner literal collapses back to the
/// outer recursive application (`Outer<Req>` instead of
/// `Outer<{ l2: boolean }>`) when the conditional has intersections in
/// both branches.
///
/// Pinned with `#[ignore]` for issue #7648 — the fix requires resolving
/// the substituted per-key recursive application during nested
/// contextual-type descent, not at the conformance-wrapper layer.
#[test]
#[ignore = "issue #7648: Schema2-pattern nested EPC loses the substituted recursive instance"]
fn schema2_two_level_nested_emits_ts2353_for_invalid_in_inner_props() {
    let source = r#"
type Side<X> = { ex?: X | null };
type Outer<X> = X extends boolean
    ? { tag: 'boolean'; } & Side<X>
    : { props: { [K in keyof X]: Outer<X[K]> }; } & Side<X>;

type Req = { l1: { l2: boolean } };

export const obj: Outer<Req> = {
  props: {
    l1: {
      props: {
        l2: { tag: 'boolean' },
        invalid: false,
      },
    },
  },
};
"#;

    let diags = diagnostics_for(source);
    let hits = count_with_message_contains(&diags, 2353, "'invalid'");
    assert_eq!(
        hits, 1,
        "Schema2-shape two-level nested literal must emit exactly one TS2353 for `invalid`; got: {:?}",
        render_diag_codes(&diags)
    );
}
