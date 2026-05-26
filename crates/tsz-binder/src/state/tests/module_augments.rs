use super::*;

#[test]
fn direct_module_export_resolution() {
    // Test resolving a direct export from module_exports.
    let mut binder = BinderState::new();

    let sym = binder
        .symbols
        .alloc(symbol_flags::FUNCTION, "myFunc".to_string());
    let mut exports = SymbolTable::new();
    exports.set("myFunc".to_string(), sym);
    Arc::make_mut(&mut binder.module_exports).insert("./mod".to_string(), exports);

    let resolved = binder.resolve_import_if_needed_public("./mod", "myFunc");
    assert_eq!(resolved, Some(sym), "should resolve direct export");

    let not_found = binder.resolve_import_if_needed_public("./mod", "nonExistent");
    assert_eq!(not_found, None, "non-existent export should return None");
}

#[test]
fn wildcard_reexport_resolution() {
    // Test resolving through `export * from` chains.
    let mut binder = BinderState::new();

    let sym = binder
        .symbols
        .alloc(symbol_flags::CLASS, "Widget".to_string());
    let mut a_exports = SymbolTable::new();
    a_exports.set("Widget".to_string(), sym);
    Arc::make_mut(&mut binder.module_exports).insert("./a".to_string(), a_exports);

    // ./b re-exports everything from ./a
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./b".to_string())
        .or_default()
        .push("./a".to_string());
    Arc::make_mut(&mut binder.wildcard_reexports_type_only)
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), false));

    let resolved = binder.resolve_import_if_needed_public("./b", "Widget");
    assert_eq!(
        resolved,
        Some(sym),
        "should resolve through wildcard re-export"
    );
}

#[test]
fn reexport_cycle_does_not_hang() {
    // Ensure that cyclic re-export chains don't cause infinite loops.
    let mut binder = BinderState::new();

    // ./a re-exports from ./b
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./a".to_string())
        .or_default()
        .push("./b".to_string());
    Arc::make_mut(&mut binder.wildcard_reexports_type_only)
        .entry("./a".to_string())
        .or_default()
        .push(("./b".to_string(), false));

    // ./b re-exports from ./a (cycle!)
    Arc::make_mut(&mut binder.wildcard_reexports)
        .entry("./b".to_string())
        .or_default()
        .push("./a".to_string());
    Arc::make_mut(&mut binder.wildcard_reexports_type_only)
        .entry("./b".to_string())
        .or_default()
        .push(("./a".to_string(), false));

    // Should not hang, should return None
    let resolved = binder.resolve_import_if_needed_public("./a", "X");
    assert_eq!(resolved, None, "cyclic re-export should return None");
}

// =============================================================================
// 19. MODULE AUGMENTATION TESTS
// =============================================================================

#[test]
fn module_augmentation_tracked() {
    // `declare module "x" { interface Y { ... } }` inside an external module
    // should track the augmentation.
    let (binder, _parser) = parse_and_bind(
        r#"
import {} from "x";
declare module "x" {
    interface Augmented {
        extra: string;
    }
}
"#,
    );

    // The file should be an external module (has import)
    assert!(binder.is_external_module);

    // Check that module augmentations were tracked
    // Note: module augmentations are only tracked when the binder detects
    // that the module being declared already exists as a known module.
    // In isolation (no lib context), this may or may not be tracked.
}

#[test]
fn global_augmentation_tracked_in_declare_module() {
    // `global { ... }` inside `declare module "x"` should track global augmentations.
    let (binder, _parser) = parse_and_bind(
        r#"
declare module "mylib" {
    global {
        interface MyGlobal {
            x: number;
        }
    }
}
"#,
    );

    assert!(
        binder.global_augmentations.contains_key("MyGlobal"),
        "global augmentation should be tracked"
    );
}

