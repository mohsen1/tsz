//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/state_checking/property_access.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c2dcb24ce97d4dbb600430947d77e8a6608ad8e1785e40e2a8e0bdd3b5509a5c 1154 mapped_type_name_collision_readonly_of_type_param
    /// Mapped type template with name collision: `MyReadonly`<P> where P is a
    /// user type parameter with the same name as the mapped key param.
    /// Name-based substitution must be bypassed to avoid incorrectly
    /// replacing the outer P with the key literal.
    #[test]
    fn mapped_type_name_collision_readonly_of_type_param() {
        let diags = check_source_diagnostics(
            "interface Foo { foo(): void }
type MyPartial<T> = { [P in keyof T]?: T[P] };
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
class A<P extends MyPartial<Foo>> {
    constructor(public props: MyReadonly<P>) {}
    doSomething() {
        this.props.foo && this.props.foo()
    }
}",
        );
        let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
        assert!(
            relevant.is_empty(),
            "expected only TS2318 (if any), got: {:?}",
            relevant
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END c2dcb24ce97d4dbb600430947d77e8a6608ad8e1785e40e2a8e0bdd3b5509a5c

// TSZ_INLINE_TEST_BEGIN 31bedce8738312081422bf5d18f37be84180f244b498bc4f1282d506523b7c91 1180 type_param_property_access_with_mapped_constraint
    /// Property access on a type parameter with a mapped-type constraint
    /// should resolve through the constraint.
    #[test]
    fn type_param_property_access_with_mapped_constraint() {
        let diags = check_source_diagnostics(
            "interface Foo { foo(): void }
type MyPartial<T> = { [P in keyof T]?: T[P] };
function f<P extends MyPartial<Foo>>(p: P) {
    p.foo;
}",
        );
        let relevant: Vec<_> = diags.iter().filter(|d| d.code != 2318).collect();
        assert!(
            relevant.is_empty(),
            "expected only TS2318 (if any), got: {:?}",
            relevant
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 31bedce8738312081422bf5d18f37be84180f244b498bc4f1282d506523b7c91

// TSZ_INLINE_TEST_BEGIN 17d3b754502adba8b4f932deed7e0694d99ffbfd4188402f9d5240d60d05efe0 1225 mapped_type_application_property_resolution_preserves_optional_method_type
    #[test]
    fn mapped_type_application_property_resolution_preserves_optional_method_type() {
        let source = "interface Foo { foo(): void }
type MyPartial<T> = { [P in keyof T]?: T[P] };
type MyReadonly<T> = { readonly [P in keyof T]: T[P] };
class A<P extends MyPartial<Foo>> {
    constructor(public props: MyReadonly<P>) {}
    doSomething() {
        this.props.foo && this.props.foo()
    }
}";

        let (parser, root, binder, types) = build_checker(source);
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);

        let call = find_node_by_text_and_kind(
            parser.get_arena(),
            source,
            syntax_kind_ext::CALL_EXPRESSION,
            "this.props.foo()",
        )
        .expect("call expression");
        let callee_access = parser
            .get_arena()
            .get(call)
            .and_then(|node| parser.get_arena().get_call_expr(node))
            .map(|call| call.expression)
            .expect("call callee");
        let object_access = parser
            .get_arena()
            .get(callee_access)
            .and_then(|node| parser.get_arena().get_access_expr(node))
            .map(|access| access.expression)
            .expect("callee object access");

        let object_ty = checker.get_type_of_node(object_access);
        let raw_lookup = checker.resolve_property_access_with_env(object_ty, "foo");
        let tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. } =
            raw_lookup
        else {
            panic!("expected successful property lookup on MyReadonly<P>, got {raw_lookup:?}");
        };

        let formatted = checker.format_type(type_id);
        assert!(
            formatted.contains("=> void") && formatted.contains("undefined"),
            "expected MyReadonly<P>.foo to preserve optional method type, got {formatted}",
        );
    }
// TSZ_INLINE_TEST_END 17d3b754502adba8b4f932deed7e0694d99ffbfd4188402f9d5240d60d05efe0

// TSZ_INLINE_TEST_BEGIN e76a3cc6f2aeb876d88b87ce3f11d554c933ab61d089a103338fa6a47dc084e6 1283 mapped_enum_discriminant_application_exposes_member_property
    #[test]
    fn mapped_enum_discriminant_application_exposes_member_property() {
        let source = r#"
enum ABC { A, B }

type Gen<T extends ABC> = { v: T } & (
  { v: ABC.A, a: string } |
  { v: ABC.B, b: string }
);

type Gen2<T extends ABC> = {
  [Property in keyof Gen<T>]: string;
};

type ProbeGen = Gen<ABC.A>;
type Probe = Gen2<ABC.A>;
"#;

        let (parser, root, binder, types) = build_checker(source);
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);

        let probe_sym = checker
            .ctx
            .binder
            .file_locals
            .get("Probe")
            .expect("Probe symbol");
        let probe_gen_sym = checker
            .ctx
            .binder
            .file_locals
            .get("ProbeGen")
            .expect("ProbeGen symbol");
        let probe_gen_type = checker.type_reference_symbol_type(probe_gen_sym);
        let probe_type = checker.type_reference_symbol_type(probe_sym);
        let gen_a_result = checker.resolve_property_access_with_env(probe_gen_type, "a");
        let a_result = checker.resolve_property_access_with_env(probe_type, "a");

        assert!(
            matches!(
                gen_a_result,
                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
            ),
            "expected ProbeGen.a to resolve, got {gen_a_result:?} for type {}",
            checker.format_type(probe_gen_type),
        );

        assert!(
            matches!(
                a_result,
                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
            ),
            "expected Probe.a to resolve, got {a_result:?} for type {}",
            checker.format_type(probe_type),
        );
    }
// TSZ_INLINE_TEST_END e76a3cc6f2aeb876d88b87ce3f11d554c933ab61d089a103338fa6a47dc084e6

// TSZ_INLINE_TEST_BEGIN 949a1c4e7180c7cfed63e8c68f7642ae4fa4389b2971f1715ff818e847224043 1387 mapped_string_enum_discriminant_application_renamed_binders_exposes_member
    /// Adjacent case: renamed binders + string enum. A concrete discriminant
    /// reached through the alias body (`Weekday.Tue`, still a `Lazy(DefId)`)
    /// must still prune the impossible constituent so the mapped-over-`keyof`
    /// application exposes the member-specific key.
    #[test]
    fn mapped_string_enum_discriminant_application_renamed_binders_exposes_member() {
        let source = r#"
enum Weekday { Mon = "mon", Tue = "tue" }
type Slot<D extends Weekday> = { day: D } & (
  { day: Weekday.Mon, open: string } |
  { day: Weekday.Tue, close: string }
);
type SlotView<D extends Weekday> = { [K in keyof Slot<D>]: string };
type Probe = SlotView<Weekday.Mon>;
"#;
        assert_alias_property(source, "Probe", "open", true);
    }
// TSZ_INLINE_TEST_END 949a1c4e7180c7cfed63e8c68f7642ae4fa4389b2971f1715ff818e847224043

// TSZ_INLINE_TEST_BEGIN d54db608a43b73062999e0627f948390433245f500bbb1469f0fbc59410e2277 1402 mapped_numeric_enum_discriminant_application_exposes_member
    /// Adjacent case: numeric enum with explicit values.
    #[test]
    fn mapped_numeric_enum_discriminant_application_exposes_member() {
        let source = r#"
enum Level { Low = 1, High = 2 }
type Cell<L extends Level> = { lvl: L } & (
  { lvl: Level.Low, floor: string } |
  { lvl: Level.High, ceil: string }
);
type CellView<L extends Level> = { [K in keyof Cell<L>]: string };
type Probe = CellView<Level.Low>;
"#;
        assert_alias_property(source, "Probe", "floor", true);
    }
// TSZ_INLINE_TEST_END d54db608a43b73062999e0627f948390433245f500bbb1469f0fbc59410e2277

// TSZ_INLINE_TEST_BEGIN 7d36bfe3f6ee09f9ad4f64fe50733f650605e327985804caf5effe0236329e39 1419 mapped_enum_same_discriminant_keeps_only_shared_keys
    /// Negative control: when both constituents share the *same* discriminant
    /// member, neither is impossible, so `keyof` keeps only the shared keys and
    /// a member-specific key must stay unresolved (matching tsc).
    #[test]
    fn mapped_enum_same_discriminant_keeps_only_shared_keys() {
        let source = r#"
enum ABC { A, B }
type Gen<T extends ABC> = { v: T } & (
  { v: ABC.A, a: string } |
  { v: ABC.A, b: string }
);
type Gen2<T extends ABC> = { [K in keyof Gen<T>]: string };
type Probe = Gen2<ABC.A>;
"#;
        // `a` and `b` live on different same-discriminant constituents, so
        // `keyof (A | B) = keyof A & keyof B` drops both; only `v` survives.
        assert_alias_property(source, "Probe", "a", false);
        assert_alias_property(source, "Probe", "v", true);
    }
// TSZ_INLINE_TEST_END 7d36bfe3f6ee09f9ad4f64fe50733f650605e327985804caf5effe0236329e39

// TSZ_INLINE_TEST_BEGIN d39ecf850298d6cdce03e3c95211fd3e863af8bba38e75f77e93aecd8aa886fd 1445 concrete_enum_key_mapped_alias_preserves_mapped_identity
    /// Rule: a concrete (type-parameter-free) mapped body `{ [K in E]: V }`
    /// keeps its `Mapped` structural identity through type-alias stabilization,
    /// rather than being eagerly materialized to a plain object (#15392, culprit
    /// #10522). Preserving the identity is what lets diagnostics recover the enum
    /// key origin and lets mapped-over-`keyof` property resolution see the
    /// iteration constraint. Binder names are varied (`Weekday`/`Schedule`, not
    /// `E`/`M`) so the check is structural, not keyed on spelling. A wrapper alias
    /// (`type S2 = Schedule`) must preserve it too, and a plain object alias is
    /// the negative control that stays materialized.
    #[test]
    fn concrete_enum_key_mapped_alias_preserves_mapped_identity() {
        let source = r#"
enum Weekday { Mon = "mon", Tue = "tue" }
type Schedule = { [K in Weekday]: number };
type ScheduleAlias = Schedule;
type Plain = { mon: number; tue: number };
"#;
        let (parser, root, binder, types) = build_checker(source);
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);

        let db = checker.ctx.types.as_type_database();
        let resolve = |c: &mut CheckerState, name: &str| -> tsz_solver::TypeId {
            let sym = c.ctx.binder.file_locals.get(name).expect("symbol");
            c.type_reference_symbol_type(sym)
        };

        let schedule = resolve(&mut checker, "Schedule");
        assert!(
            crate::query_boundaries::state::checking::is_mapped_type(db, schedule),
            "concrete enum-key mapped alias must stay a Mapped type, got {}",
            checker.format_type(schedule),
        );

        let schedule_alias = resolve(&mut checker, "ScheduleAlias");
        assert!(
            crate::query_boundaries::state::checking::is_mapped_type(db, schedule_alias),
            "wrapper alias of a concrete mapped type must preserve Mapped identity, got {}",
            checker.format_type(schedule_alias),
        );

        let plain = resolve(&mut checker, "Plain");
        assert!(
            !crate::query_boundaries::state::checking::is_mapped_type(db, plain),
            "plain object alias must not be a Mapped type (negative control), got {}",
            checker.format_type(plain),
        );
    }
