#[test]
fn test_async_arrow_destructuring_default_param_temp_var_no_collision() {
    // Regression: async arrow with a destructuring default param AND an
    // awaited call must not produce two `var _a;` hoists or two `_a = ...`
    // assignments in the same `__awaiter` scope. The temp var counter from
    // the AsyncES5Emitter must be synced back to the main emitter so the
    // destructuring prologue and the awaiter generator body use disjoint
    // temp names.
    let source = "var f = async ({x} = {x: 1}) => { return fn(await p); };\n";
    let output = parse_lower_print(source, PrintOptions::es5());

    // Sanity: output should still use the awaiter/generator pipeline.
    assert!(
        output.contains("__awaiter"),
        "Expected __awaiter in output:\n{output}"
    );

    // Each underscore-prefixed temp identifier should have at most one
    // `var <name>` declaration inside a single __awaiter callback. Prior
    // to the fix, the destructuring prologue and the generator body both
    // chose `_b`, producing both `var _b;` (hoisted) and a separate
    // `var _b = _a === void 0 ? ...` in the same scope.
    for letter in b'a'..=b'z' {
        let name = format!("_{}", letter as char);
        let var_decl = format!("var {name}");
        let count = output.matches(var_decl.as_str()).count();
        assert!(
            count <= 1,
            "Expected at most one `var {name}` declaration, got {count}.\n\
             This indicates the AsyncES5 temp var counter was not synced back \
             to the main emitter, causing the destructuring prologue and the \
             awaiter body to collide on the same temp name.\nOutput:\n{output}"
        );
    }
}

/// `this.#field ??= rhs` on a private field must lower through the
/// `__classPrivateFieldSet(get() ?? rhs)` pattern, mirroring the existing
/// `+=`/`-=`/etc. compound-assignment lowering. Without this, the helper
/// emit produces `__classPrivateFieldGet(...) ??= rhs` — invalid JS, since
/// `??=` cannot apply to a function call. Mirrors tsc's emit for issue
/// `microsoft/TypeScript#61109`.
#[test]
fn private_field_nullish_assign_lowers_to_set_get_nullish_rhs() {
    let source = "class Cls {\n  #privateProp: number | undefined;\n  problem() {\n    this.#privateProp ??= 20;\n  }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("__classPrivateFieldSet(this, _Cls_privateProp, __classPrivateFieldGet(this, _Cls_privateProp, \"f\") ?? 20, \"f\")"),
        "Private-field `??=` must lower to set(get() ?? rhs).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("??="),
        "Lowered output must not still contain a `??=` operator.\nOutput:\n{output}"
    );
}

/// When the RHS of `??=`/`||=`/`&&=` on a private field is a
/// conditional expression, the lowered `get() <op> rhs` must wrap the
/// conditional in parens. `??`, `||`, and `&&` all bind tighter than the
/// conditional operator, so `get() ?? a ? b : c` would otherwise reparse
/// as `(get() ?? a) ? b : c` and silently change semantics.
#[test]
fn private_field_nullish_assign_parenthesizes_conditional_rhs() {
    let source = "class Cls {\n  #privateProp: number | undefined;\n  problem() {\n    this.#privateProp ??= false ? noop() : 20;\n  }\n}\nfunction noop(): number { return 0; }\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("?? (false ? noop() : 20)"),
        "Conditional RHS of `??=` must be parenthesized to preserve precedence.\nOutput:\n{output}"
    );
}

/// `||=` on a private field follows the same lowering shape as `??=`.
/// Locks in coverage so a future refactor of the compound-assignment
/// list can't regress one operator while leaving the others working.
#[test]
fn private_field_logical_or_assign_lowers_to_set_get_or_rhs() {
    let source =
        "class Cls {\n  #flag: boolean = false;\n  toggle() {\n    this.#flag ||= true;\n  }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("__classPrivateFieldSet(this, _Cls_flag, __classPrivateFieldGet(this, _Cls_flag, \"f\") || true, \"f\")"),
        "Private-field `||=` must lower to set(get() || rhs).\nOutput:\n{output}"
    );
}

