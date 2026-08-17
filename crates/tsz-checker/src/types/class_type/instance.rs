//! Phase helpers for class instance type resolution.
//!
//! `get_class_instance_type_inner` (in `core.rs`) is a thin orchestrator that
//! drives the phases below in order through a shared [`ClassInstanceBuilder`]
//! that holds the cross-phase accumulators (collected properties/methods/
//! accessors, index signatures, deferred members, type parameters, and the
//! flags that gate the final instance-type construction).
//!
//! The phases are pure code motion out of the original ~2000-line function:
//! each one accumulates into the builder, and the orchestrator preserves the
//! original ordering, the early-return/cleanup semantics, and the type
//! construction unchanged.

use super::helpers::{AccessorAggregate, MethodAggregate};
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::context::{EnclosingClassInfo, is_js_file_name};
use crate::query_boundaries::class_type;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{IndexSignature, PropertyInfo, TypeId, TypeParamInfo, Visibility};

/// Whether class-instance construction swapped in a temporary
/// `enclosing_class` and, if so, the previous value to restore afterward.
///
/// This flattens what would otherwise be an `Option<Option<EnclosingClassInfo>>`
/// (outer = "did we swap?", inner = "the prior enclosing class, possibly none").
/// [`RestoreEnclosingClass::Skip`] means no swap happened (leave state as-is);
/// [`RestoreEnclosingClass::To`] carries the prior value to restore.
pub(super) enum RestoreEnclosingClass {
    Skip,
    To(Option<EnclosingClassInfo>),
}

/// A get/set accessor deferred to phase 2 so its body is checked under a
/// partial `this` type built from the class's other members.
pub(super) struct DeferredAccessor<'b> {
    pub(super) member_idx: NodeIndex,
    pub(super) accessor: &'b tsz_parser::parser::node::AccessorData,
    pub(super) is_getter: bool,
    pub(super) name_atom: Atom,
    pub(super) is_symbol_named: bool,
    /// The key is a plain (non-unique) `symbol` binding, so the accessor's
    /// resolved type folds into the shape's symbol index signature instead of
    /// becoming a named accessor member.
    pub(super) keys_symbol_index: bool,
    pub(super) visibility: Visibility,
    pub(super) declaration_order: u32,
}

/// The independent control-flow flags carried across the class-instance build
/// phases, packed into a single `u8` so the builder stays under the
/// `clippy::struct_excessive_bools` threshold without a suppression. Each flag
/// is the same boolean the original megafn tracked as a local; the named bit
/// constants below document each one.
#[derive(Clone, Copy, Default)]
pub(super) struct ClassInstanceFlags(u8);

impl ClassInstanceFlags {
    /// We inserted `current_sym` into the global resolution set and own its
    /// cleanup on every exit path.
    const DID_INSERT_INTO_GLOBAL_SET: u8 = 1 << 0;
    /// At least one member requires nominal typing (adds the private brand).
    const HAS_NOMINAL_MEMBERS: u8 = 1 << 1;
    /// At least one member has an unresolved computed (late-bound) name.
    const HAS_LATE_BOUND_MEMBERS: u8 = 1 << 2;
    /// Phase 0 pushed a prescan `this` type that must be popped before Phase 2.
    const PUSHED_PRESCAN_THIS: u8 = 1 << 3;

