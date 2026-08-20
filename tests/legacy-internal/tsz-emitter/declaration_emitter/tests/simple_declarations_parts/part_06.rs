// Inferred object-literal return types must be indented relative to the
// emitter's current `indent_level`. A class method (or a namespaced function)
// nests its synthesized return shape one level deeper than a top-level
// function, matching tsc's declaration indentation. Regression coverage for the
// fix that bases `infer_fallback_type_text_at` on `self.indent_level` instead
// of a fixed depth of 0 (see declarationMapsMultifile emit parity).

#[test]
fn inferred_class_method_object_return_uses_member_relative_indent() {
    let output = emit_dts_with_usage_analysis(
        r#"
export class Foo {
    doThing(x: { a: number }) {
        return { b: x.a };
    }
}
"#,
    );

    // Method body lives at class-member indent (one level): the inferred object
    // return type's members sit at two levels (8 spaces) and the closing brace
    // at one level (4 spaces).
    assert!(
        output.contains("    }): {\n        b: number;\n    };"),
        "Expected inferred method object return type to nest one level deeper than the method: {output}"
    );
    // The bug emitted members at the base indent (4 spaces) with a column-0
    // closing brace; ensure that broken shape is gone.
    assert!(
        !output.contains("    }): {\n    b: number;\n};"),
        "Did not expect the inferred method object return type to ignore the member indent level: {output}"
    );
}

#[test]
fn inferred_namespaced_method_object_return_uses_deeper_indent() {
    // Rename every bound surface (namespace/class/method/parameter/property) to
    // prove the rule keys on structural nesting depth, not on identifier names.
    let output = emit_dts_with_usage_analysis(
        r#"
export namespace Outer {
    export class Widget {
        build(input: { width: number }) {
            return { size: input.width };
        }
    }
}
"#,
    );

    // namespace (1) -> class (2) -> method members (3 -> 12 spaces), closing (2 -> 8 spaces).
    assert!(
        output.contains("        }): {\n            size: number;\n        };"),
        "Expected namespaced method object return type to track the namespace+class indent depth: {output}"
    );
}

#[test]
fn inferred_method_nested_object_return_scales_recursively() {
    let output = emit_dts_with_usage_analysis(
        r#"
export class Box {
    pack(p: { weight: number }) {
        return { value: p.weight, meta: { tag: p.weight } };
    }
}
"#,
    );

    // The nested object literal inside the method return must indent one level
    // deeper again. Method return members sit at indent level 2 (8 spaces), so
    // the nested object's own members sit at level 3 (12 spaces) with its
    // closing brace back at level 2 (8 spaces).
    assert!(
        output.contains("        meta: {\n            tag: number;\n        };"),
        "Expected nested inferred object members to indent recursively relative to the method: {output}"
    );
}

#[test]
fn inferred_top_level_function_object_return_keeps_base_indent() {
    // Negative/control: a top-level function emits at indent level 0, so its
    // inferred object return type keeps the base (4-space members, column-0
    // closing brace). This proves the fix is no-op at the base level.
    let output = emit_dts_with_usage_analysis(
        r#"
export function make(x: { a: number }) {
    return { b: x.a };
}
"#,
    );

    assert!(
        output.contains("): {\n    b: number;\n};"),
        "Expected a top-level function inferred object return type to keep base indentation: {output}"
    );
    assert!(
        !output.contains("): {\n        b: number;\n    };"),
        "Did not expect a top-level function return type to be indented as a class member: {output}"
    );
}

#[test]
fn test_js_whole_prototype_object_keeps_function_namespace_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {number} len */
export function VectorSurface(len) {
    /** @type {number[]} */
    this.storage = new Array(len);
}
VectorSurface.prototype = {
    /** @param {VectorSurface} peer */
    dot(peer) { return peer.storage.length; }
};
"#,
    );

    assert_eq!(
        output.trim(),
        "/** @param {number} len */\nexport declare function VectorSurface(len: number): void;\nexport declare namespace VectorSurface {\n    var prototype: {\n        /** @param {VectorSurface} peer */\n        dot(peer: VectorSurface): any;\n    };\n}"
    );
}

#[test]
fn test_js_whole_prototype_base_of_rich_surface_keeps_class_projection() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @param {number} size */
export function PrototypeBase(size) {
    /** @type {number[]} */
    this.values = new Array(size);
}
PrototypeBase.prototype = {
    first() { return this.values[0]; },
    last() { return this.values[this.values.length - 1]; }
};