/// `&&=` on a private field follows the same lowering shape as `??=`.
#[test]
fn private_field_logical_and_assign_lowers_to_set_get_and_rhs() {
    let source =
        "class Cls {\n  #flag: boolean = true;\n  guard() {\n    this.#flag &&= false;\n  }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("__classPrivateFieldSet(this, _Cls_flag, __classPrivateFieldGet(this, _Cls_flag, \"f\") && false, \"f\")"),
        "Private-field `&&=` must lower to set(get() && rhs).\nOutput:\n{output}"
    );
}

/// Regression: a nested namespace's name lives in the parent IIFE's
/// function scope, not at file scope. So a *file-scope* namespace with
/// the same name must still receive its own `var` declaration. The
/// lowering pass tracks `declared_names` to suppress duplicate `var`
/// emits, but the set must reset when entering and exiting a namespace
/// body: names declared inside a nested IIFE don't leak out.
#[test]
fn nested_namespace_name_does_not_suppress_outer_var_declaration() {
    let source = "namespace m1 {\n    namespace m2 {\n        export var p = 1;\n    }\n}\nnamespace m2 {\n    export var q = 2;\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    // Both the m1.m2 IIFE and the file-scope m2 IIFE need their own
    // `var m2;` preamble because each lives in a distinct scope.
    let var_count = output.matches("var m2;").count();
    assert_eq!(
        var_count, 2,
        "Each scope-local `m2` namespace needs its own `var m2;`. Found {var_count}.\nOutput:\n{output}"
    );
}

/// Counterpart: same-named namespace *reopened* at the same scope must
/// declare `var` only once. (Standard merging.)
#[test]
fn reopened_same_scope_namespace_declares_var_only_once() {
    let source =
        "namespace m1 {\n    export var p = 1;\n}\nnamespace m1 {\n    export var q = 2;\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    let var_count = output.matches("var m1;").count();
    assert_eq!(
        var_count, 1,
        "Reopened namespace at same scope should declare `var m1;` once. Found {var_count}.\nOutput:\n{output}"
    );
}

/// Regression: a same-named inner declaration that is `declare`-ambient is
/// erased at emit, so it must not trigger renaming of the namespace IIFE
/// parameter. tsc emits `(function (M) { ... })`, not `(function (M_1) { ... })`,
/// for `namespace M { export declare namespace M { } }`.
#[test]
fn namespace_iife_param_not_renamed_when_inner_same_name_is_declare() {
    let source = "namespace M {\n    export declare var x;\n    export declare function f();\n    export declare class C { }\n    export declare enum E { }\n    export declare namespace M { }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("(function (M) {"),
        "Declare-only inner `M` must not trigger IIFE param renaming.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(function (M_1)"),
        "IIFE param must not be renamed to `M_1` when the only same-name binding is ambient.\nOutput:\n{output}"
    );
}

/// Counterpart: a *concrete* inner declaration with the same name DOES
/// require IIFE-param renaming (so the outer-name reference and inner-name
/// reference don't collide).
#[test]
fn namespace_iife_param_renamed_when_inner_same_name_is_concrete() {
    let source = "namespace M {\n    export class M { foo() {} }\n}\n";
    let output = parse_lower_print(source, PrintOptions::es6());

    assert!(
        output.contains("M_1") || !output.contains("(function (M) {"),
        "Concrete same-name inner declaration must rename IIFE param.\nOutput:\n{output}"
    );
}

