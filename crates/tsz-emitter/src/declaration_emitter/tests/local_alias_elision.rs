//! Function-local type-alias elision in inferred declaration printing.
//!
//! Rule: a type alias declared inside a function body can never be referenced
//! by name in a `.d.ts`. When such an alias survives in an inferred type, the
//! printer renders its application as `Arg | /*elided*/ any` (single visible
//! type argument) or `/*elided*/ any`, and a bare reference as
//! `/*elided*/ any`. The elision is keyed on the alias `DefId`, not on its
//! spelling, so same-named visible types and literal text containing the name
//! are unaffected (the retired post-print text rewrite corrupted both).

use super::*;

/// Parse `source`, bind it, and return an emitter whose type cache maps
/// `def_id` to the alias symbol named `alias_name`, mirroring the cache the
/// checker builds for a function-local alias application.
fn emitter_with_alias_def<'a>(
    parser: &'a ParserState,
    binder: &'a BinderState,
    interner: &'a TypeInterner,
    def_id: DefId,
    alias_name: &str,
) -> DeclarationEmitter<'a> {
    let alias_sym = binder
        .symbols
        .iter()
        .find(|symbol| symbol.escaped_name == alias_name)
        .map(|symbol| symbol.id)
        .unwrap_or_else(|| panic!("missing symbol for alias {alias_name}"));

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.def_to_symbol.insert(def_id, alias_sym);
    type_cache
        .def_to_name
        .insert(def_id, alias_name.to_string());

    DeclarationEmitter::with_type_info(&parser.arena, type_cache, interner, binder)
}

fn parse_and_bind(source: &str) -> (ParserState, BinderState) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    (parser, binder)
}

const FUNCTION_LOCAL_ALIAS_SOURCE: &str = r#"
export function make() {
    type Wrapped<T> = T | { next: Wrapped<T> };
    var x: Wrapped<number>;
    return x;
}
"#;

#[test]
fn single_argument_application_prints_arg_union_elided_any() {
    let (parser, binder) = parse_and_bind(FUNCTION_LOCAL_ALIAS_SOURCE);
    let interner = TypeInterner::new();
    let def_id = DefId(9301);
    let emitter = emitter_with_alias_def(&parser, &binder, &interner, def_id, "Wrapped");

    let app = interner.application(interner.lazy(def_id), vec![TypeId::NUMBER]);
    let printed = emitter.print_type_id_for_inferred_declaration(app);

    assert_eq!(printed, "number | /*elided*/ any");
}

#[test]
fn multi_argument_application_prints_elided_any() {
    let source = r#"
export function pair() {
    type Both<A, B> = [A, B, Both<A, B>];
    var x: Both<number, string>;
    return x;
}
"#;
    let (parser, binder) = parse_and_bind(source);
    let interner = TypeInterner::new();
    let def_id = DefId(9302);
    let emitter = emitter_with_alias_def(&parser, &binder, &interner, def_id, "Both");

    let app = interner.application(interner.lazy(def_id), vec![TypeId::NUMBER, TypeId::STRING]);
    let printed = emitter.print_type_id_for_inferred_declaration(app);

    assert_eq!(printed, "/*elided*/ any");
}

#[test]
fn elision_keys_on_scope_not_name() {
    // Same structural scenario with every binder-bound name changed. If the
    // elision were keyed on a spelling, this renamed variant would slip
    // through and leak the local alias name.
    let source = r#"
export function build() {
    type Chain<U> = U | { tail: Chain<U> };
    var z: Chain<string>;
    return z;
}
"#;
    let (parser, binder) = parse_and_bind(source);
    let interner = TypeInterner::new();
    let def_id = DefId(9303);
    let emitter = emitter_with_alias_def(&parser, &binder, &interner, def_id, "Chain");

    let app = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);
    let printed = emitter.print_type_id_for_inferred_declaration(app);

    assert_eq!(printed, "string | /*elided*/ any");
}

