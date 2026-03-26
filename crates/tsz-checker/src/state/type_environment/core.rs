//! Core type environment building, application type evaluation, and
//! property access type resolution.

use crate::query_boundaries::state::type_environment as query;
use crate::state::{CheckerState, EnumKind, MAX_INSTANTIATION_DEPTH};
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::MappedTypeId;
use tsz_solver::SourceLocation;
use tsz_solver::TypeId;
use tsz_solver::Visibility;
use tsz_solver::{CallSignature, CallableShape, ParamInfo};

// Global instantiation depth and fuel counters now live in
// `EvaluationSession` (shared via `Rc` on `CheckerContext::eval_session`).
// Previously these were `thread_local!` counters that survived cross-arena
// delegation. The explicit session approach makes the state visible, testable,
// and compatible with future multi-threaded evaluation.

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
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);
        let enum_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let enum_binder = self
            .ctx
            .get_binder_for_file(file_idx)
            .unwrap_or(self.ctx.binder);

        let mut props: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        for &decl_idx in &symbol.declarations {
            let Some(node) = enum_arena.get(decl_idx) else {
                continue;
            };
            let Some(enum_decl) = enum_arena.get_enum(node) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = enum_arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = enum_arena.get_enum_member(member_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(member.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                // Fix: Create nominal enum member types for each member
                // This preserves nominal identity so E.A is not assignable to E.B
                let Some(member_sym_id) = enum_binder
                    .get_node_symbol(member_idx)
                    .or_else(|| enum_binder.get_node_symbol(member.name))
                    .or_else(|| {
                        self.ctx
                            .binder
                            .get_node_symbol(member_idx)
                            .or_else(|| self.ctx.binder.get_node_symbol(member.name))
                    })
                else {
                    continue;
                };
                let member_def_id = self.ctx.get_or_create_def_id(member_sym_id);
                let literal_type = if std::ptr::eq(enum_arena, self.ctx.arena) {
                    self.enum_member_type_from_decl(member_idx)
                } else if member.initializer.is_some() {
                    match enum_arena.get(member.initializer) {
                        Some(init_node) => match init_node.kind {
                            k if k == SyntaxKind::StringLiteral as u16 => enum_arena
                                .get_literal(init_node)
                                .map(|lit| factory.literal_string(&lit.text))
                                .unwrap_or(TypeId::STRING),
                            k if k == SyntaxKind::NumericLiteral as u16 => enum_arena
                                .get_literal(init_node)
                                .and_then(|lit| lit.value.or_else(|| lit.text.parse::<f64>().ok()))
                                .map(|value| factory.literal_number(value))
                                .unwrap_or(TypeId::NUMBER),
                            _ => TypeId::NUMBER,
                        },
                        None => TypeId::NUMBER,
                    }
                } else {
                    TypeId::NUMBER
                };
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

        // Check BOTH per-context and session-level instantiation depth/fuel.
        // Per-context counters reset to 0 on cross-arena delegation (with_parent_cache),
        // but the session is shared via Rc so its counters survive delegation.
        if self.ctx.instantiation_depth.get() >= MAX_INSTANTIATION_DEPTH
            || self.ctx.eval_session.instantiation_limits_exceeded()
        {
            self.ctx.depth_exceeded.set(true);
            self.ctx.application_eval_set.remove(&type_id);
            return type_id;
        }
        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() + 1);
        self.ctx.eval_session.enter_instantiation();

        let result = self.evaluate_application_type_inner(type_id);

        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() - 1);
        self.ctx.eval_session.leave_instantiation();
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
            && !query::contains_infer_types_db(self.ctx.types, result)
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
            if !args.is_empty() && query::type_query_symbol(self.ctx.types, base).is_some() {
                let instantiated = self.instantiate_callable_type_params(body_type, &args);
                if instantiated != body_type {
                    return self.evaluate_type_with_env(instantiated);
                }
            }
            return body_type;
        }

        // Homomorphic identity mapped type passthrough: delegate to solver query.
        // For identity mapped types `{ [K in keyof T]: T[K] }`, primitives pass
        // through directly, `any` with array constraint passes through, and
        // `any` without array constraint produces `{ [x: string]: any; [x: number]: any }`.
        if let Some(mapped_id) = query::mapped_type_id(self.ctx.types, body_type)
            && let Some(identity_info) = query::classify_identity_mapped(self.ctx.types, mapped_id)
            && let Some(idx) = type_params
                .iter()
                .position(|p| p.name == identity_info.source_param_name)
            && idx < args.len()
        {
            let arg = self.evaluate_type_with_env(args[idx]);
            // Use the solver's centralized passthrough query. For `any`-like
            // args, the solver checks the raw constraint from IdentityMappedInfo.
            // If the constraint is a Lazy(DefId), fall through to the checker's
            // full evaluation path which resolves it.
            if let Some(result) =
                query::evaluate_identity_mapped_passthrough(self.ctx.types, mapped_id, arg)
            {
                return result;
            }
            // Fallback for `any`-like args where the constraint is unresolved
            // (Lazy/Application). Evaluate the constraint and retry.
            if (arg == TypeId::ANY || arg == TypeId::UNKNOWN || arg == TypeId::NEVER)
                && identity_info.source_constraint.is_some_and(|c| {
                    let evaluated = self.evaluate_type_for_assignability(c);
                    query::is_array_or_tuple_type(self.ctx.types, evaluated)
                })
            {
                return arg;
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
            if let Some(keyof_source) = query::keyof_inner_type(self.ctx.types, mapped.constraint) {
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

                // tsc rule: when a homomorphic mapped type is instantiated with `any`
                // and the type parameter has an array/tuple constraint, use the
                // constraint's shape instead of `any` as the source. This preserves
                // array/tuple identity: e.g., `{ -readonly [K in keyof T]: string }`
                // with T=any (T extends readonly any[]) produces `string[]`, not
                // `{ [x: string]: string }`. Matches tsc's instantiateMappedArrayType.
                //
                // Only applies when no `as` clause or identity name mapping, matching
                // the solver's array/tuple preservation condition.
                let effective_source = if evaluated_source == TypeId::ANY {
                    let is_identity_or_no_name = mapped.name_type.is_none()
                        || query::is_identity_name_mapping(self.ctx.types, &mapped);
                    if is_identity_or_no_name {
                        self.find_array_tuple_constraint_for_keyof_source(
                            keyof_source,
                            &type_params,
                        )
                        .unwrap_or(evaluated_source)
                    } else {
                        evaluated_source
                    }
                } else {
                    evaluated_source
                };

                let inst_template = instantiate_type(self.ctx.types, mapped.template, &subst);
                let inst_name_type = mapped
                    .name_type
                    .map(|nt| instantiate_type(self.ctx.types, nt, &subst));
                let inst_mapped = tsz_solver::MappedType {
                    type_param: mapped.type_param,
                    constraint: self.ctx.types.keyof(effective_source),
                    name_type: inst_name_type,
                    template: inst_template,
                    readonly_modifier: mapped.readonly_modifier,
                    optional_modifier: mapped.optional_modifier,
                };
                let mapped_type_id = self.ctx.types.mapped(inst_mapped);
                if query::is_union_or_intersection(self.ctx.types, effective_source)
                    && self.contains_type_parameters_cached(effective_source)
                {
                    return mapped_type_id;
                }
                // Route through the solver's TypeEvaluator which handles
                // mapped type expansion with full resolver context.
                let evaluated = self.evaluate_type_with_env(mapped_type_id);
                return if evaluated != mapped_type_id {
                    evaluated
                } else {
                    // Fall back to checker-side expansion if the solver couldn't
                    // evaluate it (e.g., deferred mapped types with type params).
                    self.evaluate_mapped_type_with_resolution(mapped_type_id)
                };
            }
        }

        // Resolve type arguments so distributive conditionals can see unions.
        // For conditional type bodies whose extends side contains infer patterns,
        // preserve generic/type-parameter arguments so the conditional evaluator
        // can still use their original constraints during infer matching.
        //
        // The solver classifies the body to determine the arg preservation policy:
        // - ConditionalInfer: preserve type-parameter and Application-form args
        // - ConditionalApplicationInfer: preserve Application-form args specifically
        // - EvaluateAll: evaluate all args normally
        let arg_preservation = query::classify_body_for_arg_preservation(self.ctx.types, body_type);
        let evaluated_args: Vec<TypeId> = args
            .iter()
            .map(|&arg| {
                match arg_preservation {
                    query::BodyArgPreservation::ConditionalInfer
                        if self.contains_type_parameters_cached(arg)
                            || query::application_info(self.ctx.types, arg).is_some() =>
                    {
                        arg
                    }
                    query::BodyArgPreservation::ConditionalInfer
                    | query::BodyArgPreservation::ConditionalApplicationInfer
                        if query::application_info(self.ctx.types, arg).is_some() =>
                    {
                        // Preserve Application args so the conditional evaluator can
                        // match at the Application level for infer pattern matching.
                        arg
                    }
                    _ => self.evaluate_type_with_env(arg),
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
        if query::contains_this_type(self.ctx.types, instantiated) {
            instantiated = query::substitute_this_type(self.ctx.types, instantiated, type_id);
        }
        // Recursively evaluate in case the result contains more applications
        let evaluated_result = self.evaluate_application_type(instantiated);
        let result = self.prune_impossible_object_union_members_with_env(evaluated_result);

        // Mapped types in the result are now evaluated by the solver's TypeEvaluator
        // via evaluate_type_with_env, which has a second-pass with CheckerContext
        // resolver that can resolve Lazy(DefId) types on the fly. This eliminates
        // the need for checker-side mapped type expansion here.

        // Preserve instantiated discriminated object intersections in their deferred
        // intersection form. Eager env evaluation collapses these into distributed
        // unions, which loses both discriminant-aware `keyof` and fresh EPC behavior.
        if query::is_discriminated_object_intersection(self.ctx.types, result) {
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
        if let Some(shape_id) = query::callable_shape_id(self.ctx.types, body_type) {
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
        } else if let Some(members) = query::get_intersection_members(self.ctx.types, body_type) {
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

    /// For a `keyof T` source in a homomorphic mapped type, check if the type
    /// parameter T has an array/tuple constraint. If so, return the evaluated
    /// constraint type. Used when T is instantiated to `any` to preserve array/tuple
    /// shape in mapped type evaluation.
    fn find_array_tuple_constraint_for_keyof_source(
        &mut self,
        keyof_source: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> Option<TypeId> {
        // Use the solver query to extract the type parameter name, avoiding
        // direct TypeData pattern matching in the checker (architecture rule §4).
        let param_info = query::type_param_name(self.ctx.types, keyof_source)
            .and_then(|name| type_params.iter().find(|p| p.name == name))?;

        let constraint = param_info.constraint?;
        let evaluated_constraint = self.evaluate_type_for_assignability(constraint);
        if query::is_array_or_tuple_type(self.ctx.types, evaluated_constraint) {
            Some(evaluated_constraint)
        } else {
            None
        }
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

        // Pre-resolve lazy DefIds in the template so the solver's evaluator can
        // access them through the TypeEnvironment.  Even when we fall through to
        // checker-local expansion below, this improves cache hit rates for
        // subsequent evaluate_type_with_env calls on the instantiated template.
        self.ensure_relation_input_ready(mapped.template);
        if let Some(nt) = mapped.name_type {
            self.ensure_relation_input_ready(nt);
        }

        // For homomorphic mapped types where the source is a type parameter
        // (e.g., `{ [K in keyof P]: P[K] }` with `P extends SomeType<Foo>`),
        // pre-resolve the type parameter's constraint into the TypeEnvironment.
        // The solver's evaluate_index_access resolves IndexAccess on type parameters
        // through their constraints, but this requires the constraint's Lazy types
        // (Application/Lazy DefIds) to be resolvable via the environment. Without
        // this, the solver defers IndexAccess because the constraint's types aren't
        // in the environment, and the checker falls back to local expansion.
        if let query::MappedConstraintKind::KeyOf(source) =
            query::classify_mapped_constraint(self.ctx.types, mapped.constraint)
            && let Some(constraint) = query::type_parameter_constraint(self.ctx.types, source)
        {
            self.ensure_relation_input_ready(constraint);
        }

        // Use solver classification to decide whether to preserve array/tuple identity.
        // This replaces the checker-local `mapped_constraint_source_needs_array_like_preservation`
        // with a solver-owned query, keeping structural classification behind the boundary.
        if let query::MappedConstraintKind::KeyOf(source) =
            query::classify_mapped_constraint(self.ctx.types, mapped.constraint)
        {
            let resolved_source = self.evaluate_type_with_resolution(source);
            let source_kind = query::classify_mapped_source(self.ctx.types, resolved_source);
            if !matches!(source_kind, query::MappedSourceKind::Object) {
                // Source is array/tuple-like — delegate to the solver's evaluator
                // which preserves the structural identity.
                let evaluated = self.evaluate_type_with_env(type_id);
                if evaluated != type_id {
                    return evaluated;
                }
            }
        }

        let resolved_mapped_id = if keys != mapped.constraint {
            query::reconstruct_mapped_with_constraint(self.ctx.types, mapped_id, keys)
        } else {
            mapped_id
        };

        // Detect homomorphic source early: needed both for the solver retry
        // decision and for property expansion below.
        let is_homomorphic_source = query::keyof_inner_type(self.ctx.types, mapped.constraint);
        let is_homomorphic = is_homomorphic_source.is_some();

        // For non-homomorphic mapped types with a resolved constraint, retry the
        // solver's evaluator. The pre-resolved template (ensure_relation_input_ready
        // above) and resolved constraint should give the solver enough to expand
        // the mapped type directly, eliminating checker-local property expansion.
        //
        // Skip for homomorphic types: their templates (`T[K]`, `Box<T[K]>`) need
        // the checker's `resolve_property_access_with_env` for deeper IndexAccess
        // resolution than the solver provides.
        if resolved_mapped_id != mapped_id && !is_homomorphic {
            let resolved_mapped = self.ctx.types.mapped_type(resolved_mapped_id);
            let resolved_type_id = self.ctx.types.mapped(*resolved_mapped);
            let evaluated = self.evaluate_type_with_env(resolved_type_id);
            if evaluated != resolved_type_id {
                return evaluated;
            }
        }

        // Fallback: checker-local expansion when the solver still can't evaluate.
        // This handles homomorphic mapped types and deferred types with constraints
        // the solver can't resolve even with the CheckerContext resolver.

        // Prefer the shared finite-key collector once the constraint has been
        // resolved. This keeps mapped expansion aligned with property access and
        // exact `keyof` key-space semantics.
        let string_keys: Vec<_> = if let Some(names) =
            query::collect_finite_mapped_property_names(self.ctx.types, resolved_mapped_id)
        {
            names.into_iter().collect()
        } else {
            query::extract_string_literal_keys(self.ctx.types, keys)
        };
        if string_keys.is_empty() {
            // Can't evaluate - return original
            return type_id;
        }

        let source_prop_map: rustc_hash::FxHashMap<tsz_common::Atom, (bool, bool, TypeId)> =
            if let Some(source) = is_homomorphic_source {
                // Pre-resolve lazy refs in the homomorphic source so property
                // collection sees the fully resolved object shape.
                self.ensure_relation_input_ready(source);
                query::collect_homomorphic_source_properties(self.ctx.types, source)
            } else {
                Default::default()
            };

        // Build the resulting object properties using solver-centralized modifier logic.
        let mut properties = Vec::new();
        for key_name in string_keys {
            // Use env-evaluated template instantiation (needed for Lazy/DefId resolution).
            let mut property_type =
                self.instantiate_mapped_property_template_with_env(&mapped, key_name);

            // Look up source property info for modifier computation
            let source_info = source_prop_map.get(&key_name);
            let (source_optional, source_readonly) =
                source_info.map_or((false, false), |(opt, ro, _)| (*opt, *ro));

            // Use solver-centralized modifier computation
            let (optional, readonly) = query::compute_mapped_modifiers(
                &mapped,
                is_homomorphic,
                source_optional,
                source_readonly,
            );

            // For homomorphic mapped types with optional source properties, use the
            // source property's declared type to avoid double-encoding undefined.
            // This matches the solver's evaluate_mapped behavior.
            if is_homomorphic
                && source_optional
                && let Some((_, _, declared_type)) = source_info
            {
                property_type = *declared_type;
            }

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

        // When the template produces an IndexAccess (e.g., T[K] → ObjType["key"]),
        // resolve the object part through evaluate_type_with_resolution so that
        // Lazy(DefId) references become concrete types.  Then attempt property
        // access resolution with the resolved object.
        if let Some((obj, _idx)) = query::index_access_types(self.ctx.types, property_type) {
            let obj_type = self.evaluate_type_with_resolution(obj);

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

        // Reset the session's instantiation fuel for this file. Each file gets a
        // fresh budget for Application type evaluations. Without this, a complex
        // file (react16.d.ts) would exhaust the fuel and starve subsequent files.
        self.ctx.eval_session.reset_instantiation_fuel();

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
        // are processed BEFORE variable/parameter/property symbols.
        // EXPORT_VALUE is included because `export default class` creates a symbol with
        // EXPORT_VALUE | ALIAS flags (not CLASS). Processing it before class member
        // PROPERTY symbols ensures the class instance type is built first, so member
        // initializers that reference `this` can resolve correctly via the prescan type.
        symbols_with_flags.sort_by_key(|&(sym_id, flags)| {
            let is_type_defining = flags
                & (symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::INTERFACE
                    | symbol_flags::TYPE_ALIAS
                    | symbol_flags::ENUM
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::EXPORT_VALUE)
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

            // IMPORTANT: get_type_of_symbol internally calls compute_type_of_symbol which
            // returns both the type AND the correct type_params, then inserts them into
            // ctx.type_env. We MUST NOT separately call get_type_params_for_symbol because
            // that creates fresh type parameter IDs that won't match those used in the type body.
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
        let mixed_class_interface =
            (flags & symbol_flags::CLASS) != 0 && (flags & symbol_flags::INTERFACE) != 0;
        if let Some(node) = checker.ctx.arena.get(decl_idx) {
            if flags & symbol_flags::TYPE_ALIAS != 0
                && let Some(type_alias) = checker.ctx.arena.get_type_alias(node)
            {
                let (params, updates) = checker.push_type_parameters(&type_alias.type_parameters);
                checker.pop_type_parameters(updates);
                return Some(params);
            }
            if !mixed_class_interface
                && flags & symbol_flags::CLASS != 0
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
        let prefers_type_only_decls =
            (flags & symbol_flags::CLASS) != 0 && (flags & symbol_flags::INTERFACE) != 0;

        // Use only the local def_type_params cache, NOT get_def_type_params which
        // falls through to the DefinitionStore. The DefinitionStore may contain
        // pre-populated placeholder params (from from_semantic_defs) that have
        // constraint: None even when the actual type parameter declarations have
        // constraints. The local cache is only populated after full AST-based
        // resolution via insert_def_type_params, so it always has correct constraints.
        //
        // Merged class+interface symbols are special: class-side resolution paths can
        // seed the cache with the class arity before the interface-side defaults are
        // merged. Recompute those through the merged declaration walk instead of
        // trusting a potentially stale cache entry.
        let cached_params = (!prefers_type_only_decls)
            .then(|| self.ctx.def_type_params.borrow().get(&def_id).cloned())
            .flatten();
        if let Some(cached) = cached_params {
            let cached_is_placeholder = self.ctx.symbol_is_from_lib(sym_id)
                && !cached.is_empty()
                && cached
                    .iter()
                    .all(|param| param.constraint.is_none() && param.default.is_none());
            if !cached_is_placeholder {
                self.ctx.leave_recursion();
                return cached;
            }
            self.ctx.def_type_params.borrow_mut().remove(&def_id);
        }
        if !prefers_type_only_decls && self.ctx.def_no_type_params.borrow().contains(&def_id) {
            self.ctx.leave_recursion();
            return Vec::new();
        }

        // Fast path: only class/interface/type alias symbols can declare type parameters.
        if flags & (symbol_flags::TYPE_ALIAS | symbol_flags::CLASS | symbol_flags::INTERFACE) == 0 {
            self.ctx.def_no_type_params.borrow_mut().insert(def_id);
            self.ctx.leave_recursion();
            return Vec::new();
        }

        let mut decl_candidates = Vec::new();
        if !prefers_type_only_decls && value_decl != tsz_parser::parser::NodeIndex::NONE {
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
                        let decl_binder = self
                            .ctx
                            .get_binder_for_arena(arena.as_ref())
                            .unwrap_or(self.ctx.binder);
                        let decl_file_name = arena
                            .source_files
                            .first()
                            .map(|sf| sf.file_name.clone())
                            .unwrap_or_else(|| self.ctx.file_name.clone());
                        let mut checker = Box::new(CheckerState::with_parent_cache(
                            arena.as_ref(),
                            decl_binder,
                            self.ctx.types,
                            decl_file_name,
                            self.ctx.compiler_options.clone(),
                            self,
                        ));
                        if let Some(file_idx) = self.ctx.get_file_idx_for_arena(arena.as_ref()) {
                            checker.ctx.current_file_idx = file_idx;
                        }
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

    pub(crate) fn get_display_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let params = self.get_type_params_for_symbol(sym_id);
        if !params.is_empty() {
            return params;
        }

        self.get_type_param_names_for_symbol_from_ast(sym_id)
            .into_iter()
            .map(|name| tsz_solver::TypeParamInfo {
                name: self.ctx.types.intern_string(&name),
                constraint: None,
                default: None,
                is_const: false,
            })
            .collect()
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
        let builtin_override = self.get_symbol_globally(sym_id).and_then(|symbol| {
            match symbol.escaped_name.as_str() {
                "Iterator"
                | "Iterable"
                | "AsyncIterator"
                | "AsyncIterable"
                | "IterableIterator"
                | "AsyncIterableIterator"
                | "IteratorObject"
                | "AsyncIteratorObject" => Some(1),
                "Generator" | "AsyncGenerator" => Some(0),
                _ => None,
            }
        });

        // First try the fast AST-level check. This avoids recursive resolution
        // issues when a type parameter default references the type being declared
        // (e.g., `interface SelfRef<T = SelfRef> {}`). In such cases,
        // `get_type_params_for_symbol` would recursively try to resolve the
        // default, fail, and incorrectly report the param as required.
        if let Some(ast_count) = self.count_required_type_params_from_ast(sym_id) {
            if let Some(override_count) = builtin_override
                && ast_count > override_count
            {
                return override_count;
            }
            return ast_count;
        }
        let type_params = self.get_type_params_for_symbol(sym_id);
        let required = type_params.iter().filter(|p| p.default.is_none()).count();
        if let Some(override_count) = builtin_override
            && required > override_count
        {
            return override_count;
        }
        required
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

        // Track the minimum required count across all declarations.
        // For merged interfaces (e.g., local `interface Generator<T>` merged with
        // lib `interface Generator<T = unknown, TReturn = any, TNext = any>`),
        // a declaration with defaults on its type params reduces the required count.
        let mut best_required: Option<usize> = None;

        for decl_idx in decl_candidates {
            // Try the current arena first, then cross-arena lookup for lib files.
            let result = Self::count_required_params_in_arena(self.ctx.arena, flags, decl_idx)
                .or_else(|| {
                    // For lib file declarations, the node lives in a different arena.
                    // Look up the correct arena via declaration_arenas.
                    if let Some(arenas) =
                        self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                    {
                        for arena in arenas {
                            if let Some(count) = Self::count_required_params_in_arena(
                                arena.as_ref(),
                                flags,
                                decl_idx,
                            ) {
                                return Some(count);
                            }
                        }
                    }
                    self.ctx
                        .binder
                        .symbol_arenas
                        .get(&sym_id)
                        .and_then(|arena| {
                            Self::count_required_params_in_arena(arena.as_ref(), flags, decl_idx)
                        })
                });

            if let Some(required) = result {
                best_required = Some(match best_required {
                    Some(prev) => prev.min(required),
                    None => required,
                });
            }
        }
        best_required
    }

    fn get_type_param_names_for_symbol_from_ast(&self, sym_id: SymbolId) -> Vec<String> {
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return Vec::new();
        };

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
            if let Some(names) = Self::type_param_names_in_arena(
                self.ctx.arena,
                flags,
                decl_idx,
                &symbol.escaped_name,
            ) && !names.is_empty()
            {
                return names;
            }

            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    if let Some(names) = Self::type_param_names_in_arena(
                        arena.as_ref(),
                        flags,
                        decl_idx,
                        &symbol.escaped_name,
                    ) && !names.is_empty()
                    {
                        return names;
                    }
                }
            }

            if let Some(names) = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .and_then(|arena| {
                    Self::type_param_names_in_arena(
                        arena.as_ref(),
                        flags,
                        decl_idx,
                        &symbol.escaped_name,
                    )
                })
                && !names.is_empty()
            {
                return names;
            }
        }

        Vec::new()
    }

    /// Count required type params for a single declaration in a specific arena.
    fn count_required_params_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        flags: u32,
        decl_idx: NodeIndex,
    ) -> Option<usize> {
        let node = arena.get(decl_idx)?;
        let type_params_list = if flags & tsz_binder::symbol_flags::INTERFACE != 0 {
            arena
                .get_interface(node)
                .and_then(|iface| iface.type_parameters.as_ref())
        } else if flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0 {
            arena
                .get_type_alias(node)
                .and_then(|ta| ta.type_parameters.as_ref())
        } else if flags & tsz_binder::symbol_flags::CLASS != 0 {
            arena
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
                    arena
                        .get(param_idx)
                        .and_then(|n| arena.get_type_parameter(n))
                        .is_some_and(|tp| tp.default == tsz_parser::parser::NodeIndex::NONE)
                })
                .count();
            return Some(required);
        }
        None
    }

    fn type_param_names_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        flags: u32,
        decl_idx: NodeIndex,
        sym_escaped_name: &str,
    ) -> Option<Vec<String>> {
        let node = arena.get(decl_idx)?;
        let type_params_list = if flags & tsz_binder::symbol_flags::INTERFACE != 0 {
            let iface = arena.get_interface(node)?;
            if let Some(name_node) = arena.get(iface.name)
                && let Some(name_ident) = arena.get_identifier(name_node)
                && name_ident.escaped_text.as_str() != sym_escaped_name
            {
                return None;
            }
            iface.type_parameters.as_ref()
        } else if flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0 {
            arena
                .get_type_alias(node)
                .and_then(|ta| ta.type_parameters.as_ref())
        } else if flags & tsz_binder::symbol_flags::CLASS != 0 {
            arena
                .get_class(node)
                .and_then(|c| c.type_parameters.as_ref())
        } else {
            None
        }?;

        let names = type_params_list
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = arena.get(param_idx)?;
                let param = arena.get_type_parameter(param_node)?;
                let name_node = arena.get(param.name)?;
                let ident = arena.get_identifier(name_node)?;
                Some(arena.resolve_identifier_text(ident).to_string())
            })
            .collect::<Vec<_>>();

        Some(names)
    }

    /// Create a union type from multiple types.
    ///
    /// Handles empty (→ NEVER), single (→ that type), and multi-member cases.
    /// Automatically normalizes: flattens nested unions, deduplicates, sorts.
    pub fn get_union_type(&self, types: Vec<TypeId>) -> TypeId {
        tsz_solver::utils::union_or_single(self.ctx.types, types)
    }

    pub fn get_source_location(&self, idx: NodeIndex) -> Option<SourceLocation> {
        let node = self.ctx.arena.get(idx)?;
        Some(SourceLocation::new(
            self.ctx.file_name.clone(),
            node.pos,
            node.end,
        ))
    }
}