/// Regression: `export var [a, b] = init;` inside a namespace must lower
/// to a temp + indexed comma assignments — `var _a; _a = init, M.a =
/// _a[0], M.b = _a[1];`. The pre-fix emit was `M.a = init, M.b = init`
/// which evaluates the initializer twice and assigns the whole array
/// to each member.
#[test]
fn namespace_exported_array_destructuring_lowers_to_temp_and_indices() {
    let source = "namespace M {\n    export var [a, b] = [1, 2];\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a;"),
        "Destructuring lowering must declare a temp `_a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = [1, 2], M.a = _a[0], M.b = _a[1];"),
        "Array destructuring lowering must assign init once, then index.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("M.a = [1, 2], M.b = [1, 2];"),
        "Pre-fix shape (initializer evaluated per binding) must not appear.\nOutput:\n{output}"
    );
}

/// Object-pattern counterpart: keys are accessed by name, not index.
#[test]
fn namespace_exported_object_destructuring_lowers_to_temp_and_keys() {
    let source = "function f() { return { a4: 1, b4: 2, c4: 3 }; }\nnamespace m {\n    export var { a4, b4, c4 } = f();\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a;"),
        "Destructuring lowering must declare a temp `_a`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = f(), m.a4 = _a.a4, m.b4 = _a.b4, m.c4 = _a.c4;"),
        "Object destructuring lowering must assign init once, then access by key.\nOutput:\n{output}"
    );
}

/// Object-pattern with rename: `{ x: a }` → key `x`, target `M.a`.
#[test]
fn namespace_exported_object_destructuring_rename_uses_property_name() {
    let source =
        "function f() { return { x: 1 }; }\nnamespace m {\n    export var { x: a } = f();\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("_a = f(), m.a = _a.x;"),
        "Renamed object binding must read source key but assign to renamed target.\nOutput:\n{output}"
    );
}

/// Instantiation expressions strip the type arguments and wrap the
/// expression in parens (`fx<T>` → `(fx)`). The empty-arg parser-recovery
/// shape `fx<>` has no real arguments, so tsc emits the bare expression
/// without parens (`fx<>` → `fx`).
#[test]
fn instantiation_expression_with_args_wraps_in_parens() {
    let source = "declare function fx<T>(x: T): T;\nfunction f1() {\n    let f1 = fx<string>;\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("let f1 = (fx);"),
        "Non-empty instantiation expression must wrap the expression in parens.\nOutput:\n{output}"
    );
}

#[test]
fn instantiation_expression_with_empty_args_emits_bare() {
    let source = "declare function fx<T>(x: T): T;\nfunction f1() {\n    let f0 = fx<>;\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("let f0 = fx;"),
        "Empty type-argument list must emit the bare expression with no parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("let f0 = (fx);"),
        "Empty type-argument list must not retain the wrapping parens.\nOutput:\n{output}"
    );
}

/// Regression: `(({}) as any).foo` was emitting `(({}).foo)` — wrapping
/// the entire property access in extra outer parens because the
/// "object-literal access" emitter unconditionally wrote `(` and `)`
/// even when the inner emit was already producing `({})` (from the
/// nested `ParenthesizedExpression`). tsc emits `({}).foo`.
#[test]
fn property_access_on_paren_cast_paren_object_literal_emits_single_paren() {
    let source = "interface T {}\n(({}) as any as T).foo;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("({}).foo"),
        "Receiver should be `({{}})` with `.foo` suffix outside the parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(({}).foo)"),
        "Outer parens around the property access are redundant when the receiver is already parenthesized.\nOutput:\n{output}"
    );
}

#[test]
fn erased_object_literal_access_does_not_wrap_return_expression() {
    let source = r#"
function prop() {
    return ({ a: 1 } as { a: number }).a;
}
function elem(key: string) {
    return ({ a: 1 } as Record<string, number>)[key];
}
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("return { a: 1 }.a;"),
        "Return property access should not keep type-erasure parens.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return { a: 1 }[key];"),
        "Return element access should not keep type-erasure parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("return ({ a: 1 }.a);") && !output.contains("return ({ a: 1 }[key]);"),
        "Return expressions should not be wrapped like statement expressions.\nOutput:\n{output}"
    );
}