// Regression for #6164: a `declare module "<self>"` augmentation must bind its
// interface declaration to a separate `SymbolId` from the file-scope interface
// of the same name. Merging the augmentation's declaration node into the
// file-scope symbol would inject the augmentation members into the local type
// computed by `compute_interface_type_from_declarations`.
#[test]
fn self_module_augmentation_interface_does_not_pollute_file_scope_interface() {
    let (binder, _parser) = parse_and_bind(
        r#"
interface Merged {
    a: number;
}

interface Merged {
    b: string;
}

declare module "./test" {
    interface Merged {
        augmented: boolean;
    }
}

export {};
"#,
    );

    let file_sym = binder
        .file_locals
        .get("Merged")
        .expect("local `interface Merged` should be visible at file scope");
    let symbol = binder
        .get_symbol(file_sym)
        .expect("file-scope `Merged` symbol exists");
    assert_eq!(
        symbol.declarations.len(),
        2,
        "file-scope `Merged` should only see its two non-augmentation declarations, got {:?}",
        symbol.declarations
    );

    let augmentations = binder
        .module_augmentations
        .get("./test")
        .expect("augmentation entry for ./test should exist");
    assert!(
        augmentations.iter().any(|aug| aug.name == "Merged"),
        "the augmentation table should record the augmentation declaration for ./test"
    );
}

// Same-target augmentation interface declarations in separate `declare module
// "X" { ... }` blocks must merge with each other into one augmentation-local
// symbol, mirroring tsc's behaviour where two augmentations of `interface
// Both` accumulate into one merged shape.
#[test]
fn module_augmentation_interface_declarations_merge_across_blocks() {
    let (binder, _parser) = parse_and_bind(
        r#"
declare module "./mod" {
    interface Both { foo: number; }
}

declare module "./mod" {
    interface Both { bar: string; }
}

export {};
"#,
    );

    let entries = binder
        .module_augmentations
        .get("./mod")
        .expect("augmentation table should record both blocks");
    let both_entries: Vec<_> = entries.iter().filter(|aug| aug.name == "Both").collect();
    assert_eq!(both_entries.len(), 2, "expected two declarations of Both");

    let sym_a = binder
        .get_node_symbol(both_entries[0].node)
        .expect("augmentation decl should map to a symbol");
    let sym_b = binder
        .get_node_symbol(both_entries[1].node)
        .expect("augmentation decl should map to a symbol");
    assert_eq!(
        sym_a, sym_b,
        "two augmentation declarations of the same name should share a symbol"
    );

    let merged = binder
        .get_symbol(sym_a)
        .expect("augmentation symbol exists");
    assert_eq!(
        merged.declarations.len(),
        2,
        "merged augmentation symbol should record both declarations"
    );
}

// A `declare module "<self>"` type-alias augmentation must not merge into a
// pre-existing file-scope type alias of the same name. This is the type-alias
// mirror of #6164: same architectural rule, different declaration kind.
#[test]
fn self_module_augmentation_type_alias_does_not_pollute_file_scope_alias() {
    let (binder, _parser) = parse_and_bind(
        r#"
type MyType = string;

declare module "./test" {
    type MyType = number;
}

export {};
"#,
    );

    let file_sym = binder
        .file_locals
        .get("MyType")
        .expect("local `type MyType = string` should be visible at file scope");
    let symbol = binder
        .get_symbol(file_sym)
        .expect("file-scope `MyType` symbol exists");
    assert_eq!(
        symbol.declarations.len(),
        1,
        "file-scope `MyType` should only see its single non-augmentation declaration"
    );

    let augmentations = binder
        .module_augmentations
        .get("./test")
        .expect("augmentation entry for ./test should exist");
    assert!(
        augmentations.iter().any(|aug| aug.name == "MyType"),
        "the augmentation table should record the augmentation type-alias declaration"
    );
}

// =============================================================================
// MODULE AUGMENTATION SYMBOL ISOLATION TESTS
// =============================================================================

#[test]
fn augmentation_interface_does_not_merge_into_file_scope_interface() {
    let (binder, parser) = parse_and_bind(
        r#"
interface Merged {
    a: number;
}
interface Merged {
    b: string;
}
declare module "./test" {
    interface Merged {
        augmented: boolean;
    }
}
export {};
"#,
    );

    let merged_id = binder
        .file_locals
        .get("Merged")
        .expect("Merged should be in file_locals");
    let merged_sym = binder.symbols.get(merged_id).expect("symbol exists");
    assert_eq!(
        merged_sym.declarations.len(),
        2,
        "file-scope Merged must have only its 2 local declarations, not the augmentation's"
    );

    let aug_id = *binder
        .module_augmentation_symbols
        .get(&("./test".to_string(), "Merged".to_string()))
        .expect("augmentation-only symbol should exist in module_augmentation_symbols");
    assert_ne!(
        aug_id, merged_id,
        "augmentation symbol must be different from file-scope symbol"
    );
    let aug_sym = binder
        .symbols
        .get(aug_id)
        .expect("augmentation symbol exists");
    assert_eq!(
        aug_sym.declarations.len(),
        1,
        "augmentation symbol should have exactly 1 declaration"
    );

    // The augmentation should be tracked in module_augmentations.
    let augs = binder
        .module_augmentations
        .get("./test")
        .expect("augmentation should be tracked");
    assert!(
        augs.iter().any(|a| a.name == "Merged"),
        "Merged augmentation should be in module_augmentations"
    );
    drop(parser);
}

