#[test]
fn legacy_decorated_es2015_static_this_and_super_do_not_reserve_dead_temps() {
    let source = "declare const dec: any;\nclass Base { static value = 1; }\n@dec\nclass Decorated extends Base {\n    static value = super.value + this.name;\n}\nclass Plain {\n    static value = this.name;\n}\nclass PlainDerived extends Base {\n    static value = super.value + this.name;\n}\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        legacy_decorators: true,
        target: ScriptTarget::ES2015,
        use_define_for_class_fields: false,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("Decorated.value = (void 0).value + (void 0).name;"),
        "Externalized decorated static initializers should lower `this` and `super` receivers to undefined.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = Plain;\nPlain.value = _a.name;"),
        "The first live static `this` alias after the decorated class should use `_a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("class PlainDerived extends (_c = Base)")
            && output.contains("PlainDerived.value = Reflect.get(_c, \"value\", _b) + _b.name;"),
        "The following derived class should reserve only its live class and super aliases.\nOutput:\n{output}"
    );
}

#[test]
fn es5_static_this_aliases_advance_across_classes() {
    let source = "class First {\n    static value = this.name;\n}\nclass Second {\n    static value = this.name;\n}\n";

    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        use_define_for_class_fields: false,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var _a;\n    _a = First;\n    First.value = _a.name;"),
        "The first ES5 static `this` initializer should use `_a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _b;\n    _b = Second;\n    Second.value = _b.name;"),
        "The next ES5 static `this` initializer should advance to `_b`.\nOutput:\n{output}"
    );
}

#[test]
fn es5_object_literal_setter_downlevels_destructured_parameter() {
    let source = "const foo = {\n    set foo([start, end]: [any, any]) {\n        void start;\n        void end;\n    },\n};\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("set foo(_a) {\n        var start = _a[0], end = _a[1];"),
        "ES5 object literal setters should lower destructured parameters.\nOutput:\n{output}"
    );
}

#[test]
fn decorator_metadata_conditional_type_uses_common_branch_runtime_type() {
    let source = "declare function d(): PropertyDecorator;\nabstract class BaseEntity<T> {\n    @d()\n    public attributes: T extends { attributes: infer A } ? A : undefined;\n}\nclass C {\n    @d()\n    x: number extends string ? false : true;\n}\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            legacy_decorators: true,
            emit_decorator_metadata: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains(
            "__metadata(\"design:type\", Object)\n], BaseEntity.prototype, \"attributes\", void 0);"
        ),
        "Generic conditional metadata should stay Object.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:type\", Boolean)\n], C.prototype, \"x\", void 0);"),
        "Conditional metadata with boolean literal branches should emit Boolean.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_top_level_using_direct_exported_legacy_class_stays_inline() {
    let source =
        "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport class C {}\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::CommonJS,
            legacy_decorators: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("exports.C = C = class C {"),
        "CommonJS top-level using should keep direct legacy-decorated class exports inline.\nOutput:\n{output}"
    );
    assert!(
        output.contains("exports.C = C = __decorate(["),
        "CommonJS top-level using should preserve the exported __decorate reassignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("};\nexports.C = C;\n    exports.C = C = __decorate(["),
        "CommonJS top-level using should not insert a redundant trailing export between the class and __decorate.\nOutput:\n{output}"
    );
}

#[test]
fn commonjs_deferred_class_export_alias_emits_after_declaration() {
    let source = "export { J as JJ };\nexport class J {}\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let class_pos = output
        .find("class J")
        .expect("class declaration should emit");
    let direct_export_pos = output
        .find("exports.J = J;")
        .expect("direct class export should emit after the class");
    let alias_export_pos = output
        .find("exports.JJ = J;")
        .expect("deferred export alias should emit after the class");

    assert!(
        class_pos < direct_export_pos && direct_export_pos < alias_export_pos,
        "CommonJS class export aliases should be emitted after the class in tsc order.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_decorated_declare_computed_property_emits_decorator_target() {
    let source = "declare function decorator(target: any, key: any): any;\nconst b = Symbol('b');\nclass Foo {\n    @decorator declare [b]: number;\n}\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            legacy_decorators: true,
            target: ScriptTarget::ESNext,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("], Foo.prototype, b, void 0);"),
        "Legacy decorators on computed declare fields should emit the computed target expression.\nOutput:\n{output}"
    );
}
