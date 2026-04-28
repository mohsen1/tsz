use super::super::core::*;

#[test]
fn test_named_default_import_from_export_equals_class_uses_default_import_rules() {
    let files = [
        (
            "/mod.d.ts",
            r#"
declare class Example {
    static answer(): number;
}
export = Example;
"#,
        ),
        (
            "/index.ts",
            r#"
import { default as Example } from "./mod";
Example.answer();
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/index.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            es_module_interop: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "Expected named default import from export= class to use default-import semantics without extra import diagnostics. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Importing a named member from an `export = X` module where the imported
/// name matches `X`'s declaration name should NOT emit TS2459 ("declares
/// locally but is not exported") or TS2460 ("declares locally but is exported
/// as 'export='"). The TS2497 + TS2616/TS2595/TS2597 path already reports the
/// import-style mismatch; the additional TS2459/TS2460 is a duplicate.
#[test]
fn test_named_import_of_export_equals_target_skips_ts2459_ts2460() {
    let files = [
        (
            "/a.ts",
            r#"
class Foo {}
export = Foo;
"#,
        ),
        (
            "/b.ts",
            r#"
import { Foo } from "./a";
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/b.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            es_module_interop: true,
            ..CheckerOptions::default()
        },
    );

    let codes: Vec<u32> = diagnostics.iter().map(|(c, _)| *c).collect();
    assert!(
        !codes.contains(&2459) && !codes.contains(&2460),
        "TS2459/TS2460 must not fire for named import of export-equals target. Codes: {codes:?}"
    );
    // The TS2616 (or TS2595/TS2597 depending on module/file kind) is the
    // canonical diagnostic for this mismatch and must still fire.
    assert!(
        codes.contains(&2616) || codes.contains(&2595) || codes.contains(&2597),
        "Expected TS2616/TS2595/TS2597 for named import of export-equals target. Codes: {codes:?}"
    );
}

#[test]
fn test_named_default_reexport_from_export_equals_class_uses_default_import_rules() {
    let files = [
        (
            "/mod.d.ts",
            r#"
declare class Example {
    static answer(): number;
}
export = Example;
"#,
        ),
        (
            "/index.ts",
            r#"
export { default, default as Alias } from "./mod";
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/index.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            es_module_interop: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "Expected named default re-exports from export= class to use default-import semantics without extra export diagnostics. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_named_default_reexport_from_export_equals_alias_chain_uses_default_import_rules() {
    let files = [
        (
            "/mod.ts",
            r#"
declare function fun(): void;
export default fun;
"#,
        ),
        (
            "/a.ts",
            r#"
import mod = require("./mod");
export = mod;
"#,
        ),
        (
            "/b.ts",
            r#"
export { default } from "./a";
export { default as def } from "./a";
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options_and_import_reporting(
        &files,
        "/b.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            es_module_interop: true,
            ..CheckerOptions::default()
        },
        true,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected named default re-exports from export= alias chain to use default-import semantics without extra export diagnostics. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2344_concrete_type_ref_constraint() {
    let diagnostics = compile_and_get_diagnostics(
        r"
interface A {
    a: number;
}

interface B {
    b: string;
}

interface C<T extends A> {
    x: T;
}

declare var v1: C<A>;
declare var v2: C<B>;
        ",
    );
    let ts2344_count = diagnostics.iter().filter(|(code, _)| *code == 2344).count();
    assert_eq!(
        ts2344_count, 1,
        "Should emit exactly 1 TS2344 for C<B>. Actual: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2314_for_merged_function_interface_same_name() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Mixin {
    mixinMethod(): void;
}
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass): TBaseClass & (abstract new (...args: any) => Mixin) {
    abstract class MixinClass extends baseClass implements Mixin {
        mixinMethod() {}
    }
    return MixinClass;
}
        "#,
    );
    let ts2314: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2314).collect();
    assert!(
        ts2314.is_empty(),
        "Should NOT emit TS2314 for merged function+interface 'Mixin'. Got: {ts2314:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2314_for_merged_function_type_alias_same_name() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function Mixin<TBase extends {new (...args: any[]): {}}>(Base: TBase) {
    return class extends Base {};
}
type Mixin = any;
type Crashes = number & Mixin;
        "#,
    );
    let ts2314: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2314).collect();
    assert!(
        ts2314.is_empty(),
        "Should NOT emit TS2314 for merged function+type alias 'Mixin'. Got: {ts2314:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2314_for_type_alias_function_type_shared_symbol_shape() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function Mixin<TBase extends {new (...args: any[]): {}}>(Base: TBase) {
    return class extends Base {
    };
}

type Mixin = ReturnTypeOf<typeof Mixin>

type ReturnTypeOf<V> = V extends (...args: any[])=>infer R ? R : never;

type Crashes = number & Mixin;
"#,
    );
    let ts2314: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2314).collect();
    assert!(
        ts2314.is_empty(),
        "Should NOT emit TS2314 for the typeAliasFunctionTypeSharedSymbol shape. Got: {ts2314:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2314_for_mixin_abstract_classes_shape() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface Mixin {
    mixinMethod(): void;
}

function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass): TBaseClass & (abstract new (...args: any) => Mixin) {
    abstract class MixinClass extends baseClass implements Mixin {
        mixinMethod() {
        }
    }
    return MixinClass;
}

class ConcreteBase {
    baseMethod() {}
}

abstract class AbstractBase {
    abstract abstractBaseMethod(): void;
}

class DerivedFromConcrete extends Mixin(ConcreteBase) {
}

const wasConcrete = new DerivedFromConcrete();
wasConcrete.baseMethod();
wasConcrete.mixinMethod();

class DerivedFromAbstract extends Mixin(AbstractBase) {
    abstractBaseMethod() {}
}

const wasAbstract = new DerivedFromAbstract();
wasAbstract.abstractBaseMethod();
wasAbstract.mixinMethod();
"#,
        CheckerOptions {
            target: ScriptTarget::ESNext,
            emit_declarations: true,
            ..Default::default()
        },
    );
    let ts2314: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2314).collect();
    assert!(
        ts2314.is_empty(),
        "Should NOT emit TS2314 for the mixinAbstractClasses shape. Got: {ts2314:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2314_for_qualified_merged_namespace_member_type_reference() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
namespace N {
    export namespace Collection {
        export namespace Keyed {}
        export function Keyed<K, V>(collection: Iterable<[K, V]>): Collection.Keyed<K, V>;
        export function Keyed<V>(obj: { [key: string]: V }): Collection.Keyed<string, V>;
        export interface Keyed<K, V> {}
    }
}

type Works = N.Collection.Keyed<string, number>;
"#,
    );
    let ts2314: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2314)
        .collect();
    assert!(
        ts2314.is_empty(),
        "Qualified merged namespace members in type position should not validate against the value-side arity. Got: {ts2314:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2314_for_type_position_merged_function_interface_symbol() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Collection<K, V> {}
declare function Collection<I extends Collection<any, any>>(collection: I): I;
declare function Collection<T>(collection: Iterable<T>): Collection<number, T>;

type Works = Collection<any, any>;
"#,
    );
    let ts2314: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2314)
        .collect();
    assert!(
        ts2314.is_empty(),
        "Merged function/interface symbols should use the interface arity in type position. Got: {ts2314:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_no_false_ts2314_for_unqualified_namespace_merged_function_interface_symbol() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare namespace Immutable {
    export function Collection<I extends Collection<any, any>>(collection: I): I;
    export function Collection<T>(collection: Iterable<T>): Collection<number, T>;
    export interface Collection<K, V> {}
    export interface Uses {
        value: Collection<any, any>;
    }
}
"#,
    );
    let ts2314: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2314)
        .collect();
    assert!(
        ts2314.is_empty(),
        "Unqualified names inside a namespace body should use the merged type-side arity. Got: {ts2314:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_ambient_nested_namespace_merge_does_not_emit_false_ts2395_or_ts2434() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare namespace N {
    export namespace Seq {
        namespace Indexed {
            function of<T>(...values: Array<T>): Seq.Indexed<T>;
        }
        export function Indexed(): Seq.Indexed<any>;
        export function Indexed<T>(): Seq.Indexed<T>;
        export function Indexed<T>(collection: Iterable<T>): Seq.Indexed<T>;
        export interface Indexed<T> extends Seq<number, T> {}
    }
    export interface Seq<K, V> {}
}
"#,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2395 || *code == 2434)
        .collect();
    assert!(
        relevant.is_empty(),
        "Ambient namespace merges should not emit TS2395/TS2434. Got: {relevant:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_polymorphic_this_in_indexed_interface_extension_emits_ts2430() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Collection<K, V> { toSeq(): this; }
interface Seq<K, V> extends Collection<K, V> {}
interface Indexed<T> extends Collection<number, T> { toSeq(): SeqIndexed<T>; }
interface SeqIndexed<T> extends Seq<number, T>, Indexed<T> { toSeq(): this; }
"#,
    );
    let ts2430: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2430)
        .collect();
    assert!(
        !ts2430.is_empty(),
        "Polymorphic this mismatch should emit TS2430 for Indexed<T>. All: {diagnostics:?}"
    );
}

