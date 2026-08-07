// =================================================================
// Error-type sentinel display (#15359)
//
// tsc models a failed-resolution type as `errorType` — an `Any`-flagged
// intrinsic whose internal name is `"error"` — but its printer always
// renders it as `any`. tsz mirrors the architecture with a distinct
// `TypeId::ERROR` sentinel (which prevents any-poisoning), so the sentinel
// keeps its identity while the diagnostic formatter must render it `any`
// in every structural position it can occupy. The `NONE` placeholder
// (`TypeId::NONE`) shares `TypeData::Error` and renders `any` too.
//
// (Standalone `TypeId::ERROR` rendering is covered by `format_error_type`
// and `format_all_primitive_type_ids`; this shard covers the structural
// positions and the `NONE` / application-collapse render sites.)
// =================================================================

#[test]
fn none_placeholder_renders_as_any() {
    // `TypeId::NONE` shares `TypeData::Error` (reached via the key-dispatch
    // arm, not the `TypeId::ERROR` fast path); it must render `any` too.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(TypeId::NONE), "any");
}

#[test]
fn error_sentinel_renders_as_any_array_element() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let arr = db.array(TypeId::ERROR);
    assert_eq!(fmt.format(arr), "any[]");
}

#[test]
fn error_sentinel_renders_as_any_function_return() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::ERROR,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert_eq!(fmt.format(func), "() => any");
}

#[test]
fn error_sentinel_renders_as_any_object_property_value() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let obj = db.object(vec![PropertyInfo::new(
        db.intern_string("r"),
        TypeId::ERROR,
    )]);
    assert_eq!(fmt.format(obj), "{ r: any; }");
}

#[test]
fn error_sentinel_renders_as_any_tuple_element() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let tuple = db.tuple(vec![crate::types::TupleElement {
        type_id: TypeId::ERROR,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert_eq!(fmt.format(tuple), "[any]");
}

#[test]
fn error_sentinel_renders_as_any_union_arm() {
    // A union that structurally holds the error sentinel as one arm must
    // render that arm as `any`, never `error`. (Display simplification then
    // collapses `string | any` — the any-flagged sentinel absorbs the other
    // arm, matching tsc — so this asserts the rendered `any` without pinning
    // the exact collapsed form.)
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let union = db.union_preserve_members(vec![TypeId::STRING, TypeId::ERROR]);
    let rendered = fmt.format(union);
    assert!(
        rendered.contains("any") && !rendered.contains("error"),
        "union arm should render the error sentinel as `any`, got: {rendered}"
    );
}

#[test]
fn error_sentinel_renders_as_any_application_base_collapse() {
    // An `Application` whose base is the error sentinel collapses to the bare
    // sentinel (avoiding `any<any<...>>` cascades) — and that collapse renders
    // `any`, not `error`.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let app = db.application(TypeId::ERROR, vec![TypeId::STRING]);
    assert_eq!(fmt.format(app), "any");
}

// =================================================================
// Fixed-tuple spread splicing at display time (#16732)
//
// tsc normalizes a tuple that spreads a fixed-length tuple by splicing the
// inner elements inline, so its printer never renders the `...[...]` wrapper.
// tsz's evaluator does the same when a tuple is built from a concrete rest
// operand, but a rest element left un-reduced (a deferred recursive alias
// application `[H, ...Split<R>]`) can reach the printer un-spliced. The
// formatter re-normalizes so the rendered form matches tsc regardless of the
// stored representation. Array spreads (`...T[]`) and generic spreads stay.
// =================================================================

fn rest_of(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

#[test]
fn format_tuple_splices_fixed_tuple_spread() {
    // `[string, ...[number, boolean]]` renders `[string, number, boolean]`.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let inner = db.tuple(vec![
        crate::types::TupleElement::fixed(TypeId::NUMBER),
        crate::types::TupleElement::fixed(TypeId::BOOLEAN),
    ]);
    let outer = db.tuple(vec![
        crate::types::TupleElement::fixed(TypeId::STRING),
        rest_of(inner),
    ]);
    assert_eq!(fmt.format(outer), "[string, number, boolean]");
}

#[test]
fn format_tuple_splices_nested_fixed_tuple_spread() {
    // `[string, ...[number, ...[boolean]]]` renders `[string, number, boolean]`
    // — the splice recurses through the inner spread.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let innermost = db.tuple(vec![crate::types::TupleElement::fixed(TypeId::BOOLEAN)]);
    let inner = db.tuple(vec![
        crate::types::TupleElement::fixed(TypeId::NUMBER),
        rest_of(innermost),
    ]);
    let outer = db.tuple(vec![
        crate::types::TupleElement::fixed(TypeId::STRING),
        rest_of(inner),
    ]);
    assert_eq!(fmt.format(outer), "[string, number, boolean]");
}

#[test]
fn format_tuple_keeps_array_rest_spread() {
    // An array spread stays `...T[]` — tsc only inlines fixed-tuple spreads.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let arr = db.array(TypeId::NUMBER);
    let outer = db.tuple(vec![
        crate::types::TupleElement::fixed(TypeId::STRING),
        rest_of(arr),
    ]);
    assert_eq!(fmt.format(outer), "[string, ...number[]]");
}

#[test]
fn format_tuple_splices_fixed_head_before_array_rest() {
    // A fixed head spliced out of an inner tuple keeps the inner tuple's own
    // trailing array rest: `[string, ...[number, ...boolean[]]]` renders
    // `[string, number, ...boolean[]]`.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let bool_arr = db.array(TypeId::BOOLEAN);
    let inner = db.tuple(vec![
        crate::types::TupleElement::fixed(TypeId::NUMBER),
        rest_of(bool_arr),
    ]);
    let outer = db.tuple(vec![
        crate::types::TupleElement::fixed(TypeId::STRING),
        rest_of(inner),
    ]);
    assert_eq!(fmt.format(outer), "[string, number, ...boolean[]]");
}
