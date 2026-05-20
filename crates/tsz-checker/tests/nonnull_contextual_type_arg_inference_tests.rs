//! Tests for contextual-type propagation through `expr!` to type-argument
//! inference of overloaded generic methods.
//!
//! Regression: tsz emitted a false TS2740 for
//!     let r: `SVGRectElement` = document.querySelector('.svg-rectangle')!;
//! because the contextual-return-type substitution computed for the matched
//! overload (`querySelector<E extends Element = Element>(s: string): E | null`)
//! was not applied to the final return type when other overloads in the same
//! group failed to bind their type parameter from the argument. The re-resolve
//! step in `overload_resolution.rs` would default `E` to `Element` and discard
//! the inferred binding `E = SVGRectElement` already computed via the
//! contextual return type.

#[test]
fn nonnull_assertion_propagates_contextual_type_through_overloaded_generic_call() {
    let source = r#"
interface Element { base: unknown; }
interface SVGRectElement extends Element { rect: unknown; }
interface MySVGMap { "rect": SVGRectElement; }
interface Foo {
  qs<K extends keyof MySVGMap>(s: K): MySVGMap[K] | null;
  qs<E extends Element = Element>(s: string): E | null;
}
declare const f: Foo;
let r: SVGRectElement = f.qs('.bad-name')!;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "expected contextual `SVGRectElement` to infer E for the generic overload, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn selector_key_overloads_do_not_use_return_context_for_selector_type_param() {
    let source = r#"
interface Element { base: unknown; }
interface RectElement extends Element { rect: unknown; }
interface CircleElement extends Element { circle: unknown; }
interface MathElement extends Element {}
interface Shapes { rect: RectElement; circle: CircleElement; }
interface Maths { mi: MathElement; }
interface Query {
  qs<K extends keyof Shapes>(selector: K): Shapes[K] | null;
  qs<K extends keyof Maths>(selector: K): Maths[K] | null;
  qs<E extends Element = Element>(selector: string): E | null;
}
declare const query: Query;
let rect: RectElement = query.qs('.svg-rectangle')!;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "expected non-key selector to skip keyed overloads and infer E from the contextual non-null target, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn lib_signature_selector_key_overloads_resolve_lazy_return_overlap() {
    let mut libs = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    libs.push(std::sync::Arc::new(
        tsz_binder::lib_loader::LibFile::from_source(
            "lib.selector.d.ts".to_string(),
            r#"
interface Element { base: unknown; }
interface RectElement extends Element { rect: unknown; }
interface CircleElement extends Element { circle: unknown; }
interface MathElement extends Element {}
interface Shapes { rect: RectElement; circle: CircleElement; }
interface Maths { mi: MathElement; }
interface Query {
  qs<K extends keyof Shapes>(selector: K): Shapes[K] | null;
  qs<K extends keyof Maths>(selector: K): Maths[K] | null;
  qs<E extends Element = Element>(selector: string): E | null;
}
declare const query: Query;
"#
            .to_string(),
        ),
    ));
    let source = r#"
let rect: RectElement = query.qs('.svg-rectangle')!;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::CheckerOptions::default(),
        &libs,
    );
    assert!(
        diagnostics.is_empty(),
        "expected lib-sourced lazy overloads to reject keyed selectors before using return context, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn overload_contextual_return_does_not_override_argument_inference() {
    let source = r#"
interface Api {
  choose<T>(value: T): T;
  choose(value: boolean): boolean;
}
declare const api: Api;
let n: number = api.choose("s");
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected TS2322 from argument-driven T = string to survive contextual return inference, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_does_not_override_bare_argument_inference() {
    let source = r#"
declare function choose<T>(value: T): T;
let n: number = choose("s");
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected TS2322 from direct argument inference to survive contextual return inference, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_can_refine_transparent_wrapper_inference() {
    let source = r#"
type Box<T> = { value: T };
declare function box<T>(value: T): Box<T>;
type Awaited<T> = T extends null | undefined ? T :
    T extends object & { then(onfulfilled: infer F, ...args: infer _): any; } ?
        F extends ((value: infer V, ...args: infer _) => any) ? Awaited<V> : never :
    T;
interface Promise<T> { value: T; }
declare function resolve<T>(value: T): Promise<Awaited<T>>;
type Readonly<T> = { readonly [P in keyof T]: T[P] };
declare function freeze<T>(value: T): Readonly<T>;
type Win = { kind: "a"; n: 1 } | { kind: "b"; n: 2 };
let boxed: Box<Win> = box({ kind: "a", n: 1 });
let tuple: Box<[string, number]> = box(["a", 1]);
let promise: Promise<true> = resolve(true);
let promiseObj: Promise<{ x: "x" }> = resolve({ x: "x" });
let frozen: readonly [string, number][] = freeze([["a", 1]]);
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "expected return context to refine widened object and tuple wrapper inference, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_refinement_still_checks_conflicting_literals() {
    let source = r#"
type Box<T> = { value: T };
declare function box<T>(value: T): Box<T>;
let boxed: Box<{ kind: "a" }> = box({ kind: "b" });
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected TS2322 when return-context refinement conflicts with the argument literal, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_refines_nested_awaited_by_container_element() {
    let source = r#"
type Awaited<T> = T extends null | undefined ? T : T;
declare function makeTuple<T>(): [Awaited<T>];
declare function makeArray<T>(): Awaited<T>[];
let tuple: [string] = makeTuple();
let array: string[] = makeArray();
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "expected nested Awaited<T> to infer from the tuple/array element target, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_does_not_treat_nonconditional_awaited_alias_as_builtin() {
    let source = r#"
type Awaited<T> = { value: T };
declare function wrap<T>(): Awaited<T>;
let wrapped: { value: string } = wrap();
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "expected a non-conditional alias named Awaited to infer structurally, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_checks_promise_awaited_conflicting_literal() {
    let source = r#"
const value = { kind: "b" };
let p: Promise<{ kind: "a" }> = Promise.resolve(value);
"#;
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let diagnostics = tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::CheckerOptions::default(),
        &libs,
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected TS2322 when Promise.resolve return context conflicts with a fresh literal, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_checks_readonly_tuple_conflicting_literal() {
    let source = r#"
type Readonly<T> = { readonly [P in keyof T]: T[P] };
declare function freeze<T>(value: T): Readonly<T>;
let frozen: readonly [string, number][] = freeze([["a", "oops"]]);
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected TS2322 when Readonly<T> return context conflicts with a tuple element, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_does_not_rewrite_widened_variable_through_transparent_wrapper() {
    let source = r#"
type Awaited<T> = T extends null | undefined ? T :
    T extends object & { then(onfulfilled: infer F, ...args: infer _): any; } ?
        F extends ((value: infer V, ...args: infer _) => any) ? Awaited<V> : never :
    T;
interface Promise<T> { value: T; }
declare function resolve<T>(value: T): Promise<Awaited<T>>;

const ret = { x: "x" };
let p: Promise<{ x: "x" }> = resolve(ret);
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected TS2322 when return context would otherwise rewrite widened `ret.x: string` to the literal type, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contextual_return_preserves_fresh_tuple_from_async_callback() {
    let source = r#"
interface ILocalExtension { isApplicationScoped: boolean; publisherId: string | null; }
type Metadata = { updated: boolean };
declare function scanMetadata(local: ILocalExtension): Promise<Metadata | undefined>;

async function copyExtensions(fromExtensions: ILocalExtension[]): Promise<void> {
  const extensions: [ILocalExtension, Metadata | undefined][] =
    await Promise.all(
      fromExtensions
        .filter((e) => !e.isApplicationScoped)
        .map(async (e) => [e, await scanMetadata(e)])
    );
}
"#;
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let diagnostics = tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::CheckerOptions {
            no_implicit_any: true,
            ..tsz_checker::CheckerOptions::default()
        },
        &libs,
    );
    assert!(
        diagnostics.is_empty(),
        "expected contextual return typing to preserve the fresh tuple from an async callback, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn nonnull_assertion_keys_off_structural_overload_shape_not_identifier_names() {
    // The fix is shape-based on the matched overload's signature; it should
    // work whichever names the user picks for the type parameter, the
    // method, or the tag-name map.
    for (tparam, method, mapname) in [
        ("E", "qs", "MySVGMap"),
        ("T", "find", "Names"),
        ("Result", "select", "_M"),
    ] {
        let source = format!(
            r#"
interface Element {{ base: unknown; }}
interface SVGRectElement extends Element {{ requiredExtensions: any; }}
interface {mapname} {{ "rect": SVGRectElement; }}
interface Foo {{
  {method}<K extends keyof {mapname}>(s: K): {mapname}[K] | null;
  {method}<{tparam} extends Element = Element>(s: string): {tparam} | null;
}}
declare const f: Foo;
let r: SVGRectElement = f.{method}('.bad-name')!;
"#
        );
        let diagnostics = tsz_checker::test_utils::check_source_diagnostics(&source);
        assert!(
            diagnostics.is_empty(),
            "names {tparam}/{method}/{mapname}: expected contextual return inference to avoid assignment errors, got: {:?}",
            diagnostics
                .iter()
                .map(|d| format!("TS{}: {}", d.code, d.message_text))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn nonnull_assertion_does_not_invent_type_when_no_contextual() {
    // Without a contextual return type, the generic overload's E defaults
    // to its bound (Element). This must not regress.
    let source = r#"
interface Element { base: unknown; }
interface SVGRectElement extends Element { rect: unknown; }
interface MySVGMap { "rect": SVGRectElement; }
interface Foo {
  qs<K extends keyof MySVGMap>(s: K): MySVGMap[K] | null;
  qs<E extends Element = Element>(s: string): E | null;
}
declare const f: Foo;
let r: Element = f.qs('.bad-name')!;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2740: Vec<_> = diagnostics.iter().filter(|d| d.code == 2740).collect();
    assert!(
        ts2740.is_empty(),
        "Element-targeted assignment should still type-check, got: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}