#[test]
fn test_polymorphic_this_in_set_interface_extension_emits_ts2430() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Collection<K, V> { toSeq(): this; }
interface Seq<K, V> extends Collection<K, V> {}
interface SetCollection<T> extends Collection<never, T> { toSeq(): SeqSet<T>; }
interface SeqSet<T> extends Seq<never, T>, SetCollection<T> { toSeq(): this; }
"#,
    );
    let ts2430: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2430)
        .collect();
    assert!(
        !ts2430.is_empty(),
        "Polymorphic this mismatch should emit TS2430 for SetCollection<T>. All: {diagnostics:?}"
    );
}

#[test]
fn test_exported_arrow_function_expando_assignment_no_false_ts2339() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
export interface Point {
    readonly x: number;
    readonly y: number;
}

export interface Rect<p extends Point> {
    readonly a: p;
    readonly b: p;
}

export const Point = (x: number, y: number): Point => ({ x, y });
export const Rect = <p extends Point>(a: p, b: p): Rect<p> => ({ a, b });

Point.zero = (): Point => Point(0, 0);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            emit_declarations: true,
            ..Default::default()
        },
    );
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected exported arrow-function expando assignment to avoid TS2339. Got: {ts2339:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_relative_module_augmentation_namespace_import_member_resolves_in_type_position() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "/backbone.d.ts",
                r#"
declare namespace Backbone {
    interface Model<T extends object = any, TQuery = any, TOptions = any> {}
}
export = Backbone;
"#,
            ),
            (
                "/augment.d.ts",
                r#"
import * as Backbone from "./backbone";
declare module "./backbone" {
    interface ModelWithCache extends Backbone.Model<any, any, any> {
        cache: boolean;
    }
}
"#,
            ),
            (
                "/index.ts",
                r#"
import * as Backbone from "./backbone";
import "./augment";

let model: Backbone.ModelWithCache;
model.cache;
"#,
            ),
        ],
        "/index.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2503 || *code == 2694),
        "Expected module augmentation member on namespace import to resolve in type position, got: {diagnostics:#?}"
    );
}

/// TS2719: when two different types share the same display name (e.g. a type
/// parameter `T` shadowing an interface `T`), the checker should emit "Two
/// different types with this name exist, but they are unrelated" instead of
/// the generic TS2322 "Type 'T' is not assignable to type 'T'."
#[test]
fn test_ts2719_incompatible_assignment_of_identically_named_types() {
    let code = r#"
interface T { }
declare const a: T;
class Foo<T> {
    x: T;
    fn() {
        this.x = a;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        code,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error(&diagnostics, 2719),
        "Expected TS2719 for identically named but different types, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2345_same_named_namespace_classes_use_qualified_pair() {
    let code = r#"
declare namespace N {
    export class Token {
        kind: "n";
    }
}
declare namespace M {
    export class Token {
        kind: "m";
    }
}

declare const n: N.Token;
function acceptsM(value: M.Token): void {}
acceptsM(n);
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        code,
        CheckerOptions {
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for same-named namespace class argument, got: {diagnostics:?}"
    );
    let message = diagnostic_message(&diagnostics, 2345).unwrap_or("");
    assert!(
        message.contains("N.Token") && message.contains("M.Token"),
        "Expected namespace-qualified Token names in TS2345 message. Message: {message}"
    );
}

#[test]
#[ignore = "merged backlog: needs nested property mismatch to preserve qualified related TS2345"]
fn test_ts2345_related_property_mismatch_uses_qualified_pair() {
    let code = r#"
declare namespace N {
    export class Token {
        kind: "n";
    }
}
declare namespace M {
    export class Token {
        kind: "m";
    }
}

interface Source {
    value: N.Token;
}
interface Target {
    value: M.Token;
}

declare const source: Source;
function acceptsTarget(value: Target): void {}
acceptsTarget(source);
"#;

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        code,
        CheckerOptions {
            no_lib: true,
            ..CheckerOptions::default()
        },
    );
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 2345)
        .unwrap_or_else(|| panic!("Expected TS2345, got: {diagnostics:?}"));
    let related_messages: Vec<&str> = diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.as_str())
        .collect();

    assert!(
        related_messages
            .iter()
            .any(|message| message.contains("Types of property 'value' are incompatible.")),
        "Expected property mismatch related info. Related: {related_messages:#?}"
    );
    assert!(
        related_messages
            .iter()
            .any(|message| message.contains("Type 'N.Token' is not assignable to type 'M.Token'.")),
        "Expected qualified Token pair in related info. Related: {related_messages:#?}"
    );
}

