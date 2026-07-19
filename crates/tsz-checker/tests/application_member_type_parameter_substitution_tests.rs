//! Explicit generic arguments must retain their enclosing declaration's type-
//! parameter identity through inherited application-member lookup.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_strict_codes};

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