/** @param {number} size */
export function PrototypeDerived(size) {
    PrototypeBase.call(this, size);
    this.current = size;
}
PrototypeDerived.prototype = {
    __proto__: PrototypeBase,
    get current() { return this.values[0]; },
    /** @param {number} value */
    set current(value) { this.values[0] = value; }
};
"#,
    );

    assert!(
        output.contains("export function PrototypeBase(size: number): void;")
            && output.contains("export class PrototypeBase {")
            && output.contains("first(): any;")
            && output.contains("last(): any;"),
        "Expected the prototype base to retain its complete class projection: {output}"
    );
    assert!(
        output.contains("__proto__: typeof PrototypeBase;")
            && !output.contains("namespace PrototypeBase"),
        "Expected the connected prototype group to avoid a split namespace/class surface: {output}"
    );
}

#[test]
fn test_js_whole_prototype_reassigned_base_before_use_keeps_namespace_projection() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function BeforeBase() { this.value = 1; }
BeforeBase.prototype = { read() { return this.value; } };
function BeforeReplacement() {}
BeforeBase = BeforeReplacement;

export function BeforeDerived() {}
BeforeDerived.prototype = {
    __proto__: BeforeBase,
    get current() { return this.value; }
};
"#,
    );

    assert!(
        output.contains("export declare namespace BeforeBase {")
            && output.contains("read(): any;")
            && !output.contains("export class BeforeBase"),
        "Expected a base reassigned before the prototype use to retain the callable namespace projection: {output}"
    );
}

#[test]
fn test_js_whole_prototype_reassigned_base_after_use_keeps_namespace_projection() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function AfterBase() { this.value = 1; }
AfterBase.prototype = { read() { return this.value; } };
function AfterReplacement() {}

export function AfterDerived() {}
AfterDerived.prototype = {
    __proto__: AfterBase,
    get current() { return this.value; }
};
AfterBase = AfterReplacement;
"#,
    );

    assert!(
        output.contains("export declare namespace AfterBase {")
            && output.contains("read(): any;")
            && !output.contains("export class AfterBase"),
        "Expected a base reassigned after the prototype use to retain the callable namespace projection: {output}"
    );
}

#[test]
fn test_js_whole_prototype_namespace_preserves_intervening_jsdoc() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function OrderedSurface() {
    /** @type {number} */
    const localOnly = 1;
}
/** Kept with the exported marker. */
export const documentedMarker = 1;
OrderedSurface.prototype = {
    /** @param {number} value */
    apply(value) { return value; }
};
"#,
    );

    assert!(
        output.contains(
            "        /** @param {number} value */\n        apply(value: number): any;"
        ),
        "Expected only the prototype member's attached JSDoc in the namespace: {output}"
    );
    assert!(
        output.contains(
            "/** Kept with the exported marker. */\nexport const documentedMarker: 1;"
        ),
        "Expected out-of-order namespace emission to preserve intervening declaration comments: {output}"
    );
    assert!(
        !output.contains("@type {number}"),
        "Did not expect function-body JSDoc to leak into the declaration surface: {output}"
    );
}

#[test]
fn test_js_whole_prototype_namespace_can_rebase_to_earlier_assignment() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
EarlierSurface.prototype = {
    /** Earlier member documentation. */
    run() {}
};
export function EarlierSurface() {}
"#,
    );

    assert!(
        output.contains(
            "export declare namespace EarlierSurface {\n    var prototype: {\n        /** Earlier member documentation. */\n        run(): void;"
        ),
        "Expected the scoped comment cursor to rebase to an earlier prototype initializer: {output}"
    );
}

#[test]
fn test_js_whole_prototype_collector_matches_dot_and_bracket_access() {
    let source = r#"
function DotSurface() {}
DotSurface.prototype = {};
DotSurface.prototype = replacementSurface;
function BracketSurface() {}
BracketSurface["prototype"] = {};
module.exports.CommonSurface.prototype = {};
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let _ = emitter.emit(root);

    assert_eq!(
        emitter
            .js_prototype_assignments
            .get("DotSurface")
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        emitter
            .js_prototype_assignments
            .get("BracketSurface")
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        emitter
            .js_prototype_assignments
            .get("CommonSurface")
            .and_then(|initializers| initializers.first())
            .map(|initializer| initializer.receiver_is_commonjs),
        Some(true)
    );
}

#[test]
fn test_js_mixed_whole_prototype_assignments_keep_existing_fallback() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function MixedSurface() {}
MixedSurface.prototype = { method() {} };
MixedSurface.prototype = replacementSurface;
"#,
    );

    assert!(
        !output.contains("var prototype:"),
        "Did not expect one object initializer to hide another whole-prototype assignment: {output}"
    );
}