#[test]
fn erased_object_literal_access_wraps_arrow_concise_body() {
    let source = r#"
const prop = (x: string) => ({ "1": "one", "2": "two" } as { [key: string]: string }).x;
const elem = (x: string) => ({ "1": "one", "2": "two" } as { [key: string]: string })[x];
const nested = () => ({ a: { b: 1 } } as any).a.b;
const bracket = () => ({ a: { b: 1 } } as any)["a"].b;
const call = () => ({ f() { return 1; } } as any).f();
const plain = () => ({ a: 1 }).a;
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("const prop = (x) => ({ \"1\": \"one\", \"2\": \"two\" }.x);"),
        "Arrow property access must be grouped so the object literal is not parsed as a block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const elem = (x) => ({ \"1\": \"one\", \"2\": \"two\" }[x]);"),
        "Arrow element access must be grouped so the object literal is not parsed as a block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const nested = () => ({ a: { b: 1 } }.a.b);"),
        "Nested property access rooted at an erased object assertion must be grouped.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const bracket = () => ({ a: { b: 1 } }[\"a\"].b);"),
        "Nested element access rooted at an erased object assertion must be grouped.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const call = () => (({ f() { return 1; } }.f()));"),
        "Call chains rooted at an erased object assertion must be grouped.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const plain = () => ({ a: 1 }).a;"),
        "Already-parenthesized plain object access must not be double-wrapped.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("const plain = () => (({ a: 1 }).a);"),
        "Plain parenthesized access should not be treated as erased assertion output.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("=> { \"1\": \"one\", \"2\": \"two\" }"),
        "Arrow concise bodies must not start with a bare object literal after type erasure.\nOutput:\n{output}"
    );
}

#[test]
fn erased_object_literal_access_wraps_statement_expression() {
    let source = r#"
({ a: 1 } as { a: number }).a;
({ a: 1 } as Record<string, number>)["a"];
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("({ a: 1 }.a);"),
        "Statement property access must stay parenthesized to avoid parsing as a block.\nOutput:\n{output}"
    );
    assert!(
        output.contains("({ a: 1 }[\"a\"]);"),
        "Statement element access must stay parenthesized to avoid parsing as a block.\nOutput:\n{output}"
    );
}

/// Regression: `export default (X as T)` where `X` is a class or function
/// expression. The parens only existed to delimit the type cast; after
/// erasure they look removable, but stripping them silently changes the
/// export from "default-export an expression" to "default-export a
/// declaration". tsc preserves the parens.
#[test]
fn export_default_paren_class_expression_with_cast_keeps_parens() {
    let source = "export default (class Foo {} as any);\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("export default (class Foo {"),
        "Parens around the class expression must be preserved.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export default class Foo {"),
        "Stripping parens would change the export shape from expression to declaration.\nOutput:\n{output}"
    );
}

/// Counterpart: `export default class Foo {}` (no parens, no cast) is a
/// class declaration export and stays unchanged.
#[test]
fn export_default_class_declaration_unchanged() {
    let source = "export default class Foo {}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2015,
            ..Default::default()
        },
    );

    assert!(
        output.contains("export default class Foo"),
        "Bare default-class export should not gain parens.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("export default (class Foo"),
        "Bare default-class export must not be wrapped in parens.\nOutput:\n{output}"
    );
}

/// Regression: `({ foo, bar } = foo)` reassigns the same identifier on
/// both sides. Inlining as `(foo = foo.foo, bar = foo.bar)` reads
/// `foo.bar` AFTER `foo` has been clobbered. tsc captures the RHS in a
/// temp first: `_a = foo, foo = _a.foo, bar = _a.bar`.
#[test]
fn es5_assignment_destructuring_reassigning_rhs_uses_temp() {
    let source = "var foo: any = { foo: 1, bar: 2 };\nvar bar: any;\n({ foo, bar } = foo);\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("(_a = foo, foo = _a.foo, bar = _a.bar);"),
        "RHS reassigned by LHS must capture in `_a` first.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(foo = foo.foo, bar = foo.bar);"),
        "Direct inline reads the clobbered `foo` for the second access.\nOutput:\n{output}"
    );
}