/// Union of tuple types with `.filter()` should contextually type callback parameters
/// as the union of element types, matching tsc behavior.
///
/// When calling `.filter()` on a union of tuple types like `[Fizz] | readonly [Buzz?]`,
/// the callback parameter `item` should be `Fizz | Buzz | undefined` (the union of
/// all tuple element types). This is because filter has mixed generic/non-generic
/// overloads, and tsc computes a combined callback with unioned parameters.
///
/// The fix merges per-member callback function types into a single combined callable
/// with unioned parameter types, rather than creating a union of callbacks that
/// `get_parameter_type` can't extract parameter types from.
#[test]
fn test_union_of_tuple_types_filter_callback_contextual_typing() {
    let source = r#"
interface Fizz {
    id: number;
    fizz: string;
}

interface Buzz {
    id: number;
    buzz: string;
}

([] as [Fizz] | readonly [Buzz?]).filter(item => item?.id < 5);
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    // tsc expects TS18048: 'item.id' is possibly 'undefined'.
    // This verifies that `item` is correctly typed as `Fizz | Buzz | undefined`
    // (not `any`), because `item?.id` returns `number | undefined` and
    // `(number | undefined) < 5` triggers the nullish comparison check.
    assert!(
        has_error(&diagnostics, 18048),
        "Expected TS18048 for 'item.id' possibly undefined in tuple union filter callback. \
         item should be Fizz | Buzz | undefined, not any. Got: {diagnostics:?}"
    );
}

/// forEach on a union of array types should still give `any` for the callback parameter.
///
/// Unlike filter (which has mixed generic/non-generic overloads), forEach has a single
/// overload. tsc gives `any` for the callback parameter in this case, and we should too.
#[test]
fn test_union_of_array_types_foreach_callback_stays_any() {
    let source = r#"
interface Fizz { id: number; }
interface Buzz { id: number; }

([] as Fizz[] | Buzz[]).forEach(item => {
    const x: never = item;
});
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    // item should be `any` (tsc behavior for forEach on union of arrays).
    // `any` is not assignable to `never`, so TS2322 should fire.
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for any->never assignment. forEach on union should give `any` for item. Got: {diagnostics:?}"
    );
    // Verify the error message mentions `any`, confirming item is `any` not a specific type.
    let msg = diagnostic_message(&diagnostics, 2322).unwrap_or("");
    assert!(
        msg.contains("'any'"),
        "Expected error message to contain 'any', got: {msg}"
    );
}

// ── TS2413: Unconstrained type parameter not assignable to index sig ──

#[test]
fn test_ts2413_unconstrained_type_param_not_assignable_to_number_index() {
    // TS2413: 'number' index type 'T' is not assignable to 'string' index type 'number'.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f<T>() {
    var b: {
        [x: string]: number;
        [x: number]: T;
    };
}
"#,
    );
    assert!(
        has_error(&diagnostics, 2413),
        "Expected TS2413 for unconstrained T vs number index signature. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2413_constrained_type_param_assignable_no_error() {
    // When T extends number, T IS assignable to number, so no TS2413.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f<T extends number>() {
    var b: {
        [x: string]: number;
        [x: number]: T;
    };
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2413),
        "Should NOT emit TS2413 when T extends number. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2413_unconstrained_type_param_vs_object_with_lib() {
    if !lib_files_available() {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Skipping: lib files not available");
        }
        return;
    }
    // TS2413: 'number' index type 'T' is not assignable to 'string' index type 'Object'.
    // An unconstrained type parameter T should NOT be assignable to Object because
    // T could be instantiated with null/undefined/void.
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
function other<T>(arg: T) {
    var b: {
        [x: string]: Object;
        [x: number]: T;
    };
}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_error(&diagnostics, 2413),
        "Expected TS2413 for unconstrained T vs Object index signature. Got: {diagnostics:?}"
    );
}

#[test]
fn test_instanceof_conflicting_properties_narrows_to_never() {
    // When an interface and a class have a common property with incompatible types
    // (e.g., x: string vs x: number), instanceof narrowing should produce `never`
    // because the intersection is uninhabitable.
    let opts = CheckerOptions {
        strict_null_checks: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
class Foo { x: number = 1; y: number = 2; }
interface Bar { x: string; }
declare var b: Bar;

if (b instanceof Foo) {
    let test: never = b; // should work: b is narrowed to never (conflicting x types)
}
"#,
        opts,
    );
    // If b is correctly narrowed to never, assigning to `never` should not error.
    assert!(
        !has_error(&diagnostics, 2322),
        "Expected no TS2322: b should be narrowed to never due to conflicting property types. Got: {diagnostics:?}"
    );
}

