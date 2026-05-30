//! Parity tests for downlevel dynamic `import()` emit under AMD / UMD / System.
//!
//! Structural rule: when a downlevel dynamic `import()` lowers to a low-
//! precedence substitution (a UMD `__syncRequire ? ... : ...` conditional, or a
//! `_a = spec, ...` comma sequence when a complex specifier is captured), tsz
//! must parenthesize it exactly where `tsc` does — based on the parent
//! expression, not on a fixed wrap — and must capture a temp only for the
//! specifier shapes `tsc` captures (UMD, non-string, non-identifier). System
//! modules emit `context_1.import(<specifier>)` and drop the options/attributes
//! argument. Verified against `tsc` 6.x.
//!
//! Owner layer: emitter (`crates/tsz-emitter/src/emitter/expressions/call.rs`).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as emit;

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