/// Same hazard for `var { foo, baz } = foo;` — must lower to
/// `var _a = foo, foo = _a.foo, baz = _a.baz;`.
#[test]
fn es5_var_destructuring_reassigning_rhs_uses_temp() {
    let source = "var foo: any = { foo: 1, baz: 2 };\nvar { foo, baz } = foo;\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("var _a = foo, foo = _a.foo, baz = _a.baz;"),
        "Var declaration whose pattern reassigns the RHS identifier must capture in a temp.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var foo = foo.foo, baz = foo.baz;"),
        "Direct inline reads the clobbered `foo` for the second access.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_member_decorator_private_name_uses_native_static_block_scope() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var decorator: any;\nclass C1 {\n    #x;\n    @decorator((x: C1) => x.#x)\n    y() {}\n}\nclass C2 {\n    #x;\n    y(@decorator((x: C2) => x.#x) p) {}\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ESNext,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("static {\n        __decorate([\n            decorator((x) => x.#x),"),
        "Decorators that reference a private name must emit inside a class static block.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "static {\n        __decorate([\n            __param(0, decorator((x) => x.#x)),"
        ),
        "Parameter decorators that reference a private name must emit inside a class static block.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("}\n__decorate([\n    decorator((x) => x.#x),"),
        "Private-name decorator calls must not be emitted after the class body.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_async_generator_decorator_metadata_without_annotation_stays_void() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare const dec: MethodDecorator;\nclass A {\n    @dec async inferred() {}\n    @dec async *stream() { yield 1; }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2018,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("__metadata(\"design:returntype\", Promise)"),
        "Unannotated async non-generator method metadata should use Promise.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:returntype\", void 0)"),
        "Unannotated async generator method metadata should stay void 0.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_member_decorator_private_name_uses_lowered_private_scope() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var decorator: any;\nclass C1 {\n    #x;\n    @decorator((x: C1) => x.#x)\n    y() {}\n}\nclass C2 {\n    #x;\n    y(@decorator((x: C2) => x.#x) p) {}\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        use_define_for_class_fields: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __classPrivateFieldGet ="),
        "Lowered private-name decorator expressions must request __classPrivateFieldGet.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_C1_x = new WeakMap();\n(() => {\n    __decorate(["),
        "Lowered decorator calls must run after WeakMap initialization while private lowering state is live.\nOutput:\n{output}"
    );
    assert!(
        output.contains("decorator((x) => __classPrivateFieldGet(x, _C1_x, \"f\")),"),
        "Member decorator private access should lower through __classPrivateFieldGet.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__param(0, decorator((x) => __classPrivateFieldGet(x, _C2_x, \"f\"))),"),
        "Parameter decorator private access should lower through __classPrivateFieldGet.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("x.)"),
        "Private-name lowering must not leave an empty property access.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_decorator_trailing_comments_move_to_lowered_calls() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare function y(...args: any[]): any;\ntype T = number;\n@y(1 as T, () => C) // class decorator comment\nclass C<T> {\n    @y(null as T) // method decorator comment\n    method(@y x, y) {} // method comment\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        legacy_decorators: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("method(x, y) { } // method comment"),
        "The method's own trailing comment should remain on the method.\nOutput:\n{output}"
    );
    assert!(
        output.contains("y(null) // method decorator comment\n    ,"),
        "The erased method decorator's trailing comment should move to the lowered decorator expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("y(1, () => C) // class decorator comment"),
        "The erased class decorator's trailing comment should move to the lowered class decorator expression.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("class C {\n    //"),
        "Decorator comments must not leak into the class body after decorator tokens are erased.\nOutput:\n{output}"
    );
}

/// Regression: classes inside a namespace IIFE were missing
/// `__metadata("design:type", T)` calls under `--emitDecoratorMetadata`.
/// The namespace transformer instantiated an `ES5ClassTransformer` but
/// never forwarded the metadata flag, so decorator arrays only contained
/// the bare decorator without the type metadata.
#[test]
fn namespace_es5_class_emits_decorator_metadata() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "namespace M {\n    export function inject(t: any, k: string): void {}\n    export class Leg {}\n    export class Person {\n        @inject leftLeg: Leg;\n    }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("__metadata(\"design:type\", Leg)"),
        "Decorator metadata for the property type must emit inside the namespace IIFE.\nOutput:\n{output}"
    );
}

#[test]
fn legacy_accessor_decorator_metadata_uses_accessor_pair_types() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var dec: any;\nclass A {\n    @dec get x() { return 0; }\n    set x(value: number) { }\n}\nclass E {\n    @dec get x() { return 0; }\n}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        legacy_decorators: true,
        emit_decorator_metadata: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var __metadata ="),
        "Decorated accessors with metadata enabled must request the __metadata helper.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__metadata(\"design:type\", Number),\n    __metadata(\"design:paramtypes\", [Number])"
        ),
        "Accessor pairs should serialize the setter parameter type for design:type and design:paramtypes.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__metadata(\"design:type\", Object),\n    __metadata(\"design:paramtypes\", [])"
        ),
        "Getter-only accessors without an explicit type should use Object and an empty paramtypes array.\nOutput:\n{output}"
    );
}