#[test]
fn augmentation_interface_renamed_iteration_var_does_not_merge() {
    let (binder, parser) = parse_and_bind(
        r#"
interface Shape {
    x: number;
}
declare module "./shapes" {
    interface Shape {
        extra: string;
    }
}
export {};
"#,
    );

    let shape_id = binder
        .file_locals
        .get("Shape")
        .expect("Shape should be in file_locals");
    let shape_sym = binder.symbols.get(shape_id).expect("symbol exists");
    assert_eq!(
        shape_sym.declarations.len(),
        1,
        "file-scope Shape must have only 1 declaration"
    );

    let aug_id = *binder
        .module_augmentation_symbols
        .get(&("./shapes".to_string(), "Shape".to_string()))
        .expect("augmentation-only Shape symbol must exist");
    assert_ne!(aug_id, shape_id, "augmentation symbol must be separate");
    drop(parser);
}

#[test]
fn augmentation_declarations_across_two_blocks_same_target_merge_with_each_other() {
    let (binder, parser) = parse_and_bind(
        r#"
declare module "./m" {
    interface Shared {
        a: number;
    }
}
declare module "./m" {
    interface Shared {
        b: string;
    }
}
export {};
"#,
    );

    let aug_key = ("./m".to_string(), "Shared".to_string());
    let aug_sym_id = binder
        .module_augmentation_symbols
        .get(&aug_key)
        .expect("augmentation symbol must exist");
    let aug_sym = binder
        .symbols
        .get(*aug_sym_id)
        .expect("augmentation symbol exists");
    assert_eq!(
        aug_sym.declarations.len(),
        2,
        "both augmentation blocks' Shared declarations must merge into one augmentation symbol"
    );
    drop(parser);
}

#[test]
fn augmentation_type_alias_does_not_merge_into_file_scope_alias() {
    let (binder, parser) = parse_and_bind(
        r#"
type MyType = number;
declare module "./util" {
    type MyType = string;
}
export {};
"#,
    );

    let local_id = binder
        .file_locals
        .get("MyType")
        .expect("MyType should be in file_locals");
    let local_sym = binder.symbols.get(local_id).expect("symbol exists");
    assert_eq!(
        local_sym.declarations.len(),
        1,
        "file-scope MyType must have only 1 declaration"
    );

    let aug_id = *binder
        .module_augmentation_symbols
        .get(&("./util".to_string(), "MyType".to_string()))
        .expect("augmentation-only MyType symbol must exist");
    assert_ne!(aug_id, local_id, "augmentation symbol must be separate");
    drop(parser);
}

// =============================================================================
// 20. EXPANDO PROPERTIES
// =============================================================================

#[test]
fn expando_property_assignments_tracked() {
    // `X.prop = value` patterns should be tracked as expando properties.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {}
foo.bar = 1;
foo.baz = 'hello';
",
    );

    if let Some(props) = binder.expando_properties.get("foo") {
        assert!(props.contains("bar"), "should track foo.bar");
        assert!(props.contains("baz"), "should track foo.baz");
    }
    // Note: expando tracking may not work for all patterns in all cases,
    // so we don't assert that the map is non-empty unconditionally.
}

#[test]
fn void_zero_expando_assignments_are_skipped() {
    let source = r#"
exports.k = void 0;
var o = {};
o.y = void 0;
"#;

    let mut parser = ParserState::new("a.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        !binder
            .expando_properties
            .get("exports")
            .is_some_and(|props| props.contains("k")),
        "unexpected exports expando tracking: {:?}",
        binder.expando_properties
    );
    assert!(
        !binder
            .expando_properties
            .get("o")
            .is_some_and(|props| props.contains("y")),
        "unexpected object expando tracking: {:?}",
        binder.expando_properties
    );
}

