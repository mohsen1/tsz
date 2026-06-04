impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(crate) fn rest_tuple_inference_target(
        &mut self,
        params: &[ParamInfo],
        arg_types: &[TypeId],
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> Option<(usize, TypeId, TypeId)> {
        let rest_param = params.last().filter(|param| param.rest)?;
        let rest_start = params.len().saturating_sub(1);

        let rest_param_type = self.unwrap_readonly(rest_param.type_id);
        let target = match self.interner.lookup(rest_param_type) {
            Some(TypeData::TypeParameter(_)) if var_map.contains_key(&rest_param_type) => {
                Some((rest_start, rest_param_type, 0))
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                // Two or more adjacent variadic type parameters (`...args: [...A, ...B]`)
                // cannot be split without an implied arity, which a tuple-typed rest
                // parameter never has (tsc's `getNonArrayRestType` returns `undefined`
                // for it). tsc infers nothing here, leaving `A`/`B` to fall back to
                // their constraints — so bail out of the single-variadic slicing below
                // rather than mis-distributing the arguments.
                let infer_var_rest_count = elements
                    .iter()
                    .filter(|elem| elem.rest && var_map.contains_key(&elem.type_id))
                    .count();
                if infer_var_rest_count >= 2 {
                    return None;
                }
                elements.iter().enumerate().find_map(|(i, elem)| {
                    if !elem.rest {
                        return None;
                    }
                    if !var_map.contains_key(&elem.type_id) {
                        return None;
                    }

                    // Count trailing elements after the variadic part, but allow optional
                    // tail elements to be omitted when they don't match.
                    let tail = &elements[i + 1..];
                    let min_index = rest_start + i;
                    let mut trailing_count = 0usize;
                    let mut arg_index = arg_types.len();
                    for tail_elem in tail.iter().rev() {
                        if arg_index <= min_index {
                            break;
                        }
                        let arg_type = arg_types[arg_index - 1];
                        let assignable = self.checker.is_assignable_to(arg_type, tail_elem.type_id);
                        if tail_elem.optional && !assignable {
                            break;
                        }
                        trailing_count += 1;
                        arg_index -= 1;
                    }
                    Some((rest_start + i, elem.type_id, trailing_count))
                })
            }
            // Application rest param: e.g., `...args: TupleMapper<Tuple>` where Tuple
            // is an inference variable and TupleMapper is a mapped type alias.
            // Pack rest args into a tuple and constrain against the Application.
            // The constraint solver's (_, Application) handler will expand the alias
            // to its mapped type body, enabling reverse-mapped tuple inference.
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                let has_infer_arg = app.args.iter().any(|arg| var_map.contains_key(arg));
                let has_spread_marker_arg = arg_types[rest_start..]
                    .iter()
                    .any(|&arg| self.spread_argument_marker_inner(arg).is_some());
                let evaluated_rest_type = self.evaluate_rest_param_type(rest_param_type);
                if self.rest_type_needs_aggregate_argument_check(evaluated_rest_type)
                    && !has_spread_marker_arg
                {
                    return None;
                }
                if has_infer_arg {
                    Some((rest_start, rest_param_type, 0))
                } else {
                    None
                }
            }
            _ => None,
        }?;

        let (start_index, target_type, trailing_count) = target;
        if start_index >= arg_types.len() {
            return None;
        }

        // Extract the arguments that should be inferred for the variadic type parameter,
        // excluding both prefix fixed elements and trailing fixed elements.
        // For example, for `...args: [number, ...T, boolean]` with call `foo(1, 'a', 'b', true)`:
        //   - rest_start = 0 (rest param index)
        //   - start_index = 1 (after the prefix `number`)
        //   - trailing_count = 1 (the trailing `boolean`)
        //   - we should infer T from ['a', 'b'], not [1, 'a', 'b', true]
        //
        // The variadic arguments start at start_index and end before trailing elements.
        let end_index = arg_types.len().saturating_sub(trailing_count);
        let tuple_elements: Vec<TupleElement> = if start_index < end_index {
            arg_types[start_index..end_index]
                .iter()
                .flat_map(|&ty| {
                    if let Some(inner) = self.spread_argument_marker_inner(ty) {
                        return vec![TupleElement {
                            type_id: inner,
                            name: None,
                            optional: false,
                            rest: true,
                        }];
                    }
                    // Recognize spread marker tuples [...T] from the checker.
                    // Only match markers whose inner type is a TypeParameter.
                    if let Some(TypeData::Tuple(elems_id)) = self.interner.lookup(ty) {
                        let elems = self.interner.tuple_list(elems_id);
                        if elems.len() == 1
                            && elems[0].rest
                            && matches!(
                                self.interner.lookup(elems[0].type_id),
                                Some(TypeData::TypeParameter(_))
                            )
                        {
                            return elems.to_vec();
                        }
                    }
                    vec![TupleElement {
                        type_id: ty,
                        name: None,
                        optional: false,
                        rest: false,
                    }]
                })
                .collect()
        } else {
            Vec::new()
        };
        // When all elements are rest-spread type parameters (e.g., [...U] from a
        // single spread argument), use the inner type directly rather than wrapping
        // in another tuple.  This ensures `f(...u)` where `u: U extends string[]`
        // constrains `T = U` (not `T = [U]`) against `...args: T`.
        if tuple_elements.len() == 1 && tuple_elements[0].rest {
            return Some((start_index, target_type, tuple_elements[0].type_id));
        }
        Some((
            start_index,
            target_type,
            self.interner.tuple(tuple_elements),
        ))
    }

    /// Check if a type evaluates to or contains a function type.
    /// This includes:
    /// - Direct Function or Callable types
    /// - Union/intersection members that evaluate to functions
    /// - Aliases/applications that only become callable after evaluation
    pub(crate) fn type_evaluates_to_function(&self, type_id: TypeId) -> bool {
        with_evaluates_visited(|visited| self.type_evaluates_to_function_inner(type_id, visited))
    }

    pub(crate) fn should_directly_constrain_same_base_application(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let evaluated_source = self.checker.evaluate_type(source);
        let evaluated_target = self.checker.evaluate_type(target);
        !self.type_evaluates_to_function(evaluated_source)
            && !self.type_evaluates_to_function(evaluated_target)
    }

    fn type_evaluates_to_function_inner(
        &self,
        type_id: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(type_id) {
            return false;
        }

        // Intrinsics never evaluate to Function/Callable.
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.type_evaluates_to_function_inner(member, visited)),
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                self.type_evaluates_to_function_inner(inner, visited)
            }
            _ => {
                let evaluated = self.interner.evaluate_type(type_id);
                evaluated != type_id && self.type_evaluates_to_function_inner(evaluated, visited)
            }
        }
    }

    /// Check if an arg type contains `TypeParameter`s whose names match the
    /// caller's type parameter names (from the substitution). This detects when the
    /// checker's union-contextual pass leaked unresolved type parameters from overload
    /// signatures into arg types.
    pub(crate) fn arg_contains_callers_type_params(
        &self,
        arg_type: TypeId,
        substitution: &crate::instantiation::instantiate::TypeSubstitution,
    ) -> bool {
        if substitution.map().is_empty() {
            return false;
        }
        if arg_type.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(arg_type) {
            // Function types: check parameter types (most common leak path via callbacks).
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                shape.params.iter().any(|param| {
                    self.type_references_substitution_keys(param.type_id, substitution)
                })
            }
            // Application types (e.g. Op<A, string> where A is the caller's type param)
            // also carry caller TypeParameters in their type args.
            Some(TypeData::Application(_)) => {
                self.type_references_substitution_keys(arg_type, substitution)
            }
            _ => false,
        }
    }

    #[inline]
    pub(crate) fn type_contains_placeholder(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if var_map.contains_key(&ty) {
            return true;
        }
        // Fast path: intrinsic types (primitives, never, any, etc.) never contain placeholders
        if ty.is_intrinsic() {
            return false;
        }
        if !visited.insert(ty) {
            return false;
        }

        let key = match self.interner.lookup(ty) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeData::Array(elem) => self.type_contains_placeholder(elem, var_map, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.type_contains_placeholder(elem.type_id, var_map, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_placeholder(member, var_map, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
                    || shape.string_index.as_ref().is_some_and(|idx| {
                        self.type_contains_placeholder(idx.key_type, var_map, visited)
                            || self.type_contains_placeholder(idx.value_type, var_map, visited)
                    })
                    || shape.number_index.as_ref().is_some_and(|idx| {
                        self.type_contains_placeholder(idx.key_type, var_map, visited)
                            || self.type_contains_placeholder(idx.value_type, var_map, visited)
                    })
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_placeholder(app.base, var_map, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_placeholder(arg, var_map, visited))
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                shape.type_params.iter().any(|tp| {
                    tp.constraint.is_some_and(|constraint| {
                        self.type_contains_placeholder(constraint, var_map, visited)
                    }) || tp.default.is_some_and(|default| {
                        self.type_contains_placeholder(default, var_map, visited)
                    })
                }) || shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_placeholder(param.type_id, var_map, visited))
                    || shape.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    })
                    || self.type_contains_placeholder(shape.return_type, var_map, visited)
                    || shape.type_predicate.as_ref().is_some_and(|pred| {
                        pred.type_id
                            .is_some_and(|ty| self.type_contains_placeholder(ty, var_map, visited))
                    })
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let in_call = shape.call_signatures.iter().any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_placeholder(constraint, var_map, visited)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_placeholder(default, var_map, visited)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_placeholder(param.type_id, var_map, visited)
                    }) || sig.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    }) || self.type_contains_placeholder(sig.return_type, var_map, visited)
                        || sig.type_predicate.as_ref().is_some_and(|pred| {
                            pred.type_id.is_some_and(|ty| {
                                self.type_contains_placeholder(ty, var_map, visited)
                            })
                        })
                });
                if in_call {
                    return true;
                }
                let in_construct = shape.construct_signatures.iter().any(|sig| {
                    sig.type_params.iter().any(|tp| {
                        tp.constraint.is_some_and(|constraint| {
                            self.type_contains_placeholder(constraint, var_map, visited)
                        }) || tp.default.is_some_and(|default| {
                            self.type_contains_placeholder(default, var_map, visited)
                        })
                    }) || sig.params.iter().any(|param| {
                        self.type_contains_placeholder(param.type_id, var_map, visited)
                    }) || sig.this_type.is_some_and(|this_type| {
                        self.type_contains_placeholder(this_type, var_map, visited)
                    }) || self.type_contains_placeholder(sig.return_type, var_map, visited)
                        || sig.type_predicate.as_ref().is_some_and(|pred| {
                            pred.type_id.is_some_and(|ty| {
                                self.type_contains_placeholder(ty, var_map, visited)
                            })
                        })
                });
                if in_construct {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_placeholder(prop.type_id, var_map, visited))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.type_contains_placeholder(cond.check_type, var_map, visited)
                    || self.type_contains_placeholder(cond.extends_type, var_map, visited)
                    || self.type_contains_placeholder(cond.true_type, var_map, visited)
                    || self.type_contains_placeholder(cond.false_type, var_map, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                mapped.type_param.constraint.is_some_and(|constraint| {
                    self.type_contains_placeholder(constraint, var_map, visited)
                }) || mapped.type_param.default.is_some_and(|default| {
                    self.type_contains_placeholder(default, var_map, visited)
                }) || self.type_contains_placeholder(mapped.constraint, var_map, visited)
                    || self.type_contains_placeholder(mapped.template, var_map, visited)
            }
            TypeData::IndexAccess(obj, idx) => {
                self.type_contains_placeholder(obj, var_map, visited)
                    || self.type_contains_placeholder(idx, var_map, visited)
            }
            TypeData::KeyOf(operand)
            | TypeData::ReadonlyType(operand)
            | TypeData::NoInfer(operand) => {
                self.type_contains_placeholder(operand, var_map, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => {
                        self.type_contains_placeholder(*inner, var_map, visited)
                    }
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_placeholder(type_arg, var_map, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                self.type_contains_placeholder(member_type, var_map, visited)
            }
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Error => false,
        }
    }

    /// Check if a type is contextually sensitive (requires contextual typing for inference).
    ///
    /// Contextually sensitive types include:
    /// - Function types (lambda expressions)
    /// - Callable types (object with call signatures)
    /// - Union/Intersection types containing contextually sensitive members
    /// - Object literals with callable properties (methods)
    ///
    /// These types need deferred inference in Round 2 after non-contextual
    /// arguments have been processed and type variables have been fixed.
    pub(crate) fn is_contextually_sensitive(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        // Check memoization cache to avoid exponential re-traversal on deeply
        // nested type structures (e.g., Application chains where each level
        // references the previous type multiple times via keyof).
        if let Some(&cached) = self.contextual_sensitivity_cache.borrow().get(&type_id) {
            return cached;
        }
        let result = self.is_contextually_sensitive_inner(type_id);
        self.contextual_sensitivity_cache
            .borrow_mut()
            .insert(type_id, result);
        result
    }

    fn is_contextually_sensitive_inner(&self, type_id: TypeId) -> bool {
        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return false,
        };

        match key {
            // Function types are contextually sensitive only when one of their
            // parameter types still needs contextual typing (has `any` type or
            // inference placeholder). Fully annotated function arguments --
            // including generic function references like `id<T>(x: T) => T` --
            // should participate in Round 1 generic inference.
            //
            // In tsc, contextual sensitivity is an AST-level check
            // (isContextSensitive) that looks at whether the expression is a
            // function expression/arrow with unannotated parameters. A simple
            // identifier referring to a generic function is NOT contextually
            // sensitive. We approximate this by only checking parameter types
            // for placeholder/any markers, not the presence of type_params.
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                self.function_signature_is_contextually_sensitive(&shape.params)
            }
            // Union/Intersection: contextually sensitive if any member is
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.is_contextually_sensitive(member))
            }

            // Object types: only fresh object literals can be contextually sensitive.
            // Non-fresh objects (class instances, evaluated generic types like Set<T>)
            // are never contextually sensitive — their types are already determined.
            // This matches tsc's isContextSensitive which checks the AST expression,
            // not the type: variable references are never contextually sensitive.
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .flags
                    .contains(crate::types::ObjectFlags::FRESH_LITERAL)
                    && (shape.all_properties_context_sensitive()
                        || shape
                            .properties
                            .iter()
                            .any(|prop| self.is_contextually_sensitive(prop.type_id)))
            }

            // Array types: check element type
            TypeData::Array(elem) => self.is_contextually_sensitive(elem),

            // Tuple types: check all elements
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.is_contextually_sensitive(elem.type_id))
            }

            // Type applications: check base and arguments
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.is_contextually_sensitive(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.is_contextually_sensitive(arg))
            }

            // Readonly types: look through to inner type
            TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.is_contextually_sensitive(inner)
            }

            // Type parameters with constraints: check constraint
            TypeData::TypeParameter(info) | TypeData::Infer(info) => info
                .constraint
                .is_some_and(|constraint| self.is_contextually_sensitive(constraint)),

            // Index access: check both object and key types
            TypeData::IndexAccess(obj, key) => {
                self.is_contextually_sensitive(obj) || self.is_contextually_sensitive(key)
            }

            // Conditional types: check all branches
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.is_contextually_sensitive(cond.check_type)
                    || self.is_contextually_sensitive(cond.extends_type)
                    || self.is_contextually_sensitive(cond.true_type)
                    || self.is_contextually_sensitive(cond.false_type)
            }

            // Mapped types: check constraint and template
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                self.is_contextually_sensitive(mapped.constraint)
                    || self.is_contextually_sensitive(mapped.template)
            }

            // KeyOf, StringIntrinsic: check operand
            TypeData::KeyOf(operand)
            | TypeData::StringIntrinsic {
                type_arg: operand, ..
            } => self.is_contextually_sensitive(operand),

            // Enum types: check member type
            TypeData::Enum(_def_id, member_type) => self.is_contextually_sensitive(member_type),

            // Template literals: check type spans
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.is_contextually_sensitive(*inner),
                })
            }

            // Non-contextually sensitive types (Callable = class constructor values)
            TypeData::Callable(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Error => false,
        }
    }
}
