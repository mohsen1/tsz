//! Parity tests for downlevel dynamic `import()` emit under CommonJS / AMD / UMD / System.
//!
//! Structural rules matched to tsc (verified against `tsc` 5.8.3):
//! - **CJS / UMD-CJS branch** wrap every *non-string-literal* specifier in a
//!   `` `${…}` `` template coercion so the specifier is evaluated eagerly, then
//!   pass the resolved string to `require`:
//!   `Promise.resolve(`${spec}`).then(s => __importStar(require(s)))`. This applies
//!   uniformly to identifiers, property accesses, conditionals **and**
//!   `TemplateExpression` specifiers. A template specifier is therefore *nested*
//!   inside the coercion wrapper — `` `./s/${id}` `` emits as
//!   `` Promise.resolve(`${`./s/${id}`}`) ``. tsc does not special-case templates
//!   here; the wrapper is added unconditionally for non-inlineable arguments.
//! - **String-literal** (and no-substitution-template) specifiers stay lazy:
//!   `Promise.resolve().then(() => __importStar(require("mod")))` — no coercion.
//! - **AMD** and **System** always inline the specifier verbatim without a temp
//!   (`require([`./s/${id}`], …)` / `context_1.import(`./s/${id}`)`).
//! - **UMD** captures non-string, non-identifier specifiers (including template
//!   expressions) into a temp so both branches share the evaluated value; the
//!   captured temp then uses the lazy `Promise.resolve().then(() => require(_a))`
//!   form in the sync branch.
//! - **UMD conditional** parenthesization follows parent-expression binding, not a
//!   fixed rule — verified against `tsc` 6.x.
//!
//! - **`esModuleInterop` gate**: the `__importStar` wrapper (CJS/UMD-CJS) and the
//!   `.then(__importStar)` callback (AMD/UMD-AMD) are emitted ONLY under
//!   `esModuleInterop`. With interop off, tsc emits a bare `require(...)` /
//!   `new Promise(...)`. The shape tests below run with interop on; the
//!   interop-off forms are asserted in the dedicated section at the end.
//!
//! Owner layer: emitter (`crates/tsz-emitter/src/emitter/expressions/call.rs`).

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::PrintOptions;

#[path = "test_support.rs"]
mod test_support;

use test_support::parse_and_lower_print as emit;

// The `__importStar`-wrapped forms asserted below match tsc's output under
// `esModuleInterop` (with interop off tsc emits a bare `require(...)`; that
// gate is covered separately by the interop-off tests at the end of this file
// and in `cjs_module_exports.rs`). These helpers therefore enable interop.
fn cjs(source: &str) -> String {
    emit(
        source,
        PrintOptions {
            target: ScriptTarget::ES2022,
            module: ModuleKind::CommonJS,
            es_module_interop: true,
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
            es_module_interop: true,
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
            es_module_interop: true,
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
            es_module_interop: true,
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
            es_module_interop: true,
            ..Default::default()
        },
    )
}