#[test]
fn module_level_alias_application_is_not_elided() {
    // Negative case: an identically shaped alias declared at module scope IS
    // nameable, so its application must keep the named reference.
    let source = r#"
export type Wrapped<T> = T | { next: Wrapped<T> };
export function make() {
    var x: Wrapped<number>;
    return x;
}
"#;
    let (parser, binder) = parse_and_bind(source);
    let interner = TypeInterner::new();
    let def_id = DefId(9304);
    let emitter = emitter_with_alias_def(&parser, &binder, &interner, def_id, "Wrapped");

    let app = interner.application(interner.lazy(def_id), vec![TypeId::NUMBER]);
    let printed = emitter.print_type_id_for_inferred_declaration(app);

    assert_eq!(printed, "Wrapped<number>");
}

#[test]
fn string_literal_text_matching_alias_name_is_untouched() {
    // A string-literal type whose text happens to contain the alias name (and
    // even an application-shaped spelling) is literal data, not a reference;
    // it must survive verbatim while the real application is elided.
    let (parser, binder) = parse_and_bind(FUNCTION_LOCAL_ALIAS_SOURCE);
    let interner = TypeInterner::new();
    let def_id = DefId(9305);
    let emitter = emitter_with_alias_def(&parser, &binder, &interner, def_id, "Wrapped");

    let app = interner.application(interner.lazy(def_id), vec![TypeId::NUMBER]);
    let literal = interner.literal_string("see Wrapped<number> docs");
    let union = interner.union(vec![app, literal]);
    let printed = emitter.print_type_id_for_inferred_declaration(union);

    assert!(
        printed.contains("\"see Wrapped<number> docs\""),
        "literal text containing the alias name must stay verbatim: {printed}"
    );
    assert!(
        printed.contains("number | /*elided*/ any"),
        "the real application must still be elided: {printed}"
    );
}

#[test]
fn property_named_like_alias_is_untouched() {
    // A property that shares the alias spelling is a member name, not a type
    // reference; only the property's type is elided. The retired text rewrite
    // corrupted the member name itself.
    let (parser, binder) = parse_and_bind(FUNCTION_LOCAL_ALIAS_SOURCE);
    let interner = TypeInterner::new();
    let def_id = DefId(9306);
    let emitter = emitter_with_alias_def(&parser, &binder, &interner, def_id, "Wrapped");

    let app = interner.application(interner.lazy(def_id), vec![TypeId::NUMBER]);
    let prop_name = interner.intern_string("Wrapped");
    let object = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::new(prop_name, app)],
        string_index: None,
        number_index: None,
        symbol_index: None,
        symbol: None,
    });
    let union = interner.union(vec![object, app]);
    let printed = emitter.print_type_id_for_inferred_declaration(union);

    assert!(
        printed.contains("Wrapped: number | /*elided*/ any"),
        "the property name must stay while its type is elided: {printed}"
    );
}

#[test]
fn bare_reference_to_collected_alias_prints_elided_any() {
    // A bare `Lazy` reference to an alias that also appears applied in the
    // same type renders as `/*elided*/ any`, never as the local name.
    let (parser, binder) = parse_and_bind(FUNCTION_LOCAL_ALIAS_SOURCE);
    let interner = TypeInterner::new();
    let def_id = DefId(9307);
    let emitter = emitter_with_alias_def(&parser, &binder, &interner, def_id, "Wrapped");

    let app = interner.application(interner.lazy(def_id), vec![TypeId::NUMBER]);
    let prop_name = interner.intern_string("inner");
    let object = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::new(prop_name, interner.lazy(def_id))],
        string_index: None,
        number_index: None,
        symbol_index: None,
        symbol: None,
    });
    let union = interner.union(vec![object, app]);
    let printed = emitter.print_type_id_for_inferred_declaration(union);

    assert!(
        printed.contains("inner: /*elided*/ any"),
        "a bare reference must elide to any: {printed}"
    );
    assert!(
        !printed.contains("inner: Wrapped"),
        "the local alias name must not leak through a bare reference: {printed}"
    );
}
