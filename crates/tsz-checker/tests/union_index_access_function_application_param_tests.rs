//! Regression tests for `evaluate_union` simplification incorrectly collapsing
//! union members that are functions whose parameters are
//! `Application` / `Lazy` types.
//!
//! Background: `simplify_union_members` runs the `SubtypeChecker` with
//! `bypass_evaluation = true`. With that flag, `Application` types nested
//! inside function parameters are not expanded to their structural body,
//! so two distinct generic instantiations (e.g. `Foo<any>` vs `Bar<any>`)
//! used as parameter types could be incorrectly considered subtypes via the
//! cached subtype path, collapsing
//! `((e: Foo<any>) => void) | ((e: Bar<any>) => void)` to a single function.
//!
//! `is_complex_type` must therefore mark such function members (and the
//! object types that carry them as properties) as complex so the union
//! reduction is skipped. The conformance test
//! `signatureCombiningRestParameters3` and the realistic patterns below
//! all rely on this behaviour.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ScriptTarget;

fn get_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| diag.code)
    .collect()
}

#[test]
fn union_of_functions_with_application_params_is_preserved() {
    // `Pass<F1 | F2>` distributes to `F1 | F2`. Calling that union with one
    // member's argument must intersect parameters and emit TS2345.
    let source = r#"
interface A<O = any> { x: number; }
interface B<O = any> { y: string; }

type F1 = (x: A<any>) => void;
type F2 = (x: B<any>) => void;

type Pass<T> = T extends (...args: any) => any ? T : never;
type PU = Pass<F1 | F2>;

declare const fu: PU;
declare const objA: A<any>;
fu(objA);
"#;

    let codes = get_codes(source);
    assert!(
        codes.contains(&2345),
        "expected TS2345 from intersected parameter union, got {codes:?}"
    );
}

#[test]
fn index_access_on_union_with_optional_nullable_function_property_is_preserved() {
    // `(I1 | I2)["f"]` where `f` is the `(... => ...) | null` shape (optional
    // + nullable union) used by the `signatureCombiningRestParameters3`
    // conformance test. The Object-level simplification was incorrectly
    // collapsing I1 and I2 because `bypass_evaluation=true` hides the
    // structural difference between the two function-typed members.
    //
    // We avoid lib types here (`Record`, `NonNullable`) because the test
    // harness disables the lib context.
    let source = r#"
declare class M<O = any> { x: number; }
declare class N<O = any> { y: string; }

interface I1 {
  f?: ((e: M<any>) => void) | null;
}
interface I2 {
  f?: ((e: N<any>) => void) | null;
}

declare const fu: (I1 | I2)["f"];
declare const m: M<any>;
if (fu) {
  fu(m);
}
"#;

    let codes = get_codes(source);
    assert!(
        codes.contains(&2345),
        "expected TS2345 from intersected parameter union via indexed access, got {codes:?}"
    );
}

#[test]
fn signature_combining_rest_parameters_3_minimized_emits_ts2345() {
    // Mirrors the conformance fixture
    // `compiler/signatureCombiningRestParameters3.ts`. Distributes
    // `RemoveThis` over `AnyConfig["extendMarkSchema"]`. The narrowed
    // function union must intersect its parameter to `Mark<any> & Node<any>`,
    // producing TS2345 on the call.
    let source = r#"
interface ExtensionConfig<Options = any> {
  extendMarkSchema?:
    | ((this: { name: string; options: Options; }, extension: Mark) => Record<string, any>)
    | null;
}

declare class Extension<Options = any> {
  type: string;
  name: string;
  parent: Extension | null;
  child: Extension | null;
  options: Options;
  config: ExtensionConfig;
}

declare class Node<Options = any> {
  type: string;
  name: string;
  parent: Node | null;
  child: Node | null;
  options: Options;
}

interface NodeConfig<Options = any> {
  extendMarkSchema?:
    | ((this: { name: string; options: Options; }, extension: Node) => Record<string, any>)
    | null;
}

declare class Mark<Options = any> {
  options: Options;
  config: MarkConfig;
}

interface MarkConfig<Options = any> {
  extendMarkSchema?:
    | ((this: { name: string; options: Options; }, extension: Mark) => Record<string, any>)
    | null;
}

type AnyConfig = ExtensionConfig | NodeConfig | MarkConfig;
type AnyExtension = Extension | Node | Mark;

declare const e: AnyExtension;

type RemoveThis<T> = T extends (...args: any) => any
  ? (...args: Parameters<T>) => ReturnType<T>
  : T;

declare function getExtensionField<T = any>(
  extension: AnyExtension,
  field: string,
): RemoveThis<T>;

const extendMarkSchema = getExtensionField<AnyConfig["extendMarkSchema"]>(e, "extendMarkSchema");

declare const extension: Mark<any>;

if (extendMarkSchema) {
  extendMarkSchema(extension);
}

export {};
"#;

    let codes = get_codes(source);
    assert!(
        codes.contains(&2345),
        "signatureCombiningRestParameters3 must emit TS2345, got {codes:?}"
    );
}

#[test]
fn alternate_iteration_var_name_does_not_change_invariant() {
    // Ensure the fix is structural, not tied to `T`/`P`/etc. Re-running with
    // a different conditional iteration parameter name must still produce the
    // diagnostic.
    let source = r#"
interface A<X = any> { x: number; }
interface B<X = any> { y: string; }

type F1 = (x: A<any>) => void;
type F2 = (x: B<any>) => void;

type Pass<U> = U extends (...args: any) => any ? U : never;
type PU = Pass<F1 | F2>;

declare const fu: PU;
declare const objB: B<any>;
fu(objB);
"#;

    let codes = get_codes(source);
    assert!(
        codes.contains(&2345),
        "expected TS2345 with alternate type-parameter name, got {codes:?}"
    );
}

#[test]
fn non_generic_param_classes_still_intersect_call_parameters() {
    // Sanity check: pre-existing behaviour for non-generic parameter types
    // must not regress.
    let source = r#"
declare class A { x: number; }
declare class B { y: string; }

type F1 = (x: A) => void;
type F2 = (x: B) => void;

type Pass<T> = T extends (...args: any) => any ? T : never;
type PU = Pass<F1 | F2>;

declare const fu: PU;
declare const objA: A;
fu(objA);
"#;

    let codes = get_codes(source);
    assert!(
        codes.contains(&2345),
        "non-generic union should still intersect parameters, got {codes:?}"
    );
}
