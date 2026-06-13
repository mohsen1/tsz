use super::misc_features::parse_test_source;
use super::*;

#[test]
fn test_export_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
    export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };
    "#,
    );

    assert!(
        output.contains(
            r#"export type { RequireInterface } from "pkg" with { "resolution-mode": "require" };"#
        ),
        "Expected export type attributes to be preserved: {output}"
    );
}

#[test]
fn test_export_json_attributes_are_stripped_from_declarations() {
    let output = emit_dts(r#"export { default as data } from "./dep.json" with { type: "json" };"#);

    assert!(
        output.contains(r#"export { default as data } from "./dep.json";"#),
        "Expected JSON export attribute to be stripped from declaration output: {output}"
    );
    assert!(
        !output.contains("with {"),
        "Did not expect non-resolution-mode attributes in declaration output: {output}"
    );
}

#[test]
fn test_inferred_printer_reduces_conditional_alias_applications() {
    use tsz_solver::types::{ConditionalType, TypeParamInfo};

    let (parser, _root) = parse_test_source("");

    let mut foreign_parser = ParserState::new(
        "lib.d.ts".to_string(),
        "type Select<T> = T extends string ? 1 : 2;".to_string(),
    );
    let _ = foreign_parser.parse_source_file();
    let alias_decl = foreign_parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION).then_some(NodeIndex(idx as u32))
        })
        .expect("missing conditional type alias declaration");

    let mut binder = BinderState::new();
    let select_sym = binder
        .symbols
        .alloc(symbol_flags::TYPE_ALIAS, "Select".to_string());
    binder
        .symbols
        .get_mut(select_sym)
        .expect("missing synthetic conditional alias symbol")
        .declarations
        .push(alias_decl);

    let interner = TypeInterner::new();
    let type_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: tsz_solver::types::TypeParamOrigin::User,
    };
    let cond = interner.conditional(ConditionalType {
        check_type: interner.type_param(type_param),
        extends_type: TypeId::STRING,
        true_type: interner.literal_number(1.0),
        false_type: interner.literal_number(2.0),
        is_distributive: false,
    });

    let def_id = DefId(99);
    let app = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(def_id, select_sym);
    type_cache.def_types.insert(def_id.0, cond);
    type_cache
        .def_type_params
        .insert(def_id.0, vec![type_param]);

    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    assert_eq!(emitter.print_type_id(app), "Select<string>");
    assert_eq!(emitter.print_type_id_for_inferred_declaration(app), "1");
}

#[test]
fn test_asserted_import_type_with_resolution_mode_attributes_is_preserved() {
    let output = emit_dts(
        r#"
    export type LocalInterface = import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface;
    export const value = (null as any as import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface);
    "#,
    );

    assert!(
        output.contains(
            r#"export type LocalInterface = import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface;"#
        ),
        "Expected import type attributes to be formatted canonically in type aliases: {output}"
    );
    assert!(
        output.contains(
            r#"export declare const value: import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface;"#
        ),
        "Expected asserted import type with attributes to be preserved on exported values: {output}"
    );
}

#[test]
fn test_import_type_non_string_argument_formats_object_as_type_literal() {
    let output = emit_dts(r#"export const x: import({x: 12}) = undefined as any;"#);

    assert!(
        output.contains("export declare const x: import({\n    x: 12;\n});"),
        "Expected non-string import type argument to be formatted as a type literal: {output}"
    );
}

#[test]
fn test_invalid_resolution_mode_attribute_is_dropped_and_unused_mixed_import_is_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
    import type { RequireInterface } from "pkg" with { "resolution-mode": "foobar" };
    import { ImportInterface } from "pkg" with { "resolution-mode": "import" };
    import { type RequireInterface as Req, RequireInterface as Req2 } from "pkg" with { "resolution-mode": "require" };

    export interface LocalInterface extends RequireInterface, ImportInterface {}
    "#,
    );

    assert!(
        output.contains(r#"import type { RequireInterface } from "pkg";"#),
        "Expected invalid resolution-mode attribute to be dropped: {output}"
    );
    assert!(
        output.contains(
            r#"import { ImportInterface } from "pkg" with { "resolution-mode": "import" };"#
        ),
        "Expected valid resolution-mode attribute to be preserved: {output}"
    );
    assert!(
        !output.contains("Req2"),
        "Expected unused mixed import bindings to be elided: {output}"
    );
}

// =============================================================================
// 29. Namespace export as
// =============================================================================