#[test]
fn test_instanceof_interface_narrows_to_never_when_incompatible() {
    // From conformance/expressions/typeGuards/typeGuardsWithInstanceOf.ts
    // When `result` is `I = { global: string }` and we check `result instanceof RegExp`,
    // the true branch should narrow to `never` because I.global is `string` but
    // RegExp.global is `boolean` — the types are structurally incompatible.
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
interface I { global: string; }
var result!: I;
var result2!: I;

if (!(result instanceof RegExp)) {
    result = result2;
} else if (!result.global) {
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    // tsc expects: TS2339 "Property 'global' does not exist on type 'never'."
    let ts2339_count = diagnostics.iter().filter(|(c, _)| *c == 2339).count();
    assert!(
        ts2339_count >= 1,
        "Expected at least 1 TS2339 for accessing property on never after instanceof narrowing. Got: {diagnostics:?}"
    );
}

#[test]
#[ignore = "CFA flow merge after instanceof doesn't preserve the class type in the union - separate issue from instanceof narrowing"]
fn test_instanceof_class_narrows_union_at_merge_point() {
    // From conformance/expressions/typeGuards/typeGuardsWithInstanceOf.ts (#31155 repro)
    // After `if (v instanceof C) { ... }`, the type of `v` should be
    // `C | (Validator & Partial<OnChanges>)` at the merge point, so accessing
    // `v.onChanges` should error because `onChanges` doesn't exist on `C`.
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "test.ts",
        r#"
interface OnChanges {
    onChanges(changes: Record<string, unknown>): void
}
interface Validator {
    validate(): null | Record<string, unknown>;
}

class C {
    validate() {
        return {}
    }
}

function foo() {
    let v: Validator & Partial<OnChanges> = null as any;
    if (v instanceof C) {
        v
    }
    v

    if (v.onChanges) {
        v.onChanges({});
    }
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    // tsc expects: two TS2339 errors for v.onChanges on lines accessing it
    let ts2339_msgs: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        ts2339_msgs.len() >= 2,
        "Expected at least 2 TS2339 for 'onChanges' on union type. Got: {diagnostics:?}"
    );
}

/// TS2576: Accessing a static member on a class instance through an interface
/// property should emit "Did you mean to access the static member?" when the
/// class is merged with a namespace.
///
/// Root cause: `symbol_member_is_type_only` incorrectly classified class static
/// methods as type-only (METHOD flag without FUNCTION flag), which caused the
/// `namespace_has_type_only_member` check to short-circuit property access
/// resolution before reaching the TS2576 diagnostic path.
#[test]
fn test_ts2576_class_namespace_merge_via_interface_property() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Sammy {
   foo() { return "hi"; }
  static bar() {
    return -1;
   }
}
namespace Sammy {
    export var x = 1;
}
interface JQueryStatic {
    sammy: Sammy;
}
declare var $: JQueryStatic;
var r3 = $.sammy.bar();
var r4 = $.sammy.x;
"#,
    );

    // $.sammy.bar() should emit TS2576 — `bar` is a static member
    let has_ts2576 = diagnostics
        .iter()
        .any(|(code, msg)| *code == 2576 && msg.contains("bar") && msg.contains("Sammy"));
    assert!(
        has_ts2576,
        "Expected TS2576 for accessing static member 'bar' on Sammy instance via $.sammy. Got: {diagnostics:#?}"
    );

    // $.sammy.x should emit TS2339 — `x` is a namespace export, not on the instance
    let has_ts2339_x = diagnostics
        .iter()
        .any(|(code, msg)| *code == 2339 && msg.contains("'x'"));
    assert!(
        has_ts2339_x,
        "Expected TS2339 for 'x' not existing on Sammy instance type. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_export_type_star_as_namespace_emits_ts1362_in_value_context() {
    // exportNamespace10.ts: `export type * as ns from "./a"` makes ns type-only.
    // Using `ns` in value context (e.g., `new ns.A()`) should emit TS1362.
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            ("a.ts", "export class A {}\n"),
            ("b.ts", "export type * as ns from './a';\n"),
            (
                "c.ts",
                "import { ns } from './b';\nlet _: ns.A = new ns.A();\n",
            ),
        ],
        "c.ts",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    let has_ts1362 = diagnostics.iter().any(|(code, _)| *code == 1362);
    assert!(
        has_ts1362,
        "Expected TS1362 for type-only namespace export used as value. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_export_type_star_collides_with_value_star_reexport() {
    let files = [
        ("a.ts", "export type A = number;\n"),
        ("b.ts", "export type * from './a';\n"),
        (
            "c.ts",
            "import { A } from './b';\nconst A = 1;\nexport { A };\n",
        ),
        ("d.ts", "import { A } from './c';\nA;\ntype _ = A;\n"),
        ("e.ts", "export const A = 1;\n"),
        ("f.ts", "export * from './e';\nexport type * from './a';\n"),
        ("g.ts", "import { A } from './f';\nA;\ntype _ = A;\n"),
    ];
    let options = CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        no_lib: true,
        ..CheckerOptions::default()
    };

    let d_diagnostics =
        compile_named_files_get_diagnostics_with_options(&files, "d.ts", options.clone());
    assert!(
        d_diagnostics.iter().all(|(code, _)| *code != 2749),
        "`export {{ A }}` must preserve the imported type meaning when local A also has a value. Got: {d_diagnostics:#?}"
    );

    let f_diagnostics =
        compile_named_files_get_diagnostics_with_options(&files, "f.ts", options.clone());

    assert!(
        f_diagnostics
            .iter()
            .any(|(code, msg)| { *code == 2308 && msg.contains("\"./e\"") && msg.contains("'A'") }),
        "Expected TS2308 for type-only star export colliding with value star export. Got: {f_diagnostics:#?}"
    );

    let g_diagnostics = compile_named_files_get_diagnostics_with_options(&files, "g.ts", options);
    assert!(
        g_diagnostics
            .iter()
            .any(|(code, msg)| { *code == 2749 && msg.contains("'A' refers to a value") }),
        "Expected TS2749 follow-on after ambiguous value/type star re-export. Got: {g_diagnostics:#?}"
    );
}

#[test]
fn test_export_equals_typeof_import_namespace_import_exposes_referenced_named_exports() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "pkg/index.d.ts",
                r#"
