impl<'a> CheckerState<'a> {
    /// Walk heritage clauses to find a generator-like base and extract a type argument at `arg_index`.
    ///
    /// Heritage types are `ExpressionWithTypeArguments` nodes (e.g., `Iterator<0, 1, 2>`).
    /// We check syntactically if the heritage expression names a generator-like type,
    /// then extract the type argument at the requested index using `get_type_from_type_node`.
    ///
    /// When a type extends multiple generator-like bases at the same level
    /// (e.g. `extends Iterator<number>, Iterable<string>`), tsc derives the
    /// iteration type from `[Symbol.iterator]()`, which only exists on the
    /// Iterable-family side. We mirror that by collecting all direct matches
    /// and returning the highest-priority one (see
    /// `generator_like_priority`), falling back to transitive heritage only
    /// when no direct generator-like base is present.
    fn find_generator_arg_in_heritage(
        &mut self,
        heritage_clauses: &Option<tsz_parser::parser::base::NodeList>,
        arg_index: usize,
        depth: u32,
    ) -> Option<TypeId> {
        let heritage_clauses = heritage_clauses.as_ref()?;

        let mut best_direct: Option<(u8, Option<TypeId>)> = None;

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Heritage types are ExpressionWithTypeArguments nodes
                let (expr_idx, type_arguments) =
                    if let Some(expr_data) = self.ctx.arena.get_expr_type_args(type_node) {
                        (expr_data.expression, expr_data.type_arguments.clone())
                    } else {
                        (type_idx, None)
                    };

                // Check if the base expression names a generator-like type
                let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                    continue;
                };
                if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                    let priority = Self::generator_like_priority(&ident.escaped_text);
                    if priority > 0 {
                        // Direct generator-like heritage — record a candidate
                        // for this arg_index. Higher priority wins; ties
                        // keep the earliest match (source order).
                        if best_direct.is_none_or(|(p, _)| priority > p) {
                            let extracted = if let Some(type_args) = &type_arguments {
                                if arg_index < type_args.nodes.len() {
                                    Some(self.get_type_from_type_node(type_args.nodes[arg_index]))
                                } else if arg_index == 1 && type_args.nodes.len() == 1 {
                                    // Single type arg: TReturn defaults to `any`.
                                    Some(TypeId::ANY)
                                } else {
                                    // Generator-like but missing the requested arg.
                                    None
                                }
                            } else {
                                None
                            };
                            best_direct = Some((priority, extracted));
                        }
                        continue;
                    }
                }
            }
        }

        if let Some((_, extracted)) = best_direct {
            return extracted;
        }

        // No direct generator-like heritage at this level — recurse through
        // transitive heritage as a fallback.
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                let expr_idx = if let Some(expr_data) = self.ctx.arena.get_expr_type_args(type_node)
                {
                    expr_data.expression
                } else {
                    type_idx
                };

                let heritage_base_type = self.get_type_of_node(expr_idx);
                if heritage_base_type != TypeId::ERROR
                    && let Some(result) = self.resolve_generator_arg_from_heritage(
                        heritage_base_type,
                        arg_index,
                        depth + 1,
                    )
                {
                    return Some(result);
                }
            }
        }

        None
    }

    /// Unwrap Promise<T> to T for async function return type checking.
    ///
    /// For async functions with declared return type `Promise<T>`, the function body
    /// should return values of type `T` (which get auto-wrapped in Promise).
    /// This function extracts T from Promise<T>.
    ///
    /// Returns None if the type is not a Promise type or if T cannot be extracted.
    pub fn unwrap_promise_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.promise_like_return_type_argument(type_id)
    }

    /// Unwrap Promise members from an async function's return type for body checking.
    /// `Awaited<X>` payloads are evaluated so the body sees the same awaited
    /// structural type tsc uses instead of the raw alias application.
    pub fn unwrap_async_return_type_for_body(&mut self, return_type: TypeId) -> TypeId {
        // Try simple unwrap first
        if let Some(unwrapped) = self.unwrap_promise_type(return_type) {
            return self.evaluate_awaited_application(unwrapped);
        }
        // For unions, unwrap each Promise member individually
        if let Some(members) = query::union_members(self.ctx.types, return_type) {
            let mut new_members: Vec<TypeId> = Vec::new();
            for member in &members {
                if let Some(unwrapped) = self.unwrap_promise_type(*member) {
                    new_members.push(self.evaluate_awaited_application(unwrapped));
                } else {
                    new_members.push(*member);
                }
            }
            return self.ctx.types.factory().union(new_members);
        }
        return_type
    }

    /// If `type_id` is an `Awaited<X>` application, evaluate it through the
    /// conditional-type machinery (which folds it to `X` for non-thenable X);
    /// otherwise return `type_id` unchanged. Other generic applications (e.g.
    /// `Box<T>`, `Partial<T>`) keep their alias-form display, matching tsc's
    /// preference for the named alias when one is in scope.
    fn evaluate_awaited_application(&mut self, type_id: TypeId) -> TypeId {
        if self.is_awaited_application(type_id) {
            self.evaluate_application_type(type_id)
        } else {
            type_id
        }
    }

    /// Check that `Generator<TYield, any, any>` (or `AsyncGenerator`) is assignable
    /// to the declared return type of an annotated generator function.
    ///
    /// This catches cases like `function* g(): WeirdIter {}` where `WeirdIter`
    /// extends `IterableIterator` with extra properties that `Generator<>` lacks.
    pub fn check_generator_return_type_assignability(
        &mut self,
        is_async: bool,
        yield_type: Option<TypeId>,
        declared_return_type: TypeId,
        error_node: NodeIndex,
    ) {
        if declared_return_type == TypeId::ANY
            || declared_return_type == TypeId::ERROR
            || declared_return_type == TypeId::VOID
            || self.type_contains_error(declared_return_type)
        {
            return;
        }
        // Direct standard iterator/generator return annotations are already handled
        // by body-level `return`/`yield` checking. The extra whole-signature
        // assignability check is only needed for custom iterator-like types
        // that add requirements beyond the standard library contracts.
        if let Some(type_ref) = self
            .ctx
            .arena
            .get(error_node)
            .and_then(|node| self.ctx.arena.get_type_ref(node))
            && let Some(name) = self.node_text(type_ref.type_name)
            && Self::is_generator_like_name(&name)
        {
            return;
        }
        // Skip the check for interfaces that extend a single generator-like type
        // and have no own body members. Such interfaces are trivially satisfied by
        // Generator<>. But when the interface has MULTIPLE heritage clauses that
        // could conflict (e.g., `BadGenerator extends Iterator<number>, Iterable<string> {}`),
        // we must still check because Generator<> may not satisfy the combined requirements.
        if self
            .get_generator_return_type_argument(declared_return_type)
            .is_some()
        {
            let def_id =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, declared_return_type);
            let sym_id = def_id.and_then(|d| self.ctx.def_to_symbol_id(d));
            let should_skip = sym_id
                .and_then(|s| {
                    let symbol = self.get_symbol_globally(s)?;
                    let declarations = symbol.declarations.clone();
                    Some(declarations.iter().all(|decl_idx| {
                        self.ctx
                            .arena
                            .get(*decl_idx)
                            .and_then(|node| self.ctx.arena.get_interface(node))
                            .is_some_and(|iface| {
                                // Skip only if the interface has no own body members AND
                                // extends at most one type (no conflicting heritage).
                                // E.g., `extends Iterator<0, 1, 2>` (1 type) is safe to skip,
                                // but `extends Iterator<number>, Iterable<string>` (2 types) is not.
                                let has_own_members = !iface.members.nodes.is_empty();
                                let extends_type_count: usize = iface
                                    .heritage_clauses
                                    .as_ref()
                                    .map(|clauses| {
                                        clauses
                                            .nodes
                                            .iter()
                                            .filter_map(|&clause_idx| {
                                                self.ctx
                                                    .arena
                                                    .get(clause_idx)
                                                    .and_then(|n| {
                                                        self.ctx.arena.get_heritage_clause(n)
                                                    })
                                                    .map(|h| h.types.nodes.len())
                                            })
                                            .sum()
                                    })
                                    .unwrap_or(0);
                                !has_own_members && extends_type_count <= 1
                            })
                    }))
                })
                .unwrap_or(false);
            if should_skip {
                return;
            }
        }
        let gen_name = if is_async {
            "AsyncGenerator"
        } else {
            "Generator"
        };
        // Ensure the lib type is loaded, then get a Lazy(DefId) reference
        // so the type displays as `Generator<...>` in error messages.
        let _resolved = self.resolve_lib_type_by_name(gen_name);
        let lazy_base = self.ctx.binder.file_locals.get(gen_name).map(|sym_id| {
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx.types.factory().lazy(def_id)
        });
        if let Some(base) = lazy_base {
            let yield_t = yield_type.unwrap_or(TypeId::ANY);
            // TNext defaults to `unknown` when the declared return type has no
            // extractable TYield (i.e., isn't Generator-like). This matches tsc:
            // `function* g(): number {}` reports `Generator<any, any, unknown>`.
            // When the declared return type IS Generator-like (provides TYield),
            // tsc uses `any` for TNext — e.g. `function* g(): BadGenerator {}`
            // (heritage Iterator<number>, Iterable<string>) reports
            // `Generator<string, any, any>`.
            let next_t = if yield_type.is_some() {
                TypeId::ANY
            } else {
                TypeId::UNKNOWN
            };
            let inferred_gen = self
                .ctx
                .types
                .factory()
                .application(base, vec![yield_t, TypeId::ANY, next_t]);
            self.ensure_relation_input_ready(inferred_gen);
            self.ensure_relation_input_ready(declared_return_type);
            self.check_assignable_or_report(inferred_gen, declared_return_type, error_node);
        }
    }
}
