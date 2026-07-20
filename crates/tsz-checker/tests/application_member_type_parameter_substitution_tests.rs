//! Explicit generic arguments must retain their enclosing declaration's type-
//! parameter identity through inherited application-member lookup.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_diagnostics, check_source_strict_codes};

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

fn js_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "application-member-rebind.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn inherited_definition_member_keeps_explicit_secondary_binder() {
    let source = r#"
interface TypeDefinition {}

abstract class Schema<
  Output,
  Definition extends TypeDefinition = TypeDefinition,
  Input = Output,
> {
  readonly output!: Output;
  readonly input!: Input;
  readonly definition!: Definition;

  abstract parse(): Output;
}

type AnySchema = Schema<any, any, any>;
type SchemaItems = [AnySchema, ...AnySchema[]];

type Outputs<Items extends SchemaItems | []> = {
  [Key in keyof Items]: Items[Key] extends Schema<infer Output, any, any>
    ? Output
    : never;
};

type Inputs<Items extends SchemaItems | []> = {
  [Key in keyof Items]: Items[Key] extends Schema<any, any, infer Input>
    ? Input
    : never;
};

type OutputsWithTail<
  Items extends SchemaItems | [],
  Tail extends AnySchema | null = null,
> = Tail extends AnySchema ? [...Outputs<Items>, ...Tail["output"][]] : Outputs<Items>;

type InputsWithTail<
  Items extends SchemaItems | [],
  Tail extends AnySchema | null = null,
> = Tail extends AnySchema ? [...Inputs<Items>, ...Tail["input"][]] : Inputs<Items>;

interface TupleDefinition<
  Items extends SchemaItems | [] = SchemaItems,
  Tail extends AnySchema | null = null,
> extends TypeDefinition {
  items: Items;
  tail: Tail;
}

class TupleSchema<
  Items extends SchemaItems | [] = SchemaItems,
  Tail extends AnySchema | null = null,
> extends Schema<
  OutputsWithTail<Items, Tail>,
  TupleDefinition<Items, Tail>,
  InputsWithTail<Items, Tail>
> {
  parse(): OutputsWithTail<Items, Tail> {
    const tail = this.definition.tail;
    if (tail) {
      tail.parse();
    }
    return [] as any;
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "a successful member lookup on the instantiated definition must retain the secondary class binder",
    );
}