declare const pluginImportX: typeof import("./lib/index");
export = pluginImportX;
"#,
            ),
            (
                "pkg/lib/index.d.ts",
                r#"
interface PluginConfig {
    parser?: string | null;
}
declare const configs: {
    "stage-0": PluginConfig;
};
declare const _default: {
    configs: {
        "stage-0": PluginConfig;
    };
};
export default _default;
export { configs };
"#,
            ),
            (
                "main.ts",
                r#"
import * as pluginImportX from "./pkg/index";
const cfg = pluginImportX.configs["stage-0"];
interface Plugin {
  configs?: Record<string, { parser: string | null }>;
}
const p: Plugin = pluginImportX;
"#,
            ),
        ],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2020,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2339 || *code == 2551),
        "Namespace import should expose `configs` from `export = typeof import(...)`; got: {diagnostics:#?}"
    );

    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();
    assert_eq!(
        ts2322_messages.len(),
        1,
        "Expected one assignability mismatch for Plugin assignment. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !ts2322_messages[0].contains("_default"),
        "Namespace surface should not leak internal `_default` alias. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_typeof_import_nested_export_equals_qualifier_includes_cross_file_path() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[
            (
                "foo2.d.ts",
                r#"
export namespace bar {
    export const existing: number;
}
"#,
            ),
            (
                "foo.d.ts",
                r#"
declare const x: typeof import("./foo2");
export = x;
"#,
            ),
            (
                "main.ts",
                r#"
type T = typeof import("./foo").bar.missing;
"#,
            ),
        ],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2020,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    let ts2694_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2694)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2694_messages.len(),
        1,
        "Expected one TS2694 for missing nested export. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2694_messages[0]
            .contains("Namespace '\"foo\".bar.export=' has no exported member 'missing'."),
        "Expected nested cross-file qualifier path in TS2694. Actual diagnostics: {diagnostics:#?}"
    );
}

// ============================================================
// TS1294: erasableSyntaxOnly
// ============================================================

#[test]
fn test_ts1294_erasable_syntax_only_enums() {
    let options = CheckerOptions {
        erasable_syntax_only: true,
        ..Default::default()
    };

    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
enum NotLegal { A = 1 }
declare enum Legal { B = 1 }
"#,
        options,
    );

    let ts1294_count = diagnostics.iter().filter(|(c, _)| *c == 1294).count();
    assert_eq!(
        ts1294_count, 1,
        "Expected exactly 1 TS1294 for non-ambient enum, got {ts1294_count}. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts1294_erasable_syntax_only_parameter_properties() {
    let options = CheckerOptions {
        erasable_syntax_only: true,
        ..Default::default()
    };

    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class Foo {
    constructor(public x: number) {}
}
"#,
        options,
    );

    let ts1294_count = diagnostics.iter().filter(|(c, _)| *c == 1294).count();
    assert_eq!(
        ts1294_count, 1,
        "Expected exactly 1 TS1294 for parameter property, got {ts1294_count}. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts1294_erasable_syntax_only_instantiated_namespace() {
    let options = CheckerOptions {
        erasable_syntax_only: true,
        ..Default::default()
    };

    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
namespace Instantiated { export const x = 1; }
namespace NotInstantiated { export interface I {} }
"#,
        options,
    );

    let ts1294_count = diagnostics.iter().filter(|(c, _)| *c == 1294).count();
    assert_eq!(
        ts1294_count, 1,
        "Expected exactly 1 TS1294 for instantiated namespace, got {ts1294_count}. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts1294_erasable_syntax_only_import_export_equals_in_cts() {
    let options = CheckerOptions {
        erasable_syntax_only: true,
        ..Default::default()
    };

    let diagnostics = compile_and_get_diagnostics_named(
        "commonjs.cts",
        r#"
import foo = require("./other");
export = foo;
"#,
        options,
    );

    let ts1294_count = diagnostics.iter().filter(|(c, _)| *c == 1294).count();
    assert_eq!(
        ts1294_count, 2,
        "Expected 2 TS1294 for import= and export= in .cts file, got {ts1294_count}. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts1294_erasable_syntax_only_not_enabled() {
    // When erasableSyntaxOnly is false (default), no TS1294 should be emitted
    let diagnostics = compile_and_get_diagnostics(
        r#"
enum OK { A = 1 }
class Foo { constructor(public x: number) {} }
namespace NS { export const x = 1; }
"#,
    );

    let ts1294_count = diagnostics.iter().filter(|(c, _)| *c == 1294).count();
    assert_eq!(
        ts1294_count, 0,
        "Expected 0 TS1294 when erasableSyntaxOnly is disabled. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_setter_parameter_type_constraint_ts2344_in_interface_and_object_literal() {
    // divergentAccessorsTypes6.ts: tsc eagerly checks setter parameter type annotations
    // even when the setter is never observed (getter returns early in type computation).
    // `Fail<string>` where `type Fail<T extends never> = T` should emit TS2344 because
    // `string` does not satisfy `never`.
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Fail<T extends never> = T;

// Interface setter — parameter type must be validated
interface I1 {
    get x(): number;
    set x(value: Fail<string>);
}

// Object literal setter — parameter type must be validated
const o1 = {
    get x(): number { return 0; },
    set x(value: Fail<string>) {}
}
"#,
    );

    let ts2344_count = diagnostics.iter().filter(|(c, _)| *c == 2344).count();
    assert_eq!(
        ts2344_count, 2,
        "Expected 2 TS2344 errors (one for interface setter, one for object literal setter). Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_setter_parameter_type_constraint_ts2344_with_lib_files() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        r#"
export {};
type Fail<T extends never> = T;
interface I1 {
    get x(): number;
    set x(value: Fail<string>);
}
const o1 = {
    get x(): number { return 0; },
    set x(value: Fail<string>) {}
}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts2344_count = diagnostics.iter().filter(|(c, _)| *c == 2344).count();
    assert_eq!(
        ts2344_count, 2,
        "Expected 2 TS2344 errors with lib files. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Regression test: generic interface variance must reject assignments that
/// are checked AFTER a successful assignment in the opposite direction.
///
/// When `a = b` (Promise<Bar> <: Promise<Foo>) succeeds covariant check,
/// the subsequent `b = a` (Promise<Foo> <: Promise<Bar>) must still be rejected.
/// Previously, the coinductive cycle detection in structural comparison
/// incorrectly assumed compatibility after the first successful check cached
/// intermediate results.
#[test]
fn test_generic_variance_order_independent_rejection() {
    let source = r#"
interface MyPromise<T> {
    then<U>(cb: (x: T) => MyPromise<U>): MyPromise<U>;
}

interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: MyPromise<Foo>;
declare var b: MyPromise<Bar>;
a = b;
b = a;
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for 'b = a' (MyPromise<Foo> not assignable to MyPromise<Bar>). Diagnostics: {diagnostics:#?}"
    );
}

/// Same as above but with non-recursive interface to isolate the recursion aspect.
#[test]
fn test_generic_variance_order_independent_non_recursive() {
    let source = r#"
interface Box<T> {
    get(): T;
}

interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: Box<Foo>;
declare var b: Box<Bar>;
a = b;
b = a;
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for 'b = a' (Box<Foo> not assignable to Box<Bar>). Diagnostics: {diagnostics:#?}"
    );
}

/// T in direct method parameter — requires flow analysis to preserve Application
/// types for annotated variables. Structural comparison is correct (contravariant
/// params pass), so only variance-based checking can reject this.
#[test]
fn test_generic_variance_method_param_order_independent() {
    let source = r#"
interface Setter<T> {
    set(value: T): void;
}

interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: Setter<Foo>;
declare var b: Setter<Bar>;
a = b;
b = a;
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for 'b = a' (Setter<Foo> not assignable to Setter<Bar>). Diagnostics: {diagnostics:#?}"
    );
}

/// Sanity check: generic interface variance rejects a single bad assignment.
#[test]
fn test_generic_variance_simple_rejection() {
    let source = r#"
interface MyPromise<T> {
    then<U>(cb: (x: T) => MyPromise<U>): MyPromise<U>;
}

interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: MyPromise<Foo>;
declare var b: MyPromise<Bar>;
b = a;
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for 'b = a' alone. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_parameter_function_return_type_not_equivalent() {
    // Function types with different type parameter return types should NOT be assignable.
    // This is the typeParameterArgumentEquivalence conformance test family.

    // () => T is NOT assignable to () => U (and vice versa)
    let d = compile_and_get_diagnostics(
        "function f<T,U>() { var x!: () => U; var y!: () => T; x = y; y = x; }",
    );
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Expected 2 TS2322 for () => T vs () => U, got: {d:?}"
    );

    // (a: T) => boolean is NOT assignable to (a: U) => boolean (and vice versa)
    let d = compile_and_get_diagnostics(
        "function f<T,U>() { var x!: (a: U) => boolean; var y!: (a: T) => boolean; x = y; y = x; }",
    );
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Expected 2 TS2322 for (a:T) vs (a:U), got: {d:?}"
    );

    // But () => T IS assignable to () => T (same type parameter)
    let d =
        compile_and_get_diagnostics("function f<T>() { var x!: () => T; var y!: () => T; x = y; }");
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Same type param should be assignable, got: {d:?}"
    );
}

/// TS2416: class method `f(a: T): void` is not compatible with interface
/// property `f: (a: { a: number }) => void` because T extends { a: string }
/// and { a: number } is not assignable to { a: string }.
#[test]
fn test_generic_type_with_non_generic_base_mismatch_ts2416() {
    let source = r#"
interface I {
    f: (a: { a: number }) => void
}
class X<T extends { a: string }> implements I {
    f(a: T): void { }
}
var x = new X<{ a: string }>();
var i: I = x;
"#;
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_function_types: true,
        ..CheckerOptions::default()
    };
    let d = compile_and_get_diagnostics_with_options(source, options);
    let ts2416_count = d.iter().filter(|(c, _)| *c == 2416).count();
    assert!(
        ts2416_count >= 1,
        "Expected TS2416 for property 'f' incompatibility, got: {d:?}"
    );
    // Should also emit TS2322 for the assignment `var i: I = x`
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert!(
        ts2322_count >= 1,
        "Expected TS2322 for incompatible assignment, got: {d:?}"
    );
}

#[test]
fn test_type_parameter_nested_function_return_type_not_equivalent() {
    // Nested function types with different type parameter return types should NOT be assignable.
    // This is the typeParameterArgumentEquivalence5 conformance test.

    // () => (item: any) => T is NOT assignable to () => (item: any) => U (and vice versa)
    let d = compile_and_get_diagnostics(
        "function foo<T,U>() { var x!: () => (item: any) => U; var y!: () => (item: any) => T; x = y; y = x; }",
    );
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Expected 2 TS2322 for () => (item: any) => T vs () => (item: any) => U, got: {d:?}"
    );

    // But same type parameter through nesting IS assignable
    let d = compile_and_get_diagnostics(
        "function foo<T>() { var x!: () => (item: any) => T; var y!: () => (item: any) => T; x = y; }",
    );
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Same type param through nesting should be assignable, got: {d:?}"
    );
}

