//! Regression tests for false TS2554/TS2739/TS2740 on self-referential class
//! hierarchies (zod-style schema builders).
//!
//! Structural rule: when class `B`'s type computation resolves a member
//! annotation referencing a subclass `S extends B` (e.g. `m(): S<this>`), the
//! nested computation of `S`'s constructor type runs while `B` is still
//! mid-resolution. `S` has no own constructor, so its construct signatures are
//! inherited from `B`. tsc resolves the inherited signature lazily and sees the
//! full parameter list; tsz must expose `B`'s own constructor arity through a
//! published partial constructor type instead of degrading to the default
//! zero-parameter signature (false TS2554 "Expected 0 arguments, but got 1" at
//! every `new S(...)` site), and `new S(...)` must keep the class identity
//! (`Application(Lazy(S), ...)`) as its type rather than a structural snapshot
//! of declared members (false TS2739/TS2740 "missing the following
//! properties").
//!
//! Witness reduced from the zod 3.9.7 project corpus (zod-project row,
//! `src/types.ts` `ZodType`/`ZodEffects`/`ZodOptional` cluster, 28 false
//! TS2554 + same-family TS2322/TS2345/TS2739).

use tsz_checker::test_utils::check_source_codes;

/// The zod shape: generic base with a 1-arg constructor; two ctor-less
/// generic subclasses referenced from the base's own method annotations and
/// constructed inside their own `static create` initializers. tsc-clean.
#[test]
fn self_referential_subclass_static_create_no_false_positives() {
    let source = r#"
interface BaseDef {
  errorMap?: string;
}

type AnyBase = Base<any, any>;

interface SubDef<T extends AnyBase = AnyBase> extends BaseDef {
  schema: T;
}

interface OtherDef<T extends AnyBase = AnyBase> extends BaseDef {
  inner: T;
}

abstract class Base<Output, Def extends BaseDef = BaseDef, Input = Output> {
  readonly _output!: Output;
  readonly _input!: Input;
  readonly _def!: Def;

  constructor(def: Def) {
    this._def = def;
  }

  refine(): Sub<this> {
    return new Sub({ schema: this }) as any;
  }

  other(): Other<this> {
    return Other.create(this);
  }
}

class Sub<
  T extends AnyBase,
  Output = T["_output"],
  Input = T["_input"]
> extends Base<Output, SubDef<T>, Input> {
  inner() {
    return this._def.schema;
  }

  static create = <U extends AnyBase>(schema: U): Sub<U> => {
    return new Sub({ schema });
  };
}

class Other<T extends AnyBase> extends Base<
  T["_output"] | undefined,
  OtherDef<T>,
  T["_input"] | undefined
> {
  unwrap() {
    return this._def.inner;
  }

  static create = <U extends AnyBase>(inner: U): Other<U> => {
    return new Other({ inner });
  };
}
"#;
    let errors = check_source_codes(source);
    assert!(
        errors.is_empty(),
        "tsc reports no errors for the zod-style self-referential hierarchy, got: {errors:?}"
    );
}

/// Renamed binders + concrete (non-generic) subclass + method that spreads
/// `this._def` into a self-construction with an inferred return type.
#[test]
fn concrete_subclass_def_spread_self_construct_clean() {
    let source = r#"
interface CoreCfg {
  onFail?: string;
}

type AnyCore = Core<any, any>;

interface LeafCfg extends CoreCfg {
  marks: { kind: string }[];
}

interface WrapCfg<T extends AnyCore = AnyCore> extends CoreCfg {
  wrapped: T;
}

abstract class Core<Out, Cfg extends CoreCfg = CoreCfg> {
  readonly out!: Out;
  readonly cfg!: Cfg;

  constructor(cfg: Cfg) {
    this.cfg = cfg;
  }

  wrap(): Wrap<this> {
    return Wrap.make(this);
  }
}

class Leaf extends Core<string, LeafCfg> {
  addMark(kind: string) {
    return new Leaf({
      ...this.cfg,
      marks: [...this.cfg.marks, { kind }],
    });
  }

  static make = (): Leaf => {
    return new Leaf({ marks: [] });
  };
}

class Wrap<T extends AnyCore> extends Core<T["out"] | undefined, WrapCfg<T>> {
  unwrap() {
    return this.cfg.wrapped;
  }

  static make = <U extends AnyCore>(wrapped: U): Wrap<U> => {
    return new Wrap({ wrapped });
  };
}
"#;
    let errors = check_source_codes(source);
    assert!(
        errors.is_empty(),
        "concrete subclass spreading own def must be clean, got: {errors:?}"
    );
}