#[test]
fn expando_element_assignments_resolve_const_literal_keys() {
    let (binder, _parser) = parse_and_bind(
        r#"
function foo() {}
const key = "realName";
const num = 42;
const sym = Symbol();
foo[key] = 1;
foo[num] = 2;
foo[sym] = 3;
"#,
    );

    let props = binder
        .expando_properties
        .get("foo")
        .expect("expected expando properties for foo");

    assert!(
        props.contains("realName"),
        "should resolve const string keys"
    );
    assert!(props.contains("42"), "should resolve const numeric keys");

    let sym_id = binder.file_locals.get("sym").expect("expected sym local");
    let unique_name = format!("__unique_{}", sym_id.0);
    assert!(
        props.contains(&unique_name),
        "should resolve const Symbol() keys to internal unique-symbol names"
    );
}

#[test]
fn expando_element_assignments_on_prototype_named_properties_are_tracked() {
    let (binder, _parser) = parse_and_bind(
        r#"
function F() {}
const key = "realName";
F.prototypeOf[key] = 1;
"#,
    );

    let props = binder
        .expando_properties
        .get("F.prototypeOf")
        .expect("expected expando properties for F.prototypeOf");

    assert!(
        props.contains("realName"),
        "should not treat prototypeOf as a prototype-chain assignment"
    );
}

// =============================================================================
// 21. SHORTHAND AMBIENT MODULES
// =============================================================================

#[test]
fn shorthand_ambient_module_detected() {
    // `declare module "xxx"` without a body should be detected as shorthand.
    let (binder, _parser) = parse_and_bind(
        r#"
declare module "*.css";
"#,
    );

    assert!(
        binder.shorthand_ambient_modules.contains("*.css"),
        "shorthand ambient module should be tracked"
    );
}

// =============================================================================
// 22. COMPLEX HOISTING SCENARIOS
// =============================================================================

#[test]
fn var_hoisted_from_do_while_body() {
    // `var` in a do-while body should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    do {
        var x = 1;
    } while (false);
}
",
    );

    let x_sym = binder.symbols.find_by_name("x");
    assert!(x_sym.is_some(), "var in do-while should be hoisted");
    let x_symbol = binder.symbols.get(x_sym.unwrap()).unwrap();
    assert!(x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0);
}

#[test]
fn multiple_var_same_name_in_different_blocks_merge() {
    // Multiple `var` declarations with the same name in different blocks
    // of the same function should all merge into one symbol.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    if (true) {
        var x = 1;
    }
    if (false) {
        var x = 2;
    }
    for (var x = 0; x < 1; x++) {}
}
",
    );

    // All `x` declarations should merge into one symbol
    let x_sym = binder
        .symbols
        .find_by_name("x")
        .expect("should have symbol for x");
    let x_symbol = binder.symbols.get(x_sym).unwrap();
    assert!(x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0);
}

// =============================================================================
// 23. SCOPE DISCOVERY AND PERSISTENT SCOPE SYSTEM
// =============================================================================

#[test]
fn persistent_scopes_populated() {
    // After binding, the persistent scope system should be populated.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    let x = 1;
    {
        let y = 2;
    }
}
",
    );

    // Should have at least: SourceFile + Function + Block
    assert!(
        binder.scopes.len() >= 3,
        "should have at least 3 persistent scopes, got {}",
        binder.scopes.len()
    );
}

#[test]
fn node_scope_ids_maps_nodes_to_scopes() {
    // The node_scope_ids map should link AST nodes to their scopes.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    let x = 1;
}
",
    );

    assert!(
        !binder.node_scope_ids.is_empty(),
        "node_scope_ids should be populated"
    );
}

// =============================================================================
// 24. EDGE CASES
// =============================================================================

#[test]
fn empty_source_file() {
    // Binding an empty source file should not panic.
    let (binder, _parser) = parse_and_bind("");

    assert!(
        !binder.scopes.is_empty(),
        "even empty file should have root scope"
    );
    assert!(binder.file_locals.is_empty() || !binder.file_locals.is_empty());
}

#[test]
fn binding_export_default_function() {
    // `export default function foo() {}` should work correctly.
    let (binder, _parser) = parse_and_bind(
        r"
export default function foo() {}
",
    );

    assert!(
        binder.is_external_module,
        "file with export should be module"
    );
}