#[test]
fn test_star_export_as_namespace() {
    let output = emit_dts(r#"export * as utils from "./utils";"#);
    assert!(
        output.contains("export * as utils from"),
        "Expected namespace re-export: {output}"
    );
}

// =============================================================================
// 30. Asserts modifier in type predicate
// =============================================================================

#[test]
fn test_assertion_function() {
    let output = emit_dts(
        r#"
    export function assertDefined(val: unknown): asserts val {
        if (val == null) throw new Error();
    }
    "#,
    );
    assert!(
        output.contains("asserts val"),
        "Expected asserts modifier: {output}"
    );
}

#[test]
fn test_setter_parameter_asserts_this_predicate_is_rescued_from_source() {
    let output = emit_dts(
        r#"
    declare class Wat {
        set p2(x: asserts this is string);
    }
    "#,
    );

    assert!(
        output.contains("set p2(x: asserts this is string);"),
        "Expected setter parameter asserts predicate to be preserved: {output}"
    );
}

#[test]
fn test_const_identity_call_preserves_numeric_literal_initializer() {
    let output = emit_dts(
        r#"
function id<T>(x: T): T {
    return x;
}

const value = id(123);
"#,
    );

    assert!(
        output.contains("declare const value = 123;"),
        "Expected const identity call to preserve numeric literal initializer: {output}"
    );
}

#[test]
fn test_const_identity_call_preserves_negative_numeric_literal_initializer() {
    let output = emit_dts(
        r#"
function id<T>(x: T): T {
    return x;
}

const value = id(-123);
"#,
    );

    assert!(
        output.contains("declare const value = -123;"),
        "Expected const identity call to preserve negative numeric literal initializer: {output}"
    );
}

// =============================================================================
// 31. Multiple variable declarations on one line
// =============================================================================

#[test]
fn test_multiple_variable_declarators() {
    let output = emit_dts("export var x: number, y: string;");
    assert!(
        output.contains("x: number"),
        "Expected first variable: {output}"
    );
    assert!(
        output.contains("y: string"),
        "Expected second variable: {output}"
    );
}

#[test]
fn test_grouped_let_declarator_preserves_null_initializer_type() {
    let output = emit_dts(r#"let l9 = 0, l10: string = "", l11 = null;"#);
    assert!(
        output.contains("declare let l9: number, l10: string, l11: null;"),
        "Expected grouped let null initializer to emit null: {output}"
    );

    let const_output = emit_dts("const c = null;");
    assert!(
        const_output.contains("declare const c: any;"),
        "Expected const null initializer to keep tsc-compatible any: {const_output}"
    );
}

#[test]
fn test_type_only_same_name_interface_reference_does_not_emit_local_value_dependency() {
    let output = emit_dts_with_usage_analysis(
        r#"
export interface Component {
    play(): void;
}

declare function createComponent(): void;
const Component = createComponent();

export type ComponentDefinition = Partial<Component>;
"#,
    );

    assert!(
        output.contains("export type ComponentDefinition = Partial<Component>;"),
        "Expected exported type alias to remain: {output}"
    );
    assert!(
        !output.contains("declare const Component"),
        "Did not expect type-only Component reference to emit local const: {output}"
    );
}

#[test]
fn test_const_shadowing_non_exported_type_alias_emits_value_declaration() {
    // Regression for genericContextualTypes1: in a script-mode file (no
    // imports/exports) a `const fn: fn = …` whose name shadows a
    // non-exported `type fn = …` must still be emitted as `declare const`.
    // The earlier behavior treated the value-side const as "type-only
    // exported" because the shared symbol carried a type-alias declaration,
    // even though that type alias itself was not exported.
    let output = emit_dts_with_usage_analysis(
        r#"
type fn = <A>(a: A) => A;
const fn: fn = a => a;
"#,
    );
    assert!(
        output.contains("type fn = <A>(a: A) => A;"),
        "Expected type alias to remain: {output}"
    );
    assert!(
        output.contains("declare const fn: fn;"),
        "Expected value-side const shadowing the non-exported type alias to be emitted: {output}"
    );
}

#[test]
fn test_top_level_export_import_alias_preferred_over_qualified_target() {
    // Regression for internalAliasClassInsideTopLevelModuleWithExport:
    // when `export import xc = x.c;` is at the file root, references to the
    // class instance type should be emitted using the alias `xc`, not the
    // canonical target `x.c`. The alias-target rewrite previously kicked in
    // unconditionally for every exported import alias, so the printer's
    // correct `xc` output was being clobbered into `x.c`. Top-level aliases
    // are always in scope wherever the d.ts is consumed, so the rewrite
    // should only canonicalize aliases declared inside a namespace where
    // the local short name might not be reachable from an outer reference.
    let output = emit_dts_with_usage_analysis(
        r#"
export namespace x {
    export class c {
        foo(a: number) {
            return a;
        }
    }
}

export import xc = x.c;
export var cProp = new xc();
"#,
    );
    assert!(
        output.contains("export declare var cProp: xc;"),
        "Expected top-level export import alias to be preferred over its qualified target: {output}"
    );
}

#[test]
fn test_js_named_export_function_emitted_at_unfold_position_not_hoisted() {
    // Regression for nodeModulesAllowJsGeneratedNameCollisions: when a JS
    // function declaration's name appears in a folded `export { foo }`
    // statement, the unfold path emits `export function foo(): ...` at the
    // export statement's source position. Hoisting the same function to the
    // top of the file would emit it twice (once hoisted, once unfolded) and
    // also reorder it before sibling inline-exported declarations like
    // `export const __esModule = false`.
    let output = emit_js_dts_with_usage_analysis(
        r#"
function require() {}
const exports = {};
class Object {}
export const __esModule = false;
export {require, exports, Object};
"#,
    );
    assert_eq!(
        output.matches("export function require(): void;").count(),
        1,
        "Expected `export function require(): void;` to be emitted exactly once: {output}"
    );
    let esmodule_pos = output
        .find("export const __esModule")
        .expect("__esModule line missing");
    let require_pos = output
        .find("export function require")
        .expect("require line missing");
    assert!(
        esmodule_pos < require_pos,
        "Expected `__esModule` to be emitted before `require` (matching the source order of inline + folded exports): {output}"
    );
}

#[test]
fn test_export_assignment_keeps_uninitialized_value_declaration() {
    // Regression for privacyCheckExportAssignmentOnExportedGenericInterface1:
    // a `var X: T;` (no initializer, with type annotation) whose only public
    // API consumer is `export = X` was being filtered out by the
    // initializer-only-dependency check, because that check only looked at
    // `export { X }` specifiers and did not recognize commonjs
    // `export = X` as an exporter of the value-side name.
    let output = emit_dts_with_usage_analysis(
        r#"
namespace Foo {
    export interface A<T> {
    }
}
interface Foo<T> {
}
var Foo: new () => Foo.A<Foo<string>>;
export = Foo;
"#,
    );
    assert!(
        output.contains("declare var Foo:"),
        "Expected `declare var Foo` to be emitted when `export = Foo` is the consumer: {output}"
    );
    assert!(
        output.contains("export = Foo;"),
        "Expected the export assignment to be preserved: {output}"
    );
}

#[test]
fn test_inferred_const_initializer_call_preserves_local_alias() {
    // Regression for #3755: declaration emit was dropping a local type alias
    // that an `export const` *only* references through the inferred type of
    // its call-expression initializer. The emitted .d.ts referenced the
    // alias but never declared it, producing invalid output.
    let output = emit_dts_with_usage_analysis(
        r#"
type Box = { value: number };
function make(): Box { return { value: 1 }; }
export const item = make();
"#,
    );
    assert!(
        output.contains("type Box ="),
        "Expected the local `type Box` to be retained when `export const item = make()` \
         depends on it through the callee's declared return-type annotation: {output}"
    );
    assert!(
        output.contains("export declare const item: Box"),
        "Expected the inferred const to keep its alias-named annotation: {output}"
    );
}

#[test]
fn test_export_default_identifier_keeps_ambient_value_declaration() {
    // Regression for uniqueSymbolPropertyDeclarationEmit: a `declare const X`
    // (no initializer, with a value-side type annotation) whose only public
    // API consumer is `export default X` was being filtered out by the
    // initializer-only-dependency check. The check's name-export lookup
    // only considered `EXPORT_SPECIFIER` and `EXPORT_ASSIGNMENT` nodes;
    // tsz parses `export default X` as an `EXPORT_DECLARATION` with
    // `is_default_export: true` and the identifier in `export_clause`,
    // which neither path matched.
    let output = emit_dts_with_usage_analysis(
        r#"
declare const Op: {
  readonly or: unique symbol;
};

export default Op;
"#,
    );
    assert!(
        output.contains("declare const Op:"),
        "Expected `declare const Op` to be emitted when `export default Op` is the consumer: {output}"
    );
    assert!(
        output.contains("export default Op;"),
        "Expected the default export to be preserved: {output}"
    );
}
