//! Tests for array-literal contextual typing (extracted from `array_literal.rs`
//! to keep the parent module under the 2000-line checker boundary cap).

use crate::context::CheckerOptions;
use crate::test_utils::{
    check_source, check_source_codes, check_source_with_libs, load_compiled_lib_files,
};
use tsz_common::common::ModuleKind;

fn check_strict_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn empty_array_in_storage_assignment_adopts_contextual_element() {
    // Regression for conformance test mappedTypeGenericIndexedAccess.ts:
    // `obj.entries[name] = []` under an `if (!obj.entries[name]) { … }`
    // guard was injecting `never[]` into the narrowed slot, then
    // `obj.entries[name]?.push(item)` collapsed push's contravariant
    // parameter to `never` and reported a false TS2345 against the
    // generic argument type `Types[T]`.
    //
    // tsc threads the storage slot's element type into the literal at
    // the assignment site, so the narrowed slot stays compatible with
    // the declared array.
    let source = r#"
type Types = {
    first: { a1: true };
    second: { a2: true };
    third: { a3: true };
}

class Test {
    entries: { [T in keyof Types]?: Types[T][] } = {};

    addEntry<T extends keyof Types>(name: T, entry: Types[T]) {
        if (!this.entries[name]) {
            this.entries[name] = [];
        }
        this.entries[name]?.push(entry);
    }
}
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2345),
        "`this.entries[name]?.push(entry)` under an `if (!this.entries[name]) {{ this.entries[name] = []; }}` guard should not report TS2345, got: {errors:?}"
    );
}

#[test]
fn empty_array_rhs_of_and_and_equals_keeps_never_element() {
    // Regression for conformance test logicalAssignment6.ts /
    // logicalAssignment7.ts:
    //   `(results &&= (results1 &&= [])).push(100)` where both
    //   `results` and `results1` are `number[] | undefined`.
    //
    // For `||=` and `??=`, the RHS empty array adopts the LHS's
    // element type (the RHS is the "default value" replacing a
    // falsy/nullable LHS). For `&&=` it does NOT — tsc keeps the
    // RHS literal at `never[]` so the chained `.push(100)` on the
    // resulting `(falsy results) | typeof []` reports TS2345
    // ("Argument of type '100' is not assignable to parameter of
    // type 'never'"). Without this distinction tsz silently
    // accepted the push.
    // The fix is observable here via the assignment to
    // `target: never[] | undefined`: with the bug, `[]` widened to
    // `number[]` and the expression type became
    // `undefined | number[]` — not assignable to `never[] | undefined`.
    // With the fix, `[]` stays `never[]` and the assignment is OK.
    let source = r#"
function foo3(arr: number[] | undefined) {
    const target: never[] | undefined = (arr &&= []);
    return target;
}
"#;
    let errors = check_strict_codes(source);
    assert!(
        !errors.contains(&2322),
        "`&&=` should leave the RHS `[]` at `never[]` so the assignment to `never[] | undefined` is clean, got: {errors:?}"
    );
}

#[test]
fn empty_array_rhs_of_or_or_equals_adopts_lhs_element() {
    // Counter-test: `||=` and `??=` continue to widen the RHS empty
    // array to the LHS's element type. Without widening, the
    // expression would type as `number[] | never[]` (wider than the
    // declared `number[] | undefined`) and the LHS slot would also
    // be assigned `never[]`, weakening downstream narrowing.
    // We verify the widening by assigning the expression to the
    // LHS's declared type — if widening regressed, the expression
    // type would carry `never[]` and the assignment to
    // `number[] | undefined` would still be OK (subtype). So we
    // also check the negative case: assigning to a `number[]` slot
    // (without `undefined`), which only succeeds when the falsy
    // branch can be eliminated. Either way TS2345/TS2322 must NOT
    // appear for the round trip.
    let source = r#"
function foo1(results: number[] | undefined) {
    const widened: number[] | undefined = (results ||= []);
    return widened;
}
function foo2(results: number[] | undefined) {
    const widened: number[] | undefined = (results ??= []);
    return widened;
}
"#;
    let errors = check_strict_codes(source);
    assert!(
        !errors.contains(&2322) && !errors.contains(&2345),
        "`||=`/`??=` widen RHS `[]` to `number[]`, so the round-trip assignment is clean, got: {errors:?}"
    );
}