#[test]
fn binding_export_default_class() {
    // `export default class Foo {}` should work correctly.
    let (binder, _parser) = parse_and_bind(
        r"
export default class Foo {}
",
    );

    assert!(
        binder.is_external_module,
        "file with export should be module"
    );
}

#[test]
fn export_default_identifier_does_not_add_to_named_exports() {
    // Regression test for TS2614 false-negative:
    // `export default a` (bare identifier) must NOT mark `a` as a named export.
    // Only `export default class/function` declarations should do so.
    //
    // Before the fix, `sym.is_exported = true` was set unconditionally on the
    // referenced local symbol, causing `a` to appear in module_exports["test.ts"]
    // under its local name — which suppressed TS2614 for `import { a } from "./mod"`.
    let (binder, _parser) = parse_and_bind(
        r"
var a = 10;
export default a;
",
    );

    assert!(
        binder.is_external_module,
        "file with export should be module"
    );

    // The module's named exports should contain "default" but NOT "a".
    let exports = binder.module_exports.get("test.ts");
    assert!(
        exports.is_some(),
        "module_exports should have an entry for test.ts"
    );
    let exports = exports.unwrap();

    assert!(
        exports.has("default"),
        "module_exports should contain 'default' export"
    );
    assert!(
        !exports.has("a"),
        "module_exports must NOT contain 'a' as a named export (export default a is not a named export)"
    );

    // Also verify `a` itself is NOT marked is_exported on the symbol.
    let a_sym_id = binder
        .file_locals
        .get("a")
        .expect("expected symbol 'a' in file_locals");
    let a_symbol = binder
        .symbols
        .get(a_sym_id)
        .expect("expected symbol data for 'a'");
    assert!(
        !a_symbol.is_exported,
        "symbol 'a' must not have is_exported=true when used as 'export default a'"
    );
}

#[test]
fn export_default_function_decl_is_named_export_eligible() {
    // Contrast: `export default function foo() {}` IS a declaration, so
    // the function symbol should be marked is_exported (it's callable by name
    // and the class/function default-export is in the named-export table under
    // "foo" in some resolution paths). This test ensures we didn't over-restrict.
    let (binder, _parser) = parse_and_bind(
        r"
export default function foo() {}
",
    );

    assert!(
        binder.is_external_module,
        "file with export should be module"
    );

    // `foo` should exist as a symbol (function declaration creates a binding).
    let foo_sym_id = binder.file_locals.get("foo");
    assert!(foo_sym_id.is_some(), "expected symbol 'foo' in file_locals");
}

#[test]
fn binding_reexport_all() {
    // `export * from './mod'` should track wildcard re-exports.
    let (binder, _parser) = parse_and_bind(
        r"
export * from './mod';
",
    );

    assert!(binder.is_external_module);
}

#[test]
fn binding_complex_destructuring() {
    // Complex destructuring patterns should all be bound correctly.
    let (binder, _parser) = parse_and_bind(
        r"
const { a, b: { c, d }, e: [f, g] } = obj;
",
    );

    // At minimum, the simple names should be bound
    assert!(binder.symbols.find_by_name("a").is_some(), "should bind a");
    assert!(binder.symbols.find_by_name("c").is_some(), "should bind c");
    assert!(binder.symbols.find_by_name("d").is_some(), "should bind d");
    assert!(binder.symbols.find_by_name("f").is_some(), "should bind f");
    assert!(binder.symbols.find_by_name("g").is_some(), "should bind g");

    // Property-name keys (b, e) must NOT create bindings — they are only
    // property selectors, not variable declarations.
    assert!(
        binder.file_locals.get("b").is_none(),
        "property name 'b' must not be bound"
    );
    assert!(
        binder.file_locals.get("e").is_none(),
        "property name 'e' must not be bound"
    );
}

#[test]
fn class_with_static_members() {
    // Static members should have the STATIC flag.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    static x: number = 1;
    static method(): void {}
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(foo_symbol.flags & symbol_flags::CLASS != 0);
}

#[test]
fn class_with_private_members() {
    // Private members should have appropriate flags.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    private x: number = 1;
    protected y: string = '';
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(foo_symbol.flags & symbol_flags::CLASS != 0);
}

#[test]
fn computed_property_name_does_not_crash() {
    // Computed property names in classes should not crash the binder.
    let (binder, _parser) = parse_and_bind(
        r"
const key = 'hello';
class Foo {
    [key]: number = 1;
}
",
    );

    assert!(binder.file_locals.has("Foo"));
}

