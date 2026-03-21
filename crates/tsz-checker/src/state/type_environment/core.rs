//! Core type environment building, application type evaluation, property access
//! type resolution, and type node resolution.

use crate::query_boundaries::state::type_environment as query;
use crate::state::{CheckerState, EnumKind, MAX_INSTANTIATION_DEPTH};
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::MappedTypeId;
use tsz_solver::SourceLocation;
use tsz_solver::TypeId;
use tsz_solver::Visibility;
use tsz_solver::{CallSignature, CallableShape, ParamInfo};

// Thread-local counters for `evaluate_application_type` that survive cross-arena
// delegation. Per-context counters (`instantiation_depth`, `application_eval_set`)
// reset to 0 when `with_parent_cache` creates child contexts, allowing cascading
// recursion across context boundaries (e.g., react16.d.ts with deeply-nested
// generics like InferProps<V>, RequiredKeys<V>, Validator<T>).
//
// Two thread-local counters work together:
// - **Depth**: tracks nesting of `evaluate_application_type` calls (like the
//   per-context `instantiation_depth` but global). Catches individual deep chains.
// - **Fuel**: limits the TOTAL number of non-cached `evaluate_application_type`
//   calls across ALL contexts. Catches wide, shallow cascades where thousands of
//   unique Application types each trigger moderate-depth evaluation.
//
// Fuel resets at the start of `build_type_environment` so each file gets a fresh budget.
thread_local! {
    static GLOBAL_INSTANTIATION_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static GLOBAL_INSTANTIATION_FUEL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// Global instantiation depth limit — tighter than the per-context MAX_INSTANTIATION_DEPTH (50).
const MAX_GLOBAL_INSTANTIATION_DEPTH: u32 = 50;

// Global instantiation fuel limit — maximum non-cached `evaluate_application_type`
// invocations per file. react16.d.ts can trigger thousands of unique Application
// evaluations via `build_type_environment`; this caps the total work.
const MAX_GLOBAL_INSTANTIATION_FUEL: u32 = 2000;

impl<'a> CheckerState<'a> {
    // Get type of object literal.
    // =========================================================================
    // Type Relations (uses solver::CompatChecker for assignability)
    // =========================================================================

    // Note: enum_symbol_from_type and enum_symbol_from_value_type are defined in type_checking.rs

    pub(crate) fn enum_object_type(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        use rustc_hash::FxHashMap;
        use tsz_solver::{IndexSignature, ObjectShape, PropertyInfo};

        let factory = self.ctx.types.factory();
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let mut props: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(member.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                // Fix: Create nominal enum member types for each member
                // This preserves nominal identity so E.A is not assignable to E.B
                let Some(member_sym_id) = self
                    .ctx
                    .binder
                    .get_node_symbol(member_idx)
                    .or_else(|| self.ctx.binder.get_node_symbol(member.name))
                else {
                    continue;
                };
                let Some(member_def_id) =
                    self.ctx.symbol_to_def.borrow().get(&member_sym_id).copied()
                else {
                    continue;
                };
                let literal_type = self.enum_member_type_from_decl(member_idx);
                let specific_member_type = factory.enum_type(member_def_id, literal_type);

                props.entry(name_atom).or_insert(PropertyInfo {
                    name: name_atom,
                    type_id: specific_member_type,
                    write_type: specific_member_type,
                    optional: false,
                    readonly: true,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                });
            }
        }

        let properties: Vec<PropertyInfo> = props.into_values().collect();
        let is_const_enum = symbol.flags & symbol_flags::CONST_ENUM != 0;
        let flags = if is_const_enum {
            tsz_solver::ObjectFlags::CONST_ENUM
        } else {
            tsz_solver::ObjectFlags::empty()
        };
        if self.enum_kind(sym_id) == Some(EnumKind::Numeric) {
            let number_index = Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: true,
                param_name: None,
            });
            return Some(factory.object_with_index(ObjectShape {
                flags,
                properties,
                number_index,
                ..ObjectShape::default()
            }));
        }

        Some(factory.object_with_flags_and_symbol(properties, flags, None))
    }

    /// Evaluate complex type constructs for assignability checking.
    ///
    /// This function pre-processes types before assignability checking to ensure
    /// that complex type constructs are properly resolved. This is necessary because
    /// some types need to be expanded or evaluated before compatibility can be determined.
    ///
    /// ## Type Constructs Evaluated:
    /// - **Application** (`Map<string, number>`): Generic type instantiation
    /// - **`IndexAccess`** (`Type["key"]`): Indexed access types
    /// - **`KeyOf`** (`keyof Type`): Keyof operator types
    /// - **Mapped** (`{ [K in Keys]: V }`): Mapped types
    /// - **Conditional** (`T extends U ? X : Y`): Conditional types
    ///
    /// ## Evaluation Strategy:
    /// - **Application types**: Full symbol resolution with type environment
    /// - **Index/KeyOf/Mapped/Conditional**: Type environment evaluation
    /// - **Other types**: No evaluation needed (already in simplest form)
    ///
    /// ## Why Evaluation is Needed:
    /// - Generic types may be unevaluated applications (e.g., `Promise<T>`)
    /// - Indexed access types need to compute the result type
    /// - Mapped types need to expand the mapping
    /// - Conditional types need to check the condition and select branch
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Application types
    /// type App = Map<string, number>;
    /// let x: App;
    /// let y: Map<string, number>;
    /// // evaluate_type_for_assignability expands App for comparison
    ///
    /// // Indexed access types
    /// type User = { name: string; age: number };
    /// type UserName = User["name"];  // string
    /// // Evaluation needed to compute that UserName = string
    ///
    /// // Keyof types
    /// type Keys = keyof { a: string; b: number };  // "a" | "b"
    /// // Evaluation needed to compute the union of keys
    ///
    /// // Mapped types
    /// type Readonly<T> = { readonly [P in keyof T]: T[P] };
    /// type RO = Readonly<{ a: string }>;
    /// // Evaluation needed to expand the mapping
    ///
    /// // Conditional types
    /// type NonNull<T> = T extends null ? never : T;
    /// Evaluate an Application type by resolving the base symbol and instantiating.
    ///
    /// This handles types like `Store<ExtractState<R>>` by:
    /// 1. Resolving the base type reference to get its body
    /// 2. Getting the type parameters
    /// 3. Instantiating the body with the provided type arguments
    /// 4. Recursively evaluating the result
    pub(crate) fn evaluate_application_type(&mut self, type_id: TypeId) -> TypeId {
        if !query::is_generic_type(self.ctx.types, type_id) {
            return type_id;
        }

        if let Some(&cached) = self
            .ctx
            .narrowing_cache
            .resolve_cache
            .borrow()
            .get(&type_id)
        {
            return cached;
        }

        // Memoize application evaluation. This is a hot path for repeated accesses
        // on aliases like DeepPartial<{...}> and generic types like Result<T>.
        // TypeIds are interned, so the same Application TypeId always produces
        // the same evaluation result within a file check context.
        let is_monomorphic = !self.contains_type_parameters_cached(type_id);

        // Canonicalize application keys by evaluating type arguments first. This
        // allows structurally equivalent applications from different declaration
        // sites (e.g., repeated inline object-literal args) to share a cache hit.
        // Only applies to monomorphic types where argument evaluation is meaningful.
        let mut canonical_key: Option<TypeId> = None;
        if is_monomorphic
            && let Some((base, args)) = query::application_info(self.ctx.types, type_id)
            && !args.is_empty()
        {
            let canonical_args: Vec<TypeId> = args
                .into_iter()
                .map(|arg| self.resolve_lazy_type(arg))
                .collect();
            let key = self.ctx.types.application(base, canonical_args);
            if key != type_id {
                canonical_key = Some(key);
                let cached_opt = self
                    .ctx
                    .narrowing_cache
                    .resolve_cache
                    .borrow()
                    .get(&key)
                    .copied();
                if let Some(cached) = cached_opt {
                    self.ctx
                        .narrowing_cache
                        .resolve_cache
                        .borrow_mut()
                        .insert(type_id, cached);
                    return cached;
                }
            }
        }

        if !self.ctx.application_eval_set.insert(type_id) {
            // Re-entrancy guard: the same Application is already being evaluated
            // up the call stack. Return the type_id as-is to break the cycle.
            // Note: we do NOT flag depth_exceeded here because convergent recursive
            // types (e.g. GetChars<'AB'>) legitimately hit this guard during
            // tail-recursive conditional evaluation. The solver's own cycle detection
            // (seen_tail_call_apps + MAX_TAIL_RECURSION_DEPTH) handles TS2589.
            return type_id;
        }

        // Check BOTH per-context and thread-local instantiation depth/fuel.
        // Per-context counters reset to 0 on cross-arena delegation (with_parent_cache),
        // so thread-local counters provide global bounds that survive delegation.
        let global_depth = GLOBAL_INSTANTIATION_DEPTH.get();
        let global_fuel = GLOBAL_INSTANTIATION_FUEL.get();
        if self.ctx.instantiation_depth.get() >= MAX_INSTANTIATION_DEPTH
            || global_depth >= MAX_GLOBAL_INSTANTIATION_DEPTH
            || global_fuel >= MAX_GLOBAL_INSTANTIATION_FUEL
        {
            self.ctx.depth_exceeded.set(true);
            self.ctx.application_eval_set.remove(&type_id);
            return type_id;
        }
        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() + 1);
        GLOBAL_INSTANTIATION_DEPTH.set(global_depth + 1);
        GLOBAL_INSTANTIATION_FUEL.set(global_fuel + 1);

        let result = self.evaluate_application_type_inner(type_id);

        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() - 1);
        GLOBAL_INSTANTIATION_DEPTH.set(GLOBAL_INSTANTIATION_DEPTH.get().saturating_sub(1));
        self.ctx.application_eval_set.remove(&type_id);
        {
            let mut cache = self.ctx.narrowing_cache.resolve_cache.borrow_mut();
            cache.insert(type_id, result);
            if let Some(key) = canonical_key {
                cache.insert(key, result);
            }
        }

        // Also store in env_eval_cache so that the solver's TypeEvaluator
        // (pre-seeded from env_eval_cache) can reuse this result when
        // evaluating the same Application type during arg expansion.
        // Without this, long chains like `merge(merge(merge(...)))` cause
        // exponential re-evaluation because each step's Application is only
        // in resolve_cache (checker-only) and not visible to the solver.
        // Skip results containing unbound infer types — these arise when
        // conditional evaluation can't bind infer variables (e.g., empty
        // interface extends pattern). Caching them poisons later lookups.
        if result != type_id
            && !type_id.is_intrinsic()
            && !tsz_solver::type_queries::contains_infer_types_db(self.ctx.types, result)
        {
            self.ctx
                .env_eval_cache
                .borrow_mut()
                .entry(type_id)
                .or_insert(crate::context::EnvEvalCacheEntry {
                    result,
                    depth_exceeded: false,
                });
        }

        result
    }

    pub(crate) fn evaluate_application_type_inner(&mut self, type_id: TypeId) -> TypeId {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
        use tsz_solver::instantiate_type_with_depth_status;

        let Some((base, args)) = query::application_info(self.ctx.types, type_id) else {
            return type_id;
        };

        // Check if the base is a Lazy or Enum type
        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base) else {
            return type_id;
        };

        // CRITICAL FIX: Get BOTH the body type AND the type parameters together
        // to ensure the TypeIds in the body match the TypeIds in the substitution.
        // Previously we called type_reference_symbol_type and get_type_params_for_symbol
        // separately, which created DIFFERENT TypeIds for the same type parameters.
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        if body_type == TypeId::ANY || body_type == TypeId::ERROR {
            return type_id;
        }

        if type_params.is_empty() {
            // Instantiation expression: Application(TypeQuery(value), [type_args])
            // For `Err<number>` where `Err: typeof ErrImpl & (<T>() => T)`,
            // substitute callable type parameters in all signatures with the
            // provided type arguments. tsc's getTypeOfInstantiationExpression does this
            // per-signature: each signature with matching type param count gets
            // its params replaced.
            if !args.is_empty() && tsz_solver::type_query_symbol(self.ctx.types, base).is_some() {
                let instantiated = self.instantiate_callable_type_params(body_type, &args);
                if instantiated != body_type {
                    return self.evaluate_type_with_env(instantiated);
                }
            }
            return body_type;
        }

        // Homomorphic identity mapped type passthrough: if the body is
        // `{ [K in keyof T]: T[K] }` (identity mapping) and the argument for T
        // is a genuine primitive type, return the arg directly.
        // This matches tsc behavior where e.g. `Partial<number>` evaluates to `number`.
        // Only applies when the template is `T[K]` — NOT for non-identity templates
        // like `{ [K in keyof T]: Data }` which should produce an index signature.
        //
        // For `any` arguments: passthrough only when the type parameter is constrained
        // to array/tuple types. In tsc, `Arrayish<any>` (T extends unknown[]) produces
        // an array, but `Objectish<any>` (T extends unknown) produces `{ [x: string]: any }`.
        if let Some(mapped_id) = query::mapped_type_id(self.ctx.types, body_type) {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            if let Some(keyof_source) =
                tsz_solver::keyof_inner_type(self.ctx.types, mapped.constraint)
                && let Some(tp) = tsz_solver::type_param_info(self.ctx.types, keyof_source)
                && let Some(idx) = type_params.iter().position(|p| p.name == tp.name)
                && idx < args.len()
                // Verify template is T[K] (identity indexed access)
                && let Some((obj, key)) =
                    tsz_solver::index_access_parts(self.ctx.types, mapped.template)
                && obj == keyof_source
                && tsz_solver::type_param_info(self.ctx.types, key)
                    .is_some_and(|kp| kp.name == mapped.type_param.name)
            {
                let arg = self.evaluate_type_with_env(args[idx]);
                if tsz_solver::is_primitive_type(self.ctx.types, arg) {
                    // For `any`: only passthrough when the type parameter has an
                    // array/tuple constraint. Otherwise, `any` must flow through
                    // mapped type expansion to produce { [x: string]: any }.
                    let should_passthrough = if arg == TypeId::ANY
                        || arg == TypeId::UNKNOWN
                        || arg == TypeId::NEVER
                        || arg == TypeId::ERROR
                    {
                        // Check if the type parameter is constrained to array/tuple
                        tp.constraint.is_some_and(|c| {
                            let evaluated_constraint = self.evaluate_type_for_assignability(c);
                            tsz_solver::is_array_type(self.ctx.types, evaluated_constraint)
                                || tsz_solver::is_tuple_type(self.ctx.types, evaluated_constraint)
                        })
                    } else {
                        true
                    };
                    if should_passthrough {
                        return arg;
                    }
                }
            }
        }

        // HOMOMORPHIC MAPPED TYPE RESOLUTION WITH FULL RESOLVER
        // When the body is a homomorphic mapped type (constraint = keyof T), we must
        // avoid the normal instantiate_type path because it eagerly evaluates the
        // mapped type with NoopResolver. NoopResolver can't resolve Application types
        // like `Func<T[K]>` (Lazy(DefId) references), causing property values to
        // contain unresolved Application types. This affects both tuple/array
        // preservation and recursive mapped types like `Spec<T>`.
        // Instead, we instantiate only the template and construct a new MappedType
        // with `keyof <resolved_arg>` as constraint, then let evaluate_type_with_env
        // (which has the full resolver) evaluate it — correctly resolving all types.
        if let Some(mapped_id) = query::mapped_type_id(self.ctx.types, body_type) {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            if let Some(keyof_source) =
                tsz_solver::keyof_inner_type(self.ctx.types, mapped.constraint)
            {
                // Evaluate all args first so instantiated `keyof` sources like
                // `Gen<T>` become `Gen<ABC.A>` before we rebuild the mapped type.
                let evaluated_args: Vec<TypeId> = args
                    .iter()
                    .map(|&arg| self.evaluate_type_with_env(arg))
                    .collect();
                let mut subst =
                    TypeSubstitution::from_args(self.ctx.types, &type_params, &evaluated_args);
                // Keep the mapped iteration variable intact while substituting outer params.
                let k_unconstrained = self.ctx.types.type_param(tsz_solver::TypeParamInfo {
                    name: mapped.type_param.name,
                    constraint: None,
                    default: None,
                    is_const: false,
                });
                subst.insert(mapped.type_param.name, k_unconstrained);

                let instantiated_source = instantiate_type(self.ctx.types, keyof_source, &subst);
                let evaluated_source = if self.contains_type_parameters_cached(instantiated_source)
                {
                    instantiated_source
                } else {
                    self.evaluate_type_with_resolution(instantiated_source)
                };

                let inst_template = instantiate_type(self.ctx.types, mapped.template, &subst);
                let inst_name_type = mapped
                    .name_type
                    .map(|nt| instantiate_type(self.ctx.types, nt, &subst));
                let inst_mapped = tsz_solver::MappedType {
                    type_param: mapped.type_param,
                    constraint: self.ctx.types.keyof(evaluated_source),
                    name_type: inst_name_type,
                    template: inst_template,
                    readonly_modifier: mapped.readonly_modifier,
                    optional_modifier: mapped.optional_modifier,
                };
                let mapped_type_id = self.ctx.types.mapped(inst_mapped);
                if query::is_union_or_intersection(self.ctx.types, evaluated_source)
                    && self.contains_type_parameters_cached(evaluated_source)
                {
                    return mapped_type_id;
                }
                return self.evaluate_mapped_type_with_resolution(mapped_type_id);
            }
        }

        // Resolve type arguments so distributive conditionals can see unions.
        // For conditional type bodies whose extends side contains infer patterns,
        // preserve generic/type-parameter arguments so the conditional evaluator
        // can still use their original constraints during infer matching.
        // Also preserve Application-form arguments: when a type arg like
        // `Synthetic<number, number>` is eagerly evaluated to its structural form
        // (e.g., an empty object `{}`), the conditional evaluator loses the ability
        // to do Application-level infer matching (e.g., matching
        // `Synthetic<number, number>` against `Synthetic<number, infer V>`).
        // Preserving the Application form lets `try_application_infer_match` extract
        // infer bindings by comparing type arguments positionally.
        let body_has_conditional_infer = self.body_is_conditional_with_infer(body_type);
        // When the conditional's extends_type is an Application with infer
        // (e.g., `U extends Synthetic<T, infer V>`), also preserve Application-type
        // args. Evaluating Application args to structural Objects would destroy
        // the Application structure needed by `try_application_infer_match`.
        let body_has_conditional_app_infer =
            self.body_is_conditional_with_application_infer(body_type);
        let evaluated_args: Vec<TypeId> = args
            .iter()
            .map(|&arg| {
                if body_has_conditional_infer
                    && (self.contains_type_parameters_cached(arg)
                        || query::application_info(self.ctx.types, arg).is_some())
                {
                    arg
                } else if body_has_conditional_app_infer
                    && query::application_info(self.ctx.types, arg).is_some()
                {
                    // Preserve Application args so the conditional evaluator can
                    // match at the Application level for infer pattern matching.
                    arg
                } else {
                    self.evaluate_type_with_env(arg)
                }
            })
            .collect();

        // Create substitution and instantiate
        let substitution =
            TypeSubstitution::from_args(self.ctx.types, &type_params, &evaluated_args);
        let (mut instantiated, depth_exceeded) =
            instantiate_type_with_depth_status(self.ctx.types, body_type, &substitution);
        if depth_exceeded {
            self.ctx.depth_exceeded.set(true);
        }
        if tsz_solver::contains_this_type(self.ctx.types, instantiated) {
            instantiated = tsz_solver::substitute_this_type(self.ctx.types, instantiated, type_id);
        }
        // Recursively evaluate in case the result contains more applications
        let evaluated_result = self.evaluate_application_type(instantiated);
        let result = self.prune_impossible_object_union_members_with_env(evaluated_result);

        // If the result is a Mapped type, try to evaluate it with symbol resolution
        let result = self.evaluate_mapped_type_with_resolution(result);

        // Preserve instantiated discriminated object intersections in their deferred
        // intersection form. Eager env evaluation collapses these into distributed
        // unions, which loses both discriminant-aware `keyof` and fresh EPC behavior.
        if tsz_solver::type_queries::is_discriminated_object_intersection(self.ctx.types, result) {
            return result;
        }

        // Evaluate meta-types (conditional, index access, keyof) with symbol resolution

        self.evaluate_type_with_env(result)
    }

    /// Instantiate callable type parameters for instantiation expressions.
    ///
    /// For `Err<number>` where `Err: { new<E>(): ErrImpl<E>; } & (<T>() => T)`,
    /// this creates new signatures with type parameters replaced:
    /// - `new<E>(): ErrImpl<E>` with 1 type param, 1 arg → `new(): ErrImpl<number>`
    /// - `<T>() => T` with 1 type param, 1 arg → `() => number`
    fn instantiate_callable_type_params(
        &mut self,
        body_type: TypeId,
        type_args: &[TypeId],
    ) -> TypeId {
        if let Some(shape_id) = tsz_solver::callable_shape_id(self.ctx.types, body_type) {
            let shape = self.ctx.types.callable_shape(shape_id);
            let new_call_sigs = self.instantiate_signatures(&shape.call_signatures, type_args);
            let new_construct_sigs =
                self.instantiate_signatures(&shape.construct_signatures, type_args);
            if new_call_sigs.is_none() && new_construct_sigs.is_none() {
                return body_type;
            }
            let new_shape = CallableShape {
                call_signatures: new_call_sigs.unwrap_or_else(|| shape.call_signatures.clone()),
                construct_signatures: new_construct_sigs
                    .unwrap_or_else(|| shape.construct_signatures.clone()),
                properties: shape.properties.clone(),
                string_index: shape.string_index.clone(),
                number_index: shape.number_index.clone(),
                symbol: shape.symbol,
                is_abstract: shape.is_abstract,
            };
            self.ctx.types.callable(new_shape)
        } else if let Some(members) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, body_type)
        {
            let mut changed = false;
            let new_members: Vec<TypeId> = members
                .iter()
                .map(|&m| {
                    let inst = self.instantiate_callable_type_params(m, type_args);
                    if inst != m {
                        changed = true;
                    }
                    inst
                })
                .collect();
            if changed {
                self.ctx.types.intersection(new_members)
            } else {
                body_type
            }
        } else {
            body_type
        }
    }

    /// Instantiate call/construct signatures by replacing type parameters with args.
    /// Returns `None` if no signatures were modified.
    fn instantiate_signatures(
        &mut self,
        signatures: &[CallSignature],
        type_args: &[TypeId],
    ) -> Option<Vec<CallSignature>> {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};

        let mut changed = false;
        let new_sigs: Vec<CallSignature> = signatures
            .iter()
            .map(|sig| {
                if sig.type_params.len() == type_args.len() && !sig.type_params.is_empty() {
                    changed = true;
                    let subst =
                        TypeSubstitution::from_args(self.ctx.types, &sig.type_params, type_args);
                    let new_params: Vec<ParamInfo> = sig
                        .params
                        .iter()
                        .map(|p| ParamInfo {
                            name: p.name,
                            type_id: instantiate_type(self.ctx.types, p.type_id, &subst),
                            optional: p.optional,
                            rest: p.rest,
                        })
                        .collect();
                    let new_return = instantiate_type(self.ctx.types, sig.return_type, &subst);
                    CallSignature {
                        type_params: vec![], // Remove type params after substitution
                        params: new_params,
                        this_type: sig
                            .this_type
                            .map(|t| instantiate_type(self.ctx.types, t, &subst)),
                        return_type: new_return,
                        type_predicate: sig.type_predicate.clone(),
                        is_method: sig.is_method,
                    }
                } else {
                    sig.clone()
                }
            })
            .collect();

        if changed { Some(new_sigs) } else { None }
    }

    /// Check if a type body is a Conditional type whose `extends_type` contains infer patterns.
    fn body_is_conditional_with_infer(&self, body_type: TypeId) -> bool {
        let Some(cond) = query::get_conditional_type(self.ctx.types, body_type) else {
            return false;
        };
        tsz_solver::contains_infer_types(self.ctx.types, cond.extends_type)
    }

    /// Check if a type body is a Conditional type whose `extends_type` is an Application
    /// containing infer patterns (e.g., `U extends Synthetic<T, infer V> ? V : never`).
    /// When true, Application-type arguments must be preserved during arg expansion
    /// so the solver's `try_application_infer_match` can match at the Application level.
    fn body_is_conditional_with_application_infer(&self, body_type: TypeId) -> bool {
        let Some(cond) = query::get_conditional_type(self.ctx.types, body_type) else {
            return false;
        };
        query::application_info(self.ctx.types, cond.extends_type).is_some()
            && tsz_solver::contains_infer_types(self.ctx.types, cond.extends_type)
    }

    /// Evaluate a mapped type with symbol resolution.
    /// This handles cases like `{ [K in keyof Ref(sym)]: Template }` where the Ref
    /// needs to be resolved to get concrete keys.
    pub(crate) fn evaluate_mapped_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        // NOTE: Manual lookup preferred here - we need the mapped_id directly
        // to call mapped_type(mapped_id) below. Using get_mapped_type would
        // return the full Arc<MappedType>, which is more than needed.
        let Some(mapped_id) = query::mapped_type_id(self.ctx.types, type_id) else {
            return type_id;
        };

        if let Some(&cached) = self
            .ctx
            .narrowing_cache
            .resolve_cache
            .borrow()
            .get(&type_id)
        {
            return cached;
        }

        // Memoize mapped-type expansion for monomorphic inputs.
        // This is a hot path for repeated property access on mapped aliases
        // (e.g., DeepPartial<...>).
        let can_cache = !self.contains_type_parameters_cached(type_id);

        if !self.ctx.mapped_eval_set.insert(type_id) {
            return type_id;
        }

        if self.ctx.instantiation_depth.get() >= MAX_INSTANTIATION_DEPTH {
            self.ctx.depth_exceeded.set(true);
            self.ctx.mapped_eval_set.remove(&type_id);
            return type_id;
        }
        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() + 1);

        let result = self.evaluate_mapped_type_with_resolution_inner(type_id, mapped_id);

        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() - 1);
        self.ctx.mapped_eval_set.remove(&type_id);
        if can_cache {
            self.ctx
                .narrowing_cache
                .resolve_cache
                .borrow_mut()
                .insert(type_id, result);
        }
        result
    }

    pub(crate) fn evaluate_mapped_type_with_resolution_inner(
        &mut self,
        type_id: TypeId,
        mapped_id: MappedTypeId,
    ) -> TypeId {
        use tsz_solver::PropertyInfo;
        let factory = self.ctx.types.factory();

        let mapped = self.ctx.types.mapped_type(mapped_id);

        // Evaluate the constraint to get concrete keys
        let keys = self.evaluate_mapped_constraint_with_resolution(mapped.constraint);

        // Preserve the solver's tuple/array mapped-type semantics for homomorphic
        // cases like `{ [K in keyof T]: T[K] }` over tuple/array sources. The
        // checker-local object expansion below is correct for object members, but
        // it loses tuple/array identity and causes rest parameters to stop being
        // recognized as array-like.
        if self.mapped_constraint_source_needs_array_like_preservation(mapped.constraint) {
            let evaluated = self.evaluate_type_with_env(type_id);
            if evaluated != type_id {
                return evaluated;
            }
        }

        let resolved_mapped_id = if keys != mapped.constraint {
            let resolved_mapped = tsz_solver::MappedType {
                type_param: mapped.type_param,
                constraint: keys,
                name_type: mapped.name_type,
                template: mapped.template,
                readonly_modifier: mapped.readonly_modifier,
                optional_modifier: mapped.optional_modifier,
            };
            tsz_solver::mapped_type_id(self.ctx.types, self.ctx.types.mapped(resolved_mapped))
                .unwrap_or(mapped_id)
        } else {
            mapped_id
        };

        // Prefer the shared finite-key collector once the constraint has been
        // resolved. This keeps mapped expansion aligned with property access and
        // exact `keyof` key-space semantics.
        let string_keys: Vec<_> = if let Some(names) =
            tsz_solver::type_queries::collect_finite_mapped_property_names(
                self.ctx.types,
                resolved_mapped_id,
            ) {
            names.into_iter().collect()
        } else {
            tsz_solver::type_queries::extract_string_literal_keys(self.ctx.types, keys)
        };
        if string_keys.is_empty() {
            // Can't evaluate - return original
            return type_id;
        }

        // For homomorphic mapped types with `-?`, collect source properties so we
        // can use their raw type_id (without implicit undefined from optionality).
        // This distinguishes `{ a?: string }` (raw type = string) from
        // `{ a?: string | undefined }` (raw type = string | undefined).
        let is_remove_optional =
            mapped.optional_modifier == Some(tsz_solver::MappedModifier::Remove);
        let source_prop_map: rustc_hash::FxHashMap<tsz_common::Atom, (bool, TypeId)> =
            if is_remove_optional {
                if let Some(source) =
                    tsz_solver::keyof_inner_type(self.ctx.types, mapped.constraint)
                {
                    crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source)
                        .map(|shape| {
                            shape
                                .properties
                                .iter()
                                .map(|p| (p.name, (p.optional, p.type_id)))
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    Default::default()
                }
            } else {
                Default::default()
            };

        // Build the resulting object properties
        let mut properties = Vec::new();
        for key_name in string_keys {
            let mut property_type =
                self.instantiate_mapped_property_template_with_env(&mapped, key_name);

            // When `-?` removes optionality from a homomorphic mapped type, use the
            // source property's raw type_id instead of the template-evaluated type.
            // The template evaluation (T[K]) adds `| undefined` for optional properties,
            // but the raw type_id preserves the distinction between implicit undefined
            // (from `a?: string` → raw type `string`) and explicit undefined
            // (from `a?: string | undefined` → raw type `string | undefined`).
            // This matches tsc's `removeMissingType` which only strips the implicit part.
            if is_remove_optional
                && let Some(&(source_optional, raw_type)) = source_prop_map.get(&key_name)
                && source_optional
            {
                property_type = raw_type;
            }

            let optional = matches!(
                mapped.optional_modifier,
                Some(tsz_solver::MappedModifier::Add)
            );
            let readonly = matches!(
                mapped.readonly_modifier,
                Some(tsz_solver::MappedModifier::Add)
            );

            properties.push(PropertyInfo {
                name: key_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            });
        }

        factory.object(properties)
    }

    pub(crate) fn instantiate_mapped_property_template_with_env(
        &mut self,
        mapped: &tsz_solver::MappedType,
        key_name: Atom,
    ) -> TypeId {
        let key_literal = self.ctx.types.literal_string_atom(key_name);
        let property_type =
            crate::query_boundaries::state::checking::instantiate_mapped_template_for_property(
                self.ctx.types,
                mapped.template,
                mapped.type_param.name,
                key_literal,
            );

        if let Some((obj, _idx)) = query::index_access_types(self.ctx.types, property_type) {
            let obj_type = if let Some(def_id) = query::lazy_def_id(self.ctx.types, obj) {
                self.ctx
                    .def_to_symbol_id(def_id)
                    .map(|sym_id| self.get_type_of_symbol(sym_id))
                    .unwrap_or(obj)
            } else {
                obj
            };

            let prop_name_arc = self.ctx.types.resolve_atom_ref(key_name);
            let prop_name: &str = &prop_name_arc;
            match self.resolve_property_access_with_env(obj_type, prop_name) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                }
                | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => return type_id,
                _ => {}
            }
        }

        self.evaluate_type_with_env(property_type)
    }

    fn mapped_constraint_source_needs_array_like_preservation(
        &mut self,
        constraint: TypeId,
    ) -> bool {
        let query::MappedConstraintKind::KeyOf(source) =
            query::classify_mapped_constraint(self.ctx.types, constraint)
        else {
            return false;
        };

        let source = self.evaluate_type_with_resolution(source);
        self.is_array_like_mapped_source(source)
    }

    fn is_array_like_mapped_source(&mut self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id).is_some()
            || crate::query_boundaries::common::array_element_type(self.ctx.types, type_id)
                .is_some()
        {
            return true;
        }

        let Some(constraint) = crate::query_boundaries::state::checking::type_parameter_constraint(
            self.ctx.types,
            type_id,
        ) else {
            return false;
        };

        let constraint = self.evaluate_type_with_resolution(constraint);
        crate::query_boundaries::common::tuple_elements(self.ctx.types, constraint).is_some()
            || crate::query_boundaries::common::array_element_type(self.ctx.types, constraint)
                .is_some()
    }

    /// Evaluate a mapped type constraint with symbol resolution.
    /// Handles keyof Ref(sym) by resolving the Ref and getting its keys.
    pub(crate) fn evaluate_mapped_constraint_with_resolution(
        &mut self,
        constraint: TypeId,
    ) -> TypeId {
        match query::classify_mapped_constraint(self.ctx.types, constraint) {
            query::MappedConstraintKind::KeyOf(operand) => {
                // Evaluate the operand with symbol resolution
                let evaluated = self.evaluate_type_with_resolution(operand);
                self.get_keyof_type(evaluated)
            }
            query::MappedConstraintKind::Resolved => constraint,
            query::MappedConstraintKind::Other => {
                // Resolve Lazy(DefId) and other unresolved constraint types.
                // For example, `type Keys = "a" | "b"; { [P in Keys]: T }` has a
                // Lazy(DefId) constraint that must be resolved to get `"a" | "b"`.
                let resolved = self.evaluate_type_with_resolution(constraint);
                if resolved != constraint {
                    resolved
                } else {
                    constraint
                }
            }
        }
    }

    // Lazy type resolution, property access type resolution, and type environment
    // population methods are in `lazy.rs`.

    /// Create a `TypeEnvironment` populated with resolved symbol types.
    ///
    /// This can be passed to `is_assignable_to_with_env` for type checking
    /// that needs to resolve type references.
    pub fn build_type_environment(&mut self) -> tsz_solver::TypeEnvironment {
        use tsz_binder::symbol_flags;

        // Reset the global instantiation fuel for this file. Each file gets a
        // fresh budget for Application type evaluations. Without this, a complex
        // file (react16.d.ts) would exhaust the fuel and starve subsequent files.
        GLOBAL_INSTANTIATION_FUEL.set(0);

        // Collect unique symbols from user code only (node_symbols).
        // Lib symbols from file_locals are NOT included here — they are resolved
        // lazily on demand during statement checking. This avoids the O(N) upfront
        // cost of eagerly resolving ~2000 lib symbols, saving ~30-50ms per file.
        let mut symbols_with_flags: Vec<(SymbolId, u32)> =
            Vec::with_capacity(self.ctx.binder.node_symbols.len());
        let mut seen: FxHashSet<SymbolId> = FxHashSet::default();
        for &sym_id in self.ctx.binder.node_symbols.values() {
            if seen.insert(sym_id) {
                let flags = self.ctx.binder.get_symbol(sym_id).map_or(0, |s| s.flags);
                symbols_with_flags.push((sym_id, flags));
            }
        }

        // Sort symbols so type-defining symbols (functions, classes, interfaces, type aliases)
        // are processed BEFORE variable/parameter symbols.
        symbols_with_flags.sort_by_key(|&(sym_id, flags)| {
            let is_type_defining = flags
                & (symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::INTERFACE
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::ENUM
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE)
                != 0;
            (u8::from(!is_type_defining), sym_id.0)
        });

        // Resolve each symbol and add to the environment.
        // Skip variable/parameter symbols — their types are computed lazily during
        // statement checking when proper enclosing_class context is available.
        for (sym_id, flags) in symbols_with_flags {
            // Skip variable and parameter symbols - their types will be computed
            // lazily during statement checking with proper class context
            if flags
                & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
                != 0
                && flags
                    & (symbol_flags::CLASS
                        | symbol_flags::FUNCTION
                        | symbol_flags::INTERFACE
                        | symbol_flags::TYPE_ALIAS
                        | symbol_flags::ENUM
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE)
                    == 0
            {
                continue;
            }

            // Get the type for this symbol
            // IMPORTANT: get_type_of_symbol internally calls compute_type_of_symbol which
            // returns both the type AND the correct type_params, then inserts them into
            // ctx.type_env. We MUST NOT separately call get_type_params_for_symbol because
            // that creates fresh type parameter IDs that won't match those used in the type body.
            // This was causing generic type instantiation to fail (e.g., Promise<string>.then()).
            let _type_id = self.get_type_of_symbol(sym_id);
        }

        // Return a clone of ctx.type_env which was correctly populated by get_type_of_symbol
        // with matching type parameter IDs
        self.ctx.type_env.borrow().clone()
    }

    /// Get type parameters for a symbol (generic types).
    ///
    /// Extracts type parameter information for generic types (classes, interfaces,
    /// type aliases). Used for populating the type environment and for generic
    /// type instantiation.
    ///
    /// ## Symbol Types Handled:
    /// - **Type Alias**: Extracts type parameters from type alias declaration
    /// - **Interface**: Extracts type parameters from interface declaration
    /// - **Class**: Extracts type parameters from class declaration
    /// - **Other**: Returns empty vector (no type parameters)
    ///
    /// ## Cross-Arena Resolution:
    /// - Handles symbols defined in other arenas (e.g., imported symbols)
    /// - Creates a temporary `CheckerState` for the other arena
    /// - Delegates type parameter extraction to the temporary checker
    ///
    /// ## Type Parameter Information:
    /// - Returns Vec<TypeParamInfo> with parameter names and constraints
    /// - Includes default type arguments if present
    /// - Used by `TypeEnvironment` for generic type expansion
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Type alias with type parameters
    /// type Pair<T, U> = [T, U];
    /// // get_type_params_for_symbol(Pair) → [T, U]
    ///
    /// // Interface with type parameters
    /// interface Box<T> {
    ///   value: T;
    /// }
    /// // get_type_params_for_symbol(Box) → [T]
    ///
    /// // Class with type parameters
    /// class Container<T> {
    ///   constructor(public item: T) {}
    /// }
    /// // get_type_params_for_symbol(Container) → [T]
    ///
    /// // Type parameters with constraints
    /// interface SortedMap<K extends Comparable, V> {}
    /// // get_type_params_for_symbol(SortedMap) → [K: Comparable, V]
    /// ```
    fn extract_type_params_from_decl(
        checker: &mut CheckerState,
        flags: u32,
        decl_idx: NodeIndex,
        sym_escaped_name: &str,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        if let Some(node) = checker.ctx.arena.get(decl_idx) {
            if flags & symbol_flags::TYPE_ALIAS != 0
                && let Some(type_alias) = checker.ctx.arena.get_type_alias(node)
            {
                let (params, updates) = checker.push_type_parameters(&type_alias.type_parameters);
                checker.pop_type_parameters(updates);
                return Some(params);
            }
            if flags & symbol_flags::CLASS != 0
                && let Some(class) = checker.ctx.arena.get_class(node)
            {
                let (params, updates) = checker.push_type_parameters(&class.type_parameters);
                checker.pop_type_parameters(updates);
                if !params.is_empty() {
                    return Some(params);
                }

                if let Some(class_jsdoc_params) =
                    Self::jsdoc_template_type_params_for_decl(checker, decl_idx, sym_escaped_name)
                {
                    return Some(class_jsdoc_params);
                }

                return Some(Vec::new());
            }
            if flags & symbol_flags::INTERFACE != 0
                && let Some(iface) = checker.ctx.arena.get_interface(node)
            {
                if let Some(name_node) = checker.ctx.arena.get(iface.name)
                    && let Some(name_ident) = checker.ctx.arena.get_identifier(name_node)
                {
                    if name_ident.escaped_text.as_str() != sym_escaped_name {
                        return None;
                    }
                } else {
                    // Accept if name cannot be resolved for backward compatibility
                }
                let (params, updates) = checker.push_type_parameters(&iface.type_parameters);
                checker.pop_type_parameters(updates);
                return Some(params);
            }
        }
        None
    }

    fn jsdoc_template_type_params_for_decl(
        checker: &mut CheckerState,
        decl_idx: NodeIndex,
        _sym_escaped_name: &str,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let sf = checker.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let jsdoc = checker.try_leading_jsdoc(
            comments,
            checker.ctx.arena.get(decl_idx)?.pos,
            source_text,
        )?;

        let names = Self::jsdoc_template_type_params(&jsdoc);
        if names.is_empty() {
            return None;
        }

        let mut params = Vec::with_capacity(names.len());
        for name in names {
            if name.is_empty() {
                continue;
            }
            params.push(tsz_solver::TypeParamInfo {
                name: checker.ctx.types.intern_string(&name),
                constraint: None,
                default: None,
                is_const: false,
            });
        }

        if params.is_empty() {
            None
        } else {
            Some(params)
        }
    }

    pub(crate) fn get_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        // Recursion depth check: prevent stack overflow from circular generic defaults
        // (e.g. type A<T = B> = T; type B<T = A> = T;)
        if !self.ctx.enter_recursion() {
            return Vec::new();
        }

        let mut sym_id = sym_id;
        if let Some(symbol) = self.get_symbol_globally(sym_id)
            && symbol.flags & symbol_flags::ALIAS != 0
        {
            let mut visited_aliases = Vec::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                sym_id = target;
            }
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if let Some(cached) = self.ctx.get_def_type_params(def_id) {
            self.ctx.leave_recursion();
            return cached;
        }
        if self.ctx.def_no_type_params.borrow().contains(&def_id) {
            self.ctx.leave_recursion();
            return Vec::new();
        }

        // Use get_symbol_globally to find symbols in lib files and other files.
        // Extract needed data to avoid holding a borrow during deeper operations.
        let (flags, value_decl, declarations, sym_escaped_name) =
            match self.get_symbol_globally(sym_id) {
                Some(symbol) => (
                    symbol.flags,
                    symbol.value_declaration,
                    symbol.declarations.clone(),
                    symbol.escaped_name.clone(),
                ),
                None => {
                    self.ctx.leave_recursion();
                    return Vec::new();
                }
            };

        // Fast path: only class/interface/type alias symbols can declare type parameters.
        if flags & (symbol_flags::TYPE_ALIAS | symbol_flags::CLASS | symbol_flags::INTERFACE) == 0 {
            self.ctx.def_no_type_params.borrow_mut().insert(def_id);
            self.ctx.leave_recursion();
            return Vec::new();
        }
        let mut decl_candidates = Vec::new();
        if value_decl != tsz_parser::parser::NodeIndex::NONE {
            decl_candidates.push(value_decl);
        }
        for &decl in &declarations {
            if decl != value_decl {
                decl_candidates.push(decl);
            }
        }

        let mut merged_params: Option<Vec<tsz_solver::TypeParamInfo>> = None;
        let mut fallback_params = None;

        for decl_idx in decl_candidates {
            let mut checked_local = false;

            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                        checked_local = true;
                        if let Some(params) = Self::extract_type_params_from_decl(
                            self,
                            flags,
                            decl_idx,
                            &sym_escaped_name,
                        ) {
                            if !params.is_empty() {
                                if let Some(ref mut merged) = merged_params {
                                    for (i, p) in params.into_iter().enumerate() {
                                        if i < merged.len()
                                            && merged[i].default.is_none()
                                            && p.default.is_some()
                                        {
                                            merged[i].default = p.default;
                                        }
                                        if i < merged.len()
                                            && merged[i].constraint.is_none()
                                            && p.constraint.is_some()
                                        {
                                            merged[i].constraint = p.constraint;
                                        }
                                    }
                                } else {
                                    merged_params = Some(params);
                                }
                            } else if fallback_params.is_none() {
                                fallback_params = Some(params);
                            }
                        }
                    } else {
                        if !Self::enter_cross_arena_delegation() {
                            continue;
                        }
                        let mut checker = Box::new(CheckerState::with_parent_cache(
                            arena.as_ref(),
                            self.ctx.binder,
                            self.ctx.types,
                            self.ctx.file_name.clone(),
                            self.ctx.compiler_options.clone(),
                            self,
                        ));
                        if let Some(params) = Self::extract_type_params_from_decl(
                            &mut checker,
                            flags,
                            decl_idx,
                            &sym_escaped_name,
                        ) {
                            if !params.is_empty() {
                                if let Some(ref mut merged) = merged_params {
                                    for (i, p) in params.into_iter().enumerate() {
                                        if i < merged.len()
                                            && merged[i].default.is_none()
                                            && p.default.is_some()
                                        {
                                            merged[i].default = p.default;
                                        }
                                        if i < merged.len()
                                            && merged[i].constraint.is_none()
                                            && p.constraint.is_some()
                                        {
                                            merged[i].constraint = p.constraint;
                                        }
                                    }
                                } else {
                                    merged_params = Some(params);
                                }
                            } else if fallback_params.is_none() {
                                fallback_params = Some(params);
                            }
                        }
                        Self::leave_cross_arena_delegation();
                    }
                }
            }

            if !checked_local
                && let Some(params) =
                    Self::extract_type_params_from_decl(self, flags, decl_idx, &sym_escaped_name)
            {
                if !params.is_empty() {
                    if let Some(ref mut merged) = merged_params {
                        for (i, p) in params.into_iter().enumerate() {
                            if i < merged.len()
                                && merged[i].default.is_none()
                                && p.default.is_some()
                            {
                                merged[i].default = p.default;
                            }
                            if i < merged.len()
                                && merged[i].constraint.is_none()
                                && p.constraint.is_some()
                            {
                                merged[i].constraint = p.constraint;
                            }
                        }
                    } else {
                        merged_params = Some(params);
                    }
                } else if fallback_params.is_none() {
                    fallback_params = Some(params);
                }
            }
        }

        if let Some(params) = merged_params {
            self.ctx.insert_def_type_params(def_id, params.clone());
            self.ctx.def_no_type_params.borrow_mut().remove(&def_id);
            self.ctx.leave_recursion();
            return params;
        }

        if let Some(params) = fallback_params {
            self.ctx.def_no_type_params.borrow_mut().insert(def_id);
            self.ctx.leave_recursion();
            return params;
        }

        self.ctx.def_no_type_params.borrow_mut().insert(def_id);
        self.ctx.leave_recursion();
        Vec::new()
    }

    /// Count the number of required type parameters for a symbol.
    ///
    /// A type parameter is "required" if it doesn't have a default value.
    /// This is important for validating generic type usage and error messages.
    ///
    /// ## Required vs Optional:
    /// - **Required**: Must be explicitly provided by the caller
    /// - **Optional**: Has a default value, can be omitted
    ///
    /// ## Use Cases:
    /// - Validating that enough type arguments are provided
    /// - Error messages: "Expected X type arguments but got Y"
    /// - Generic function/method overload resolution
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // All required
    /// interface Pair<T, U> {}
    /// // count_required_type_params(Pair) → 2
    /// const x: Pair = {};  // ❌ Error: Expected 2 type arguments
    /// const y: Pair<string, number> = {};  // ✅
    ///
    /// // One optional
    /// interface Box<T = string> {}
    /// // count_required_type_params(Box) → 0 (T has default)
    /// const a: Box = {};  // ✅ T defaults to string
    /// const b: Box<number> = {};  // ✅ Explicit number
    ///
    /// // Mixed required and optional
    /// interface Map<K, V = any> {}
    /// // count_required_type_params(Map) → 1 (K required, V optional)
    /// const m1: Map<string> = {};  // ✅ K=string, V=any
    /// const m2: Map<string, number> = {};  // ✅ Both specified
    /// const m3: Map = {};  // ❌ K is required
    /// ```
    pub(crate) fn count_required_type_params(&mut self, sym_id: SymbolId) -> usize {
        // First try the fast AST-level check. This avoids recursive resolution
        // issues when a type parameter default references the type being declared
        // (e.g., `interface SelfRef<T = SelfRef> {}`). In such cases,
        // `get_type_params_for_symbol` would recursively try to resolve the
        // default, fail, and incorrectly report the param as required.
        if let Some(ast_count) = self.count_required_type_params_from_ast(sym_id) {
            return ast_count;
        }
        let type_params = self.get_type_params_for_symbol(sym_id);
        type_params.iter().filter(|p| p.default.is_none()).count()
    }

    /// Count required type params by inspecting the AST directly, without resolving
    /// defaults. Returns `Some(count)` if AST-level info is available, `None` otherwise.
    pub(crate) fn count_required_type_params_from_ast(&self, sym_id: SymbolId) -> Option<usize> {
        let symbol = self.get_symbol_globally(sym_id)?;
        let flags = symbol.flags;
        let decl_candidates: Vec<_> =
            if symbol.value_declaration != tsz_parser::parser::NodeIndex::NONE {
                std::iter::once(symbol.value_declaration)
                    .chain(symbol.declarations.iter().copied())
                    .collect()
            } else {
                symbol.declarations.clone()
            };

        for decl_idx in decl_candidates {
            let node = self.ctx.arena.get(decl_idx)?;
            let type_params_list = if flags & tsz_binder::symbol_flags::INTERFACE != 0 {
                self.ctx
                    .arena
                    .get_interface(node)
                    .and_then(|iface| iface.type_parameters.as_ref())
            } else if flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0 {
                self.ctx
                    .arena
                    .get_type_alias(node)
                    .and_then(|ta| ta.type_parameters.as_ref())
            } else if flags & tsz_binder::symbol_flags::CLASS != 0 {
                self.ctx
                    .arena
                    .get_class(node)
                    .and_then(|c| c.type_parameters.as_ref())
            } else {
                None
            };

            if let Some(list) = type_params_list {
                let required = list
                    .nodes
                    .iter()
                    .filter(|&&param_idx| {
                        self.ctx
                            .arena
                            .get(param_idx)
                            .and_then(|n| self.ctx.arena.get_type_parameter(n))
                            .is_some_and(|tp| tp.default == tsz_parser::parser::NodeIndex::NONE)
                    })
                    .count();
                return Some(required);
            }
        }
        None
    }

    /// Create a union type from multiple types.
    ///
    /// Handles empty (→ NEVER), single (→ that type), and multi-member cases.
    /// Automatically normalizes: flattens nested unions, deduplicates, sorts.
    pub fn get_union_type(&self, types: Vec<TypeId>) -> TypeId {
        tsz_solver::utils::union_or_single(self.ctx.types, types)
    }

    // =========================================================================
    // Type Node Resolution
    // =========================================================================

    /// Get type from a type node.
    ///
    /// Uses compile-time constant `TypeIds` for intrinsic types (O(1) lookup).
    /// Get the type representation of a type annotation node.
    ///
    /// This is the main entry point for converting type annotation AST nodes into
    /// `TypeId` representations. Handles all TypeScript type syntax.
    ///
    /// ## Special Node Handling:
    /// - **`TypeReference`**: Validates existence before lowering (catches missing types)
    /// - **`TypeQuery`** (`typeof X`): Resolves via binder for proper symbol resolution
    /// - **`UnionType`**: Handles specially for nested typeof expression resolution
    /// - **`TypeLiteral`**: Uses checker resolution for type parameter support
    /// - **Other nodes**: Delegated to `TypeLowering`
    ///
    /// ## Type Parameter Bindings:
    /// - Uses current type parameter bindings from scope
    /// - Allows type parameters to resolve correctly in generic contexts
    ///
    /// ## Symbol Resolvers:
    /// - Provides type/value symbol resolvers to `TypeLowering`
    /// - Resolves type references and value references (for typeof)
    ///
    /// ## Error Reporting:
    /// - Checks for missing names before lowering
    /// - Emits appropriate errors for undefined types
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Primitive types
    /// let x: string;           // → STRING
    /// let y: number | boolean; // → Union(NUMBER, BOOLEAN)
    ///
    /// // Type references
    /// interface Foo {}
    /// let z: Foo;              // → Ref to Foo symbol
    ///
    /// // Generic types
    /// let a: Array<string>;    // → Application(Array, [STRING])
    ///
    /// // Type queries
    /// let value = 42;
    /// let b: typeof value;     // → TypeQuery(value symbol)
    ///
    /// // Type literals
    /// let c: { x: number };    // → Object type with property x: number
    /// ```
    pub fn get_type_from_type_node(&mut self, idx: NodeIndex) -> TypeId {
        // Delegate to TypeNodeChecker for type node handling.
        // TypeNodeChecker handles caching, type parameter scope, and recursion protection.
        //
        // Note: For types that need binder symbol resolution (TYPE_REFERENCE, TYPE_QUERY,
        // UNION_TYPE containing typeof, TYPE_LITERAL), we still use CheckerState's
        // specialized methods to ensure proper symbol resolution.
        //
        // See: docs/TS2304_SMART_CACHING_FIX.md

        // First check if this is a type that needs special handling with binder resolution
        if let Some(node) = self.ctx.arena.get(idx) {
            // TS1228: "A type predicate is only allowed in return type position for
            // functions and methods." The parser restricts predicate parsing to return
            // type positions, but some return types (constructors, getters, setters,
            // construct signatures, constructor types) still parse predicates for error
            // recovery. The checker must flag these, matching tsc's getTypePredicateParent.
            if node.kind == syntax_kind_ext::TYPE_PREDICATE {
                let is_valid = self
                    .ctx
                    .arena
                    .get_extended(idx)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .is_some_and(|parent| {
                        matches!(
                            parent.kind,
                            syntax_kind_ext::FUNCTION_DECLARATION
                                | syntax_kind_ext::FUNCTION_EXPRESSION
                                | syntax_kind_ext::METHOD_DECLARATION
                                | syntax_kind_ext::METHOD_SIGNATURE
                                | syntax_kind_ext::CALL_SIGNATURE
                                | syntax_kind_ext::ARROW_FUNCTION
                                | syntax_kind_ext::FUNCTION_TYPE
                        )
                    });
                if !is_valid {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        idx,
                        diagnostic_messages::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO,
                        diagnostic_codes::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO,
                    );
                }
            }

            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                let should_refresh_cached_defaulted_reference =
                    self.ctx.arena.get_type_ref(node).is_some_and(|type_ref| {
                        let has_type_args = type_ref
                            .type_arguments
                            .as_ref()
                            .is_some_and(|args| !args.nodes.is_empty());
                        if has_type_args {
                            return false;
                        }

                        let sym_id = match self
                            .resolve_identifier_symbol_in_type_position(type_ref.type_name)
                        {
                            crate::symbol_resolver::TypeSymbolResolution::Type(sym_id) => {
                                Some(sym_id)
                            }
                            _ => match self
                                .resolve_qualified_symbol_in_type_position(type_ref.type_name)
                            {
                                crate::symbol_resolver::TypeSymbolResolution::Type(sym_id) => {
                                    Some(sym_id)
                                }
                                _ => None,
                            },
                        };

                        sym_id.is_some_and(|sym_id| {
                            self.get_type_params_for_symbol(sym_id)
                                .iter()
                                .any(|param| param.default.is_some())
                        })
                    });

                // Recovery path: a type reference can appear where an expression statement is expected
                // (e.g. malformed `this.x: any;` parses through a labeled statement).
                // In value position, primitive type keywords should emit TS2693.
                if let Some(ext) = self.ctx.arena.get_extended(idx) {
                    let parent = ext.parent;
                    let recovery_stmt_kind = if parent.is_some() {
                        self.ctx
                            .arena
                            .get(parent)
                            .map(|parent_node| parent_node.kind)
                    } else {
                        None
                    };
                    if matches!(
                        recovery_stmt_kind,
                        Some(k)
                            if k == syntax_kind_ext::LABELED_STATEMENT
                                || k == syntax_kind_ext::EXPRESSION_STATEMENT
                    ) && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                        && let Some(name) = self.entity_name_text(type_ref.type_name)
                        && matches!(
                            name.as_str(),
                            "number"
                                | "string"
                                | "boolean"
                                | "symbol"
                                | "void"
                                | "undefined"
                                | "null"
                                | "any"
                                | "unknown"
                                | "never"
                                | "object"
                                | "bigint"
                        )
                    {
                        self.error_type_only_value_at(&name, type_ref.type_name);
                        self.ctx.node_types.insert(idx.0, TypeId::ERROR);
                        return TypeId::ERROR;
                    }
                }

                // Validate the type reference exists before lowering
                // Check cache first - but allow re-resolution of ERROR when type params
                // are in scope, since the ERROR may have been cached when type params
                // weren't available yet (non-deterministic symbol processing order).
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !should_refresh_cached_defaulted_reference
                    {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_set.contains(&idx)
                        && !should_refresh_cached_defaulted_reference
                    {
                        return cached;
                    }
                    // cached == ERROR but type_parameter_scope is non-empty: re-resolve
                    // cached != ERROR and type_parameter_scope non-empty: re-resolve (type params may differ)
                }
                let result = self.get_type_from_type_reference(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::TYPE_QUERY {
                // Handle typeof X - need to resolve symbol properly via binder.
                // Return cached non-ERROR results when no type params in scope.
                // Always re-resolve ERROR because TypeNodeChecker may have cached
                // ERROR for qualified names it can't resolve without binder context.
                // Also re-resolve TypeQuery(SymbolRef) types — these are unresolved
                // deferred types cached by TypeNodeChecker that don't incorporate
                // control-flow narrowing.  The CheckerState path resolves them with
                // flow sensitivity (e.g., `typeof c` inside `if (typeof c === 'string')`
                // should yield the narrowed type `string`, not `string | number`).
                if let Some(&cached) = self.ctx.node_types.get(&idx.0)
                    && cached != TypeId::ERROR
                    && self.ctx.type_parameter_scope.is_empty()
                    && tsz_solver::type_queries::get_type_query_symbol_ref(self.ctx.types, cached)
                        .is_none()
                {
                    return cached;
                }
                let result = self.get_type_from_type_query(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::UNION_TYPE {
                // Handle union types specially to ensure nested typeof expressions
                // are resolved via binder (for abstract class detection)
                // Check cache first - allow re-resolution of ERROR when type params in scope
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_set.contains(&idx)
                    {
                        return cached;
                    }
                }
                let result = self.get_type_from_union_type(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::INTERSECTION_TYPE {
                // Handle intersection types specially to ensure nested typeof expressions
                // are resolved via binder (same reason as UNION_TYPE above)
                // Check cache first - allow re-resolution of ERROR when type params in scope
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_set.contains(&idx)
                    {
                        return cached;
                    }
                }
                let result = self.get_type_from_intersection_type(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::TYPE_LITERAL {
                // Type literals should use checker resolution so type parameters resolve correctly.
                // Check cache first - allow re-resolution of ERROR when type params in scope
                if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                    if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
                        return cached;
                    }
                    if cached == TypeId::ERROR
                        && self.ctx.type_parameter_scope.is_empty()
                        && !self.ctx.node_resolution_set.contains(&idx)
                    {
                        return cached;
                    }
                }
                let result = self.get_type_from_type_literal(idx);
                self.ctx.node_types.insert(idx.0, result);
                return result;
            }
            if node.kind == syntax_kind_ext::ARRAY_TYPE {
                // Route array types through CheckerState so the element type reference
                // goes through get_type_from_type_node (which checks TS2314 for generics).
                if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                    // Recovery path: malformed value expressions like `number[]` can parse
                    // as ARRAY_TYPE initializers. Emit TS2693 on the primitive keyword.
                    if let Some(ext) = self.ctx.arena.get_extended(idx) {
                        let parent = ext.parent;
                        if parent.is_some()
                            && let Some(parent_node) = self.ctx.arena.get(parent)
                            && matches!(
                                parent_node.kind,
                                k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                                    || k == syntax_kind_ext::LABELED_STATEMENT
                                    || k == syntax_kind_ext::VARIABLE_DECLARATION
                                    || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                                    || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                                    || k == syntax_kind_ext::BINARY_EXPRESSION
                                    || k == syntax_kind_ext::RETURN_STATEMENT
                            )
                            && let Some(elem_node) = self.ctx.arena.get(array_type.element_type)
                        {
                            use tsz_scanner::SyntaxKind;
                            let keyword_name = match elem_node.kind {
                                k if k == SyntaxKind::NumberKeyword as u16 => Some("number"),
                                k if k == SyntaxKind::StringKeyword as u16 => Some("string"),
                                k if k == SyntaxKind::BooleanKeyword as u16 => Some("boolean"),
                                k if k == SyntaxKind::SymbolKeyword as u16 => Some("symbol"),
                                k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                                k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined"),
                                k if k == SyntaxKind::NullKeyword as u16 => Some("null"),
                                k if k == SyntaxKind::AnyKeyword as u16 => Some("any"),
                                k if k == SyntaxKind::UnknownKeyword as u16 => Some("unknown"),
                                k if k == SyntaxKind::NeverKeyword as u16 => Some("never"),
                                k if k == SyntaxKind::ObjectKeyword as u16 => Some("object"),
                                k if k == SyntaxKind::BigIntKeyword as u16 => Some("bigint"),
                                _ => None,
                            };
                            if let Some(keyword_name) = keyword_name {
                                self.error_type_only_value_at(
                                    keyword_name,
                                    array_type.element_type,
                                );
                                self.ctx.node_types.insert(idx.0, TypeId::ERROR);
                                return TypeId::ERROR;
                            }
                        }
                    }

                    let elem_type = self.get_type_from_type_node(array_type.element_type);
                    let result = self.ctx.types.factory().array(elem_type);
                    self.ctx.node_types.insert(idx.0, result);
                    return result;
                }
            }
        }

        // Check for unused type parameters (TS6133) in function/constructor type nodes
        let type_params = self
            .ctx
            .arena
            .get(idx)
            .and_then(|n| self.ctx.arena.get_function_type(n))
            .and_then(|fd| fd.type_parameters.clone());
        if let Some(tp) = type_params {
            self.check_unused_type_params(&Some(tp), idx);
        }

        // EXPLICIT WALK: For TYPE_REFERENCE nodes, route through CheckerState's method to emit TS2304.
        // TypeNodeChecker uses TypeLowering which doesn't emit errors, so we must handle TYPE_REFERENCE
        // explicitly here to ensure undefined type names emit TS2304.
        // This fixes cases like `function A(): (public B) => C {}` where C is undefined.
        if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::TYPE_REFERENCE
        {
            return self.get_type_from_type_reference(idx);
        }

        // For other type nodes, delegate to TypeNodeChecker
        let mut checker = crate::TypeNodeChecker::new(&mut self.ctx);
        let result = checker.check(idx);

        // Post-lowering TS2314 check: TypeNodeChecker uses TypeLowering which doesn't
        // validate that generic types have required type arguments. Walk nested
        // TYPE_REFERENCE nodes in compound types (FUNCTION_TYPE, TYPE_LITERAL, etc.)
        // and emit TS2314 where needed.
        if let Some(node) = self.ctx.arena.get(idx)
            && matches!(
                node.kind,
                k if k == syntax_kind_ext::FUNCTION_TYPE
                    || k == syntax_kind_ext::CONSTRUCTOR_TYPE
                    || k == syntax_kind_ext::TYPE_LITERAL
            )
        {
            self.check_nested_type_refs_for_ts2314(idx);
        }

        result
    }

    /// Walk the AST subtree rooted at `idx` and emit TS2314 for any
    /// `TYPE_REFERENCE` nodes that reference a generic type without providing
    /// the required type arguments.
    pub(crate) fn check_nested_type_refs_for_ts2314(&mut self, root: NodeIndex) {
        use tsz_parser::parser::node::NodeAccess;

        let mut stack = vec![root];
        let mut visited = FxHashSet::default();
        while let Some(idx) = stack.pop() {
            if idx.is_none() || !visited.insert(idx) {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            {
                if type_ref.type_arguments.is_none() {
                    // No type arguments provided - check if generic type requires them
                    self.check_type_ref_requires_args(type_ref.type_name, idx);
                }
                // Don't descend into TYPE_REFERENCE children to avoid double-checking
                // type arguments (those are separately validated when the outer
                // TYPE_REFERENCE has args).
                continue;
            }
            // Push children for traversal
            stack.extend(self.ctx.arena.get_children(idx));
        }
    }

    /// Check if a `TYPE_REFERENCE` without type arguments references a generic type
    /// that requires type arguments (TS2314).
    fn check_type_ref_requires_args(&mut self, type_name_idx: NodeIndex, ref_idx: NodeIndex) {
        use crate::symbol_resolver::TypeSymbolResolution;

        let qn_sym_res = self.resolve_qualified_symbol_in_type_position(type_name_idx);
        if let TypeSymbolResolution::Type(sym_id) = qn_sym_res {
            let required_count = self.count_required_type_params(sym_id);
            if required_count > 0 {
                let name = self
                    .get_symbol_globally(sym_id)
                    .map(|s| s.escaped_name.clone())
                    .or_else(|| self.entity_name_text(type_name_idx))
                    .unwrap_or_else(|| "<unknown>".to_string());
                let type_params = self.get_type_params_for_symbol(sym_id);
                let display_name = Self::format_generic_display_name_with_interner(
                    &name,
                    &type_params,
                    self.ctx.types,
                );
                self.error_generic_type_requires_type_arguments_at(
                    &display_name,
                    required_count,
                    ref_idx,
                );
            }
        }
    }

    // =========================================================================
    // Source Location Tracking & Solver Diagnostics
    // =========================================================================

    /// Get a source location for a node.
    pub fn get_source_location(&self, idx: NodeIndex) -> Option<SourceLocation> {
        let node = self.ctx.arena.get(idx)?;
        Some(SourceLocation::new(
            self.ctx.file_name.clone(),
            node.pos,
            node.end,
        ))
    }

    // Report a type not assignable error using solver diagnostics with source tracking.
    // This is the basic error that just says "Type X is not assignable to Y".
    // For detailed errors with elaboration (e.g., "property 'x' is missing"),
    // use `error_type_not_assignable_with_reason_at` instead.

    // Report a cannot find name error using solver diagnostics with source tracking.
    // Enhanced to provide suggestions for similar names, import suggestions, and
    // library change suggestions for ES2015+ types.

    // Note: can_merge_symbols is in type_checking.rs

    /// Check if a type name is a built-in mapped type utility.
    /// These are standard TypeScript utility types that transform other types.
    /// When used with type arguments, they should not cause "cannot find type" errors.
    fn augment_js_global_value_type_with_expandos(
        &mut self,
        root_name: &str,
        sym_id: SymbolId,
        base_type: TypeId,
    ) -> TypeId {
        use tsz_solver::{ObjectShape, PropertyInfo};

        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return base_type;
        }

        let expando_props = self.collect_expando_properties_for_root(root_name);

        if expando_props.is_empty() {
            return base_type;
        }

        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, base_type)
        else {
            return base_type;
        };

        let mut properties = shape.properties.clone();
        let mut changed = false;

        for prop_name in expando_props {
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if properties.iter().any(|prop| prop.name == prop_atom) {
                continue;
            }

            properties.push(PropertyInfo {
                name: prop_atom,
                type_id: TypeId::ANY,
                write_type: TypeId::ANY,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: Some(sym_id),
                declaration_order: properties.len() as u32,
            });
            changed = true;
        }

        if !changed {
            return base_type;
        }

        self.ctx.types.factory().object_with_index(ObjectShape {
            flags: shape.flags,
            properties,
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
            symbol: shape.symbol.or(Some(sym_id)),
        })
    }

    pub(crate) fn collect_expando_properties_for_root(&self, root_name: &str) -> FxHashSet<String> {
        let mut expando_props: FxHashSet<String> = FxHashSet::default();

        if let Some(props) = self.ctx.binder.expando_properties.get(root_name) {
            expando_props.extend(props.iter().cloned());
        }

        if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if let Some(props) = binder.expando_properties.get(root_name) {
                    expando_props.extend(props.iter().cloned());
                }
            }
        }

        expando_props
    }

    pub(crate) fn augment_callable_type_with_expandos(
        &mut self,
        root_name: &str,
        sym_id: SymbolId,
        base_type: TypeId,
    ) -> TypeId {
        use rustc_hash::FxHashMap;
        use tsz_solver::PropertyInfo;

        let expando_props = self.collect_expando_properties_for_root(root_name);
        if expando_props.is_empty() {
            return base_type;
        }

        let (mut callable_shape, mut property_count) = if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, base_type)
        {
            ((*shape).clone(), shape.properties.len())
        } else if let Some(function_shape) =
            tsz_solver::type_queries::get_function_shape(self.ctx.types, base_type)
        {
            let signature = CallSignature {
                type_params: function_shape.type_params.clone(),
                params: function_shape.params.clone(),
                this_type: function_shape.this_type,
                return_type: function_shape.return_type,
                type_predicate: function_shape.type_predicate.clone(),
                is_method: function_shape.is_method,
            };
            (
                CallableShape {
                    call_signatures: if function_shape.is_constructor {
                        Vec::new()
                    } else {
                        vec![signature.clone()]
                    },
                    construct_signatures: if function_shape.is_constructor {
                        vec![signature]
                    } else {
                        Vec::new()
                    },
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: Some(sym_id),
                    is_abstract: false,
                },
                0,
            )
        } else {
            return base_type;
        };

        let mut properties: FxHashMap<Atom, PropertyInfo> = callable_shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();
        let mut changed = false;

        for prop_name in expando_props {
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if properties.contains_key(&prop_atom) {
                continue;
            }

            properties.insert(
                prop_atom,
                PropertyInfo {
                    name: prop_atom,
                    type_id: TypeId::ANY,
                    write_type: TypeId::ANY,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: Some(sym_id),
                    declaration_order: property_count as u32,
                },
            );
            property_count += 1;
            changed = true;
        }

        if !changed {
            return base_type;
        }

        callable_shape.properties = properties.into_values().collect();
        self.ctx.types.factory().callable(callable_shape)
    }

    pub(crate) fn resolve_global_this_property_type(
        &mut self,
        name: &str,
        error_node: NodeIndex,
    ) -> TypeId {
        if let Some(sym_id) = self.resolve_global_value_symbol(name) {
            if self.alias_resolves_to_type_only(sym_id) {
                self.error_type_only_value_at(name, error_node);
                return TypeId::ERROR;
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                if (symbol.flags & symbol_flags::VALUE) == 0 {
                    self.error_type_only_value_at(name, error_node);
                    return TypeId::ERROR;
                }
                // In TypeScript, `typeof globalThis` only exposes `var`-declared
                // globals (FUNCTION_SCOPED_VARIABLE) and function/class declarations.
                // Block-scoped variables (let/const) are NOT properties of globalThis.
                if symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                    && symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE == 0
                {
                    self.error_property_not_exist_on_global_this(name, error_node);
                    return TypeId::ERROR;
                }
            }
            let base_type = self.get_type_of_symbol(sym_id);
            return self.augment_js_global_value_type_with_expandos(name, sym_id, base_type);
        }

        // Self-reference: `globalThis.globalThis` resolves to `typeof globalThis`.
        if name == "globalThis" {
            return TypeId::UNKNOWN;
        }

        if self.is_known_global_value_name(name) {
            // Emit TS2318/TS2583 for missing global type in property access context
            // TS2583 for ES2015+ types, TS2318 for other global types
            use tsz_binder::lib_loader;
            if lib_loader::is_es2015_plus_type(name) {
                self.error_cannot_find_global_type(name, error_node);
            } else {
                // For pre-ES2015 globals, emit TS2318 (global type missing) instead of TS2304
                self.error_cannot_find_global_type(name, error_node);
            }
            return TypeId::ERROR;
        }

        // For truly unknown properties, return ANY to maintain compatibility with
        // JS expando patterns (e.g., `globalThis.alpha = 4` in checkJs mode).
        // Emitting TS2339 here would require `typeof globalThis` to be a proper
        // object type rather than ANY, which is a larger refactor.
        TypeId::ANY
    }
}