/// Regression: ESM `--importHelpers` was not aliasing helper imports
/// when the helper name collides with a local declaration. tsc emits
/// `import { __decorate as __decorate_1 } from "tslib";` and uses
/// `__decorate_1(...)` at call sites to avoid shadowing.
#[test]
fn esm_import_helpers_aliases_when_helper_name_shadowed() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var dec: any, __decorate: any;\n@dec export class A {}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        import_helpers: true,
        legacy_decorators: true,
        emit_decorator_metadata: false,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import { __decorate as __decorate_1 } from \"tslib\";"),
        "Local `__decorate` shadowing must trigger import alias rename.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__decorate_1("),
        "Decorator call site must use the renamed alias.\nOutput:\n{output}"
    );
}

/// Counterpart: no local collision means no alias renaming.
#[test]
fn esm_import_helpers_no_alias_when_no_collision() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "declare var dec: any;\n@dec export class A {}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::ES2015,
        import_helpers: true,
        legacy_decorators: true,
        emit_decorator_metadata: false,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("import { __decorate } from \"tslib\";"),
        "No local collision: import name should stay unaliased.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("as __decorate_1"),
        "Don't rename when there's no local shadowing.\nOutput:\n{output}"
    );
}

/// Regression: a single-line `// comment` between two class members of an
/// ES5-lowered namespace IIFE was being dropped. The trailing-standalone
/// comment extraction was skipped for class-like members on the (now
/// incorrect) assumption that the class sub-emitter would handle them, so
/// comments after the class's `}` but before the next member fell through
/// the cracks. tsc preserves them on their own line.
#[test]
fn namespace_es5_iife_preserves_line_comment_between_classes() {
    let source =
        "namespace m {\n    export class b {}\n\n    // class d\n    export class d {}\n}\n";
    let output = parse_lower_print(
        source,
        PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );

    assert!(
        output.contains("// class d"),
        "Single-line comment between sibling classes in a namespace IIFE must survive ES5 lowering.\nOutput:\n{output}"
    );
}

#[test]
fn namespace_marker_strings_do_not_trigger_missing_arrow_fixture_recovery() {
    let source = r#"namespace missingCurliesWithArrow {
  const a = "namespace withStatement";
  const b = "namespace withoutStatement";
  const c = "=> var k = 10;";
  const d = "=> };";

  export const actual = 1;
}