#[test]
fn cached_class_summary_rebinds_refined_secondary_binder() {
    let source = r#"
type AnyRunner = Runner<any>;

class Runner<Definition> {
  readonly definition!: Definition;

  prime<Other extends AnyRunner>() {}
}

interface SequenceDefinition<
  Head,
  Tail extends AnyRunner | null = null,
> {
  tail: Tail;
}

class Sequence<
  Head,
  Tail extends AnyRunner | null = null,
> extends Runner<SequenceDefinition<Head, Tail>> {
  use(): void {
    const tail = this.definition.tail;
    if (tail) {
      tail.prime();
    }
  }

  get tail() {
    return this.definition.tail;
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "a cached inherited member must rebind the summary's exact class parameter identity",
    );
}

#[test]
fn method_shadow_does_not_capture_enclosing_class_binder() {
    let source = r#"
type AnyRunner = Runner<any>;

class Runner<Definition> {
  readonly definition!: Definition;

  prime<Other extends AnyRunner>() {}
}

interface SequenceDefinition<
  Head,
  Tail extends AnyRunner | null = null,
> {
  tail: Tail;
}

class Sequence<
  Head,
  Tail extends AnyRunner | null = null,
> extends Runner<SequenceDefinition<Head, Tail>> {
  use<Tail extends object>(): void {
    const tail = this.definition.tail;
    if (tail) {
      tail.prime();
    }
  }

  get tail() {
    return this.definition.tail;
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "a same-named method binder must neither capture nor hide the enclosing class binder",
    );
}

#[test]
fn inherited_member_application_uses_enclosing_renamed_binder() {
    let source = r#"
abstract class Runnable {
  abstract run(): void;
}

interface Definition<Slot extends Runnable | null = null> {
  tail: Slot;
}

class Base<Configuration> {
  config!: Configuration;
}

class Derived<Tail extends Runnable | null = null>
  extends Base<Definition<Tail>> {
  exact(): Tail {
    return this.config.tail;
  }

  call(): void {
    const tail = this.config.tail;
    if (tail) tail.run();
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "the explicit application argument must remain the enclosing class binder",
    );
}

#[test]
fn wrapped_inherited_member_application_keeps_explicit_binder() {
    let source = r#"
abstract class Runnable {
  abstract run(): void;
}

interface Definition<Value extends Runnable | null = null> {
  tail: Value;
}

type Wrapped<Item extends Runnable | null> = Definition<Item>;

class Base<Configuration> {
  config!: Configuration;
}

class Derived<Tail extends Runnable | null = null>
  extends Base<Wrapped<Tail>> {
  exact(): Tail {
    return this.config.tail;
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "an alias wrapper must not replace the explicit outer binder with its own binder",
    );
}

#[test]
fn omitted_application_argument_uses_target_default() {
    let source = r#"
abstract class Runnable {
  abstract run(): void;
}

interface Definition<Slot extends Runnable | null = null> {
  tail: Slot;
}

class Base<Configuration> {
  config!: Configuration;
}

class Derived<Tail extends Runnable | null = null> extends Base<Definition> {
  invalid(): Tail {
    return this.config.tail;
  }
}
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "an omitted argument must keep `Definition`'s `null` default",
    );
}

#[test]
fn unrelated_same_named_default_is_not_captured() {
    let source = r#"
class Slot<Head, Tail = null> {
  value!: Tail;
}

class Derived<Tail extends object | null = null> {
  slot!: Slot<number>;

  invalid(): Tail {
    return this.slot.value;
  }
}
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "a foreign omitted binder with the same spelling must retain its own default",
    );
}

#[test]
fn cached_class_summary_rebinds_inherited_member_in_field_initializer() {
    let source = r#"
abstract class Runnable {
  abstract run(): void;
}

class Base<Payload> {
  value!: Payload;
}

class Derived<Tail extends Runnable | null = null> extends Base<Tail> {
  copy: Tail = this.value;
  invoke = this.value?.run();
}
"#;

    assert!(
        codes(source).is_empty(),
        "early class construction must rebind cached members to the active class binder",
    );
}

#[test]
fn jsdoc_class_template_rebinds_early_member_access() {
    let source = r#"
/**
 * @template U
 */
class Box {
  /** @type {U} */
  value = /** @type {any} */ (null);

  /** @type {U} */
  copy = this.value;

  /** @returns {U} */
  read() {
    return this.value;
  }
}
"#;

    let diagnostics = check_source(
        source,
        "application-member-rebind.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        diagnostics.is_empty(),
        "JSDoc class binders must stay active during early member recovery: {diagnostics:#?}",
    );
}

#[test]
fn jsdoc_method_template_does_not_hide_enclosing_class_binder() {
    let source = r#"
class Runnable {
  run() {}
}

/**
 * @template {Runnable | null} U
 */
class Derived {
  /** @type {U} */
  value = /** @type {any} */ (null);

  /**
   * @template U
   * @param {U} local
   * @returns {U}
   */
  use(local) {
    const value = this.value;
    if (value) {
      value.run();
    }
    return local;
  }
}
"#;

    let actual = js_codes(source);
    assert!(
        actual.is_empty(),
        "a same-named JSDoc method binder must not hide the constrained class binder: {actual:?}",
    );
}

#[test]
fn generic_arrow_field_shadow_keeps_enclosing_class_binder() {
    let source = r#"
class Base<T> {
  value!: T;
}

class Derived<U extends object | null> extends Base<U> {
  get = <U extends string>(local: U) => [this.value, local] as const;

  use(): U {
    return this.get("x")[0];
  }
}
"#;

    let actual = check_source(
        source,
        "generic-arrow-class-binder.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        actual.is_empty(),
        "a generic arrow's same-named binder must not capture the enclosing class binder: {actual:#?}",
    );
}

#[test]
fn same_surface_generic_arrow_shadow_keeps_distinct_binders() {
    let source = r#"
class Root<T extends string> {
  value!: T;
}

class Child<U extends string> extends Root<U> {
  pair = <U extends string>(local: U) => [this.value, local] as const;

  outer(): U {
    return this.pair("outer")[0];
  }

  inner(): string {
    return this.pair("inner")[1];
  }

  forwarded(value: U): U {
    return this.pair(value)[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "same-named binders with identical constraints must remain declaration-distinct",
    );
}

#[test]
fn same_named_constrained_arrow_binders_keep_the_ts2719_constraint_note() {
    let diagnostics = check_source_diagnostics(
        r#"
class Child<U extends string> {
  value!: U;
  invalid = <U extends string>(local: U): U => this.value;
}
"#,
    );
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 2719)
        .unwrap_or_else(|| panic!("expected TS2719, got {diagnostics:#?}"));
    assert_eq!(
        diagnostic.message_text,
        "Type 'U' is not assignable to type 'U'. Two different types with this name exist, but they are unrelated.",
    );
    let notes: Vec<_> = diagnostic
        .related_information
        .iter()
        .map(|related| (related.code, related.message_text.as_str(), related.depth))
        .collect();
    assert_eq!(
        notes,
        vec![(
            5075,
            "'U' is assignable to the constraint of type 'U', but 'U' could be instantiated with a different subtype of constraint 'string'.",
            0,
        )],
    );
}

#[test]
fn generic_arrow_without_prescan_type_keeps_lexical_class_this() {
    let source = r#"
class Bare<T> {
  arrow = <T extends string>(local: T) => [this, local] as const;
}

const bare = new Bare<number>();
const exact: Bare<number> = bare.arrow("value")[0];
"#;

    assert!(
        codes(source).is_empty(),
        "early enclosing-class setup must tolerate an unpublished Phase-0 `this` cache",
    );
}

#[test]
fn same_surface_generic_method_shadow_keeps_class_this_binder() {
    let source = r#"
class Root<T extends string> {
  value!: T;
}

class Child<U extends string> extends Root<U> {
  pair<U extends string>(local: U) {
    return [this.value, local] as const;
  }

  outer(): U {
    return this.pair("method")[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "method-local substitution must not capture the enclosing class binder",
    );
}

#[test]
fn renamed_generic_arrow_field_keeps_enclosing_class_binder() {
    let source = r#"
class Root<Payload> {
  value!: Payload;
}

class Child<State extends object | null> extends Root<State> {
  pair = <Label extends string>(local: Label) => [this.value, local] as const;

  outer(): State {
    return this.pair("label")[0];
  }

  inner(): string {
    return this.pair("label")[1];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "class-member recovery must be keyed by binder identity rather than spelling",
    );
}

#[test]
fn two_level_inherited_arrow_field_keeps_leaf_binder() {
    let source = r#"
class Root<Payload> {
  value!: Payload;
}

class Middle<Item> extends Root<Item> {}

class Leaf<State extends object | null> extends Middle<State> {
  pair = <Local extends string>(local: Local) => [this.value, local] as const;

  outer(): State {
    return this.pair("leaf")[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "class-summary rebinding must compose through two inheritance levels",
    );
}

#[test]
fn generic_class_expression_arrow_field_keeps_class_binder() {
    let source = r#"
class Root<Payload> {
  value!: Payload;
}

const Child = class<State extends object | null> extends Root<State> {
  pair = <Local extends string>(local: Local) => [this.value, local] as const;

  outer(): State {
    return this.pair("expression")[0];
  }
};

const child = new Child<{ ready: true }>();
const exact: { ready: true } = child.pair("ok")[0];
"#;

    assert!(
        codes(source).is_empty(),
        "class expressions must install the same enclosing-class binder context as declarations",
    );
}

#[test]
fn generic_arrow_local_binder_is_not_rebound_to_class_binder() {
    let source = r#"
class Root<Payload> {
  value!: Payload;
}

class Child<State extends object | null> extends Root<State> {
  pair = <Local extends string>(local: Local) => [this.value, local] as const;

  invalid(): State {
    return this.pair("local")[1];
  }
}
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "the arrow-local binder must remain distinct from the enclosing class binder",
    );
}

#[test]
fn concrete_inherited_arrow_field_uses_unbound_fallback() {
    let source = r#"
class Root<Payload> {
  value!: Payload;
}

class Concrete extends Root<number> {
  pair = <Local extends string>(local: Local) => [this.value, local] as const;

  outer(): number {
    return this.pair("concrete")[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "a class without active root binders must retain the concrete inherited member type",
    );
}

#[test]
fn regular_function_field_this_does_not_rebind_to_class_binder() {
    let source = r#"
class Root<Payload> {
  value!: Payload;
}

class Child<State extends object> extends Root<State> {
  trigger = <Label extends string>(label: Label) => [this.value, label] as const;

  field = function<Local extends string>(this: { value: Local }, local: Local) {
    const invalid: State = this.value;
    return local;
  };
}
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "a regular function's own `this` must not inherit field-arrow class recovery",
    );
}

#[test]
fn nested_regular_function_shadow_retains_its_own_this_binder() {
    let source = r#"
class Root<T extends string> {
  value!: T;
}

class Child<U extends string> extends Root<U> {
  method<U extends string>(local: U) {
    function nested<U extends string>(this: { value: U }) {
      const own: U = this.value;
      return own;
    }
    return [this.value, local, nested] as const;
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "a nested regular function's same-named binder and own `this` must stay local",
    );
}

#[test]
fn identity_scoped_shadow_remains_alpha_equivalent_to_renamed_generic() {
    let source = r#"
function outer<Outer>() {
  const shadowed = <Outer>(value: Outer) => value;
  const renamed = <Inner>(value: Inner) => value;

  const forward: typeof shadowed = renamed;
  const reverse: typeof renamed = shadowed;
  return [forward, reverse] as const;
}
"#;

    assert!(
        codes(source).is_empty(),
        "selective declaration identity must preserve generic alpha-equivalence",
    );
}

#[test]
fn trivial_type_parameter_call_does_not_capture_class_binder() {
    let source = r#"
class Root<T extends string> {
  value!: T;
}

class Child<U extends string> extends Root<U> {
  pair = <U extends string>(local: U) => [this.value, local] as const;

  relay<V extends string>(value: V): U {
    return this.pair(value)[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "the single-parameter fast path must substitute only the called signature's binder",
    );
}

#[test]
fn explicit_type_argument_call_does_not_capture_class_binder() {
    let source = r#"
class Root<T extends string> {
  value!: T;
}

class Child<U extends string> extends Root<U> {
  pair = <U extends string>(local: U) => [this.value, local] as const;

  relay<V extends string>(value: V): U {
    return this.pair<V>(value)[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "explicit instantiation must preserve a same-named captured class binder",
    );
}

#[test]
fn contextual_callback_call_keeps_class_and_local_binders_distinct() {
    let source = r#"
class Root<T extends object> {
  value!: T;
}

class Child<U extends object> extends Root<U> {
  transform = <U extends string>(callback: (value: U) => U) =>
    [this.value, callback(null as unknown as U)] as const;

  outer(): U {
    return this.transform(value => value)[0];
  }

  local(): string {
    return this.transform(value => value)[1];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "round-one contextual substitutions must retain the called signature's identity domain",
    );
}

#[test]
fn wrapped_generic_arrow_field_keeps_enclosing_class_binder() {
    let source = r#"
class Root<T> {
  value!: T;
}

class Child<U extends object | null> extends Root<U> {
  handlers = [<U extends string>(x: U) => [this.value, x] as const] as const;

  use(): U {
    return this.handlers[0]("x")[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "arrays and assertions around an arrow must remain transparent to lexical class recovery",
    );
}

#[test]
fn nested_generic_return_keeps_outer_owned_and_captured_binders_distinct() {
    let source = r#"
class Root<T> {
  value!: T;
}

class Child<U extends object> extends Root<U> {
  factory = <U extends string = string>() =>
    <V>() => [this.value, null as unknown as U] as const;

  use(): U {
    return this.factory()()[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "call-owned identity must survive through a nested generic return signature",
    );
}

#[test]
fn same_declaration_reentry_without_shadow_keeps_class_binder_stable() {
    let source = r#"
class Root<Payload> {
  value!: Payload;
}

class Child<State extends object> extends Root<State> {
  handlers = [<Label extends string>(label: Label) => [this.value, label] as const] as const;

  copy(other: Child<State>): State {
    const same: Child<State> = other;
    return same.handlers[0]("stable")[0];
  }
}
"#;

    assert!(
        codes(source).is_empty(),
        "re-pushing one declaration must not be classified as a lexical shadow",
    );
}

#[test]
fn materialized_generic_member_rewrites_outer_constraint_but_keeps_local_binder() {
    let source = r#"
type Expr<Outer, Key> = Outer | Key;

interface Builder<Outer, Key extends keyof Outer> {
  method<Local extends Expr<Outer, Key>>(value: Local): Local;
}

declare const builder: Builder<{ table: number }, "table">;

const key: "table" = builder.method("table");
const outer: { table: number } = builder.method({ table: 1 });
builder.method("other");
builder.method({ other: 1 });
"#;

    assert_eq!(
        codes(source),
        vec![2345, 2353],
        "materialization must concretize interface binders inside the local constraint without replacing `Local`",
    );
}

#[test]
fn nested_same_named_generic_arrow_substitutes_only_captured_outer_slot() {
    let source = r#"
class Holder<U> {
  nested = <U>(outer: U) => <U>(local: U) => [outer, local] as const;
}

const pair = new Holder<boolean>().nested(123)("local");
const outer: number = pair[0];
const local: string = pair[1];
"#;

    assert!(
        codes(source).is_empty(),
        "a nested same-named binder must shadow only its own declaration identity",
    );
}

#[test]
fn standalone_nested_generic_arrow_keeps_outer_and_inner_binders_distinct() {
    let source = r#"
function outer<U>(captured: U) {
  return <U>(local: U) => [captured, local] as const;
}

const pair = outer(123)("local");
const captured: number = pair[0];
const local: string = pair[1];
"#;

    assert!(
        codes(source).is_empty(),
        "nested function inference must preserve the captured outer binder independently of the local binder",
    );
}

#[test]
fn explicit_instantiation_expression_preserves_captured_class_binder() {
    let source = r#"
class Holder<U> {
  value!: U;
  pair = <U>(local: U) => [this.value, local] as const;
}

const holder = new Holder<number>();
const specialized = holder.pair<string>;
const pair = specialized("local");
const captured: number = pair[0];
const local: string = pair[1];
"#;

    assert!(
        codes(source).is_empty(),
        "explicit callable instantiation must apply the signature's exact binder domain",
    );
}

#[test]
fn computed_member_names_and_nested_class_headers_keep_outer_class_this() {
    let source = r#"
declare function key(callback: () => unknown): string;
declare function base(callback: () => unknown): new () => object;

class Root<T> {
  value!: T;
}

class Child<U> extends Root<U> {
  methods = {
    [key(() => this.value)]() {},
    get [key(() => this.value)]() { return 1; },
    set [key(() => this.value)](value: number) {},
  };

  nested = class Inner extends base(() => this.value) {
    [key(() => this.value)]() {}
  };
}

const child = new Child<{ ready: true }>();
const exact: { ready: true } = child.value;
"#;

    let actual = codes(source);
    assert!(
        actual.is_empty(),
        "computed names and class heritage must be checked in the enclosing field's lexical `this` scope, got {actual:?}",
    );
}

#[test]
fn nested_class_decorator_arrow_keeps_outer_class_this() {
    let source = r#"
declare function decorate(callback: () => unknown): any;

class Root<T> { value!: T; }
class Outer<U> extends Root<U> {
  nested = class Inner {
    @decorate(<U extends string>() => this.value)
    method() {}
  };
}
"#;

    let actual = codes(source);
    assert!(
        actual.is_empty(),
        "a class-element decorator expression runs in the enclosing field's lexical `this` scope, got {actual:?}",
    );
}

#[test]
fn nested_class_decorator_arrow_keeps_exact_renamed_outer_binder() {
    let source = r#"
declare function decorate(callback: () => unknown): any;

class Root<Element> { value!: Element; }
class Outer<OuterValue> extends Root<OuterValue> {
  nested = class Inner {
    @decorate(<Local extends string>(): OuterValue => this.value)
    method() {}
  };
}
"#;

    let actual = codes(source);
    assert!(
        actual.is_empty(),
        "lexical-header recovery must rebind the inherited member to the exact renamed outer binder, got {actual:?}",
    );
}

#[test]
fn nested_class_decorator_arrow_does_not_alias_same_named_local_binder() {
    let source = r#"
declare function decorate(callback: () => unknown): any;

class Root<T> { value!: T; }
class Outer<U> extends Root<U> {
  nested = class Inner {
    @decorate(<U extends string>(): U => this.value)
    method() {}
  };
}
"#;

    let diagnostics = check_source_diagnostics(source);
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 2719)
        .unwrap_or_else(|| panic!("expected TS2719, got {diagnostics:#?}"));
    assert_eq!(
        diagnostic.message_text,
        "Type 'U' is not assignable to type 'U'. Two different types with this name exist, but they are unrelated.",
    );
    assert_eq!(
        diagnostic
            .related_information
            .iter()
            .map(|related| (related.code, related.message_text.as_str(), related.depth))
            .collect::<Vec<_>>(),
        vec![(
            5082,
            "'U' could be instantiated with an arbitrary type which could be unrelated to 'U'.",
            0,
        )],
    );
}

#[test]
fn nested_class_decorator_arrow_keeps_static_outer_class_this() {
    let source = r#"
declare function decorate(callback: () => unknown): any;

class Outer {
  static value: 1 = 1;
  static nested = class Inner {
    @decorate((): 1 => this.value)
    method() {}
  };
}
"#;

    let actual = codes(source);
    assert!(
        actual.is_empty(),
        "a nested decorator in a static field must capture the outer constructor, got {actual:?}",
    );
}

#[test]
fn nested_class_decorator_is_transparent_but_its_method_body_remains_opaque() {
    let source = r#"
declare function decorate(value: unknown): any;

class Outer<U> {
  value!: U;
  nested = class Inner {
    value = 1;

    @decorate(() => this.value)
    method() {
      const invalid: U = (() => this.value)();
    }
  };
}
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "only the method-body arrow should use the nested class receiver",
    );
}

#[test]
fn nested_class_field_arrow_remains_in_nested_this_scope() {
    let source = r#"
class Outer<U> {
  nested = class Inner {
    value = 1;
    arrow = () => {
      const invalid: U = this.value;
      return invalid;
    };
  };
}
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "a nested class field must not inherit the outer class receiver",
    );
}

#[test]
fn constructor_parameter_property_arrow_gets_early_class_context() {
    let source = r#"
class Child<U> {
  value!: U;
  constructor(public read = <U extends string>() => this.value) {}
}

const child = new Child<number>();
const exact: number = child.read();
"#;

    let actual = codes(source);
    assert!(
        actual.is_empty(),
        "parameter-property initializers are checked before deferred class setup and need the same early context as fields, got {actual:?}",
    );
}

#[test]
fn constructor_parameter_property_regular_function_keeps_own_this() {
    let source = r#"
class Holder<U> {
  constructor(
    public read = function (this: { value: number }) { return this.value; },
  ) {}
}

const holder = new Holder<string>();
const value = holder.read.call({ value: 1 });
const invalid: string = value;
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "a regular function in a parameter property must keep its explicit receiver",
    );
}

#[test]
fn contextual_return_does_not_bind_owned_param_from_foreign_same_name() {
    let source = r#"
function outer<U>(captured: U) {
  const call = <U>(local: U) => captured;
  const invalid: string = call(123);
  return invalid;
}
"#;

    assert_eq!(
        codes(source),
        vec![2322],
        "a captured foreign binder in the return shape must not be classified as call-owned by spelling",
    );
}