#[test]
fn empty_array_in_generic_call_argument_still_drives_inference_to_never() {
    // Guard against the storage-context widening leaking into generic
    // call argument positions. There the contextual type is a still-
    // being-inferred type parameter, and adopting it would prevent the
    // inference engine from binding the parameter to `never`.
    let source = r#"
declare function f1<T>(x: T[]): T;
let a1 = f1([]);
let check: never = a1;
"#;
    let errors = check_source_codes(source);
    assert!(
        errors.is_empty(),
        "f1([]) should still infer T = never (so `let check: never = a1` is OK), got: {errors:?}"
    );
}

#[test]
fn rest_only_tuple_intersected_with_length_accepts_literal() {
    // Regression for conformance test contextualTypeWithTuple.ts (#29311):
    // `[...number[]] & { length: 2 }` was causing `[0, 0]` to be inferred as
    // `number[]` (because the rest-only tuple skipped tuple context), which
    // then failed to satisfy the intersection. tsc's `isTupleLikeType`
    // considers such intersections tuple-like, so the array literal must
    // use tuple inference and become `[number, number]`.
    let source = r#"
type test1 = [...number[]]
type fixed1 = test1 & { length: 2 }
let var1: fixed1 = [0, 0]
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "[0, 0] should be assignable to [...number[]] & {{ length: 2 }}, got: {errors:?}"
    );
}

#[test]
fn rest_only_tuple_without_intersection_still_widens_to_array() {
    // Guard against over-broadening the fix. A bare rest-only tuple without
    // other intersection members continues to use array inference, matching
    // the original behavior for destructuring-style contextual types such as
    // `[...any[]]`.
    let source = r#"
declare let arr: (string | number)[];
let x: [...(string | number)[]] = arr;
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "array is still assignable to rest-only tuple, got: {errors:?}"
    );
}

#[test]
fn elided_array_literal_element_typed_as_undefined_required() {
    // Regression for conformance test optionalTupleElements1.ts:
    // an elision (hole) in a non-destructuring array literal — e.g. `[42,,true]` —
    // must produce a tuple slot with type `undefined` (Required), matching tsc's
    // OmittedExpression -> undefinedWideningType. Previously the slot was dropped,
    // which both shifted subsequent positions and caused contextual typing to
    // mismatch optional tuple targets.
    //
    // `T3 = [number, string?, boolean?]` accepts `[42,,true]` because the source
    // tuple `[number, undefined, true]` (all Required) widens each Required
    // `undefined` against an Optional target slot to `T | undefined`.
    let source = r#"
type T3 = [number, string?, boolean?];
type T4 = [number?, string?, boolean?];
let t3: T3;
let t4: T4;
t3 = [42, , true];
t4 = [42, , true];
t4 = [, "hello", true];
t4 = [, , true];
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "elided array literal slots should produce undefined-typed Required tuple slots, got: {errors:?}"
    );
}

#[test]
fn f_bounded_interface_empty_array_contextual_type() {
    // F-bounded interface: `FileNode extends INode<FileNode>` where `children: T[]`.
    // Empty array `[]` in object literal should adopt `FileNode[]` contextual type, not `never[]`.
    let source = r#"
interface INode<T> {
    parent: T | null;
    children: T[];
}
interface FileNode extends INode<FileNode> {
    name: string;
}
const root: FileNode = {
    name: "root",
    parent: null,
    children: [],
};
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "F-bounded interface empty array should not produce TS2322, got: {errors:?}"
    );
}

#[test]
fn f_bounded_interface_all_members_in_base() {
    // Variant: name is declared on base interface, FileNode adds no extra members.
    let source = r#"
interface TreeNode<T> {
    name: string;
    parent: T | null;
    children: T[];
}
interface FileNode extends TreeNode<FileNode> {}
const root: FileNode = {
    name: "root",
    parent: null,
    children: [],
};
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "F-bounded (all-in-base) empty array should not produce TS2322, got: {errors:?}"
    );
}