console.log(missingCurliesWithArrow.actual);
"#;
    let output = parse_lower_print(
        source,
        PrintOptions {
            module: ModuleKind::CommonJS,
            ..PrintOptions::es6()
        },
    );

    assert!(
        output.contains("missingCurliesWithArrow.actual = 1;"),
        "Valid namespace body should be emitted instead of fixture recovery output.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const a = \"namespace withStatement\";"),
        "String marker declarations should remain in the namespace body.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var a = () => { var k = 10; };") && !output.contains("var a = () => ;"),
        "Hardcoded missingCurliesWithArrow fixture output must not be emitted.\nOutput:\n{output}"
    );
}

/// Regression: when a System module already has a runtime import from
/// `"tslib"` (e.g. a side-effect `import "tslib";`), the wrapper-tslib
/// injection used to skip *both* the dep insertion *and* the helper
/// `Assign(tslib_1)` setter action. That left `tslib_1` hoisted but never
/// assigned, so `tslib_1.__decorate(...)` referenced an unassigned binding.
///
/// The structural rule: dep insertion is guarded against duplicates, but the
/// helper `Assign("tslib_1")` is injected whenever no source-supplied
/// `Assign` already exists for `"tslib"`. The companion case — a namespace
/// import like `import * as TSLib from "tslib"` — must NOT add the helper
/// `Assign("tslib_1")` because `commonjs_tslib_import_binding` will be
/// updated to the user binding (`TSLib`), so helper calls resolve through
/// that binding without a separate `tslib_1` setter.
#[test]
fn system_side_effect_tslib_import_still_assigns_helper_setter() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    let source = "import \"tslib\";\ndeclare var dec: any;\n@dec export class A {}\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::System,
        import_helpers: true,
        no_emit_helpers: true,
        legacy_decorators: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let register_line = output
        .lines()
        .find(|line| line.contains("System.register(["))
        .unwrap_or("");
    let tslib_count = register_line.matches("\"tslib\"").count();
    assert_eq!(
        tslib_count, 1,
        "System.register deps must list `\"tslib\"` exactly once when the source already imports from it.\nDeps line: {register_line}\nFull output:\n{output}"
    );
    assert!(
        output.contains("var tslib_1"),
        "Helper namespace binding `tslib_1` must be hoisted when no user `Assign` exists for `\"tslib\"`.\nOutput:\n{output}"
    );
    assert!(
        output.contains("tslib_1 = tslib_1_1;"),
        "Setter body must assign `tslib_1 = <setter-param>;` so `tslib_1.__decorate` is defined at execute time.\nOutput:\n{output}"
    );
    assert!(
        output.contains("tslib_1.__decorate"),
        "Decorator call must use the helper namespace binding.\nOutput:\n{output}"
    );
}

#[test]
fn system_namespace_tslib_import_uses_user_binding_for_helpers() {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
    use crate::lowering::LoweringPass;

    // `import * as TSLib from "tslib"` registers `Assign("TSLib")` for the
    // `"tslib"` dep, so the helper Assign should NOT be added — helper calls
    // resolve through `TSLib.__decorate` because
    // `commonjs_tslib_import_binding` is updated to the user binding.
    let source = "import * as TSLib from \"tslib\";\ndeclare var dec: any;\n@dec export class A {}\nexport const u = TSLib;\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::System,
        import_helpers: true,
        no_emit_helpers: true,
        legacy_decorators: true,
        ..Default::default()
    };
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    let register_line = output
        .lines()
        .find(|line| line.contains("System.register(["))
        .unwrap_or("");
    assert_eq!(
        register_line.matches("\"tslib\"").count(),
        1,
        "Single `\"tslib\"` dep entry.\nDeps line: {register_line}\nFull output:\n{output}"
    );
    assert!(
        output.contains("TSLib.__decorate"),
        "Helper calls must resolve through the user binding `TSLib`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("tslib_1 = "),
        "No redundant helper-binding setter line when the user already provides a binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var tslib_1"),
        "No redundant helper-binding hoist when the user already provides a binding.\nOutput:\n{output}"
    );
}