/// TS7005 should fire for variables inside `declare namespace` that lack
/// a type annotation, when `noImplicitAny` is enabled.
/// Regression test for: conformance/implicitAnyAmbients.ts
#[test]
fn test_ts7005_emitted_for_ambient_namespace_variables() {
    let source = r#"
declare namespace m {
    var x;
    var y: any;
    namespace n {
        var z;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let ts7005_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7005)
        .collect();

    // Should emit TS7005 for `var x;` and `var z;` (no type annotation)
    // but NOT for `var y: any;` (has explicit type annotation)
    assert_eq!(
        ts7005_diags.len(),
        2,
        "Expected exactly 2 TS7005 diagnostics (for `x` and `z`), got: {ts7005_diags:?}"
    );

    // Verify the messages reference the correct variable names
    let messages: Vec<&str> = ts7005_diags.iter().map(|(_, m)| m.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("'x'")),
        "Expected TS7005 for variable 'x', got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("'z'")),
        "Expected TS7005 for variable 'z', got: {messages:?}"
    );

    // `var y: any` should NOT trigger TS7005 — it has an explicit type annotation
    assert!(
        !messages.iter().any(|m| m.contains("'y'")),
        "var y: any should NOT trigger TS7005, got: {messages:?}"
    );
}

/// TS7005 should NOT fire for ambient namespace variables in .d.ts files.
#[test]
fn test_ts7005_not_emitted_for_dts_ambient_namespace_variables() {
    let source = r#"
declare namespace m {
    var x;
}
"#;
    let diagnostics = compile_and_get_diagnostics_named(
        "test.d.ts",
        source,
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let ts7005_count = diagnostics.iter().filter(|(code, _)| *code == 7005).count();
    assert_eq!(
        ts7005_count, 0,
        "TS7005 should not fire in .d.ts files, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts7005_not_emitted_for_for_of_const_binding_with_inferred_element_type() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
for (const value of [1, 2, 3]) {
    value.toFixed();
}
"#,
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts7005_count = diagnostics.iter().filter(|(code, _)| *code == 7005).count();
    assert_eq!(
        ts7005_count, 0,
        "Loop element inference should suppress TS7005 for `for...of` bindings, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts7005_emitted_for_plain_const_without_initializer() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        "const value",
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7005),
        "Plain `const` declarations without initializers should still report TS7005, got: {diagnostics:?}"
    );
}

/// Class methods with overloads that reference class type parameters in their
/// return types should not produce false TS2394 errors. The manual signature
/// lowering used for overload compatibility doesn't have class type params in
/// scope, producing Error types in the lowered signature. The checker must
/// detect these buried Error types and fall back to `get_type_of_node`.
#[test]
fn test_class_method_overload_with_class_type_param_in_return_no_false_ts2394() {
    // Simple case: class type param T directly in return type
    let d = compile_and_get_diagnostics(
        r#"
interface Vector<T> { _brand: T }
class Foo<T> {
    test(): Vector<T>;
    test(): Vector<any> {
        return undefined as any;
    }
}
"#,
    );
    let ts2394_count = d.iter().filter(|(c, _)| *c == 2394).count();
    assert_eq!(
        ts2394_count, 0,
        "Vector<T> should be compatible with Vector<any> in overload check, got: {d:?}"
    );

    // Complex case: class type param in conditional type (Exclude) within tuple return
    let d = compile_and_get_diagnostics(
        r#"
type Exclude2<T, U> = T extends U ? never : T;
interface Seq<T> { tail(): Opt<Seq<T>>; }
class Opt<T> { toVector(): Vector<T> { return undefined as any; } }
class Vector<T> implements Seq<T> {
    tail(): Opt<Vector<T>> { return undefined as any; }
    partition2<U extends T>(predicate:(v:T)=>v is U): [Vector<U>,Vector<Exclude2<T, U>>];
    partition2(predicate:(x:T)=>boolean): [Vector<T>,Vector<T>];
    partition2<U extends T>(predicate:(v:T)=>boolean): [Vector<U>,Vector<any>] {
        return undefined as any;
    }
}
"#,
    );
    let ts2394_count = d.iter().filter(|(c, _)| *c == 2394).count();
    assert_eq!(
        ts2394_count, 0,
        "Overload with Exclude<T,U> return should be compatible with Vector<any>, got: {d:?}"
    );

    // Case with deferred conditional type in return
    let d = compile_and_get_diagnostics(
        r#"
type MyCond<T> = T extends string ? number : boolean;
interface Vector<T> { _brand: T }
class Foo<T> {
    test(): Vector<MyCond<T>>;
    test(): Vector<any> {
        return undefined as any;
    }
}
"#,
    );
    let ts2394_count = d.iter().filter(|(c, _)| *c == 2394).count();
    assert_eq!(
        ts2394_count, 0,
        "Vector<MyCond<T>> should be compatible with Vector<any> in overload check, got: {d:?}"
    );
}

#[test]
fn test_generic_identity_callback_arg_emits_ts2345() {
    // When a generic function like `<T>(value: T) => T` is passed as an argument
    // to a parameter expecting a non-generic callback, the display target must use
    // the TARGET's return type (not the source's instantiated return) so the
    // re-check in check_argument_assignable_or_report doesn't suppress the error.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function identity<T>(value: T): T;
declare function take(cb: (value: string | number) => boolean): void;
take(identity);
"#,
    );
    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for generic identity not assignable to callback, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_identity_callback_valid_case_no_error() {
    // When the generic callback IS compatible, no error should be emitted.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function identity<T>(value: T): T;
declare function take(cb: (value: string) => string): void;
take(identity);
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Should NOT emit TS2345 when generic callback is compatible, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_rest_arguments_iife_emits_ts8029() {
    let source = r#"
self.importScripts = (function (importScripts) {
    /**
     * @param {...unknown} rest
     */
    return function () {
        return importScripts.apply(this, arguments);
    };
})(importScripts);
"#;

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "index.js",
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts8029 = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 8029)
        .expect("expected TS8029");
    assert!(
        ts8029
            .message_text
            .contains("It would match 'arguments' if it had an array type."),
        "Expected TS8029 to mention implicit arguments-array matching. Diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|diagnostic| diagnostic.code != 8024),
        "Expected the implicit-arguments case to upgrade to TS8029 instead of TS8024. Diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_intersection_with_primitive_preserves_property_conflict() {
    // Regression test for commonTypeIntersection conformance failure.
    // { __typename?: 'TypeTwo' } & string should NOT be assignable to
    // { __typename?: 'TypeOne' } & string because the __typename properties conflict.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare let x1: { __typename?: 'TypeTwo' } & { a: boolean };
let y1: { __typename?: 'TypeOne' } & { a: boolean } = x1;

declare let x2: { __typename?: 'TypeTwo' } & string;
let y2: { __typename?: 'TypeOne' } & string = x2;
"#,
    );
    let ts2322_count = diagnostics.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Expected 2 TS2322 errors (one for each incompatible intersection assignment), got {ts2322_count}.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2395_still_fires_for_non_ambient_mixed_export_declarations() {
    // TS2395 should still fire when non-ambient declarations mix exported and non-exported
    let diagnostics = compile_and_get_diagnostics(
        r#"
export namespace Foo {
    export function bar(): void;
}
namespace Foo {
    export function bar(): void;
}
"#,
    );
    let ts2395 = diagnostics.iter().filter(|(c, _)| *c == 2395).count();
    assert!(
        ts2395 > 0,
        "Expected TS2395 for mixed export declarations in non-ambient context. Got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2395_suppressed_in_declare_namespace_mixed_export_merge() {
    // complexRecursiveCollections.ts pattern: inside `declare namespace`, a merged symbol
    // may have both exported and non-exported declarations (e.g., namespace + function + interface).
    // tsc does NOT emit TS2395 for ambient declarations.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare namespace Immutable {
    export namespace List {
        function isList(maybeList: any): boolean;
        function of<T>(...values: Array<T>): List<T>;
    }
    export function List(): List<any>;
    export function List<T>(): List<T>;
    export interface List<T> {
        set(index: number, value: T): List<T>;
    }
}
"#,
    );
    let ts2395 = diagnostics.iter().filter(|(c, _)| *c == 2395).count();
    assert_eq!(
        ts2395, 0,
        "Should not emit TS2395 in declare namespace context. Got: {diagnostics:#?}"
    );
}

/// Object literal union normalization must include empty objects.
///
/// When computing the best common type of `[{ a: 1, b: 2 }, { a: "abc" }, {}]`,
/// tsc normalizes the `{}` member to `{ a?: undefined; b?: undefined }` so that
/// property access on the resulting union works without TS2339. Previously, tsz
/// skipped empty objects during normalization, causing false TS2339 errors after
/// freshness was stripped for variable declarations.
#[test]
fn test_object_literal_normalization_empty_object_no_ts2339() {
    let options = CheckerOptions {
        strict: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
let a2 = [{ a: 1, b: 2 }, { a: "abc" }, {}][0];
a2.a;
a2.b;
"#,
        options,
    );
    let ts2339 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339)
        .collect::<Vec<_>>();
    assert!(
        ts2339.is_empty(),
        "Should not emit TS2339 for property access on normalized object literal union with empty object member. Got: {ts2339:#?}"
    );
}