#[test]
fn test_js_mixed_direct_and_commonjs_prototype_receivers_keep_existing_fallback() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function MixedReceiverSurface() {}
module.exports.MixedReceiverSurface = MixedReceiverSurface;
MixedReceiverSurface.prototype = { directMethod() {} };
module.exports.MixedReceiverSurface.prototype = { commonMethod() {} };
"#,
    );

    assert!(
        output.contains("directMethod(): void;")
            && output.contains("commonMethod(): void;")
            && !output.contains("var prototype:"),
        "Expected mixed receiver aliases to retain the existing complete fallback: {output}"
    );
}

#[test]
fn test_js_renamed_commonjs_prototype_receiver_joins_local_fallback() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function LocalReceiverSurface() {}
module.exports.PublicReceiverSurface = LocalReceiverSurface;
LocalReceiverSurface.prototype = { directMethod() {} };
module.exports.PublicReceiverSurface.prototype = { commonMethod() {} };
"#,
    );

    assert!(
        output.contains("directMethod(): void;")
            && output.contains("commonMethod(): void;")
            && !output.contains("var prototype:"),
        "Expected a renamed CommonJS receiver to join its proven local fallback: {output}"
    );
}

#[test]
fn test_js_commonjs_prototype_receiver_tracks_latest_export_identity() {
    let source = r#"
function EarlierLocal() {}
function CurrentLocal() {}
module.exports.PublicSurface = EarlierLocal;
EarlierLocal.prototype = { earlierMethod() {} };
module.exports.PublicSurface = CurrentLocal;
module.exports.PublicSurface.prototype = { currentMethod() {} };
module.exports = {};
module.exports.PublicSurface.prototype = { detachedMethod() {} };
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    let earlier_assignments = emitter
        .js_prototype_assignments
        .get("EarlierLocal")
        .expect("missing direct assignment for the earlier local");
    assert_eq!(earlier_assignments.len(), 1);
    assert!(!earlier_assignments[0].receiver_is_commonjs);
    let current_assignments = emitter
        .js_prototype_assignments
        .get("CurrentLocal")
        .expect("missing CommonJS assignment for the current local");
    assert_eq!(current_assignments.len(), 1);
    assert!(
        current_assignments[0].receiver_is_commonjs
            && current_assignments[0].receiver_aliases_local
            && output.contains("currentMethod(): void;"),
        "Expected reassignment to attach the later prototype only to the current local: {output}"
    );
    let detached_assignments = emitter
        .js_prototype_assignments
        .get("PublicSurface")
        .expect("missing detached assignment after module.exports replacement");
    assert_eq!(detached_assignments.len(), 1);
    assert!(
        detached_assignments[0].receiver_is_commonjs
            && !detached_assignments[0].receiver_aliases_local,
        "Expected replacing module.exports to clear named export identity: {output}"
    );
}

#[test]
fn test_js_whole_prototype_with_other_alias_keeps_existing_fallback() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const extra = () => {};
export function AliasedSurface() {}
AliasedSurface.prototype = { method() {} };
AliasedSurface.extra = extra;
"#,
    );

    assert!(
        !output.contains("var prototype:"),
        "Did not expect the focused prototype projection to split another function alias into a second namespace: {output}"
    );
}

#[test]
fn test_js_whole_prototype_with_per_member_write_keeps_member_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function AugmentedSurface() {}
AugmentedSurface.prototype = { first() {} };
AugmentedSurface.prototype.second = function () {};
AugmentedSurface.prototype.value = 1;
AugmentedSurface.prototype["bracketed"] = function () {};
"#,
    );

    assert!(
        output.contains("second(): void;")
            && output.contains("value: number;")
            && output.contains("\"bracketed\"(): void;")
            && !output.contains("var prototype:"),
        "Expected the existing fallback to preserve direct prototype-member writes: {output}"
    );
}

#[test]
fn test_js_export_equals_whole_prototype_with_per_member_write_uses_cached_fallback() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
function ExportEqualsAugmented() {}
ExportEqualsAugmented.prototype = { first() {} };
ExportEqualsAugmented.prototype.second = function () {};
module.exports = ExportEqualsAugmented;
"#,
    );

    assert!(
        output.contains("second(): void;") && !output.contains("var prototype:"),
        "Expected the cached export-equals fallback to preserve a prototype-member write: {output}"
    );
}