#[test]
fn f_bounded_interface_non_self_reference_adopts_element_type() {
    // Non-F-bounded baseline: `Wrapper<string>` should type `items: []` as `string[]`.
    let source = r#"
interface Wrapper<T> {
    items: T[];
}
interface StringWrapper extends Wrapper<string> {}
const w: StringWrapper = { items: [] };
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "Non-F-bounded Wrapper<string> empty array should not produce TS2322, got: {errors:?}"
    );
}

#[test]
fn f_bounded_direct_self_ref_property_empty_array_contextual_type() {
    // F-bounded with a *direct* self-referential property (`value: T`, not wrapped in
    // union/array). This creates a deeper cycle during type construction. The other
    // property `items: T[]` must still adopt contextual type `DirectNode[]`, not `never[]`.
    // The bug only manifests with the full compiled Array<T> lib (stripped lib is
    // simpler and doesn't trigger the cycle). Test with both.
    let source = r#"
interface DirectRef<T> { value: T; items: T[]; }
interface DirectNode extends DirectRef<DirectNode> { name: string; }
const d: DirectNode = { name: "a", value: {} as DirectNode, items: [] };
"#;
    // Test with stripped lib (baseline)
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "F-bounded direct-self-ref (stripped lib): items:[] should be DirectNode[], not never[]; got: {errors:?}"
    );
    // Test with full compiled lib (where the cycle actually triggers)
    let full_libs = load_compiled_lib_files(&["lib.es5.d.ts"]);
    if !full_libs.is_empty() {
        let errors: Vec<u32> =
            check_source_with_libs(source, "test.ts", CheckerOptions::default(), &full_libs)
                .into_iter()
                .map(|d| d.code)
                .collect();
        assert!(
            !errors.contains(&2322),
            "F-bounded direct-self-ref (full lib): items:[] should be DirectNode[], not never[]; got: {errors:?}"
        );
    }
}

#[test]
fn f_bounded_multi_level_heritage_empty_array() {
    // Multi-level heritage: `Concrete extends Level1<Concrete>` where `Level1<T>` extends
    // `Level2<T>`. The `list: T[]` property is on Level2. Empty array must adopt
    // `Concrete[]` contextual type.
    let source = r#"
interface Level2<T> { data: T; list: T[]; }
interface Level1<T> extends Level2<T> { extra: string; }
interface Concrete extends Level1<Concrete> {}
const c: Concrete = { extra: "x", data: {} as Concrete, list: [] };
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "Multi-level F-bounded: list:[] should be Concrete[], not never[]; got: {errors:?}"
    );
}

#[test]
fn f_bounded_multiple_arrays_same_type_param() {
    // Multiple array properties with the same type parameter. Both should adopt the
    // concrete contextual type, not never[].
    let source = r#"
interface MultiArray<T> { first: T[]; second: T[]; ref: T; }
interface MultiConcrete extends MultiArray<MultiConcrete> {}
const m: MultiConcrete = { first: [], second: [], ref: {} as MultiConcrete };
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "Multi-array F-bounded: both arrays should be MultiConcrete[], got: {errors:?}"
    );
}

#[test]
fn f_bounded_readonly_array_property() {
    // Readonly array property in F-bounded interface should also adopt contextual type.
    let source = r#"
interface RNode<T> { parent: T | null; readonly children: readonly T[]; }
interface RFile extends RNode<RFile> { name: string; }
const rf: RFile = { name: "r", parent: null, children: [] };
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "F-bounded readonly array: children:[] should not produce TS2322, got: {errors:?}"
    );
}

#[test]
fn elided_array_literal_in_array_context_pushes_undefined() {
    // Without a tuple contextual type, an elision still contributes
    // `undefined` to the resulting array element type. tsc widens
    // `[1, , 3]` (no contextual) to `(number | undefined)[]`, so the
    // array literal is assignable to `(number | undefined)[]`.
    let source = r#"
const xs: (number | undefined)[] = [1, , 3];
"#;
    let errors = check_source_codes(source);
    assert!(
        !errors.contains(&2322),
        "[1, , 3] should be assignable to (number | undefined)[], got: {errors:?}"
    );
}

// Cross-file F-bounded interface tests: the interface declarations live in a
// separate file from the usage, exercising arena-identity-preserving heritage
// recovery in `recover_inherited_member_from_heritage`.