/// Emit with `esModuleInterop` off (tsc's default), used by the interop-gate
/// tests at the end of this file.
fn emit_no_interop(source: &str, module: ModuleKind, target: ScriptTarget) -> String {
    emit(
        source,
        PrintOptions {
            target,
            module,
            es_module_interop: false,
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

// --- Non-string-literal CJS specifiers: eager `${…}` coercion wrapper --------
//
// Rule (verified against tsc 5.8.3): for CommonJS downlevel emit, tsc wraps every
// non-string-literal `import()` specifier in a `` `${arg}` `` template so the
// argument is evaluated eagerly, then resolves through `require(s)`:
//
//   import(p)              -> Promise.resolve(`${p}`).then(s => __importStar(require(s)))
//   import(`./w/${id}`)    -> Promise.resolve(`${`./w/${id}`}`).then(s => __importStar(require(s)))
//
// The wrapper is added unconditionally for non-inlineable arguments; tsc does NOT
// special-case `TemplateExpression` specifiers, so a template is *nested* inside
// the coercion wrapper (a `` `${`…`}` `` shape). Only string literals (and
// no-substitution templates) skip the wrapper and use the lazy form.

#[test]
fn cjs_template_specifier_is_nested_in_coercion_wrapper() {
    // tsc 5.8.3: Promise.resolve(`${`./widgets/${id}`}`).then(s => __importStar(require(s)))
    let out = cjs("async function load(id: string) { return await import(`./widgets/${id}`); }");
    assert!(
        out.contains("Promise.resolve(`${`./widgets/${id}`}`).then(s => "),
        "template specifier must be nested inside the `${{…}}` coercion wrapper.\nOutput:\n{out}"
    );
    assert!(
        out.contains("__importStar(require(s))"),
        "template specifier path must wrap require() with __importStar.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_template_specifier_multi_span_is_nested_in_wrapper() {
    // Two substitutions: rule holds regardless of the number of template spans.
    // tsc 5.8.3: Promise.resolve(`${`${prefix}/${id}.js`}`).then(s => ...)
    let out = cjs(
        "async function load(prefix: string, id: string) { return await import(`${prefix}/${id}.js`); }",
    );
    assert!(
        out.contains("Promise.resolve(`${`${prefix}/${id}.js`}`).then(s => "),
        "multi-span template must be nested inside the coercion wrapper.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_template_specifier_rule_is_independent_of_variable_name() {
    // The rule is structural (non-string-literal kind), not name-specific.
    // tsc 5.8.3: Promise.resolve(`${`./routes/${routeSegment}/page`}`).then(s => ...)
    let out = cjs(
        "async function load(routeSegment: string) { return await import(`./routes/${routeSegment}/page`); }",
    );
    assert!(
        out.contains("Promise.resolve(`${`./routes/${routeSegment}/page`}`).then(s => "),
        "template form must not depend on the bound variable name.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_identifier_specifier_uses_coercion_wrapper() {
    // Identifier specifiers use the `${…}` coercion (eager evaluation) form.
    // tsc 5.8.3: Promise.resolve(`${p}`).then(s => __importStar(require(s)))
    let out = cjs("async function load(p: string) { return await import(p); }");
    assert!(
        out.contains("Promise.resolve(`${p}`).then(s => __importStar(require(s)))"),
        "identifier specifier must use the backtick-coercion form.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_property_access_specifier_uses_coercion_wrapper() {
    // Property-access specifiers are non-string-literal -> coercion wrapper.
    // tsc 5.8.3: Promise.resolve(`${o.path}`).then(s => __importStar(require(s)))
    let out = cjs("async function load(o: { path: string }) { return await import(o.path); }");
    assert!(
        out.contains("Promise.resolve(`${o.path}`).then(s => __importStar(require(s)))"),
        "property-access specifier must use the `${{…}}` coercion wrapper.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_conditional_specifier_uses_coercion_wrapper() {
    // Conditional specifiers are non-string-literal -> coercion wrapper.
    // tsc 5.8.3: Promise.resolve(`${b ? "./a" : "./b"}`).then(s => __importStar(require(s)))
    let out =
        cjs("async function load(b: boolean) { return await import(b ? \"./a\" : \"./b\"); }");
    assert!(
        out.contains(
            "Promise.resolve(`${b ? \"./a\" : \"./b\"}`).then(s => __importStar(require(s)))"
        ),
        "conditional specifier must use the `${{…}}` coercion wrapper.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_string_literal_specifier_stays_lazy() {
    // String literals skip the coercion wrapper and use the lazy then(() => …) form.
    // tsc 5.8.3: Promise.resolve().then(() => __importStar(require("./mod")))
    let out = cjs("async function load() { return await import(\"./mod\"); }");
    assert!(
        out.contains("Promise.resolve().then(() => __importStar(require(\"./mod\")))"),
        "string-literal specifier must use the lazy Promise.resolve().then() form.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("Promise.resolve(`"),
        "string-literal specifier must not be wrapped in a coercion template.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_no_substitution_template_specifier_stays_lazy() {
    // A NoSubstitutionTemplateLiteral is string-like, so it follows the lazy form
    // with the specifier inlined verbatim — no `${…}` coercion wrapper.
    // tsc 5.8.3: Promise.resolve().then(() => __importStar(require(`./mod`)))
    let out = cjs("async function load() { return await import(`./mod`); }");
    assert!(
        out.contains("Promise.resolve().then(() => __importStar(require(`./mod`)))"),
        "no-substitution template specifier must use the lazy form with the specifier inlined.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("Promise.resolve(`${"),
        "no-substitution template specifier must not be wrapped in a `${{…}}` coercion.\nOutput:\n{out}"
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

// --- Async-lowered dynamic import: ES5 target uses function(), ES2015+ uses () => ------
//
// When an async function is lowered to a __awaiter/__generator state machine (at targets
// below ES2017), dynamic import() calls inside the generator body must use target-aware
// arrow-vs-function syntax: `function()` at ES5, `() =>` at ES2015+.
//
// Structural rule: `AsyncES5Transformer::dynamic_import_cjs_branch` /
// `dynamic_import_amd_branch` must key on `target_es5` (propagated from
// `Printer::ctx.target_es5` via `AsyncES5Emitter::set_target_es5`).

#[test]
fn async_lowered_cjs_es5_uses_function_form() {
    // At ES5, async function is lowered to __awaiter+__generator. The CJS import branch
    // must use `function ()` (not an arrow) because ES5 has no arrow functions.
    let out = emit(
        "export async function f() { const r = await import('./s'); }",
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::CommonJS,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains(
            "Promise.resolve().then(function () { return __importStar(require('./s')); })"
        ),
        "CJS ES5 async-lowered import must use function() form.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("Promise.resolve().then(() =>"),
        "CJS ES5 must not emit arrow form in async-lowered path.\nOutput:\n{out}"
    );
}

#[test]
fn async_lowered_amd_es5_uses_function_form() {
    // At ES5, the AMD import branch uses `function (resolve_N, reject_N)`.
    let out = emit(
        "export async function f() { await import('./s'); }",
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::AMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("new Promise(function (resolve_1, reject_1) { require(['./s'], resolve_1, reject_1); }).then(__importStar)"),
        "AMD ES5 async-lowered import must use function() form.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("new Promise((resolve_"),
        "AMD ES5 must not emit arrow form.\nOutput:\n{out}"
    );
}

#[test]
fn async_lowered_amd_es2015_uses_arrow_form() {
    // At ES2015 the async function is still lowered (async is ES2017), but arrow
    // functions exist. AMD import branch must use `() =>` (arrow) form.
    let out = emit(
        "export async function f() { await import('./s'); }",
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::AMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("new Promise((resolve_1, reject_1) => { require(['./s'], resolve_1, reject_1); }).then(__importStar)"),
        "AMD ES2015 async-lowered import must use arrow form.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("new Promise(function ("),
        "AMD ES2015 must not emit function() form.\nOutput:\n{out}"
    );
}

#[test]
fn async_lowered_umd_es5_uses_function_form_in_both_branches() {
    // UMD at ES5: both CJS and AMD branches in the conditional use function().
    let out = emit(
        "export async function f() { await import('./s'); }",
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::UMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains(
            "Promise.resolve().then(function () { return __importStar(require('./s')); })"
        ),
        "UMD ES5 CJS branch must use function() form.\nOutput:\n{out}"
    );
    assert!(
        out.contains("new Promise(function (resolve_1, reject_1) { require(['./s'], resolve_1, reject_1); }).then(__importStar)"),
        "UMD ES5 AMD branch must use function() form.\nOutput:\n{out}"
    );
}

#[test]
fn async_lowered_umd_es2015_uses_arrow_form_in_both_branches() {
    // UMD at ES2015: async is still lowered but arrows exist; both branches use () =>.
    let out = emit(
        "export async function f() { await import('./s'); }",
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::UMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("Promise.resolve().then(() => __importStar(require('./s')))"),
        "UMD ES2015 CJS branch must use arrow form.\nOutput:\n{out}"
    );
    assert!(
        out.contains("new Promise((resolve_1, reject_1) => { require(['./s'], resolve_1, reject_1); }).then(__importStar)"),
        "UMD ES2015 AMD branch must use arrow form.\nOutput:\n{out}"
    );
}

#[test]
fn async_lowered_amd_es2016_uses_arrow_form() {
    // ES2016 (ES7): async is still lowered, arrows exist. Identical rule to ES2015.
    let out = emit(
        "export async function f() { await import('./s'); }",
        PrintOptions {
            target: ScriptTarget::ES2016,
            module: ModuleKind::AMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("new Promise((resolve_1, reject_1) => { require(['./s'], resolve_1, reject_1); }).then(__importStar)"),
        "AMD ES2016 async-lowered import must use arrow form.\nOutput:\n{out}"
    );
}

#[test]
fn async_lowered_arrow_form_rule_is_structural_not_name_sensitive() {
    // Renaming the binding must not change the arrow-vs-function decision.
    let out = emit(
        "export async function loader() { const modHandle = await import('./widgets'); }",
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::AMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("new Promise((resolve_1, reject_1) => { require(['./widgets'], resolve_1, reject_1); }).then(__importStar)"),
        "Renamed binding must not change arrow form at ES2015.\nOutput:\n{out}"
    );
}

// --- Nested-class propagation: target_es5 must reach ES5ClassTransformer in embedded paths ---
//
// When a class with an async method lives *inside* an async function body or a namespace,
// the `ES5ClassTransformer` is constructed by `AsyncES5Transformer::lower_class_declaration_to_assignment`,
// `AsyncES5Transformer::es5_class_factory`, `AstToIr::convert_class_declaration`, or the
// namespace transformer — all of which previously omitted `set_target_es5`. The structural rule
// is identical to the top-level case: ES5 target → function() callbacks; ES2015+ → arrow callbacks.

#[test]
fn nested_class_async_method_amd_es5_uses_function_form() {
    // Class declaration inside an async function body: the class transformer is created
    // by AsyncES5Transformer::lower_class_declaration_to_assignment and must carry target_es5.
    let out = emit(
        "export async function outer() { class C { async m() { return await import('./s'); } } }",
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::AMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("new Promise(function (resolve_"),
        "Nested class async method at ES5 must use function() form for AMD import.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("new Promise((resolve_"),
        "Nested class async method at ES5 must not emit arrow form.\nOutput:\n{out}"
    );
}

#[test]
fn nested_class_async_method_amd_es2015_uses_arrow_form() {
    // Same shape as above but at ES2015 — arrows are available, import callback must use () =>.
    let out = emit(
        "export async function outer() { class C { async m() { return await import('./s'); } } }",
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::AMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("new Promise((resolve_"),
        "Nested class async method at ES2015 must use arrow form for AMD import.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("new Promise(function (resolve_"),
        "Nested class async method at ES2015 must not emit function() form.\nOutput:\n{out}"
    );
}

#[test]
fn namespace_class_async_method_amd_es5_uses_function_form() {
    // Class inside a namespace: the class transformer is created by NamespaceES5Transformer
    // and must carry target_es5 from the namespace transformer.
    let out = emit(
        "namespace N { export class C { async m() { return await import('./s'); } } }",
        PrintOptions {
            target: ScriptTarget::ES5,
            module: ModuleKind::AMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("new Promise(function (resolve_"),
        "Namespace class async method at ES5 must use function() form.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("new Promise((resolve_"),
        "Namespace class async method at ES5 must not emit arrow form.\nOutput:\n{out}"
    );
}

#[test]
fn namespace_class_async_method_amd_es2015_uses_arrow_form() {
    // Same namespace shape at ES2015 — must use arrow form.
    let out = emit(
        "namespace N { export class C { async m() { return await import('./s'); } } }",
        PrintOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::AMD,
            es_module_interop: true,
            ..Default::default()
        },
    );
    assert!(
        out.contains("new Promise((resolve_"),
        "Namespace class async method at ES2015 must use arrow form.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("new Promise(function (resolve_"),
        "Namespace class async method at ES2015 must not emit function() form.\nOutput:\n{out}"
    );
}

// --- esModuleInterop gate: interop OFF emits bare require / Promise ----------
// tsc default (`--esModuleInterop false`) does not wrap the dynamic `require`
// in `__importStar`, and AMD/UMD drop the trailing `.then(__importStar)`.

#[test]
fn cjs_string_literal_no_interop_emits_bare_require() {
    let out = emit_no_interop(
        "const m = import(\"./dep\");",
        ModuleKind::CommonJS,
        ScriptTarget::ES2022,
    );
    assert!(
        out.contains("Promise.resolve().then(() => require(\"./dep\"))"),
        "interop-off CJS string-literal import should be a bare require.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("__importStar"),
        "no __importStar wrapper.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_identifier_no_interop_emits_bare_require() {
    let out = emit_no_interop(
        "declare const p: string; const m = import(p);",
        ModuleKind::CommonJS,
        ScriptTarget::ES2022,
    );
    assert!(
        out.contains("Promise.resolve(`${p}`).then(s => require(s))"),
        "interop-off CJS identifier import should coerce then bare require.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("__importStar"),
        "no __importStar wrapper.\nOutput:\n{out}"
    );
}

#[test]
fn cjs_es5_no_interop_emits_bare_require_function_form() {
    let out = emit_no_interop(
        "const m = import(\"./dep\");",
        ModuleKind::CommonJS,
        ScriptTarget::ES5,
    );
    assert!(
        out.contains("Promise.resolve().then(function () { return require(\"./dep\"); })"),
        "interop-off ES5 CJS import should be a bare require function form.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("__importStar"),
        "no __importStar wrapper.\nOutput:\n{out}"
    );
}

#[test]
fn amd_no_interop_drops_then_import_star() {
    let out = emit_no_interop(
        "const m = import(\"./dep\");",
        ModuleKind::AMD,
        ScriptTarget::ES2017,
    );
    assert!(
        out.contains(
            "new Promise((resolve_1, reject_1) => { require([\"./dep\"], resolve_1, reject_1); })"
        ),
        "interop-off AMD import should emit the bare Promise form.\nOutput:\n{out}"
    );
    assert!(
        !out.contains(".then(__importStar)"),
        "interop-off AMD import must not append .then(__importStar).\nOutput:\n{out}"
    );
}

#[test]
fn umd_no_interop_drops_import_star_in_both_branches() {
    let out = emit_no_interop(
        "const m = import(\"./dep\");",
        ModuleKind::UMD,
        ScriptTarget::ES2017,
    );
    assert!(
        out.contains("__syncRequire ? Promise.resolve().then(() => require(\"./dep\")) : new Promise((resolve_1, reject_1) => { require([\"./dep\"], resolve_1, reject_1); })"),
        "interop-off UMD import should drop __importStar in both branches.\nOutput:\n{out}"
    );
    assert!(
        !out.contains("__importStar"),
        "no __importStar anywhere.\nOutput:\n{out}"
    );
}

#[test]
fn interop_off_rule_is_independent_of_specifier_name() {
    // The gate is structural (esModuleInterop), not keyed on the specifier name.
    let a = emit_no_interop(
        "const m = import(\"./alpha\");",
        ModuleKind::CommonJS,
        ScriptTarget::ES2022,
    );
    let b = emit_no_interop(
        "const m = import(\"./zeta\");",
        ModuleKind::CommonJS,
        ScriptTarget::ES2022,
    );
    assert!(a.contains("require(\"./alpha\")") && !a.contains("__importStar"));
    assert!(b.contains("require(\"./zeta\")") && !b.contains("__importStar"));
}