#[test]
fn test_js_whole_prototype_with_prototype_named_alias_keeps_alias_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @typedef {string} prototype */
function AliasRoot() {}
AliasRoot.prototype = { method() {} };
module.exports = AliasRoot;
"#,
    );

    assert!(
        output.contains("type prototype = string;") && !output.contains("var prototype:"),
        "Expected a user-defined alias named prototype to retain the existing namespace fallback: {output}"
    );
}

#[test]
fn test_js_whole_prototype_with_late_bound_static_keeps_combined_fallback() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
const staticName = "extra";
export function ComputedSurface() {}
ComputedSurface.prototype = { method() {} };
ComputedSurface[staticName] = 1;
"#,
    );

    assert!(
        output.contains("export namespace ComputedSurface {\n    let extra: number;\n}")
            && !output.contains("var prototype:"),
        "Expected a computed static expando to retain the existing combined fallback: {output}"
    );
}

#[test]
fn test_js_whole_prototype_namespace_follows_jsdoc_overload_signatures() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/**
 * @overload
 * @param {string} value
 * @returns {void}
 */
/** @param {unknown} value */
export function OverloadedSurface(value) {}
OverloadedSurface.prototype = { method() {} };
"#,
    );

    assert!(
        output.contains("export declare function OverloadedSurface(value: string): void;")
            && output.contains(
                "export declare namespace OverloadedSurface {\n    var prototype: {\n        method(): void;"
            )
            && !output.contains("class OverloadedSurface"),
        "Expected JSDoc overload functions to retain the callable prototype namespace surface: {output}"
    );
}

#[test]
fn test_js_whole_prototype_namespace_follows_jsdoc_function_type_signature() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @typedef {(value: string) => void} SurfaceSignature */
/** @type {SurfaceSignature} */
export function TypedSurface(value) {}
TypedSurface.prototype = { method() {} };
"#,
    );

    assert!(
        output.contains("export declare function TypedSurface(value: string): void;")
            && output.contains(
                "export declare namespace TypedSurface {\n    var prototype: {\n        method(): void;"
            )
            && !output.contains("class TypedSurface"),
        "Expected JSDoc function-type signatures to retain the callable prototype namespace surface: {output}"
    );
}

#[test]
fn test_js_whole_prototype_namespace_covers_commonjs_and_local_surfaces() {
    let export_equals = emit_js_dts_with_usage_analysis(
        r#"
function ExportEqualsSurface() {}
ExportEqualsSurface.prototype = { method() {} };
module.exports = ExportEqualsSurface;
"#,
    );
    assert_eq!(
        export_equals.trim(),
        "export = ExportEqualsSurface;\ndeclare function ExportEqualsSurface(): void;\ndeclare namespace ExportEqualsSurface {\n    var prototype: {\n        method(): void;\n    };\n}"
    );

    let named_commonjs = emit_js_dts_with_usage_analysis(
        r#"
function LocalSurface() {}
LocalSurface.prototype = { method() {} };
exports.PublicSurface = LocalSurface;
"#,
    );
    assert!(
        named_commonjs.contains("export { LocalSurface as PublicSurface };")
            && named_commonjs.contains("declare function LocalSurface(): void;")
            && named_commonjs.contains(
                "declare namespace LocalSurface {\n    var prototype: {\n        method(): void;\n    };\n}"
            )
            && !named_commonjs.contains("class LocalSurface"),
        "Expected the CommonJS alias to retain a callable local prototype namespace: {named_commonjs}"
    );

    let local_script = emit_js_dts_with_usage_analysis(
        r#"
function ScriptSurface() {}
ScriptSurface.prototype = { method() {} };
"#,
    );
    assert_eq!(
        local_script.trim(),
        "declare function ScriptSurface(): void;\ndeclare namespace ScriptSurface {\n    var prototype: {\n        method(): void;\n    };\n}"
    );
}

#[test]
fn test_js_empty_bracket_prototype_object_keeps_function_surface() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function EmptySurface() {}
EmptySurface["prototype"] = {};
"#,
    );

    assert_eq!(
        output.trim(),
        "export declare function EmptySurface(): void;\nexport declare namespace EmptySurface {\n    var prototype: {};\n}"
    );
}

#[test]
fn test_js_per_member_prototype_write_does_not_synthesize_class() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export function PerMemberSurface() {}
PerMemberSurface.prototype.method = function () {};
"#,
    );

    assert_eq!(
        output.trim(),
        "export function PerMemberSurface(): void;"
    );
}