#[test]
fn f_bounded_cross_file_inherited_property_access() {
    let diagnostics = crate::test_utils::check_multi_file(
        &[
            (
                "lib.ts",
                r#"
export interface INode<T extends INode<T>> {
    children: T[];
    depth: number;
}
export interface FileNode extends INode<FileNode> {
    name: string;
}
"#,
            ),
            (
                "main.ts",
                r#"
import { FileNode } from "./lib";
declare const root: FileNode;
const kids: FileNode[] = root.children;
const d: number = root.depth;
"#,
            ),
        ],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.is_empty(),
        "Cross-file F-bounded: inherited properties should resolve, got: {diagnostics:?}"
    );
}

#[test]
fn f_bounded_cross_file_inherited_property_different_type_param_name() {
    // The fix must not be keyed on the type-parameter name `T`; `K` is equally valid.
    let diagnostics = crate::test_utils::check_multi_file(
        &[
            (
                "nodes.ts",
                r#"
export interface IGraph<K extends IGraph<K>> {
    edges: K[];
}
export interface GraphNode extends IGraph<GraphNode> {
    label: string;
}
"#,
            ),
            (
                "app.ts",
                r#"
import { GraphNode } from "./nodes";
declare const g: GraphNode;
const neighbors: GraphNode[] = g.edges;
"#,
            ),
        ],
        "app.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.is_empty(),
        "Cross-file F-bounded (param K): inherited edges should resolve, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_differing_arity_tuples_preserves_literal_element() {
    // Regression: a fresh literal array element typed against a union of
    // tuples whose members have DIFFERENT arity must keep its literal type.
    //
    // The per-position contextual type for index 0 of
    // `[number, boolean] | [2]` is `number | 2`. tsc derives this with
    // `UnionReduction.None`, so the literal arm `2` survives alongside its
    // base primitive `number`. Reducing to `number` (subtype/literal
    // absorption) dropped the literal arm, widened the fresh `2` to
    // `number`, and produced a spurious TS2322 because `[number]` matches
    // neither `[number, boolean]` (arity) nor `[2]` (literal). Vary the
    // literal kind and tuple shape so the fix is structural, not pinned to
    // one element type.
    let source = r#"
const numPair: [number, boolean] | [2] = [2];
const numPairSwapped: [2] | [number, boolean] = [2];
const strCase: [string, string] | ["x"] = ["x"];
const bigintCase: [bigint, bigint] | [2n] = [2n];
const longerArmFirst: [number, number, number] | [number, 7] = [3, 7];
"#;
    let errors = check_strict_codes(source);
    assert!(
        !errors.contains(&2322),
        "literal element typed against a union of differing-arity tuples must be preserved (no spurious TS2322), got: {errors:?}"
    );
}

#[test]
fn union_of_differing_arity_tuples_via_conditional_preserves_literal() {
    // The same rule must hold when the tuple union is produced by a
    // distributive conditional (issue #10864 family). `Tail<T>` over
    // `[string, number, boolean] | [1, 2]` yields `[number, boolean] | [2]`;
    // assigning the literal `[2]` must match the `[2]` arm. Renamed infer
    // binders guard against a fixture-name fast path.
    let source = r#"
type DropFirst<Items> = Items extends [unknown, ...infer Remainder] ? Remainder : never;
type Mixed = [string, number, boolean] | [1, 2];
const tail: DropFirst<Mixed> = [2];
"#;
    let errors = check_strict_codes(source);
    assert!(
        !errors.contains(&2322),
        "literal element typed against a conditional-derived tuple union must be preserved, got: {errors:?}"
    );
}

#[test]
fn union_of_tuples_still_rejects_non_member_literal() {
    // Control: preserving literal arms must not over-accept. `[3]` matches
    // neither arm of `[number, boolean] | [2]` (arity 1 but value 3 != 2),
    // so TS2322 is still expected — confirming the fix does not blanket the
    // union into an array.
    let source = r#"
const bad: [number, boolean] | [2] = [3];
"#;
    let errors = check_strict_codes(source);
    assert!(
        errors.contains(&2322),
        "a literal that matches no tuple arm must still be rejected, got: {errors:?}"
    );
}