// TSZ_INLINE_TEST_END d39ecf850298d6cdce03e3c95211fd3e863af8bba38e75f77e93aecd8aa886fd

// TSZ_INLINE_TEST_BEGIN f0e4d558b8b1ae841884ec00e97f8aef7db66eeb5e7e3044bab32fcc87582f37 1495 deferred_conditional_branch_only_property_emits_ts2339
    /// Rule: when `T extends U ? A : B` is deferred (contains type parameters),
    /// property access uses `A | B` as the apparent type. Properties not on all
    /// branches must produce TS2339; properties on all branches must be accepted.
    #[test]
    fn deferred_conditional_branch_only_property_emits_ts2339() {
        let diags = check_source_diagnostics(
            "type Cond<T> = T extends string ? { a: 1 } : { b: 2 };
function f<T>(c: Cond<T>) {
  c.a;
  c.zzz;
}",
        );
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert_eq!(
            codes.iter().filter(|&&c| c == 2339).count(),
            2,
            "expected 2 TS2339 errors (c.a and c.zzz), got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END f0e4d558b8b1ae841884ec00e97f8aef7db66eeb5e7e3044bab32fcc87582f37

// TSZ_INLINE_TEST_BEGIN 1a37059934bff412c658f801c778ec8c63e4af1f7840d9e6814133f82d4910fd 1516 deferred_conditional_common_property_no_ts2339
    #[test]
    fn deferred_conditional_common_property_no_ts2339() {
        let diags = check_source_diagnostics(
            "type Cond<T> = T extends string ? { common: number } : { common: string };
function f<T>(c: Cond<T>) {
  c.common;
}",
        );
        let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
        assert!(
            ts2339.is_empty(),
            "expected no TS2339 for common property, got: {:?}",
            ts2339
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }
// TSZ_INLINE_TEST_END 1a37059934bff412c658f801c778ec8c63e4af1f7840d9e6814133f82d4910fd
