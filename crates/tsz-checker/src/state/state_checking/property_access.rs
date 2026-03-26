//! Property access resolution with environment-aware evaluation.
//!
//! Handles `resolve_property_access_with_env`, mapped-type property resolution,
//! and computed property display names. Split from the excess-property module
//! (`property`) for LOC hygiene.

use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn mapped_constraint_accepts_property_name(
        &self,
        constraint: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::{assignability, common, property_access};

        if assignability::is_any_type(self.ctx.types, constraint)
            || query::is_string_type(self.ctx.types, constraint)
        {
            return true;
        }

        let is_numeric_name =
            tsz_solver::utils::canonicalize_numeric_name(prop_name).is_some();
        if is_numeric_name && property_access::is_number_type(self.ctx.types, constraint) {
            return true;
        }

        common::union_members(self.ctx.types, constraint)
            .is_some_and(|members| {
                members.into_iter().any(|member| {
                    assignability::is_any_type(self.ctx.types, member)
                        || query::is_string_type(self.ctx.types, member)
                        || (is_numeric_name
                            && property_access::is_number_type(self.ctx.types, member))
                })
            })
    }

    pub(crate) fn computed_property_display_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        if let Some(ident_name) = self.get_identifier_text_from_idx(computed.expression) {
            return Some(format!("[{ident_name}]"));
        }

        let expr_node = self.ctx.arena.get(computed.expression)?;
        if expr_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
            let literal = self.ctx.arena.get_literal(expr_node)?;
            return Some(format!("[\"{}\"]", literal.text));
        }

        if expr_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16 {
            let literal = self.ctx.arena.get_literal(expr_node)?;
            return Some(format!(
                "[{}]",
                tsz_solver::utils::canonicalize_numeric_name(&literal.text)
                    .unwrap_or_else(|| literal.text.clone())
            ));
        }

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(expr_node)?;
            let obj_node = self.ctx.arena.get(access.expression)?;
            let obj_ident = self.ctx.arena.get_identifier(obj_node)?;
            if obj_ident.escaped_text.as_str() == "Symbol" {
                let prop_node = self.ctx.arena.get(access.name_or_argument)?;
                let prop_ident = self.ctx.arena.get_identifier(prop_node)?;
                return Some(format!("[Symbol.{}]", prop_ident.escaped_text));
            }
        }

        None
    }

    /// Resolve property access using `TypeEnvironment` (includes lib.d.ts types).
    ///
    /// This method creates a `PropertyAccessEvaluator` with the `TypeEnvironment` as the resolver,
    /// allowing primitive property access to use lib.d.ts definitions instead of just hardcoded lists.
    ///
    /// For example, "foo".length will look up the String interface from lib.d.ts.
    pub(crate) fn resolve_property_access_with_env(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> tsz_solver::operations::property::PropertyAccessResult {
        // Resolve TypeQuery types (typeof X) before property access.
        // The solver-internal evaluator has no TypeResolver, so TypeQuery types
        // can't be resolved there. Resolve them here using the checker's environment.
        let object_type = self.resolve_type_query_type(object_type);

        // Ensure preconditions are ready in the environment for non-trivial
        // property-access inputs. Already-resolved/function-like inputs don't
        // need relation preconditioning here.
        let resolution_kind =
            crate::query_boundaries::state::type_environment::classify_for_property_access_resolution(
                self.ctx.types,
                object_type,
            );
        if !matches!(
            resolution_kind,
            crate::query_boundaries::state::type_environment::PropertyAccessResolutionKind::Resolved
                | crate::query_boundaries::state::type_environment::PropertyAccessResolutionKind::FunctionLike
        ) {
            self.ensure_relation_input_ready(object_type);
        }

        // Route through QueryDatabase so repeated property lookups hit QueryCache.
        // This is especially important for hot paths like repeated `string[].push`
        // checks in class-heavy files.
        let result = self.ctx.types.resolve_property_access_with_options(
            object_type,
            prop_name,
            self.ctx.compiler_options.no_unchecked_indexed_access,
        );

        self.resolve_property_access_with_env_post_query(object_type, prop_name, result)
    }

    /// Continue environment-aware property access resolution from an already
    /// computed initial solver result.
    ///
    /// This avoids duplicate first-pass lookups in hot paths that already
    /// queried `resolve_property_access_with_options` and only need mapped/
    /// application fallback behavior.
    pub(crate) fn resolve_property_access_with_env_post_query(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
        result: tsz_solver::operations::property::PropertyAccessResult,
    ) -> tsz_solver::operations::property::PropertyAccessResult {
        let mut result = result;
        let mut resolved_object_type = object_type;
        let mut mapped_candidate_type = object_type;

        // If the receiver is an Application (e.g. Promise<number> or Pick<T, K>),
        // the QueryCache's noop TypeResolver can't expand it. Evaluate the
        // Application to its structural form so mapped-type revalidation can use
        // the real object shape. Only retry the initial lookup when it already
        // failed; otherwise preserve the original first-pass result and use the
        // expanded type only for mapped-property validation below.
        if tsz_solver::is_generic_application(self.ctx.types, object_type) {
            let expanded = self.evaluate_application_type(object_type);
            if expanded != object_type && expanded != TypeId::ANY && expanded != TypeId::ERROR {
                mapped_candidate_type = expanded;
                resolved_object_type = expanded;
                result = self.ctx.types.resolve_property_access_with_options(
                    expanded,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
            }
        }

        let pruned_object_type =
            self.prune_impossible_object_union_members_with_env(resolved_object_type);
        if pruned_object_type != resolved_object_type {
            resolved_object_type = pruned_object_type;
            mapped_candidate_type = pruned_object_type;
            result = self.ctx.types.resolve_property_access_with_options(
                pruned_object_type,
                prop_name,
                self.ctx.compiler_options.no_unchecked_indexed_access,
            );
        }

        // If the solver returned PropertyNotFound for a TypeParameter whose
        // constraint is an Application (e.g. `P extends Partial<Foo>`), the
        // solver's NoopResolver couldn't expand the Application body.  Evaluate
        // the constraint through the checker's TypeEnvironment and retry.
        // TODO: Move this resolution into the solver's PropertyAccessEvaluator
        // once it gains full TypeEnvironment/TypeResolver awareness.
        if matches!(
            result,
            tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound { .. }
        ) && let Some(constraint) =
            crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                resolved_object_type,
            )
        {
            let evaluated = self.evaluate_type_with_env(constraint);
            if evaluated != constraint && evaluated != TypeId::ANY && evaluated != TypeId::ERROR {
                let retry_result = self.ctx.types.resolve_property_access_with_options(
                    evaluated,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
                if matches!(
                    retry_result,
                    tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                ) {
                    result = retry_result;
                    resolved_object_type = evaluated;
                }
            }
        }

        if query::is_mapped_type(self.ctx.types, mapped_candidate_type)
            && let Some(mapped_property) =
                self.resolve_mapped_property_with_env(mapped_candidate_type, prop_name)
        {
            return mapped_property;
        }

        if matches!(
            result,
            tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound { .. }
        ) && let Some(members) =
            query::intersection_members(self.ctx.types, resolved_object_type)
        {
            let prop_atom = self.ctx.types.intern_string(prop_name);
            let mut member_results = Vec::new();
            let mut any_from_index = false;
            let mut saw_deferred_any_fallback = false;

            for member in members {
                match self.resolve_property_access_with_env(member, prop_name) {
                    tsz_solver::operations::property::PropertyAccessResult::Success {
                        type_id,
                        from_index_signature,
                        ..
                    } => {
                        if type_id == TypeId::ANY
                            && !from_index_signature
                            && query::needs_env_eval(self.ctx.types, member)
                        {
                            saw_deferred_any_fallback = true;
                            continue;
                        }
                        member_results.push(type_id);
                        any_from_index |= from_index_signature;
                    }
                    tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                        ..
                    } => {}
                    other => return other,
                }
            }

            if !member_results.is_empty() {
                let type_id = match member_results.len() {
                    1 => member_results[0],
                    _ => self.ctx.types.factory().intersection(member_results),
                };
                return tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id,
                    write_type: None,
                    from_index_signature: any_from_index,
                };
            }

            if saw_deferred_any_fallback {
                return tsz_solver::operations::property::PropertyAccessResult::simple(TypeId::ANY);
            }

            result = tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                type_id: resolved_object_type,
                property_name: prop_atom,
            };
        }

        // If property not found and the type is a Mapped type (e.g. { [P in Keys]: T }),
        // the solver's NoopResolver can't resolve Lazy(DefId) constraints (type alias refs).
        // Evaluate the mapped type via the solver's TypeEvaluator with full resolver
        // context (CheckerContext), which can resolve Lazy(DefId) types on the fly.
        if matches!(
            result,
            tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound { .. }
        ) && query::is_mapped_type(self.ctx.types, resolved_object_type)
        {
            let expanded = self.evaluate_type_with_env(resolved_object_type);
            if expanded != resolved_object_type
                && expanded != TypeId::ANY
                && expanded != TypeId::ERROR
            {
                return self.ctx.types.resolve_property_access_with_options(
                    expanded,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
            }
        }

        result
    }

    /// Resolve a single mapped-type property with environment-aware key/template
    /// evaluation, without expanding the whole mapped object.
    ///
    /// Returns `None` when we cannot safely decide (e.g. complex key space),
    /// allowing the caller to fall back to full mapped expansion.
    fn resolve_mapped_property_with_env(
        &mut self,
        mapped_type: TypeId,
        prop_name: &str,
    ) -> Option<tsz_solver::operations::property::PropertyAccessResult> {
        let mapped_id = tsz_solver::mapped_type_id(self.ctx.types, mapped_type)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);

        let prop_atom = self.ctx.types.intern_string(prop_name);
        let cache_key = (mapped_type, prop_atom);

        if let Some(cached) = self
            .ctx
            .narrowing_cache
            .property_cache
            .borrow()
            .get(&cache_key)
            .copied()
        {
            return Some(match cached {
                Some(type_id) => tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id,
                    write_type: None,
                    from_index_signature: false,
                },
                None => tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                    type_id: mapped_type,
                    property_name: prop_atom,
                },
            });
        }

        let constraint = self.evaluate_mapped_constraint_with_resolution(mapped.constraint);
        if let Some(property_type) =
            crate::query_boundaries::state::checking::get_finite_mapped_property_type(
                self.ctx.types,
                mapped_id,
                prop_name,
            )
        {
            self.ctx
                .narrowing_cache
                .property_cache
                .borrow_mut()
                .insert(cache_key, Some(property_type));
            return Some(
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id: property_type,
                    write_type: None,
                    from_index_signature: false,
                },
            );
        }

        if let Some(names) =
            crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                self.ctx.types,
                mapped_id,
            )
        {
            if !names.contains(&prop_atom) {
                self.ctx
                    .narrowing_cache
                    .property_cache
                    .borrow_mut()
                    .insert(cache_key, None);
            }
            if !names.contains(&prop_atom) {
                return Some(
                    tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                        type_id: mapped_type,
                        property_name: prop_atom,
                    },
                );
            }
        }

        if mapped.name_type.is_some() {
            return None;
        }

        let mut matching_source_keys = Vec::new();

        // If the constraint is an explicit literal key set, reject unknown keys early.
        // For non-literal/complex constraints, fall back to full expansion.
        if !query::is_string_type(self.ctx.types, constraint) {
            let keys = query::extract_string_literal_keys(self.ctx.types, constraint);
            if !keys.is_empty() && keys.contains(&prop_atom) {
                matching_source_keys.push(prop_atom);
            }
            if !keys.is_empty() && matching_source_keys.is_empty() {
                self.ctx
                    .narrowing_cache
                    .property_cache
                    .borrow_mut()
                    .insert(cache_key, None);
                return Some(
                    tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                        type_id: mapped_type,
                        property_name: prop_atom,
                    },
                );
            }
            if keys.is_empty() {
                if let Some(keyof_target) = query::keyof_target(self.ctx.types, mapped.constraint)
                    .or_else(|| query::keyof_target(self.ctx.types, constraint))
                {
                    if matches!(
                        self.resolve_property_access_with_env(keyof_target, prop_name),
                        tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                    ) {
                        // `keyof T`-driven mapped types like Readonly<T> preserve
                        // the property surface of T, even when the key set isn't
                        // reducible to string literals. Keep going and instantiate
                        // the template for the requested property.
                    } else {
                        self.ctx
                            .narrowing_cache
                            .property_cache
                            .borrow_mut()
                            .insert(cache_key, None);
                        return Some(
                            tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                                type_id: mapped_type,
                                property_name: prop_atom,
                            },
                        );
                    }
                } else if !self.mapped_constraint_accepts_property_name(constraint, prop_name) {
                    self.ctx
                        .narrowing_cache
                        .property_cache
                        .borrow_mut()
                        .insert(cache_key, None);
                    return Some(
                        tsz_solver::operations::property::PropertyAccessResult::PropertyNotFound {
                            type_id: mapped_type,
                            property_name: prop_atom,
                        },
                    );
                } else {
                    // Broad key spaces like `any` or `keyof any` accept
                    // arbitrary string/numeric property names even when we
                    // cannot enumerate a finite literal key set here.
                }
            }
        }

        if matching_source_keys.is_empty() {
            matching_source_keys.push(prop_atom);
        }

        let mut property_types = Vec::new();
        for source_key_atom in matching_source_keys {
            let property_type =
                self.instantiate_mapped_property_template_with_env(&mapped, source_key_atom);
            let property_type = match mapped.optional_modifier {
                Some(tsz_solver::MappedModifier::Add) => self
                    .ctx
                    .types
                    .factory()
                    .union(vec![property_type, TypeId::UNDEFINED]),
                Some(tsz_solver::MappedModifier::Remove) | None => property_type,
            };
            property_types.push(property_type);
        }

        let property_type = match property_types.len() {
            0 => return None,
            1 => property_types[0],
            _ => self.ctx.types.factory().union(property_types),
        };

        self.ctx
            .narrowing_cache
            .property_cache
            .borrow_mut()
            .insert(cache_key, Some(property_type));

        Some(
            tsz_solver::operations::property::PropertyAccessResult::Success {
                type_id: property_type,
                write_type: None,
                from_index_signature: false,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::check_source_diagnostics;
    use crate::{
        context::CheckerOptions, query_boundaries::type_construction::TypeInterner,
        state::CheckerState,
    };
    use tsz_binder::BinderState;
    use tsz_parser::parser::node::NodeArena;
    use tsz_parser::parser::{NodeIndex, ParserState, syntax_kind_ext};

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
            "expected zero errors for MyReadonly<P> property access with && guard, got: {:?}",
            relevant
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

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
            "expected zero errors for type param property access via constraint, got: {:?}",
            relevant
                .iter()
                .map(|d| (d.code, &d.message_text))
                .collect::<Vec<_>>()
        );
    }

    fn build_checker(source: &str) -> (ParserState, NodeIndex, BinderState, TypeInterner) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        (parser, root, binder, types)
    }

    fn find_node_by_text_and_kind(
        arena: &NodeArena,
        source: &str,
        kind: u16,
        text: &str,
    ) -> Option<NodeIndex> {
        (0..arena.len()).find_map(|i| {
            let idx = NodeIndex(i as u32);
            let node = arena.get(idx)?;
            (node.kind == kind && &source[node.pos as usize..node.end as usize] == text)
                .then_some(idx)
        })
    }

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
}