/// Private field-stored-function calls must use `__classPrivateFieldGet(...).call(receiver)`.
#[test]
fn test_private_field_call_expression_lowering() {
    let source = r#"class Foo {
    #field = function() { return 1; };
    test() {
        this.#field();
        this.#field(1, 2);
    }
}
"#;
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("__classPrivateFieldGet(this, _Foo_field, \"f\").call(this)"),
        "Private field call with no args should use .call(this)\nOutput:\n{output}"
    );
    assert!(
        output.contains("__classPrivateFieldGet(this, _Foo_field, \"f\").call(this, 1, 2)"),
        "Private field call with args should pass them after this\nOutput:\n{output}"
    );
}

/// Private method calls must use `__classPrivateFieldGet(..., "m", fn).call(receiver)`.
#[test]
fn test_private_method_call_expression_lowering() {
    let source = r#"class Bar {
    #run() { return 1; }
    test() {
        this.#run();
        this.#run(1, 2);
    }
}
"#;
    let output = parse_lower_print(source, PrintOptions::es6());
    assert!(
        output.contains("__classPrivateFieldGet(this, _Bar_instances, \"m\", _Bar_run).call(this)"),
        "Private method call should use __classPrivateFieldGet with 'm' kind\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__classPrivateFieldGet(this, _Bar_instances, \"m\", _Bar_run).call(this, 1, 2)"
        ),
        "Private method call with args should pass them after this\nOutput:\n{output}"
    );
}

/// Non-simple receivers must be captured in a temp var — `__classPrivateFieldGet(_a = expr, ...).call(_a)`.
/// Without the temp, the receiver is evaluated twice (side effects fire twice and `this` mismatches).
#[test]
fn test_private_field_call_complex_receiver() {
    let source = r#"class Foo {
    #fn = () => 1;
    static getInstance(): Foo { return new Foo(); }
    test() {
        Foo.getInstance().#fn();
        Foo.getInstance().#fn(1, 2);
    }
}
"#;
    let output = parse_lower_print(source, PrintOptions::es6());
    // Each call site gets its own temp (_a, _b, …) so both calls can coexist safely.
    // tsc wraps the receiver assignment in parens: `(_a = expr)`.
    assert!(
        output.contains("__classPrivateFieldGet((_a = Foo.getInstance()), _Foo_fn, \"f\").call(_a)"),
        "First call should capture receiver in temp (with parens) and reuse it in .call()\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "__classPrivateFieldGet((_b = Foo.getInstance()), _Foo_fn, \"f\").call(_b, 1, 2)"
        ),
        "Second call should use its own temp (with parens) and reuse it in .call()\nOutput:\n{output}"
    );
    // Confirm the receiver is never emitted raw in a .call() position.
    assert!(
        !output.contains(".call(Foo.getInstance())"),
        "Complex receiver must not be evaluated twice in .call()\nOutput:\n{output}"
    );
}

/// Static private field calls: simple (class-name) receiver — no temp needed, class alias substituted.
#[test]
fn test_private_static_field_call_expression() {
    let source = r#"class Baz {
    static #method = function() { return 1; };
    static test() {
        Baz.#method();
        Baz.#method(1, 2);
    }
}
"#;
    let output = parse_lower_print(source, PrintOptions::es6());
    // Static members use the class alias (_a = Baz) as both receiver and state.
    assert!(
        output.contains("__classPrivateFieldGet(_a, _a, \"f\", _Baz_method).call(_a)"),
        "Static private field call should use class alias as receiver\nOutput:\n{output}"
    );
    assert!(
        output.contains("__classPrivateFieldGet(_a, _a, \"f\", _Baz_method).call(_a, 1, 2)"),
        "Static private field call with args should pass them after alias\nOutput:\n{output}"
    );
}