#[test]
fn test_implement_array_interface_ts2416_not_ts2420() {
    // tsc emits TS2416 for the 'every' property because the class declares
    // a single-overload 'every' that is incompatible with Array's overloaded
    // 'every' (which includes a type-predicate overload).
    // We should NOT emit TS2420 (class incorrectly implements interface) at
    // the class level — only TS2416 at the specific property.
    let source = r#"
declare class MyArray<T> implements Array<T> {
    toString(): string;
    toLocaleString(): string;
    concat<U extends T[]>(...items: U[]): T[];
    concat(...items: T[]): T[];
    join(separator?: string): string;
    pop(): T;
    push(...items: T[]): number;
    reverse(): T[];
    shift(): T;
    slice(start?: number, end?: number): T[];
    sort(compareFn?: (a: T, b: T) => number): this;
    splice(start: number): T[];
    splice(start: number, deleteCount: number, ...items: T[]): T[];
    unshift(...items: T[]): number;

    indexOf(searchElement: T, fromIndex?: number): number;
    lastIndexOf(searchElement: T, fromIndex?: number): number;
    every(callbackfn: (value: T, index: number, array: T[]) => boolean, thisArg?: any): boolean;
    some(callbackfn: (value: T, index: number, array: T[]) => boolean, thisArg?: any): boolean;
    forEach(callbackfn: (value: T, index: number, array: T[]) => void, thisArg?: any): void;
    map<U>(callbackfn: (value: T, index: number, array: T[]) => U, thisArg?: any): U[];
    filter(callbackfn: (value: T, index: number, array: T[]) => boolean, thisArg?: any): T[];
    reduce(callbackfn: (previousValue: T, currentValue: T, currentIndex: number, array: T[]) => T, initialValue?: T): T;
    reduce<U>(callbackfn: (previousValue: U, currentValue: T, currentIndex: number, array: T[]) => U, initialValue: U): U;
    reduceRight(callbackfn: (previousValue: T, currentValue: T, currentIndex: number, array: T[]) => T, initialValue?: T): T;
    reduceRight<U>(callbackfn: (previousValue: U, currentValue: T, currentIndex: number, array: T[]) => U, initialValue: U): U;

    length: number;

    [n: number]: T;
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_lib(source);

    let ts2416 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2416)
        .collect::<Vec<_>>();
    let ts2420 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2420)
        .collect::<Vec<_>>();

    assert!(
        !ts2416.is_empty(),
        "Expected TS2416 for 'every' property incompatibility. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2420.is_empty(),
        "Should NOT emit TS2420 for this case. Only TS2416 expected. Got TS2420: {ts2420:#?}"
    );
}