    const fn get(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    const fn set(&mut self, bit: u8, value: bool) {
        if value {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    /// Record whether `current_sym` was inserted into the global resolution set
    /// (set once at builder construction).
    pub(super) const fn set_did_insert_into_global_set(&mut self, value: bool) {
        self.set(Self::DID_INSERT_INTO_GLOBAL_SET, value);
    }
}

/// Cross-phase accumulator state for building a class instance type.
///
/// The orchestrator constructs this after the setup guards, then threads it
/// through the phase helpers. Each phase reads and extends the accumulators;
/// the final phase consumes them to build the instance `ObjectShape`.
pub(super) struct ClassInstanceBuilder<'b> {
    pub(super) current_sym: Option<SymbolId>,
    pub(super) flags: ClassInstanceFlags,
    pub(super) class_type_params: Vec<TypeParamInfo>,
    pub(super) class_type_param_ids: Vec<TypeId>,
    pub(super) class_type_param_updates: Vec<(String, Option<TypeId>, bool)>,
    pub(super) member_count: usize,
    pub(super) properties: FxHashMap<Atom, PropertyInfo>,
    pub(super) methods: FxHashMap<Atom, MethodAggregate>,
    pub(super) accessors: FxHashMap<Atom, AccessorAggregate>,
    pub(super) string_index: Option<IndexSignature>,
    pub(super) number_index: Option<IndexSignature>,
    pub(super) symbol_index: Option<IndexSignature>,
    pub(super) merged_interface_type_for_class: Option<TypeId>,
    pub(super) prescan_this_type: Option<TypeId>,
    pub(super) deferred_methods:
        Vec<(NodeIndex, &'b tsz_parser::parser::node::MethodDeclData, u32)>,
    pub(super) deferred_accessors: Vec<DeferredAccessor<'b>>,
    pub(super) restore_enclosing_class: RestoreEnclosingClass,
}

impl ClassInstanceBuilder<'_> {
    /// Whether we inserted `current_sym` into the global resolution set.
    pub(super) const fn did_insert_into_global_set(&self) -> bool {
        self.flags
            .get(ClassInstanceFlags::DID_INSERT_INTO_GLOBAL_SET)
    }

    /// Whether any member requires nominal typing.
    pub(super) const fn has_nominal_members(&self) -> bool {
        self.flags.get(ClassInstanceFlags::HAS_NOMINAL_MEMBERS)
    }

    /// Mark that a member requires nominal typing.
    pub(super) const fn set_has_nominal_members(&mut self) {
        self.flags
            .set(ClassInstanceFlags::HAS_NOMINAL_MEMBERS, true);
    }

    /// Whether any member has an unresolved computed (late-bound) name.
    pub(super) const fn has_late_bound_members(&self) -> bool {
        self.flags.get(ClassInstanceFlags::HAS_LATE_BOUND_MEMBERS)
    }

    /// Mark that a member has an unresolved computed (late-bound) name.
    pub(super) const fn set_has_late_bound_members(&mut self) {
        self.flags
            .set(ClassInstanceFlags::HAS_LATE_BOUND_MEMBERS, true);
    }

    /// Whether Phase 0 pushed a prescan `this` type.
    pub(super) const fn pushed_prescan_this(&self) -> bool {
        self.flags.get(ClassInstanceFlags::PUSHED_PRESCAN_THIS)
    }

    /// Mark that Phase 0 pushed a prescan `this` type.
    pub(super) const fn set_pushed_prescan_this(&mut self) {
        self.flags
            .set(ClassInstanceFlags::PUSHED_PRESCAN_THIS, true);
    }

    /// Declaration-order key for the member at `member_pos`.
    ///
    /// Member positions start at 1 (synthesized members keep order 0 and stay
    /// first via stable sort) and are shifted into the high 16 bits so that
    /// constructor parameter properties can claim the low bits.
    const fn class_member_order(member_pos: usize) -> u32 {
        ((member_pos + 1) as u32) << 16
    }
}

impl<'a> CheckerState<'a> {
    /// Whether Phase 0/1 can recover a `this` member while a nested arrow's type
    /// parameters shadow the class scope. The check short-circuits for
    /// non-generic classes and reuses one traversal buffer across candidate
    /// initializers. Expression wrappers and nested class headers/computed names
    /// preserve lexical `this`; ordinary function and member bodies remain
    /// boundaries.
    pub(super) fn class_instance_needs_early_enclosing(
        &self,
        class: &tsz_parser::parser::node::ClassData,
        b: &ClassInstanceBuilder<'_>,
    ) -> bool {
        if b.class_type_param_ids.is_empty() {
            return false;
        }

        let mut traversal = Vec::new();
        class.members.nodes.iter().any(|&member_idx| {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    return false;
                };
                match member_node.kind {
                    syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(member_node)
                        .is_some_and(|property| {
                            !self.has_static_modifier(&property.modifiers)
                                && property.initializer.is_some()
                                && tsz_parser::syntax::transform_utils::contains_lexical_arrow_function_with_scratch(
                                    self.ctx.arena,
                                    property.initializer,
                                    &mut traversal,
                                )
                        }),
                    syntax_kind_ext::CONSTRUCTOR => self
                        .ctx
                        .arena
                        .get_constructor(member_node)
                        .is_some_and(|constructor| {
                            constructor.parameters.nodes.iter().any(|&param_idx| {
                                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                    return false;
                                };
                                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                    return false;
                                };
                                self.has_parameter_property_modifier(&param.modifiers)
                                    && param.initializer.is_some()
                                    && tsz_parser::syntax::transform_utils::contains_lexical_arrow_function_with_scratch(
                                        self.ctx.arena,
                                        param.initializer,
                                        &mut traversal,
                                    )
                            })
                        }),
                    _ => false,
                }
            })
    }

    /// Phase 0: Pre-scan annotated properties to build a preliminary partial
    /// `this` type. Property initializers like `n = this.s` need `this` to
    /// resolve during Phase 1. The type builder is called from
    /// `build_type_environment` BEFORE `enclosing_class` is set, so `this` in
    /// property initializers would otherwise resolve to `any`. By pushing a
    /// partial type onto `this_type_stack`, initializer expressions that
    /// reference `this.annotatedProp` can resolve correctly.
    pub(super) fn class_instance_phase0_prescan_this<'b>(
        &mut self,
        class_idx: NodeIndex,
        class: &'b tsz_parser::parser::node::ClassData,
        b: &mut ClassInstanceBuilder<'b>,
    ) {
        let current_sym = b.current_sym;
        let member_count = b.member_count;
        {
            // PERF: Single pass over class members for prescan (was 3 separate loops).
            let mut prescan_props: Vec<PropertyInfo> = Vec::with_capacity(member_count);
            let mut needs_inherited_prescan_this = false;
            for (member_pos, &member_idx) in class.members.nodes.iter().enumerate() {
                let declaration_order = ClassInstanceBuilder::class_member_order(member_pos);
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&prop.modifiers) {
                            continue;
                        }
                        if prop.initializer.is_some()
                            && tsz_parser::syntax::transform_utils::contains_this_reference(
                                self.ctx.arena,
                                prop.initializer,
                            )
                        {
                            needs_inherited_prescan_this = true;
                        }
                        let declared_type = if let Some(dt) =
                            self.effective_class_property_declared_type(member_idx, prop)
                        {
                            dt
                        } else if prop.initializer.is_some() {
                            let init_node = self.ctx.arena.get(prop.initializer);
                            if init_node.is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16) {
                                self.ctx.types.this_type()
                            } else if let (Some(current_sym), Some(init_node)) =
                                (current_sym, init_node)
                                && init_node.kind == syntax_kind_ext::NEW_EXPRESSION
                                && self
                                    .ctx
                                    .arena
                                    .get_call_expr(init_node)
                                    .and_then(|call| {
                                        self.ctx.arena.get_identifier_at(call.expression)
                                    })
                                    .is_some_and(|ident| {
                                        ident.escaped_text
                                            == self.get_class_name_from_decl(class_idx)
                                    })
                            {
                                self.ctx.create_lazy_type_ref(current_sym)
                            } else {
                                TypeId::ANY
                            }
                        } else {
                            continue;
                        };
                        // Use non-resolving get_property_name to avoid evaluating
                        // computed property expressions during prescan.
                        // Computed properties like [rC.x] would trigger circular
                        // type resolution since the class body isn't cached yet.
                        let Some(name) = self.get_property_name(prop.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let is_readonly = self.has_readonly_modifier(&prop.modifiers)
                            || self.jsdoc_has_readonly_tag(member_idx);
                        let visibility = self.get_member_visibility(&prop.modifiers, prop.name);
                        prescan_props.push(class_type::class_member_property(
                            class_type::ClassMemberProperty::new(name_atom, declared_type)
                                .optional(prop.question_token)
                                .readonly(is_readonly)
                                .visibility(visibility)
                                .parent(current_sym)
                                .declaration_order(declaration_order),
                        ));
                    }
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&method.modifiers) {
                            continue;
                        }
                        let Some(name) = self.get_property_name(method.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let visibility = self.get_member_visibility(&method.modifiers, method.name);
                        // For methods with explicit return type annotations, create a
                        // proper Callable type so that `this.method()` during other
                        // method body inference resolves to the correct return type.
                        // Without this, the prescan type has methods typed as `any`,
                        // causing `{ ...this.annotatedMethod() }` to produce `{}`.
                        let method_type = if method.type_annotation.is_some() {
                            let (type_params, type_param_updates) =
                                self.push_type_parameters(&method.type_parameters);
                            // This is a provisional prescan, not the authoritative
                            // check (see comment above) — it runs before the class's
                            // own instance type is published, so a self-referential
                            // type-parameter constraint on the return type (e.g.
                            // `<Incoming extends AnyC>` re-instantiating `C` itself)
                            // can resolve against `C`'s not-yet-published instance
                            // type and spuriously fail a constraint check here. The
                            // authoritative check runs later, in the normal member
                            // walk (`class_member_checks.rs`'s
                            // `get_type_from_type_node` on `method.type_annotation`),
                            // once the class's real instance type is published — so
                            // any diagnostic produced by this premature pass is
                            // discarded, and the node-type cache is cleared so the
                            // later pass re-validates instead of reusing this
                            // speculative result (mirrors
                            // `speculative_static_property_initializer_type`,
                            // #17589).
                            let diag_snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
                            let return_type = self.get_type_from_type_node(method.type_annotation);
                            diag_snap.rollback(&mut self.ctx.diagnostic_state());
                            self.clear_type_cache_recursive(method.type_annotation);
                            self.pop_type_parameters(type_param_updates);
                            class_type::class_method_callable_type(
                                self.ctx.types,
                                vec![class_type::class_method_call_signature(
                                    type_params,
                                    vec![class_type::class_rest_any_param()],
                                    None,
                                    return_type,
                                    None,
                                )],
                            )
                        } else {
                            TypeId::ANY
                        };
                        prescan_props.push(class_type::class_member_property(
                            class_type::ClassMemberProperty::new(name_atom, method_type)
                                .method(false)
                                .visibility(visibility)
                                .parent(current_sym)
                                .declaration_order(declaration_order),
                        ));
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&accessor.modifiers) {
                            continue;
                        }
                        let Some(name) = self.get_property_name(accessor.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let accessor_type = if k == syntax_kind_ext::GET_ACCESSOR
                            && accessor.type_annotation.is_some()
                        {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        prescan_props.push(class_type::class_member_property(
                            class_type::ClassMemberProperty::new(name_atom, accessor_type)
                                .readonly(k == syntax_kind_ext::GET_ACCESSOR)
                                .visibility(
                                    self.get_member_visibility(&accessor.modifiers, accessor.name),
                                )
                                .parent(current_sym)
                                .declaration_order(declaration_order),
                        ));
                    }
                    k if k == syntax_kind_ext::CONSTRUCTOR => {
                        let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                            continue;
                        };
                        if ctor.body.is_none() {
                            continue;
                        }
                        for (param_pos, &param_idx) in ctor.parameters.nodes.iter().enumerate() {
                            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                continue;
                            };
                            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                continue;
                            };
                            if !self.has_parameter_property_modifier(&param.modifiers) {
                                continue;
                            }
                            if param.initializer.is_some()
                                && tsz_parser::syntax::transform_utils::contains_this_reference(
                                    self.ctx.arena,
                                    param.initializer,
                                )
                            {
                                needs_inherited_prescan_this = true;
                            }
                            let Some(name) = self.get_property_name(param.name) else {
                                continue;
                            };
                            let name_atom = self.ctx.types.intern_string(&name);
                            let is_readonly = self.has_readonly_modifier(&param.modifiers);
                            let declared_type = if param.type_annotation.is_some() {
                                self.get_type_from_type_node(param.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            let visibility = self.get_visibility_from_modifiers(&param.modifiers);
                            prescan_props.push(class_type::class_member_property(
                                class_type::ClassMemberProperty::new(name_atom, declared_type)
                                    .optional(param.question_token)
                                    .readonly(is_readonly)
                                    .visibility(visibility)
                                    .parent(current_sym)
                                    .declaration_order(declaration_order + param_pos as u32 + 1),
                            ));
                        }
                    }
                    _ => {}
                }
            }

            let base_prescan_type =
                self.inherited_prescan_this_base_type(class, needs_inherited_prescan_this);

            if !prescan_props.is_empty() || base_prescan_type.is_some() {
                let prescan_type = class_type::class_member_partial_this_type(
                    self.ctx.types,
                    prescan_props,
                    None,
                    None,
                    None,
                    current_sym,
                    base_prescan_type,
                )
                .expect(
                    "guarded by `!prescan_props.is_empty() || base_prescan_type.is_some()`: \
                     at least one of own/base prescan types is present",
                );
                self.ctx
                    .class_instance_type_cache
                    .borrow_mut()
                    .insert(class_idx, prescan_type);
                if let Some(info) = self
                    .ctx
                    .enclosing_class
                    .as_mut()
                    .filter(|info| info.class_idx == class_idx)
                {
                    info.cached_instance_this_type = Some(prescan_type);
                }
                self.ctx.this_type_stack.push(prescan_type);
                b.prescan_this_type = Some(prescan_type);
                b.set_pushed_prescan_this();

                // Register prescan body early so that Application property lookup can
                // resolve Lazy(DefId(Self)) during Phase-2 method body checking. Without
                // this, `f.x` where `f: Vec2<(a:A)=>B>` fails with TS2349 because
                // resolve_lazy returns None until the end of this function. Final
                // registration below overwrites with the complete instance type.
                if let Some(sym_id) = current_sym {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    self.ctx
                        .register_class_instance_in_envs(def_id, prescan_type);
                    self.ctx.register_resolved_type(
                        sym_id,
                        prescan_type,
                        b.class_type_params.clone(),
                    );
                }
            }
        }
    }

    /// Phase 1: Process all non-method members (properties, accessors,
    /// constructors, index sigs). Methods are deferred to phase 2 so that a
    /// partial instance type (with property types) can be pushed as `this`,
    /// allowing method body inference to resolve `this.x` references.
    pub(super) fn class_instance_phase1_non_method_members<'b>(
        &mut self,
        class: &'b tsz_parser::parser::node::ClassData,
        class_idx: NodeIndex,
        b: &mut ClassInstanceBuilder<'b>,
    ) where
        'a: 'b,
    {
        let current_sym = b.current_sym;
        // A derived class can only assign `this.<prop>` after `super()`, so
        // constructor-flow property inference must respect the same gate when
        // computing definite assignment.
        let class_requires_super = self.class_has_base(class);
        for (member_pos, &member_idx) in class.members.nodes.iter().enumerate() {
            let declaration_order = ClassInstanceBuilder::class_member_order(member_pos);
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&prop.modifiers, prop.name) {
                        b.set_has_nominal_members();
                    }
                    let Some(name) = self.get_property_name_resolved(prop.name) else {
                        if self
                            .ctx
                            .arena
                            .get(prop.name)
                            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                        {
                            b.set_has_late_bound_members();
                            tracing::debug!(
                                member = member_idx.0,
                                "class member computed name unresolved -> late-bound"
                            );
                        }
                        continue;
                    };
                    let is_symbol_named = self.class_member_name_is_symbol_named(prop.name);
                    let keys_symbol_index =
                        self.class_member_computed_key_is_wide_symbol(prop.name);
                    let name_atom = self.ctx.types.intern_string(&name);
                    let is_readonly = self.has_readonly_modifier(&prop.modifiers)
                        || self.jsdoc_has_readonly_tag(member_idx);
                    let visibility = self.get_member_visibility(&prop.modifiers, prop.name);

                    // In JS/checkJs, arrow-property initializers inherit the class `this`.
                    // Pre-scan `this.prop = value` writes inside the arrow body so the
                    // partial instance type already includes those implicit members while
                    // we type-check the initializer itself.
                    if self.ctx.is_js_file()
                        && prop.initializer.is_some()
                        && let Some(init_node) = self.ctx.arena.get(prop.initializer)
                        && init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        && let Some(init_func) = self.ctx.arena.get_function(init_node)
                        && init_func.body.is_some()
                    {
                        self.collect_js_constructor_this_properties(
                            init_func.body,
                            &mut b.properties,
                            current_sym,
                            true,
                        );
                    }

                    let declared_type =
                        self.effective_class_property_declared_type(member_idx, prop);

                    let type_id = if let Some(declared_type) = declared_type {
                        declared_type
                    } else if prop.initializer.is_some() {
                        let current_property_placeholder = class_type::class_member_property(
                            class_type::ClassMemberProperty::new(name_atom, TypeId::ANY)
                                .optional(prop.question_token)
                                .readonly(is_readonly)
                                .visibility(visibility)
                                .parent(current_sym)
                                .declaration_order(declaration_order)
                                .symbol_named(is_symbol_named),
                        );
                        let mut partial_props: Vec<PropertyInfo> =
                            b.properties.values().cloned().collect();
                        if !partial_props.iter().any(|p| p.name == name_atom) {
                            partial_props.push(current_property_placeholder);
                        }
                        let refreshed_this_type = class_type::class_member_partial_this_type(
                            self.ctx.types,
                            partial_props,
                            b.string_index,
                            b.number_index,
                            b.symbol_index,
                            current_sym,
                            b.prescan_this_type,
                        );
                        if let Some(partial_this) = refreshed_this_type {
                            self.ctx.this_type_stack.push(partial_this);
                            // Property initializers may already have been typed earlier
                            // during statement checking with a stale provisional `this`.
                            // Recompute them against the refreshed partial instance type.
                            self.clear_type_cache_recursive(prop.initializer);
                        }
                        // If the initializer is exactly `this`, use the polymorphic
                        // ThisType so that `class C<T> { x = this; }` with `c: C<string>`
                        // makes `c.x` resolve to `C<string>`, not the raw class type.
                        let is_this_init = self
                            .ctx
                            .arena
                            .get(prop.initializer)
                            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);
                        let prev = self.ctx.preserve_literal_types;
                        let prev_decl = self.ctx.use_declared_type_for_identifier;
                        self.ctx.preserve_literal_types = true;
                        // Use the symbol's declared type for identifier initializers
                        // so `class C { D = DEFAULT; }` (with `const DEFAULT: AB`)
                        // inherits `AB`, not the flow-narrowed literal value 'A'.
                        self.ctx.use_declared_type_for_identifier = true;
                        let init_type = if is_this_init {
                            self.ctx.types.this_type()
                        } else {
                            self.get_type_of_node(prop.initializer)
                        };
                        if refreshed_this_type.is_some() {
                            self.ctx.this_type_stack.pop();
                        }
                        self.ctx.preserve_literal_types = prev;
                        self.ctx.use_declared_type_for_identifier = prev_decl;
                        let init_type = if init_type == TypeId::ANY
                            && self.has_accessor_modifier(&prop.modifiers)
                        {
                            self.this_access_name_node(prop.initializer)
                                .and_then(|name_idx| {
                                    self.infer_property_type_from_class_member_assignments(
                                        &class.members.nodes,
                                        name_idx,
                                        false,
                                    )
                                })
                                .unwrap_or(init_type)
                        } else {
                            init_type
                        };
                        // Widen literal types for mutable class properties.
                        // `class Foo { name = "" }` → `name: string`.
                        // Readonly properties keep literal types:
                        // `class Foo { readonly tag = "x" }` → `tag: "x"`.
                        // The freshness boundary skips widening when the
                        // initializer is not a fresh literal expression
                        // (e.g. `D = DEFAULT` where DEFAULT is a typed
                        // identifier reference) — its type already came from
                        // a declared annotation, matching tsc's
                        // getWidenedLiteralLikeTypeForContextualType.
                        if is_readonly {
                            // A bare `unique symbol` alias in a readonly field
                            // widens to `symbol` (tsc getWidenedUniqueESSymbolType);
                            // a freshly minted `= Symbol()` factory keeps its own
                            // `typeof f` identity. Mutable fields already widen via
                            // the freshness path below.
                            self.widen_readonly_field_unique_symbol_alias(member_idx, init_type)
                        } else {
                            self.widen_expression_type_if_fresh(prop.initializer, init_type)
                        }
                    } else if self.has_accessor_modifier(&prop.modifiers) {
                        self.infer_property_type_from_class_member_assignments(
                            &class.members.nodes,
                            prop.name,
                            false,
                        )
                        .unwrap_or(TypeId::ANY)
                    } else {
                        // An un-annotated, un-initialized instance property takes
                        // its type from the constructor's `this.<name> = ...`
                        // assignments (tsc's control-flow property inference);
                        // only when none exist does it fall back to implicit
                        // 'any' (TS7008 when noImplicitAny is on).
                        self.infer_property_type_from_constructor_flow(
                            &class.members.nodes,
                            prop.name,
                            class_requires_super,
                        )
                        .unwrap_or(TypeId::ANY)
                    };
                    self.ctx.node_types.insert(member_idx.0, type_id);

                    // A wide-`symbol` key contributes to the shape's symbol
                    // index signature rather than a named member; the synthetic
                    // `__symbol_<file>_<sym>` atom must not reach `properties`,
                    // or the member would only match a declaration keyed off the
                    // very same binding.
                    if keys_symbol_index {
                        b.set_has_late_bound_members();
                        self.merge_class_wide_symbol_member_index(
                            &mut b.symbol_index,
                            type_id,
                            is_readonly,
                        );
                        continue;
                    }

                    b.properties.insert(
                        name_atom,
                        class_type::class_member_property(
                            class_type::ClassMemberProperty::new(name_atom, type_id)
                                .optional(prop.question_token)
                                .readonly(is_readonly)
                                .visibility(visibility)
                                .parent(current_sym)
                                .declaration_order(declaration_order)
                                .symbol_named(is_symbol_named),
                        ),
                    );
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&method.modifiers, method.name) {
                        b.set_has_nominal_members();
                    }

                    // In JS/checkJs mode, method body `this.prop = value` assignments
                    // serve as property declarations (same as constructor assignments).
                    // Scan before deferring so properties are in the partial `this` type.
                    if self.ctx.is_js_file() && method.body.is_some() {
                        self.collect_js_constructor_this_properties(
                            method.body,
                            &mut b.properties,
                            current_sym,
                            true,
                        );
                    }

                    // Defer method processing to phase 2
                    b.deferred_methods
                        .push((member_idx, method, declaration_order));
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&accessor.modifiers, accessor.name) {
                        b.set_has_nominal_members();
                    }
                    let Some(name) = self.get_property_name_resolved(accessor.name) else {
                        if self
                            .ctx
                            .arena
                            .get(accessor.name)
                            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                        {
                            b.set_has_late_bound_members();
                            tracing::debug!(
                                member = member_idx.0,
                                "class member computed name unresolved -> late-bound"
                            );
                        }
                        continue;
                    };
                    let is_symbol_named = self.class_member_name_is_symbol_named(accessor.name);
                    let keys_symbol_index =
                        self.class_member_computed_key_is_wide_symbol(accessor.name);
                    let name_atom = self.ctx.types.intern_string(&name);
                    let visibility = self.get_member_visibility(&accessor.modifiers, accessor.name);
                    b.deferred_accessors.push(DeferredAccessor {
                        member_idx,
                        accessor,
                        is_getter: k == syntax_kind_ext::GET_ACCESSOR,
                        name_atom,
                        is_symbol_named,
                        keys_symbol_index,
                        visibility,
                        declaration_order,
                    });
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_none() {
                        continue;
                    }
                    // Process constructor parameter properties
                    for (param_pos, &param_idx) in ctor.parameters.nodes.iter().enumerate() {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        if self.has_private_modifier(&param.modifiers)
                            || self.has_protected_modifier(&param.modifiers)
                        {
                            b.set_has_nominal_members();
                        }
                        let Some(name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        if b.properties.contains_key(&name_atom) {
                            continue;
                        }
                        let is_readonly = self.has_readonly_modifier(&param.modifiers);
                        let type_id = if param.type_annotation.is_some() {
                            self.get_type_from_type_node(param.type_annotation)
                        } else if param.initializer.is_some() {
                            let init_type = self.get_type_of_node(param.initializer);
                            // Widen for mutable constructor parameter properties
                            if is_readonly {
                                init_type
                            } else {
                                self.widen_literal_type(init_type)
                            }
                        } else {
                            TypeId::ANY
                        };

                        let visibility = self.get_visibility_from_modifiers(&param.modifiers);
                        b.properties.insert(
                            name_atom,
                            class_type::class_member_property(
                                class_type::ClassMemberProperty::new(name_atom, type_id)
                                    .optional(param.question_token)
                                    .readonly(is_readonly)
                                    .visibility(visibility)
                                    .parent(current_sym)
                                    .declaration_order(declaration_order + param_pos as u32 + 1),
                            ),
                        );
                    }

                    // In JS/checkJs mode, constructor body `this.prop = value`
                    // assignments serve as property declarations.
                    // Scan the constructor body for these patterns and add
                    // them to the class instance type.
                    // Check if the class is defined in a JS file, not just if the
                    // current file being processed is a JS file. This ensures that
                    // when a TS file references a class from a JS file, the JSDoc
                    // annotated properties are still collected.
                    let class_is_in_js_file = self
                        .source_file_data_for_node(class_idx)
                        .map(|sf| is_js_file_name(&sf.file_name))
                        .unwrap_or(false);
                    if class_is_in_js_file {
                        self.collect_js_constructor_this_properties(
                            ctor.body,
                            &mut b.properties,
                            current_sym,
                            true,
                        );
                    }
                }
                k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                    let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&index_sig.modifiers) {
                        continue;
                    }

                    let param_idx = index_sig
                        .parameters
                        .nodes
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE);
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                        continue;
                    };

                    let key_type = if param.type_annotation.is_none() {
                        TypeId::ANY
                    } else {
                        self.get_type_from_type_node(param.type_annotation)
                    };

                    // TS1268: An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type
                    // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                    let has_param_grammar_error = param.dot_dot_dot_token || param.question_token;
                    // Accepts any alias reducing to a valid index key, including the
                    // cross-file lib global `PropertyKey`. A generic/literal key
                    // keeps falling through to TS1268 (this site has no TS1337
                    // branch), preserving existing class behavior.
                    let (_is_generic_or_literal, is_valid_index_type) =
                        self.classify_index_sig_param_type(key_type, param.type_annotation);

                    if !is_valid_index_type && !has_param_grammar_error {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            param_idx,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                        );
                    }

                    let value_type = if index_sig.type_annotation.is_none() {
                        TypeId::ANY
                    } else {
                        self.get_type_from_type_node(index_sig.type_annotation)
                    };
                    let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                    let param_name = self
                        .ctx
                        .arena
                        .get(param.name)
                        .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                        .map(|name_ident| self.ctx.types.intern_string(&name_ident.escaped_text));

                    let index = class_type::class_declared_index_signature(
                        key_type, value_type, readonly, param_name,
                    );

                    if is_valid_index_type {
                        if key_type == TypeId::NUMBER {
                            Self::merge_index_signature(&mut b.number_index, index);
                        } else if key_type == TypeId::SYMBOL {
                            Self::merge_index_signature(&mut b.symbol_index, index);
                        } else {
                            Self::merge_index_signature(&mut b.string_index, index);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Install the current class for the remaining construction phases and save
    /// the previous `enclosing_class` for unconditional restoration afterward.
    /// Early callers may not have published a partial instance cache yet; late
    /// callers retain the historical `TypeId::ERROR` sentinel. The exact
    /// parameter identities stored here remain visible when a property
    /// initializer pushes a same-named local binder.
    pub(super) fn class_instance_setup_enclosing<'b>(
        &mut self,
        class: &'b tsz_parser::parser::node::ClassData,
        class_idx: NodeIndex,
        b: &mut ClassInstanceBuilder<'b>,
        cache_may_be_unpublished: bool,
    ) {
        let prev_enclosing_class = self.ctx.enclosing_class.take();
        if let Some(ref prev) = prev_enclosing_class {
            self.ctx.enclosing_class_chain.push(prev.class_idx);
        }
        let class_type_param_names: Vec<String> = b
            .class_type_param_updates
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();
        let cached_instance_this_type = self
            .ctx
            .class_instance_type_cache
            .borrow()
            .get(&class_idx)
            .copied()
            .or_else(|| (!cache_may_be_unpublished).then_some(TypeId::ERROR));
        self.ctx.enclosing_class = Some(EnclosingClassInfo {
            name: self.get_class_name_from_decl(class_idx),
            class_idx,
            member_nodes: class.members.nodes.clone(),
            in_constructor: false,
            is_declared: self.is_ambient_class_declaration(class_idx),
            in_static_property_initializer: false,
            in_static_member: false,
            has_super_call_in_current_constructor: false,
            cached_instance_this_type,
            type_param_names: class_type_param_names,
            class_type_parameters: b.class_type_params.clone(),
            class_type_parameter_ids: b.class_type_param_ids.clone(),
            enclosing_async_depth: self.ctx.async_depth,
        });
        b.restore_enclosing_class = RestoreEnclosingClass::To(prev_enclosing_class);
    }

    /// Pop the Phase-0 prescan `this` at the original boundary between field
    /// construction and deferred bodies.
    pub(super) fn class_instance_finish_prescan_this(&mut self, b: &ClassInstanceBuilder<'_>) {
        if b.pushed_prescan_this() {
            self.ctx.this_type_stack.pop();
        }
    }

    /// Phase 2: Process deferred methods with a partial `this` type so that
    /// method body inference can resolve `this.x` references (e.g.
    /// `return this.b`).
    pub(super) fn class_instance_phase2_deferred_methods<'b>(
        &mut self,
        class: &'b tsz_parser::parser::node::ClassData,
        class_idx: NodeIndex,
        b: &mut ClassInstanceBuilder<'b>,
    ) {
        let current_sym = b.current_sym;
        if !b.deferred_methods.is_empty() {
            let mut partial_method_props = b.properties.clone();
            let mut partial_method_string_index = b.string_index;
            let mut partial_method_number_index = b.number_index;
            let mut partial_method_symbol_index = b.symbol_index;

            // Method body inference needs inherited `this` members up front.
            // Without the base instance surface here, overrides like
            // `return this.optionalProperty` in a subclass of a declaration-merged
            // class/interface infer `error` instead of the inherited property type.
            if let Some(ref heritage_clauses) = class.heritage_clauses {
                for &clause_idx in &heritage_clauses.nodes {
                    let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                        continue;
                    };
                    let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                        continue;
                    };
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    let Some(&type_idx) = heritage.types.nodes.first() else {
                        break;
                    };
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        break;
                    };
                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                    if let Some(base_instance_type) =
                        self.base_instance_type_from_expression(expr_idx, type_arguments)
                    {
                        self.merge_base_instance_properties(
                            base_instance_type,
                            &mut partial_method_props,
                            &mut partial_method_string_index,
                            &mut partial_method_number_index,
                            &mut partial_method_symbol_index,
                        );
                    }
                    break;
                }
            }

            // Build a partial instance type from properties collected so far,
            // including placeholder entries for ALL deferred methods so that
            // methods can reference each other via `this` (e.g. `typeof a`
            // in return type where `a` defaults to `this.getNumber()`).
            let mut partial_props: Vec<PropertyInfo> = Vec::with_capacity(
                partial_method_props.len() + b.deferred_methods.len() + b.deferred_accessors.len(),
            );
            partial_props.extend(partial_method_props.values().cloned());
            let mut partial_prop_names: FxHashSet<Atom> =
                FxHashSet::with_capacity_and_hasher(partial_props.len(), Default::default());
            partial_prop_names.extend(partial_props.iter().map(|prop| prop.name));
            for (_, method, declaration_order) in &b.deferred_methods {
                if let Some(name) = self.get_property_name_resolved(method.name) {
                    // Wide-`symbol`-keyed methods have no named member to
                    // placehold; they reach `this` through the symbol index
                    // signature. Tested only AFTER the name resolution above,
                    // which owns the one value-position evaluation of the key
                    // expression — an evaluation performed outside that context
                    // re-reports its diagnostics (a type-only-imported key in an
                    // ambient class re-fires TS1361).
                    if self.class_member_computed_key_is_wide_symbol(method.name) {
                        continue;
                    }
                    let is_symbol_named = self.class_member_name_is_symbol_named(method.name);
                    let name_atom = self.ctx.types.intern_string(&name);
                    if partial_prop_names.insert(name_atom) {
                        // For methods with explicit return type annotations, use the
                        // declared return type instead of ANY. This allows other methods
                        // that reference `this.method()` during body inference to get the
                        // correct return type. Without this, `{ ...this.annotatedMethod() }`
                        // would see return type ANY and produce an empty spread result.
                        let return_type = if method.type_annotation.is_some() {
                            let (_type_params, type_param_updates) =
                                self.push_type_parameters(&method.type_parameters);
                            let return_type = self.get_type_from_type_node(method.type_annotation);
                            self.pop_type_parameters(type_param_updates);
                            return_type
                        } else {
                            TypeId::ANY
                        };
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&method.type_parameters);
                        self.pop_type_parameters(type_param_updates);
                        let placeholder = class_type::class_method_callable_type(
                            self.ctx.types,
                            vec![class_type::class_method_call_signature(
                                type_params,
                                vec![class_type::class_rest_any_param()],
                                None,
                                return_type,
                                None,
                            )],
                        );
                        partial_props.push(class_type::class_member_property(
                            class_type::ClassMemberProperty::new(name_atom, placeholder)
                                .method(true)
                                .parent(current_sym)
                                .declaration_order(*declaration_order)
                                .symbol_named(is_symbol_named),
                        ));
                    }
                }
            }
            for deferred in &b.deferred_accessors {
                if partial_prop_names.insert(deferred.name_atom) {
                    partial_props.push(class_type::class_member_property(
                        class_type::ClassMemberProperty::new(deferred.name_atom, TypeId::ANY)
                            .with_write_type(TypeId::UNKNOWN)
                            .class_prototype(true)
                            .visibility(deferred.visibility)
                            .parent(current_sym)
                            .declaration_order(deferred.declaration_order)
                            .symbol_named(deferred.is_symbol_named),
                    ));
                }
            }
            let partial_type = class_type::class_member_object_with_indexes_type(
                self.ctx.types,
                partial_props,
                partial_method_string_index,
                partial_method_number_index,
                partial_method_symbol_index,
                current_sym,
            );
            self.ctx.this_type_stack.push(partial_type);

            // Cache the partial instance type in the node-indexed cache only.
            // Method return-type inference can trigger property access on
            // self-referential parameters (e.g. `p.x` where `p: Point` inside
            // class Point).  resolve_type_for_property_access_inner checks
            // class_instance_type_cache as a fallback for in-progress builds,
            // so Lazy(DefId) resolves to this partial type during building and
            // to the final type afterward.
            //
            // We avoid caching in symbol_instance_types here because parameter
            // types cached by get_type_of_node would permanently hold the
            // partial type, causing private-name brand-check failures.
            self.ctx
                .class_instance_type_cache
                .borrow_mut()
                .insert(class_idx, partial_type);

            // Keep enclosing_class.cached_instance_this_type in sync with the
            // partial type so that class_member_this_type returns the current
            // construction state (not the stale Phase 0 prescan type).
            if let Some(ref mut info) = self.ctx.enclosing_class {
                info.cached_instance_this_type = Some(partial_type);
            }

            // Check if the class constructor is currently being resolved.
            // When it is, method body inference can trigger a cycle:
            //   instance type → method body inference → Bar.instance → constructor type → CYCLE
            // In this case, skip body-based return type inference and use ANY
            // as a placeholder. The final constructor type will be computed
            // correctly after the instance type is done.
            let in_constructor_resolution = current_sym
                .is_some_and(|sym_id| self.ctx.class_constructor_resolution_set.contains(&sym_id));

            for (member_idx, method, declaration_order) in std::mem::take(&mut b.deferred_methods) {
                let mut signature = if in_constructor_resolution
                    && method.type_annotation.is_none()
                    && method.body.is_some()
                {
                    // Skip body inference to avoid cycle - build a minimal signature
                    self.exclude_params_for_type_param_constraints(&method.parameters);
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&method.type_parameters);
                    self.clear_excluded_params_for_type_param_constraints();
                    let (params, this_type) =
                        self.extract_params_from_parameter_list(&method.parameters);
                    self.pop_type_parameters(type_param_updates);
                    class_type::class_method_call_signature(
                        type_params,
                        params,
                        this_type,
                        TypeId::ANY,
                        None,
                    )
                } else {
                    self.call_signature_from_method(method, member_idx)
                };
                // When a class method without an explicit return type annotation
                // infers its return type from the body and the result is the partial
                // class instance type (i.e. the body does `return this;`), replace
                // with polymorphic `ThisType`.  This enables fluent method chaining
                // on subclass instances:  c.foo().bar().baz()  where each method is
                // defined on a different class in the hierarchy.
                //
                // Two checks: (1) type-based — the inferred return matches partial_type,
                // or (2) syntactic — every return statement returns `this`. The syntactic
                // check is needed because type interning or flow analysis may produce a
                // TypeId that doesn't equal partial_type even though it represents the
                // same class instance.
                if method.body.is_some() && method.type_annotation.is_none() {
                    let type_match = signature.return_type == partial_type;
                    let syntactic_match = self.method_body_returns_only_this(method.body);
                    if type_match || syntactic_match {
                        signature.return_type = self.ctx.types.this_type();
                    }
                }
                let callable_type =
                    class_type::class_method_callable_type(self.ctx.types, vec![signature.clone()]);
                let callable_or_undefined = class_type::optional_class_member_type(
                    self.ctx.types,
                    callable_type,
                    method.question_token,
                );
                let Some(name) = self.get_property_name_resolved(method.name) else {
                    if self
                        .ctx
                        .arena
                        .get(method.name)
                        .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                    {
                        b.set_has_late_bound_members();
                        tracing::debug!(
                            member = member_idx.0,
                            "class method computed name unresolved -> late-bound"
                        );
                        self.merge_index_signature_from_unresolved_computed_name(
                            method.name,
                            callable_or_undefined,
                            &mut b.string_index,
                            &mut b.number_index,
                            &mut b.symbol_index,
                        );
                    }
                    continue;
                };
                // The key resolved to a name, but a wide-`symbol` key's name is
                // the synthetic `__symbol_<file>_<sym>` atom, which only ever
                // matches a declaration keyed off the SAME binding. Route the
                // member into the shape's symbol index signature instead, so a
                // class and the interface it implements stay mutually assignable
                // when each keys off a DIFFERENT `symbol` binding — the whole
                // point of tsc's symbol-index lowering. Tested after the
                // resolution above, which owns the one value-position evaluation
                // of the key expression; re-evaluating outside that context
                // re-reports its diagnostics (TS1361 for a type-only-imported
                // key in an ambient class). The unresolved-name sibling case is
                // handled by `merge_index_signature_from_unresolved_computed_name`
                // in the `else` arm above.
                if self.class_member_computed_key_is_wide_symbol(method.name) {
                    b.set_has_late_bound_members();
                    self.merge_class_wide_symbol_member_index(
                        &mut b.symbol_index,
                        callable_or_undefined,
                        false,
                    );
                    continue;
                }
                let is_symbol_named = self.class_member_name_is_symbol_named(method.name);
                let name_atom = self.ctx.types.intern_string(&name);
                let visibility = self.get_member_visibility(&method.modifiers, method.name);
                let entry = b.methods.entry(name_atom).or_insert(MethodAggregate {
                    overload_signatures: Vec::new(),
                    impl_signatures: Vec::new(),
                    overload_optional: false,
                    impl_optional: false,
                    visibility,
                    declaration_order,
                    is_symbol_named,
                });
                if method.body.is_none() {
                    entry.overload_signatures.push(signature);
                    entry.overload_optional |= method.question_token;
                } else {
                    entry.impl_signatures.push(signature);
                    entry.impl_optional |= method.question_token;
                }
            }

            self.ctx.this_type_stack.pop();
        }
    }

    /// Process deferred accessors under a partial `this` type built from the
    /// class's properties and methods, aggregating getter/setter types.
    pub(super) fn class_instance_process_deferred_accessors(
        &mut self,
        class_idx: NodeIndex,
        b: &mut ClassInstanceBuilder<'_>,
    ) {
        let current_sym = b.current_sym;
        if !b.deferred_accessors.is_empty() {
            // Base members (own fields + own methods) shared by every accessor's
            // partial `this`. Built once; accessor placeholders are layered on
            // per iteration so a getter body that reads an *earlier* accessor
            // observes its already-resolved type rather than an `any`
            // placeholder.
            let mut base_props: Vec<PropertyInfo> =
                Vec::with_capacity(b.properties.len() + b.methods.len());
            base_props.extend(b.properties.values().cloned());
            for (&name, method) in &b.methods {
                let (signatures, optional) = if !method.overload_signatures.is_empty() {
                    (&method.overload_signatures, method.overload_optional)
                } else {
                    (&method.impl_signatures, method.impl_optional)
                };
                if signatures.is_empty() {
                    continue;
                }
                let type_id =
                    class_type::class_method_callable_type(self.ctx.types, signatures.clone());
                base_props.push(class_type::class_member_property(
                    class_type::ClassMemberProperty::new(name, type_id)
                        .optional(optional)
                        .method(true)
                        .visibility(method.visibility)
                        .parent(current_sym)
                        .symbol_named(method.is_symbol_named),
                ));
            }
            let deferred_accessors = std::mem::take(&mut b.deferred_accessors);

            // Distinct accessor names to layer onto each partial `this`, deduped
            // once (against the base members and each other) rather than per
            // iteration. Every accessor member stays present in every partial so
            // a forward reference (`get a() { return this.b; }` before `b`) never
            // draws a false TS2339; its type is read fresh from `b.accessors`
            // each iteration so an already-processed accessor contributes its
            // resolved type while the rest stay `any`.
            let mut placeholder_accessors: Vec<&DeferredAccessor> = Vec::new();
            let mut placeholder_seen: FxHashSet<Atom> =
                base_props.iter().map(|prop| prop.name).collect();
            for ad in &deferred_accessors {
                // A wide-`symbol`-keyed accessor has no named member to
                // placehold; it reaches `this` through the symbol index
                // signature that `b.symbol_index` already carries.
                if ad.keys_symbol_index {
                    continue;
                }
                if placeholder_seen.insert(ad.name_atom) {
                    placeholder_accessors.push(ad);
                }
            }

            let mut pushed_this = false;
            for deferred in &deferred_accessors {
                // (Re)build the partial `this` reflecting accessor types resolved
                // so far. The accurate field types keep `class_member_this_type`
                // from falling back to a prescan partial that degrades an
                // unannotated field to `any`, and an accessor already processed in
                // this loop contributes its resolved type, so a getter reading an
                // earlier getter (`get b() { return this.a; }`) sees `a`'s real
                // type (#14511).
                let mut props = base_props.clone();
                for ad in &placeholder_accessors {
                    let resolved = b
                        .accessors
                        .get(&ad.name_atom)
                        .and_then(|a| a.getter.or(a.setter))
                        .unwrap_or(TypeId::ANY);
                    props.push(class_type::class_member_property(
                        class_type::ClassMemberProperty::new(ad.name_atom, resolved)
                            .class_prototype(true)
                            .visibility(ad.visibility)
                            .parent(current_sym)
                            .declaration_order(ad.declaration_order)
                            .symbol_named(ad.is_symbol_named),
                    ));
                }
                let partial_type = class_type::class_member_object_with_indexes_type(
                    self.ctx.types,
                    props,
                    b.string_index,
                    b.number_index,
                    b.symbol_index,
                    current_sym,
                );
                if pushed_this {
                    self.ctx.this_type_stack.pop();
                }
                self.ctx.this_type_stack.push(partial_type);
                pushed_this = true;

                // Publish the partial as the in-progress instance type so a
                // re-entrant `this` resolution during body inference observes the
                // up-to-date field types (mirrors the phase-2 deferred-method
                // caching), and keep `cached_instance_this_type` in sync so
                // `class_member_this_type` returns the current construction state
                // rather than the stale Phase-0 prescan type.
                self.ctx
                    .class_instance_type_cache
                    .borrow_mut()
                    .insert(class_idx, partial_type);
                if let Some(ref mut info) = self.ctx.enclosing_class {
                    info.cached_instance_this_type = Some(partial_type);
                }

                if deferred.is_getter {
                    let getter_type = if deferred.accessor.type_annotation.is_some() {
                        self.get_type_from_type_node(deferred.accessor.type_annotation)
                    } else if let Some(jsdoc_type) =
                        self.jsdoc_type_annotation_for_node(deferred.member_idx)
                    {
                        jsdoc_type
                    } else {
                        let t = self.infer_getter_return_type_for_node(
                            deferred.member_idx,
                            deferred.accessor.body,
                        );
                        self.ctx.node_types.insert(deferred.member_idx.0, t);
                        // When a getter without an explicit return type annotation
                        // infers its return type from the body and the result is the
                        // partial class instance type (i.e. the body does `return this;`),
                        // replace with polymorphic `ThisType` — same as for methods.
                        //
                        // Two checks mirror the method logic (Phase 2):
                        // (1) type-based — the inferred return matches partial_type,
                        // (2) syntactic — every return statement returns `this`.
                        // The syntactic check is needed because return-type widening
                        // or phase-specific interning can produce a TypeId that
                        // doesn't equal partial_type even though it represents the
                        // same class instance.
                        let type_match = t == partial_type;
                        let syntactic_match =
                            self.method_body_returns_only_this(deferred.accessor.body);
                        if deferred.accessor.type_annotation.is_none()
                            && (type_match || syntactic_match)
                        {
                            self.ctx.types.this_type()
                        } else {
                            t
                        }
                    };
                    if deferred.keys_symbol_index {
                        b.set_has_late_bound_members();
                        self.merge_class_wide_symbol_member_index(
                            &mut b.symbol_index,
                            getter_type,
                            false,
                        );
                        continue;
                    }
                    let entry =
                        b.accessors
                            .entry(deferred.name_atom)
                            .or_insert(AccessorAggregate {
                                getter: None,
                                setter: None,
                                visibility: deferred.visibility,
                                declaration_order: deferred.declaration_order,
                                is_symbol_named: deferred.is_symbol_named,
                            });
                    entry.getter = Some(getter_type);
                } else {
                    let setter_type = deferred
                        .accessor
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&param_idx| {
                            let param_node = self.ctx.arena.get(param_idx)?;
                            let param = self.ctx.arena.get_parameter(param_node)?;
                            // TS type annotation (non-JS files)
                            if !self.ctx.is_js_file() && param.type_annotation.is_some() {
                                return Some(self.get_type_from_type_node(param.type_annotation));
                            }
                            // JSDoc @param annotation (JS files)
                            if self.ctx.is_js_file() {
                                let jsdoc = self.get_jsdoc_for_function(deferred.member_idx)?;
                                let pname = self.parameter_name_for_error(param.name);
                                let comment_start =
                                    self.get_jsdoc_comment_pos_for_function(deferred.member_idx);
                                return self.resolve_jsdoc_param_type_with_pos(
                                    &jsdoc,
                                    &pname,
                                    comment_start,
                                );
                            }
                            None
                        })
                        .unwrap_or(TypeId::UNKNOWN);
                    if deferred.keys_symbol_index {
                        b.set_has_late_bound_members();
                        self.merge_class_wide_symbol_member_index(
                            &mut b.symbol_index,
                            setter_type,
                            false,
                        );
                        continue;
                    }
                    let entry =
                        b.accessors
                            .entry(deferred.name_atom)
                            .or_insert(AccessorAggregate {
                                getter: None,
                                setter: None,
                                visibility: deferred.visibility,
                                declaration_order: deferred.declaration_order,
                                is_symbol_named: deferred.is_symbol_named,
                            });
                    entry.setter = Some(setter_type);
                }
            }

            if pushed_this {
                self.ctx.this_type_stack.pop();
            }
        }

        if let RestoreEnclosingClass::To(prev_enclosing_class) =
            std::mem::replace(&mut b.restore_enclosing_class, RestoreEnclosingClass::Skip)
        {
            self.ctx.enclosing_class = prev_enclosing_class;
            if self.ctx.enclosing_class.is_some() {
                self.ctx.enclosing_class_chain.pop();
            }
        }
    }

    /// Convert aggregated accessors and methods into properties, then add the
    /// private brand property for nominal typing if needed.
    pub(super) fn class_instance_finalize_members(
        &mut self,
        class_idx: NodeIndex,
        b: &mut ClassInstanceBuilder<'_>,
    ) {
        let current_sym = b.current_sym;

        // Convert accessors to properties
        for (name, accessor) in std::mem::take(&mut b.accessors) {
            if b.methods.contains_key(&name) {
                continue;
            }
            // When a setter parameter has no type annotation, its type is UNKNOWN
            // (sentinel). Filter it out so paired accessors fall back to the
            // getter type, matching tsc.
            let setter_type = accessor.setter.filter(|&t| t != TypeId::UNKNOWN);
            let read_type = accessor.getter.or(setter_type).unwrap_or(TypeId::UNKNOWN);
            let write_type = setter_type.or(accessor.getter).unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
            b.properties.insert(
                name,
                class_type::class_member_property(
                    class_type::ClassMemberProperty::new(name, read_type)
                        .with_write_type(write_type)
                        .readonly(readonly)
                        .class_prototype(true)
                        .visibility(accessor.visibility)
                        .parent(current_sym)
                        .declaration_order(accessor.declaration_order)
                        .symbol_named(accessor.is_symbol_named),
                ),
            );
        }

        // Convert methods to callable properties
        for (name, method) in std::mem::take(&mut b.methods) {
            // Keep existing field/accessor entries for duplicate names.
            // Duplicate member diagnostics are handled separately (TS2300/TS2393),
            // and preserving the non-method member avoids cascading TS2322 errors.
            if b.properties.contains_key(&name) {
                continue;
            }
            let (signatures, optional) = if !method.overload_signatures.is_empty() {
                (method.overload_signatures, method.overload_optional)
            } else {
                (method.impl_signatures, method.impl_optional)
            };
            if signatures.is_empty() {
                continue;
            }
            let type_id = class_type::class_method_callable_type(self.ctx.types, signatures);
            // Note: we intentionally do NOT cache instance method types in
            // node_types here. Instance methods go through deferred processing
            // where the return type may be rewritten (e.g., `this` returns).
            // Caching the final merged type can cause DTS emit regressions
            // when the type differs from what the emitter's fallback expects.
            b.properties.insert(
                name,
                class_type::class_member_property(
                    class_type::ClassMemberProperty::new(name, type_id)
                        .optional(optional)
                        .method(true)
                        .visibility(method.visibility)
                        .parent(current_sym)
                        .declaration_order(method.declaration_order)
                        .symbol_named(method.is_symbol_named),
                ),
            );
        }

        // Add private brand property for nominal typing
        if b.has_nominal_members() {
            let brand_name = if let Some(sym_id) = current_sym {
                format!("__private_brand_{}", sym_id.0)
            } else {
                format!("__private_brand_node_{}", class_idx.0)
            };
            let brand_atom = self.ctx.types.intern_string(&brand_name);
            b.properties
                .entry(brand_atom)
                .or_insert(class_type::class_member_property(
                    class_type::ClassMemberProperty::new(brand_atom, TypeId::UNKNOWN)
                        .readonly(true),
                ));
        }
    }
}