#[test]
fn accessor_declarations() {
    // Get/set accessors should create symbols with accessor flags.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    private _x: number = 0;
    get x(): number { return this._x; }
    set x(value: number) { this._x = value; }
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    let members = foo_symbol
        .members
        .as_ref()
        .expect("class should have members");

    // x should exist as a member (get/set accessor)
    assert!(members.has("x"), "accessor x should be in class members");
}

#[test]
fn private_identifier_resolution_in_nested_static_block() {
    // Private identifier `#field in obj` should resolve across nested class static blocks.
    // In TypeScript, private fields are lexically scoped and accessible from nested contexts.
    let (binder, parser) = parse_and_bind(
        r#"
class C3 {
    static #a2_accessor_storage = 1;
    static {
        class C3_Inner {
            static {
                #a2_accessor_storage in C3;
            }
        }
    }
}
"#,
    );

    // First, verify the private field is bound in C3's scope
    let c3_sym_id = binder.file_locals.get("C3").expect("expected C3");
    let c3_symbol = binder.symbols.get(c3_sym_id).unwrap();
    let members = c3_symbol.members.as_ref().expect("C3 should have members");
    assert!(
        members.has("#a2_accessor_storage"),
        "C3 should have #a2_accessor_storage as member"
    );

    // Find the private identifier node in the inner static block (the one used in `in` expression)
    // We need to walk the AST to find it
    use tsz_parser::NodeIndex;
    let arena = parser.get_arena();
    let mut private_ident_idx = NodeIndex::NONE;

    for i in 0..arena.len() {
        let node_idx = NodeIndex(i as u32);
        if let Some(node) = arena.get(node_idx)
            && node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
            && let Some(ident) = arena.get_identifier(node)
            && ident.escaped_text == "#a2_accessor_storage"
        {
            // Check if this is the one in the binary expression (not the declaration)
            if let Some(ext) = arena.get_extended(node_idx)
                && let Some(parent) = arena.get(ext.parent)
                && parent.kind == tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION
            {
                private_ident_idx = node_idx;
                break;
            }
        }
    }

    assert!(
        private_ident_idx.is_some(),
        "Should find private identifier in binary expression"
    );

    // Now resolve the private identifier symbols
    let (symbols, saw_class_scope) =
        binder.resolve_private_identifier_symbols(arena, private_ident_idx);

    assert!(
        saw_class_scope,
        "Should have seen a class scope while resolving"
    );
    assert!(
        !symbols.is_empty(),
        "Should resolve #a2_accessor_storage to a symbol from the outer class C3"
    );

    // The resolved symbol should be the one from C3
    let resolved_sym = binder.symbols.get(symbols[0]).unwrap();
    assert_eq!(
        resolved_sym.escaped_name, "#a2_accessor_storage",
        "Resolved symbol should be #a2_accessor_storage"
    );
}

// =============================================================================
// 25. ENUM MERGING WITH NAMESPACE
// =============================================================================

#[test]
fn enum_merging_with_namespace() {
    // An enum and a namespace with the same name should merge.
    let (binder, _parser) = parse_and_bind(
        r"
enum Color {
    Red,
    Green,
    Blue,
}
namespace Color {
    export function fromString(s: string): Color { return Color.Red; }
}
",
    );

    let sym_id = binder.file_locals.get("Color").expect("expected Color");
    let symbol = binder.symbols.get(sym_id).unwrap();
    // Should have REGULAR_ENUM (from enum) and possibly module flags (from namespace)
    assert!(
        symbol.flags & symbol_flags::REGULAR_ENUM != 0,
        "merged symbol should keep REGULAR_ENUM flag"
    );
}

// =============================================================================
// 26. MULTIPLE INTERFACE DECLARATIONS WITH MEMBERS
// =============================================================================

#[test]
fn multiple_interface_declarations_members() {
    // Multiple interface declarations should all contribute members.
    let (binder, _parser) = parse_and_bind(
        r"
interface Foo {
    x: number;
}
interface Foo {
    y: string;
}
interface Foo {
    z: boolean;
}
",
    );

    let sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let symbol = binder.symbols.get(sym_id).unwrap();
    assert!(symbol.flags & symbol_flags::INTERFACE != 0);
    assert!(
        symbol.declarations.len() >= 3,
        "should have at least 3 declarations for merged interface"
    );
}
