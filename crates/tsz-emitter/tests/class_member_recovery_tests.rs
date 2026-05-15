//! Integration tests for malformed class member emit recovery.

use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;
use tsz_emitter::{context::emit::EmitContext, lowering::LoweringPass};

#[path = "test_support.rs"]
mod test_support;

use test_support::{parse_and_print_with_opts, parse_source};

fn print_es2015(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::es6())
}

fn print_with_printer_options(source: &str, opts: PrinterOptions) -> String {
    let (parser, root) = parse_source(source);
    let mut printer = EmitterPrinter::with_options(&parser.arena, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn print_with_cli_style_pipeline(source: &str, opts: PrinterOptions) -> String {
    let (parser, root) = parse_source(source);
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.set_source_map_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn public_empty_block_member_emits_recovered_block_statement() {
    let output = print_es2015("class C {\n    public {};\n}\n");
    assert_eq!(output, "class C {\n}\n{ }\n;\n");
}

#[test]
fn public_index_signature_block_member_emits_recovered_block_statement() {
    let output = print_es2015("class C {\n    public {[name:string]:VariableDeclaration};\n}\n");
    assert_eq!(
        output,
        "class C {\n}\n{\n    [name, string];\n    VariableDeclaration;\n}\n;\n"
    );
}

#[test]
fn es2015_type_only_class_property_is_erased() {
    let output = print_es2015("class C {\n    foo: string;\n}\n");
    assert_eq!(output, "class C {\n}\n");
}

#[test]
fn computed_string_field_preserves_source_quotes_with_constructor() {
    let output = print_with_printer_options(
        "class C {\n    ['this'] = '';\n    constructor() {}\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("this['this'] = '';"),
        "Computed string field should preserve its source quote style.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this[\"this\"]"),
        "Computed string field should not be rewritten to double quotes.\nOutput:\n{output}"
    );
}

#[test]
fn cli_style_computed_string_field_preserves_source_quotes_with_crlf() {
    let source = "class C {\r\n    data = { foo: '' };\r\n    ['this'] = '';\r\n    constructor() {\r\n        var copy: typeof this.data = { foo: '' };\r\n    }\r\n}\r\n";
    let output = print_with_cli_style_pipeline(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("this['this'] = '';"),
        "Computed string field should preserve its source quote style.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("this[\"this\"]"),
        "Computed string field should not be rewritten to double quotes.\nOutput:\n{output}"
    );
}

#[test]
fn constructor_recovered_return_type_survives_arrow_parameter_syntax() {
    let output = print_with_printer_options(
        "class C {\n    constructor(fn: (x: number) => void, value = () => 1): Result {}\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("constructor(fn, value = () => 1): Result"),
        "Constructor recovery should preserve the return type after arrow syntax.\nOutput:\n{output}"
    );
}

#[test]
fn downlevel_define_type_only_computed_property_does_not_allocate_temp() {
    let output = print_with_printer_options(
        "class C {\n    [side.effect]: string;\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("_a = side.effect"),
        "Type-only computed property should not allocate an unused temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("}\nside.effect;"),
        "Side-effectful computed property expression should still be emitted.\nOutput:\n{output}"
    );
}

#[test]
fn native_static_block_await_recovery_matches_tsc_shape() {
    let source = r#"class C {
    static {
        ({ [await]: 1 });
    }
    static {
        class D {
            [await] = 1;
        }
    }
    static {
        ({ await });
    }
    static {
        await:
        break await; // illegal label
    }
    static {
        const ff = (await) => { };
        const fff = await => { };
    }
}
"#;
    let output = print_with_printer_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("({ [await ]: 1 });"),
        "Bare computed `await` in an object literal should print as a missing-operand await expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("[await ] = 1;"),
        "Bare computed `await` in a nested class should print as a missing-operand await expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("({ await:  });"),
        "Contextually reserved shorthand `await` should recover as an empty property assignment.\nOutput:\n{output}"
    );
    assert!(
        output.contains("await ;\n        break ;\n        await ; // illegal label"),
        "Invalid `await` labels in static blocks should recover as await and break statements.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const ff = (await );\n        { }"),
        "Parenthesized `await` arrow candidates should emit the recovered empty block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const fff = await ;\n        { }"),
        "Bare `await` arrow candidates should emit the recovered empty block.\nOutput:\n{output}"
    );
}

/// With `useDefineForClassFields: true` and target < ES2022, a typed-only
/// field (no initializer) must still be materialized as
/// `Object.defineProperty(this, "name", { value: void 0 })` in the
/// constructor — matching tsc semantics where every class field
/// declaration creates a runtime property under define-fields mode.
#[test]
fn downlevel_define_typed_only_field_emits_void0_define_property() {
    let output = print_with_printer_options(
        "class Base {}\nclass Test extends Base {\n    prop: number;\n    constructor(public p: number) {\n        super();\n    }\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    let define_prop = "Object.defineProperty(this, \"prop\", {\n            enumerable: true,\n            configurable: true,\n            writable: true,\n            value: void 0\n        });";
    assert!(
        output.contains(define_prop),
        "Typed-only field should be lowered to Object.defineProperty with void 0.\nOutput:\n{output}"
    );
}

/// Without `useDefineForClassFields`, a typed-only field has no runtime
/// effect and must not produce any assignment. This guards against the
/// fix above accidentally widening to all targets.
#[test]
fn downlevel_assign_typed_only_field_emits_nothing() {
    let output = print_with_printer_options(
        "class Test {\n    prop: number;\n    constructor() {}\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("this.prop"),
        "Typed-only field without define-fields must not emit a runtime assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("defineProperty(this, \"prop\""),
        "Typed-only field without define-fields must not emit defineProperty.\nOutput:\n{output}"
    );
}

#[test]
fn native_define_typed_only_public_field_emits_nothing() {
    let output = print_with_printer_options(
        "class Test {\n    prop: number;\n    bare;\n    #privateProp: number;\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        !output.contains("prop;"),
        "Typed-only public field should be erased in native class-field emit.\nOutput:\n{output}"
    );
    assert!(
        output.contains("bare;"),
        "Untyped public field should remain a runtime class field.\nOutput:\n{output}"
    );
    assert!(
        output.contains("#privateProp;"),
        "Private fields remain runtime declarations even when annotated.\nOutput:\n{output}"
    );
}

#[test]
fn native_define_decorated_typed_public_field_stays_runtime_field() {
    let output = print_with_printer_options(
        "class Test {\n    @dec\n    prop: number;\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2022,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("@dec"),
        "ES decorator should be preserved.\nOutput:\n{output}"
    );
    assert!(
        output.contains("prop;"),
        "Decorated typed field remains a runtime class field.\nOutput:\n{output}"
    );
}

#[test]
fn duplicate_static_field_modifier_lowers_as_instance_field() {
    let output = print_with_cli_style_pipeline(
        "class C {\n    static static foo = 1;\n    public static static bar() { }\n}\n",
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("constructor() {\n        this.foo = 1;\n    }"),
        "Duplicate static field recovery should lower `foo` as an instance field.\nOutput:\n{output}"
    );
    assert!(
        output.contains("bar() { }"),
        "Duplicate static method recovery should emit `bar` as an instance method.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("C.foo = 1"),
        "Duplicate static field recovery must not emit a static field assignment.\nOutput:\n{output}"
    );
}