/// Instance-property method alias (`superRefine = this._refinement`) plus
/// constructor-body rebinding, like zod's `ZodType` constructor. tsc-clean.
#[test]
fn method_alias_property_and_ctor_rebinding_clean() {
    let source = r#"
interface RootDef {
  errorMap?: string;
}

type AnyRoot = Root<any, any>;

interface EffDef<T extends AnyRoot = AnyRoot> extends RootDef {
  schema: T;
}

abstract class Root<Output, Def extends RootDef = RootDef, Input = Output> {
  readonly _output!: Output;
  readonly _input!: Input;
  readonly _def!: Def;

  _refinement(): Eff<this> {
    return new Eff({ schema: this }) as any;
  }
  superRefine = this._refinement;

  constructor(def: Def) {
    this._def = def;
    this.transform = this.transform.bind(this) as any;
  }

  transform(): Eff<this> {
    return new Eff({ schema: this }) as any;
  }
}

class Eff<
  T extends AnyRoot,
  Output = T["_output"],
  Input = T["_input"]
> extends Root<Output, EffDef<T>, Input> {
  innerType() {
    return this._def.schema;
  }

  static create = <U extends AnyRoot>(schema: U): Eff<U> => {
    return new Eff({ schema });
  };
}
"#;
    let errors = check_source_codes(source);
    assert!(
        errors.is_empty(),
        "method-alias property + ctor rebinding must be clean, got: {errors:?}"
    );
}

/// Negative control: genuinely wrong arity must still error. The base
/// constructor takes one argument, so a zero-argument `new` of the ctor-less
/// subclass is a real TS2554 in tsc too.
#[test]
fn genuinely_missing_ctor_argument_still_ts2554() {
    let source = r#"
interface BaseDef {
  errorMap?: string;
}

type AnyBase = Base<any, any>;

interface SubDef<T extends AnyBase = AnyBase> extends BaseDef {
  schema: T;
}

abstract class Base<Output, Def extends BaseDef = BaseDef> {
  readonly _output!: Output;
  readonly _def!: Def;

  constructor(def: Def) {
    this._def = def;
  }

  refine(): Sub<this> {
    return new Sub() as any;
  }
}

class Sub<T extends AnyBase> extends Base<T["_output"], SubDef<T>> {
  inner() {
    return this._def.schema;
  }
}
"#;
    let errors = check_source_codes(source);
    assert!(
        errors.contains(&2554),
        "zero-arg `new Sub()` against inherited 1-arg ctor must keep TS2554, got: {errors:?}"
    );
}

/// Negative control: wrong argument type for the inherited constructor must
/// still error (TS2345), including when constructed inside the mid-resolution
/// window of the base class.
#[test]
fn genuinely_wrong_ctor_argument_type_still_errors() {
    let source = r#"
interface BaseDef {
  flag: boolean;
}

type AnyBase = Base<any>;

abstract class Base<Output, Def extends BaseDef = BaseDef> {
  readonly _output!: Output;
  readonly _def!: Def;

  constructor(def: Def) {
    this._def = def;
  }

  refine(): Sub<this> {
    return new Sub(42) as any;
  }
}

class Sub<T extends AnyBase> extends Base<T["_output"]> {
  inner() {
    return this._def.flag;
  }
}
"#;
    let errors = check_source_codes(source);
    assert!(
        errors.contains(&2345),
        "number arg against inherited `def: Def` ctor must keep TS2345, got: {errors:?}"
    );
}

/// Own-constructor subclass (not inherited) constructed from the base's
/// method bodies — the published-partial path with explicit own ctor.
#[test]
fn own_ctor_subclass_constructed_from_base_clean() {
    let source = r#"
interface NodeDef {
  label?: string;
}

type AnyNode = NodeBase<any>;

interface PairDef<T extends AnyNode = AnyNode> extends NodeDef {
  left: T;
}

abstract class NodeBase<Out, Def extends NodeDef = NodeDef> {
  readonly out!: Out;
  readonly def!: Def;

  constructor(def: Def) {
    this.def = def;
  }

  pair(): Pair<this> {
    return new Pair({ left: this }, 1);
  }
}

class Pair<T extends AnyNode> extends NodeBase<T["out"], PairDef<T>> {
  constructor(def: PairDef<T>, depth: number) {
    super(def);
    void depth;
  }

  leftNode() {
    return this.def.left;
  }
}
"#;
    let errors = check_source_codes(source);
    assert!(
        errors.is_empty(),
        "subclass with own 2-arg ctor constructed from base must be clean, got: {errors:?}"
    );
}
