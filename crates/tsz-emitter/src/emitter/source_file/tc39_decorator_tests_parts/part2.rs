#[test]
fn tc39_es5_public_accessors_schedule_computed_key_decorators() {
    let source = "\
declare var dec: any;
declare var method3: any;

class C {
    @dec(11) get method1() { return 0; }
    @dec(12) set method1(value) {}
    @dec(21) get [\"method2\"]() { return 0; }
    @dec(22) set [\"method2\"](value) {}
    @dec(31) get [method3]() { return 0; }
    @dec(32) set [method3](value) {}
}

class D {
    @dec(11) static get method1() { return 0; }
    @dec(12) static set method1(value) {}
    @dec(21) static get [\"method2\"]() { return 0; }
    @dec(22) static set [\"method2\"](value) {}
    @dec(31) static get [method3]() { return 0; }
    @dec(32) static set [method3](value) {}
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES5,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert!(
        output.contains("__runInitializers(this, _instanceExtraInitializers);")
            && output.contains(
            "Object.defineProperty(C.prototype, (_get_method1_decorators = [dec(11)], _set_method1_decorators = [dec(12)], _get_member_decorators = [dec(21)], _set_member_decorators = [dec(22)], _get_member_decorators_1 = [dec(31)], _b = __propKey(method3)), {"
        )
            && output.contains(
                "Object.defineProperty(C.prototype, (_set_member_decorators_1 = [dec(32)], _c = __propKey(method3)), {"
            )
            && output.contains(
                "Object.defineProperty(D, (_static_get_method1_decorators = [dec(11)], _static_set_method1_decorators = [dec(12)], _static_get_member_decorators = [dec(21)], _static_set_member_decorators = [dec(22)], _static_get_member_decorators_1 = [dec(31)], _b = __propKey(method3)), {"
            )
            && output.contains(
                "Object.defineProperty(D, (_static_set_member_decorators_1 = [dec(32)], _c = __propKey(method3)), {"
            )
            && output.contains(
                "__esDecorate(_a, null, _get_member_decorators_1, { kind: \"getter\", name: _b, static: false, private: false, access: { has: function (obj) { return _b in obj; }, get: function (obj) { return obj[_b]; } }, metadata: _metadata }, null, _instanceExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _set_member_decorators_1, { kind: \"setter\", name: _c, static: false, private: false, access: { has: function (obj) { return _c in obj; }, set: function (obj, value) { obj[_c] = value; } }, metadata: _metadata }, null, _instanceExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _static_get_member_decorators_1, { kind: \"getter\", name: _b, static: true, private: false, access: { has: function (obj) { return _b in obj; }, get: function (obj) { return obj[_b]; } }, metadata: _metadata }, null, _staticExtraInitializers);"
            )
            && output.contains(
                "__esDecorate(_a, null, _static_set_member_decorators_1, { kind: \"setter\", name: _c, static: true, private: false, access: { has: function (obj) { return _c in obj; }, set: function (obj, value) { obj[_c] = value; } }, metadata: _metadata }, null, _staticExtraInitializers);"
            ),
        "ES5 TC39 public accessors should sink decorator/proKey assignments into computed Object.defineProperty keys and use bracket access for computed names.\nOutput:\n{output}"
    );
}

/// Structural rule: when the ES-decorators transform synthesizes a constructor
/// for a *derived* class (one with an `extends` heritage clause) that has no
/// explicit constructor but needs to run member initializers, tsc emits a
/// zero-parameter constructor whose `super` call forwards the implicit
/// `arguments` object (`constructor() { super(...arguments); }`), not an
/// explicit rest parameter (`constructor(...args) { super(...args); }`).
///
/// This test varies the base/derived class, decorator, and member names to
/// prove the behavior is keyed on the structural shape (derived + decorated
/// member + synthesized ctor), not on a particular spelling.
#[test]
fn tc39_synthesized_ctor_for_derived_decorated_class_forwards_arguments_es2022() {
    let source = "\
declare var trace: any;
class Base {}
class Derived extends Base {
    @trace
    run() {}
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES2022,
            import_helpers: false,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("constructor() {") && output.contains("super(...arguments);"),
        "Synthesized ctor for a derived decorated class must be zero-parameter and forward `...arguments`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("constructor(...args)") && !output.contains("super(...args)"),
        "Synthesized ctor must not introduce an explicit rest parameter.\nOutput:\n{output}"
    );
}

/// Same structural rule under a different target (es2015) and a fresh set of
/// names (different base/derived/decorator/member identifiers) so a fix that
/// matched a particular spelling would not satisfy both tests.
#[test]
fn tc39_synthesized_ctor_for_derived_decorated_class_forwards_arguments_es2015() {
    let source = "\
declare var log: any;
class Animal {}
class Dog extends Animal {
    @log
    bark() {}
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES2015,
            import_helpers: false,
            use_define_for_class_fields: true,
            ..Default::default()
        },
    );

    assert!(
        output.contains("super(...arguments);"),
        "Synthesized ctor for a derived decorated class (es2015) must forward `...arguments`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("constructor(...args)") && !output.contains("super(...args)"),
        "Synthesized ctor (es2015) must not introduce an explicit rest parameter.\nOutput:\n{output}"
    );
}

/// Assert no hoisted `_classThis.<name> = super...` assignment leaked a bare
/// `super` token outside the class body (a `'super' keyword unexpected here`
/// `SyntaxError` at runtime). The hoist statements are the lines that begin
/// with the class-this alias receiver.
fn assert_no_hoisted_super_leak(output: &str) {
    for line in output.lines() {
        let trimmed = line.trim_start();
        assert!(
            !(trimmed.starts_with("_classThis.") && trimmed.contains("= super")),
            "Hoisted static-field assignment must rewrite `super`, not emit a bare \
             `super` outside the class body.\nOffending line: {trimmed}\nOutput:\n{output}"
        );
    }
}

#[test]
fn tc39_es2015_decorated_static_super_read_hoists_reflect_get() {
    // Read of `super.<name>` in a hoisted static field initializer (pre-ES2022,
    // decorated class) must lower to `Reflect.get(_classSuper, "<name>",
    // _classThis)`, never a bare `super.<name>` in `_classThis.<name> = ...`.
    let source = "\
declare var dec: any;
declare class Base { static p: number; }
@dec
class C extends Base {
    static a = super.p;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert_no_hoisted_super_leak(&output);
    assert!(
        output.contains("_classThis.a = Reflect.get(_classSuper, \"p\", _classThis);"),
        "Hoisted static-field read of `super.p` should be `Reflect.get(_classSuper, \"p\", _classThis)`.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_decorated_static_super_write_hoists_reflect_set_iife() {
    // Assignment to `super.<name>` in a hoisted static field initializer must
    // lower to an IIFE wrapping `Reflect.set(...)`, preserving a valid LHS.
    let source = "\
declare var dec: any;
declare class Base { static q: number; }
@dec
class D extends Base {
    static b = super.q = 1;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert_no_hoisted_super_leak(&output);
    assert!(
        output.contains("Reflect.set(_classSuper, \"q\""),
        "Hoisted static-field write to `super.q` should lower through `Reflect.set(_classSuper, \"q\", ...)`.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("= super.q ="),
        "Hoisted static-field write must not keep a bare `super.q =` LHS.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_decorated_static_super_prefix_update_hoists_reflect() {
    // Prefix update `++super.<name>` in a hoisted static field initializer must
    // route through `Reflect.get`/`Reflect.set`, not a bare `super`.
    let source = "\
declare var dec: any;
declare class Base { static r: number; }
@dec
class E extends Base {
    static c = ++super.r;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert_no_hoisted_super_leak(&output);
    assert!(
        output.contains("Reflect.set(_classSuper, \"r\"")
            && output.contains("Reflect.get(_classSuper, \"r\""),
        "Hoisted static-field prefix update of `super.r` should use `Reflect.get`/`Reflect.set`.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_decorated_static_super_index_read_hoists_reflect_get() {
    // The rule is keyed on the `super` element-access shape, not on the
    // identifier spelling: `super["<name>"]` must lower the same way.
    let source = "\
declare var dec: any;
declare class Base { static s: number; }
@dec
class F extends Base {
    static d = super[\"s\"];
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert_no_hoisted_super_leak(&output);
    assert!(
        output.contains("_classThis.d = Reflect.get(_classSuper, \"s\", _classThis);"),
        "Hoisted static-field read of `super[\"s\"]` should be `Reflect.get(_classSuper, \"s\", _classThis)`.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2020_decorated_static_super_read_hoists_reflect_get() {
    // The rule holds for every pre-ES2022 target that hoists static fields,
    // not only ES2015.
    let source = "\
declare var dec: any;
declare class Base { static t: number; }
@dec
class G extends Base {
    static e = super.t;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2020,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert_no_hoisted_super_leak(&output);
    assert!(
        output.contains("_classThis.e = Reflect.get(_classSuper, \"t\", _classThis);"),
        "ES2020 hoisted static-field read of `super.t` should also rewrite to `Reflect.get`.\nOutput:\n{output}"
    );
}

#[test]
fn tc39_es2015_decorated_static_plain_initializer_unchanged() {
    // Negative/fallback case: a static field initializer with no `super` must
    // keep its plain hoisted assignment (no Reflect rewrite, no IIFE).
    let source = "\
declare var dec: any;
@dec
class H {
    static f = 41 + 1;
}
";

    let output = emit_with_options(
        source,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            use_define_for_class_fields: false,
            ..Default::default()
        },
    );

    assert_no_hoisted_super_leak(&output);
    assert!(
        output.contains("_classThis.f = 41 + 1;"),
        "A super-free static-field initializer must keep its plain hoisted value.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Reflect.get(_classSuper") && !output.contains("Reflect.set(_classSuper"),
        "A super-free static-field initializer must not introduce scoped-super Reflect calls.\nOutput:\n{output}"
    );
}