/// Regression test: when an object literal property uses `this` as the value
/// and the `this` type is not assignable to the expected property type, the
/// elaboration should emit TS2322 ("Type X is not assignable to type Y"),
/// not TS2741 ("Property X is missing..."). This matches tsc behavior in
/// `elaborateElementwise` for `this` keyword expressions.
///
/// Based on conformance test `fuzzy.ts`.
#[test]
fn test_elaboration_emits_ts2322_not_ts2741_for_this_keyword_property() {
    let source = r#"
interface I {
    a(): void;
    b(): void;
}
interface R {
    oneI: I;
}
class C implements I {
    a(): void {}
    doesntWork(): R {
        return { oneI: this };
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics(source);

    assert!(
        has_error(&diagnostics, 2420),
        "Expected TS2420 for class incorrectly implementing interface. Got: {diagnostics:#?}"
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for property type mismatch in object literal elaboration. Got: {diagnostics:#?}"
    );

    let ts2741 = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2741)
        .collect::<Vec<_>>();
    assert!(
        ts2741.is_empty(),
        "Should NOT emit TS2741 for `this` keyword in elaboration — tsc uses TS2322. Got: {ts2741:#?}"
    );
}

/// Regression test: var redeclaration with generic call whose argument is incompatible
/// should emit TS2403 when the call infers a different return type than the prior declaration.
///
/// tsc does NOT propagate prior var declaration types as contextual type for call expressions.
/// Without this fix, the contextual type `Function[]` from the first declaration would cause
/// `stringMapToArray(numberMap)` to infer `T=Function`, returning `Function[]` (matching the
/// prior declaration) and suppressing the TS2403 error. With the fix, `T` is correctly
/// inferred as `unknown` from the failed argument matching, returning `unknown[]`.
#[test]
#[ignore = "merged backlog: needs tsc-compatible failed generic-call inference to also surface TS2403"]
fn test_ts2403_var_redecl_generic_call_no_contextual_from_prior_decl() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface NumberMap<T> {
    [index: number]: T;
}
interface StringMap<T> {
    [index: string]: T;
}
declare function stringMapToArray<T>(object: StringMap<T>): T[];
declare var numberMap: NumberMap<string>;
var v1: string[];
var v1 = stringMapToArray(numberMap);
"#,
    );
    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for NumberMap<string> not assignable to StringMap<unknown>. Got: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2403),
        "Expected TS2403 for var redeclaration type mismatch (string[] vs unknown[]). Got: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2403_optional_param_var_redeclaration_in_constructor() {
    let source = r#"
class C {
    constructor(options?: number) {
        var options = (options || 0);
    }
}
"#;
    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2403),
        "Expected TS2403 for var re-declaring optional parameter with different type.\nActual: {diagnostics:#?}"
    );
}
