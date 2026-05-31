//! Parity tests for downlevel dynamic `import()` emit under CommonJS / AMD / UMD / System.
//!
//! Structural rules matched to tsc:
//! - **TemplateExpression** specifiers (`` `./s/${id}` ``) evaluate to a string
//!   and are emitted directly in `Promise.resolve(<template>).then(s => ...)` for
//!   CJS/AMD/System — no extra `` `${…}` `` coercion wrapper is added.
//! - **Identifier** specifiers are coerced via `` `${id}` `` in the CJS/UMD CJS branch.
//! - **UMD** captures non-string, non-identifier specifiers (including template
//!   expressions) into a temp so both branches share the evaluated value.
//! - **AMD** and **System** always inline the specifier without a temp.
//! - **UMD conditional** parenthesization follows parent-expression binding, not a
//!   fixed rule — verified against `tsc` 6.x.
//!
//! Owner layer: emitter (`crates/tsz-emitter/src/emitter/expressions/call.rs`).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as emit;

fn cjs(source: &str) -> String {
    emit(
        source,
        PrintOptions {
            target: ScriptTarget::ES2022,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    )
}

fn umd(source: &str) -> String {
    emit(
        source,
        PrintOptions {
            target: ScriptTarget::ES2017,
            module: ModuleKind::UMD,
            ..Default::default()
        },
    )
}

fn umd_target(source: &str, target: ScriptTarget) -> String {
    emit(
        source,
        PrintOptions {
            target,
            module: ModuleKind::UMD,
            ..Default::default()
        },
    )
}

fn amd(source: &str) -> String {
    emit(
        source,
        PrintOptions {
            target: ScriptTarget::ES2017,
            module: ModuleKind::AMD,
            ..Default::default()
        },
    )
}

fn system(source: &str) -> String {
    emit(
        source,
        PrintOptions {
            target: ScriptTarget::ES2017,
            module: ModuleKind::System,
            ..Default::default()
        },
    )
}

// --- UMD conditional parenthesization by parent context ---------------------

#[test]
fn umd_await_operand_parenthesizes_conditional() {
    // `await` binds tighter than `?:`, so the conditional must be wrapped;
    // otherwise the emit means `(await __syncRequire) ? ... : ...`.
    let out = umd("export async function f() { await import('./s'); }");
    assert!(
        out.contains(
            "await (__syncRequire ? Promise.resolve().then(() => __importStar(require('./s'))) : "
        ),
        "await operand must parenthesize the UMD conditional.\nOutput:\n{out}"
    );
}

#[test]
fn umd_native_await_operand_parenthesizes_conditional_es2017() {
    // At ES2017 the `await` is retained (no async-to-generator lowering), and a
    // native `await` binds tighter than `?:`, so the conditional is wrapped.
    let out = umd_target(
        "export async function f() { const req = await import('./s'); }",
        ScriptTarget::ES2017,
    );
    assert!(
        out.contains("const req = await (__syncRequire ? "),
        "native await operand must parenthesize the UMD conditional.\nOutput:\n{out}"
    );
}

#[test]
fn umd_downleveled_await_yield_operand_does_not_parenthesize_conditional_es2015() {
    // At ES2015 the async body is downleveled to a generator, so `await`
    // becomes `yield`. A `yield` operand binds looser than `?:`, so tsc emits a
    // bare conditional with no parentheses (`yield a ? b : c` parses correctly).
    let out = umd_target(
        "export async function f() { const req = await import('./s'); }",
        ScriptTarget::ES2015,
    );
    assert!(
        out.contains("const req = yield __syncRequire ? "),
        "a downleveled await→yield operand must not parenthesize the UMD conditional.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("yield (__syncRequire ?"),
        "the bare conditional under yield must not be wrapped.\nOutput:\n{out}"
    );
}

#[test]
fn umd_downleveled_await_yield_does_not_parenthesize_at_es6() {
    // Same rule at the `es6` alias target: await→yield, bare conditional.
    let out = umd_target(
        "export async function f() { const req = await import('./s'); }",
        ScriptTarget::ES2016,
    );
    assert!(
        out.contains("const req = yield __syncRequire ? ")
            && !out.contains("yield (__syncRequire ?"),
        "ES2016 await→yield must emit a bare conditional with no parens.\nOutput:\n{out}"
    );
}

#[test]
fn umd_downleveled_yield_rule_is_independent_of_binding_and_specifier_names() {
    // Renaming the import binding and the module specifier must not change the
    // paren decision: the rule keys on the await→yield lowering shape, not on
    // any user-chosen identifier name.
    let out = umd_target(
        "export async function loader() { const modHandle = await import('./other-module'); }",
        ScriptTarget::ES2015,
    );
    assert!(
        out.contains("const modHandle = yield __syncRequire ? ")
            && out.contains("require('./other-module')")
            && !out.contains("yield (__syncRequire ?"),
        "renamed binding/specifier must still emit a bare conditional under yield.\nOutput:\n{out}"
    );
}

#[test]
fn umd_member_access_object_parenthesizes_conditional() {
    let out = umd("export async function f() { import('./s').then(m => m); }");
    assert!(
        out.contains(
            "(__syncRequire ? Promise.resolve().then(() => __importStar(require('./s'))) : "
        ) && out.contains(").then(__importStar)).then(m => m);"),
        "member-access object must parenthesize the UMD conditional.\nOutput:\n{out}"
    );
}

#[test]
fn umd_statement_position_does_not_parenthesize_conditional() {
    let out = umd("export async function f() { import('./s'); }");
    assert!(
        out.contains(
            "{ __syncRequire ? Promise.resolve().then(() => __importStar(require('./s'))) : "
        ),
        "a bare conditional in statement position must not be parenthesized.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("(__syncRequire ?"),
        "statement position should not wrap the conditional.\nOutput:\n{out}"
    );
}

#[test]
fn umd_return_position_does_not_parenthesize_conditional() {
    let out = umd("export async function f() { return import('./s'); }");
    assert!(
        out.contains(
            "return __syncRequire ? Promise.resolve().then(() => __importStar(require('./s'))) : "
        ),
        "a bare conditional in return position must not be parenthesized.\nOutput:\n{out}"
    );
}

// --- Specifier shapes: capture rule + CommonJS branch form ------------------

#[test]
fn umd_string_specifier_inlines_without_temp() {
    let out = umd("export async function f() { import('./s'); }");
    assert!(
        out.contains("require('./s')") && !out.contains("_a = "),
        "string-literal specifier must be inlined without a temp.\nOutput:\n{out}"
    );
}

#[test]
fn umd_identifier_specifier_uses_template_form_without_temp() {
    // Bare identifier: no temp, CommonJS branch coerces via a template.
    let out = umd("export async function f(p: string) { import(p); }");
    assert!(
        out.contains("__syncRequire ? Promise.resolve(`${p}`).then(s => __importStar(require(s))) : new Promise((resolve_1, reject_1) => { require([p], resolve_1, reject_1); }).then(__importStar);"),
        "identifier specifier must use the template CommonJS form and inline `[p]`, with no temp.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("_a = p"),
        "identifier specifier must not be captured into a temp.\nOutput:\n{out}"
    );
}

#[test]
fn umd_complex_specifier_is_captured_into_temp() {
    // Property-access specifier is captured once and reused in both branches.
    let out = umd("class C { _p = 'x'; async m() { return import(this._p); } }");
    assert!(
        out.contains("_a = this._p, __syncRequire ? Promise.resolve().then(() => __importStar(require(_a))) : new Promise((resolve_1, reject_1) => { require([_a], resolve_1, reject_1); }).then(__importStar);"),
        "a complex specifier must be captured into a temp reused by both branches.\nOutput:\n{out}"
    );
}

#[test]
fn umd_captured_sequence_parenthesizes_in_assignment_position() {
    // A comma sequence (captured temp) needs parens as a variable initializer,
    // unlike a bare conditional.
    let out = umd("class C { _p = 'x'; async m() { const a = import(this._p); return a; } }");
    assert!(
        out.contains("const a = (_a = this._p, __syncRequire ?"),
        "a captured comma sequence must be parenthesized as a variable initializer.\nOutput:\n{out}"
    );
}

// --- AMD never captures and never parenthesizes -----------------------------

#[test]
fn amd_identifier_specifier_inlines_without_temp() {
    let out = amd("export async function f(p: string) { return import(p); }");
    assert!(
        out.contains("return new Promise((resolve_1, reject_1) => { require([p], resolve_1, reject_1); }).then(__importStar);"),
        "AMD inlines the identifier specifier without a temp.\nOutput:\n{out}"
    );
}

#[test]
fn amd_complex_specifier_inlines_without_temp() {
    // AMD has a single branch, so even a complex specifier is inlined raw.
    let out = amd("class C { _p = 'x'; async m() { await import(this._p); } }");
    assert!(
        out.contains("require([this._p], resolve_1, reject_1)") && !out.contains("_a = this._p"),
        "AMD inlines a complex specifier raw with no temp.\nOutput:\n{out}"
    );
}

// --- Iteration-variable independence: the rule is structural, not a name -----

#[test]
fn umd_identifier_rule_is_independent_of_specifier_name() {
    let out = umd("export async function f(moduleSpecifier: string) { import(moduleSpecifier); }");
    assert!(
        out.contains("Promise.resolve(`${moduleSpecifier}`).then(s => __importStar(require(s)))"),
        "the identifier template form must not depend on the specifier name.\nOutput:\n{out}"
    );
}

// --- System drops the options/attributes argument ---------------------------

#[test]
fn system_dynamic_import_drops_options_argument() {
    let out =
        system("export async function f() { await import('./s', { with: { type: 'json' } }); }");
    assert!(
        out.contains("context_1.import('./s')"),
        "System dynamic import emits only the specifier.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("with: { type:"),
        "System dynamic import must drop the import-attributes argument.\nOutput:\n{out}"
    );
}

#[test]
fn system_dynamic_import_inlines_identifier_specifier() {
    let out = system("export async function f(p: string) { await import(p); }");
    assert!(
        out.contains("context_1.import(p)"),
        "System dynamic import inlines the identifier specifier.\nOutput:\n{out}"
    );
}

// --- Template expression specifier: no double-wrapping ----------------------
//
// Rule: a TemplateExpression (`` `./s/${id}` ``) already evaluates to a string.
// tsc emits it directly in Promise.resolve() for CJS — no extra `${…}` wrapper.
// For identifiers, tsc adds the `${…}` coercion; for templates it does not.

#[test]
fn cjs_template_specifier_emits_directly_in_promise_resolve() {
    // Structural rule: Promise.resolve(`./widgets/${id}`) — not Promise.resolve(`${`./widgets/${id}`}`)
    let out = cjs("async function load(id: string) { return await import(`./widgets/${id}`); }");
    assert!(
        out.contains("Promise.resolve(`./widgets/${id}`).then(s => "),
        "template specifier must be emitted directly in Promise.resolve() without an extra wrapper.\nOutput:\n{out}"
    );
    assert!(
        out.contains("__importStar(require(s))"),
        "template specifier path must still wrap require() with __importStar.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("Promise.resolve(`${`"),
        "template specifier must not be double-wrapped in another template literal.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_template_specifier_multi_span_emits_directly() {
    // Two substitutions: rule holds regardless of the number of template spans.
    let out = cjs(
        "async function load(prefix: string, id: string) { return await import(`${prefix}/${id}.js`); }",
    );
    assert!(
        out.contains("Promise.resolve(`${prefix}/${id}.js`).then(s => "),
        "multi-span template must be emitted directly in Promise.resolve().\nOutput:\n{out}"
    );
    assert!(
        !out.contains("Promise.resolve(`${`"),
        "multi-span template must not be double-wrapped.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_template_specifier_rule_is_independent_of_variable_name() {
    // The fix is structural (TemplateExpression kind), not name-specific.
    let out = cjs(
        "async function load(routeSegment: string) { return await import(`./routes/${routeSegment}/page`); }",
    );
    assert!(
        out.contains("Promise.resolve(`./routes/${routeSegment}/page`).then(s => "),
        "template form must not depend on the bound variable name.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_identifier_specifier_still_uses_coercion_wrapper() {
    // Identifier specifiers are NOT templates and still need the `${…}` coercion.
    // This is the existing behaviour — verify it is unchanged by the template fix.
    let out = cjs("async function load(p: string) { return await import(p); }");
    assert!(
        out.contains("Promise.resolve(`${p}`).then(s => "),
        "identifier specifier must still use the backtick-coercion form.\nOutput:\n{out}"
    );
}

#[test]
fn amd_template_specifier_inlines_in_require_array() {
    // AMD never captures; template expression must be inlined in require([...]).
    let out = amd("async function load(id: string) { return await import(`./widgets/${id}`); }");
    assert!(
        out.contains("require([`./widgets/${id}`]"),
        "AMD must inline the template specifier in require([...]).\nOutput:\n{out}"
    );
    assert!(
        !out.contains("_a = "),
        "AMD must not capture the template specifier into a temp.\nOutput:\n{out}"
    );
}

#[test]
fn system_template_specifier_inlines_in_context_import() {
    // System module: template expression passed directly to context_1.import().
    let out = system("async function load(id: string) { return await import(`./widgets/${id}`); }");
    assert!(
        out.contains("context_1.import(`./widgets/${id}`)"),
        "System must inline the template specifier in context_1.import().\nOutput:\n{out}"
    );
}

#[test]
fn umd_template_specifier_captured_into_temp() {
    // UMD has two branches; a TemplateExpression is not string-like or identifier,
    // so it is captured into a temp that both branches share.
    let out = umd("async function load(id: string) { return await import(`./widgets/${id}`); }");
    assert!(
        out.contains("_a = `./widgets/${id}`"),
        "UMD must capture the template specifier into a temp.\nOutput:\n{out}"
    );
    assert!(
        out.contains("__syncRequire ? Promise.resolve().then("),
        "UMD CJS branch with captured temp must use the lazy Promise.resolve().then() form.\nOutput:\n{out}"
    );
    assert!(
        out.contains("__importStar(require(_a))"),
        "UMD CJS branch must wrap require() with __importStar.\nOutput:\n{out}"
    );
}
