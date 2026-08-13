//! Destructuring pattern type resolution and validation.

use crate::context::TypingRequest;
use crate::query_boundaries::binding_patterns;
use crate::query_boundaries::common as common_query;
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Returns the tsc apparent-type display name used in destructuring TS2339
/// messages (e.g. `string` → `String`, `object` → `{}`). Returns `None` for
/// types that use their regular diagnostic formatting.
fn apparent_type_display_for_destructuring(type_id: TypeId) -> Option<String> {
    match type_id {
        TypeId::OBJECT => Some("{}".to_string()),
        TypeId::STRING => Some("String".to_string()),
        TypeId::NUMBER => Some("Number".to_string()),
        TypeId::BOOLEAN => Some("Boolean".to_string()),
        TypeId::BIGINT => Some("BigInt".to_string()),
        TypeId::SYMBOL => Some("Symbol".to_string()),
        _ => None,
    }
}

impl<'a> CheckerState<'a> {
    fn report_unknown_empty_binding_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) {
        if parent_type != TypeId::UNKNOWN {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        if !pattern_data.elements.nodes.is_empty() {
            return;
        }

        self.error_at_node(
            pattern_idx,
            "Object is of type 'unknown'.",
            crate::diagnostics::diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
        );
    }

    /// The object-literal expression that feeds a binding *element* whose own
    /// pattern sits inside an enclosing pattern: resolve the enclosing
    /// pattern's literal source, then pick the property value matching the
    /// element's name. Recursive, so arbitrarily nested patterns resolve as
    /// long as every level is fed by a written object literal.
    fn nested_pattern_object_literal_source(
        &self,
        binding_element_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let element_node = self.ctx.arena.get(binding_element_idx)?;
        let be = self.ctx.arena.get_binding_element(element_node)?;
        let enclosing_pattern_idx = self.ctx.arena.get_extended(binding_element_idx)?.parent;
        let enclosing_source = self.object_literal_source_for_pattern(enclosing_pattern_idx)?;
        let name_idx = if be.property_name.is_some() {
            be.property_name
        } else {
            be.name
        };
        let name = &self.ctx.arena.get_identifier_at(name_idx)?.escaped_text;
        let literal_node = self.ctx.arena.get(enclosing_source)?;
        let literal = self.ctx.arena.get_literal_expr(literal_node)?;
        for &member_idx in &literal.elements.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_assignment(member_node) else {
                continue;
            };
            if self
                .ctx
                .arena
                .get_identifier_at(prop.name)
                .is_some_and(|ident| &ident.escaped_text == name)
            {
                let value = self.ctx.arena.skip_parenthesized(prop.initializer);
                if self
                    .ctx
                    .arena
                    .get(value)
                    .is_some_and(|n| n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
                {
                    return Some(value);
                }
                return None;
            }
        }
        None
    }

    /// The written object-literal source of a binding pattern, if any:
    /// a parameter default, a variable initializer (no annotation), an outer
    /// element's own default, or — recursively — the matching property value
    /// of the enclosing pattern's literal source.
    fn object_literal_source_for_pattern(&self, pattern_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let parent_idx = self.ctx.arena.get_extended(pattern_idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        let source = match parent_node.kind {
            syntax_kind_ext::PARAMETER => {
                let param = self.ctx.arena.get_parameter(parent_node)?;
                if param.name != pattern_idx
                    || param.type_annotation.is_some()
                    || param.initializer.is_none()
                {
                    return None;
                }
                param.initializer
            }
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl = self.ctx.arena.get_variable_declaration(parent_node)?;
                if decl.name != pattern_idx
                    || decl.type_annotation.is_some()
                    || decl.initializer.is_none()
                {
                    return None;
                }
                decl.initializer
            }
            syntax_kind_ext::BINDING_ELEMENT => {
                let be = self.ctx.arena.get_binding_element(parent_node)?;
                if be.name != pattern_idx {
                    return None;
                }
                if be.initializer.is_some() {
                    be.initializer
                } else {
                    return self.nested_pattern_object_literal_source(parent_idx);
                }
            }
            _ => return None,
        };
        let source = self.ctx.arena.skip_parenthesized(source);
        self.ctx
            .arena
            .get(source)
            .is_some_and(|n| n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            .then_some(source)
    }

    fn should_suppress_missing_property_for_literal_default(
        &self,
        pattern_idx: NodeIndex,
        element_data: &tsz_parser::parser::node::BindingElementData,
        _request: &TypingRequest,
    ) -> bool {
        // Suppress TS2339 for missing properties in destructuring when:
        // - For parameters: the parameter has an object literal default. The
        //   parameter-level default/assignability checks own the error, so the
        //   binding pattern itself should not also report per-property TS2339
        //   (e.g., `function f({ a }: T = {}) {}`).
        // - For variable declarations and binding elements: the binding element has
        //   its own default initializer AND the source is an object literal
        //   (e.g., `const { a = 5 } = {}`). Without a default, tsc still reports
        //   TS2339 (e.g., `const { a } = {}` is an error).
        let element_has_initializer = element_data.initializer.is_some();

        let Some(ext) = self.ctx.arena.get_extended(pattern_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        let source_expr = match parent_node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let Some(decl) = self.ctx.arena.get_variable_declaration(parent_node) else {
                    return false;
                };
                if decl.name != pattern_idx || decl.type_annotation.is_some() {
                    return false;
                }
                // Variable declarations require the element to have its own default.
                // `const { a } = {}` is TS2339, but `const { a = 5 } = {}` is OK.
                if !element_has_initializer {
                    return false;
                }
                // If the variable declaration has no initializer, it may be
                // in a for-of statement where the type comes from the iterable.
                // Check if the for-of expression contains object literals.
                if decl.initializer.is_none() {
                    return self.is_for_of_with_object_literal_elements(parent_idx);
                }
                decl.initializer
            }
            syntax_kind_ext::PARAMETER => {
                let Some(param) = self.ctx.arena.get_parameter(parent_node) else {
                    return false;
                };
                if param.name != pattern_idx {
                    return false;
                }
                param.initializer
            }
            // Nested destructuring: `{ event: { params = {} } = {} }` — the inner
            // ObjectBindingPattern's parent is the outer BindingElement.  When that
            // BindingElement has an object-literal default, suppress TS2339 for the
            // inner pattern's properties only when they have their own defaults.
            syntax_kind_ext::BINDING_ELEMENT => {
                let Some(be) = self.ctx.arena.get_binding_element(parent_node) else {
                    return false;
                };
                if be.name != pattern_idx {
                    return false;
                }
                if !element_has_initializer {
                    return false;
                }
                if be.initializer.is_some() {
                    be.initializer
                } else {
                    // The outer element has no default of its own; the inner
                    // pattern's source is the matching property value of the
                    // enclosing pattern's object-literal source:
                    // `{ a: { x = 0 } } = { a: {} }` reads `x` from the `{}`
                    // written for `a`. tsc reaches the same answer through
                    // `AccessFlags.AllowMissing` on a type still carrying
                    // `ObjectFlags.ObjectLiteral` (widening strips it, so an
                    // annotated or variable-typed parent stays an error).
                    let Some(source) = self.nested_pattern_object_literal_source(parent_idx) else {
                        return false;
                    };
                    source
                }
            }
            _ => return false,
        };

        let source_expr = self.ctx.arena.skip_parenthesized(source_expr);
        self.ctx
            .arena
            .get(source_expr)
            .is_some_and(|expr| expr.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
    }

    /// Check if a variable declaration is inside a for-of statement whose
    /// iterable expression is an array literal containing object literals.
    /// This handles `for (let {x = default} of [{}])` where the iteration
    /// element type is `{}` from an object literal, matching tsc behavior
    /// that suppresses TS2339 for missing properties with defaults.
    fn is_for_of_with_object_literal_elements(&self, var_decl_idx: NodeIndex) -> bool {
        // Walk up: VariableDeclaration -> VariableDeclarationList -> ForOfStatement
        let Some(decl_ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let decl_list_idx = decl_ext.parent;
        let Some(list_ext) = self.ctx.arena.get_extended(decl_list_idx) else {
            return false;
        };
        let for_stmt_idx = list_ext.parent;
        let Some(for_stmt_node) = self.ctx.arena.get(for_stmt_idx) else {
            return false;
        };
        if for_stmt_node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return false;
        }
        let Some(for_data) = self.ctx.arena.get_for_in_of(for_stmt_node) else {
            return false;
        };
        // Check if the iterable expression is an array literal
        let expr_idx = self.ctx.arena.skip_parenthesized(for_data.expression);
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return false;
        }
        // Check if at least one element in the array is an object literal
        let Some(arr_data) = self.ctx.arena.get_literal_expr(expr_node) else {
            return false;
        };
        arr_data.elements.nodes.iter().any(|&elem_idx| {
            let elem_idx = self.ctx.arena.skip_parenthesized(elem_idx);
            self.ctx
                .arena
                .get(elem_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        })
    }

    /// Returns true when the given binding pattern is the name of a function
    /// parameter that has a default initializer (e.g. `function f([x, y] = [])
    /// {}`). In that case tsc does not emit TS2493 for out-of-bounds element
    /// access into the inferred default tuple — the binding elements become
    /// implicitly any (TS7031) instead.
    fn binding_pattern_in_parameter_with_default(&self, pattern_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(pattern_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::PARAMETER {
            return false;
        }
        self.ctx
            .arena
            .get_parameter(parent_node)
            .is_some_and(|param| param.initializer.is_some())
    }

    /// Whether a (possibly nested) binding pattern belongs to a `const`
    /// declaration. Mirrors tsc's use of `getCombinedNodeFlags & Constant` in
    /// `widenTypeInferredFromInitializer`: a binding element's default keeps its
    /// literal type only under `const`/`readonly`; `let`/`var`/parameter
    /// defaults widen. Nested patterns (`const [[a = 0]] = ...`) inherit the
    /// outer declaration's const-ness, so we walk up through binding
    /// elements/patterns to the owning `VariableDeclaration`.
    fn binding_pattern_is_const_declaration(&self, pattern_idx: NodeIndex) -> bool {
        let mut current = pattern_idx;
        for _ in 0..16 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            match parent_node.kind {
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    return self.ctx.arena.is_const_variable_declaration(parent_idx);
                }
                k if k == syntax_kind_ext::PARAMETER => return false,
                k if k == syntax_kind_ext::BINDING_ELEMENT
                    || k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    || k == syntax_kind_ext::OBJECT_BINDING_PATTERN =>
                {
                    current = parent_idx;
                }
                _ => return false,
            }
        }
        false
    }

    pub(crate) fn normalize_parameter_binding_pattern_source_type(
        &self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) -> TypeId {
        if !self.ctx.strict_null_checks() || parent_type.is_any_unknown_or_error() {
            return parent_type;
        }

        let mut current = pattern_idx;
        let mut param_idx = NodeIndex::NONE;
        for _ in 0..6 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return parent_type;
            };
            current = ext.parent;
            if current.is_none() {
                return parent_type;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                return parent_type;
            };
            if node.kind == syntax_kind_ext::PARAMETER {
                param_idx = current;
                break;
            }
        }
        if param_idx.is_none() {
            return parent_type;
        }

        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return parent_type;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return parent_type;
        };
        if param.name != pattern_idx {
            return parent_type;
        }

        let mut function_idx = NodeIndex::NONE;
        for _ in 0..4 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if current.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if self.ctx.arena.get_function(node).is_some()
                || self.ctx.arena.get_method_decl(node).is_some()
                || self.ctx.arena.get_constructor(node).is_some()
            {
                function_idx = current;
                break;
            }
        }
        let jsdoc_optional = function_idx.is_some()
            && self.jsdoc_marks_parameter_optional(function_idx, param_idx, param.name);

        // Parameter omission handling belongs to the parameter itself.
        // For binding-pattern parameters with default initializers, tsc still
        // checks the destructuring site against the original source type, so
        // preserve `| undefined` there.
        if param.question_token || jsdoc_optional {
            flow_boundary::narrow_destructuring_default(self.ctx.types, parent_type, true)
        } else {
            parent_type
        }
    }

    /// Returns `true` when an array binding pattern's destructuring source is a
    /// fresh array-literal initializer (`var [a, ...rest] = [1, 2, 3]`).
    ///
    /// tsz keeps fresh array literals as un-widened literal tuples, so the
    /// rest-binding path uses this to decide widening (see the call site).
    ///
    /// Walks up through nested binding patterns/elements to the enclosing
    /// `VARIABLE_DECLARATION`. A `[...] as const` initializer is an
    /// `AS_EXPRESSION` (not an `ARRAY_LITERAL_EXPRESSION`), so it is not
    /// fresh; parameter sources (annotated tuple types) are never fresh.
    pub(crate) fn array_binding_source_is_fresh_array_literal(
        &self,
        pattern_idx: NodeIndex,
    ) -> bool {
        let mut current = pattern_idx;
        for _ in 0..64 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            let kind = parent_node.kind;
            if kind == syntax_kind_ext::VARIABLE_DECLARATION {
                let Some(var_decl) = self.ctx.arena.get_variable_declaration(parent_node) else {
                    return false;
                };
                if var_decl.initializer.is_none() {
                    return false;
                }
                return self
                    .ctx
                    .arena
                    .get(var_decl.initializer)
                    .is_some_and(|init| init.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION);
            }
            if kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || kind == syntax_kind_ext::BINDING_ELEMENT
            {
                current = parent;
                continue;
            }
            return false;
        }
        false
    }

    pub(crate) fn report_empty_array_destructuring_bounds(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        let Some(init_node) = self.ctx.arena.get(initializer_idx) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return;
        }
        let Some(init_lit) = self.ctx.arena.get_literal_expr(init_node) else {
            return;
        };
        if !init_lit.elements.nodes.is_empty() {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (index, &element_idx) in pattern.elements.nodes.iter().enumerate() {
            if element_idx.is_none() {
                continue;
            }
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            if element_data.dot_dot_dot_token {
                break;
            }
            // TS doesn't report tuple out-of-bounds for empty array destructuring
            // when the element has a default value.
            if element_data.initializer.is_some() {
                continue;
            }

            self.error_at_node(
                element_data.name,
                &format!("Tuple type '[]' of length '0' has no element at index '{index}'."),
                crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
            );
        }
    }

    /// Check binding pattern elements and their default values for type correctness.
    ///
    /// This function traverses a binding pattern (object or array destructuring) and verifies
    /// that any default values provided in binding elements are assignable to their expected types.
    /// Assign inferred types to binding element symbols (destructuring).
    ///
    /// The binder creates symbols for identifiers inside binding patterns (e.g., `const [x] = arr;`),
    /// but their `value_declaration` is the identifier node, not the enclosing variable declaration.
    /// We infer the binding element type from the destructured value type and cache it on the symbol.
    pub(crate) fn assign_binding_pattern_symbol_types_with_request(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
        request: &TypingRequest,
    ) {
        let parent_type =
            self.normalize_parameter_binding_pattern_source_type(pattern_idx, parent_type);

        // Skip nested pattern processing for ERROR types to prevent cascading
        // diagnostics. When a parent element resolves to ERROR (e.g., from
        // destructuring `unknown`), nested patterns should not emit further errors.
        if parent_type == TypeId::ERROR {
            return;
        }

        self.report_unknown_empty_binding_pattern(pattern_idx, parent_type);

        // A binding default's literal type is only preserved (under `const`) when
        // the destructuring source is a tuple — i.e. a fresh array literal or a
        // tuple-typed value, where the positional element literal is meaningful
        // (`const [first = 0] = [10, 20]` → `0 | 10`). For non-tuple sources
        // (arrays, or unions like `RegExpMatchArray | never[]`) the default
        // widens as usual, matching tsc once the source element is taken into
        // account.
        let source_is_tuple = query::tuple_elements(
            self.ctx.types,
            query::unwrap_readonly_deep(self.ctx.types, parent_type),
        )
        .is_some();

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
            if element_idx.is_none() {
                continue;
            }

            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }

            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };

            let mut element_type = if parent_type == TypeId::ANY {
                TypeId::ANY
            } else {
                self.get_binding_element_type_with_request(
                    pattern_idx,
                    i,
                    parent_type,
                    element_data,
                    request,
                )
            };

            // If there's an initializer, the type incorporates it.
            // TypeScript widens the inferred type with the initializer type.
            // Set contextual type for function-like defaults so parameter types
            // are inferred from the expected element type (e.g., `{ f: id = arg => arg }: T`).
            if element_data.initializer.is_some() {
                // A default value guarantees the binding won't be undefined at runtime,
                // so strip `undefined` from the element type. This matches tsc behavior:
                // `{ name = "default" }: { name?: string }` gives `name` type `string`.
                // Route through flow observation boundary for centralized policy.
                if self.ctx.strict_null_checks() {
                    element_type = flow_boundary::narrow_destructuring_default(
                        self.ctx.types,
                        element_type,
                        true,
                    );
                }

                // Provide the element type as contextual type for the default
                // value expression. This is needed for:
                // - Arrow/function defaults: infers parameter types
                // - Array literal defaults: produces tuples instead of widened arrays
                //   e.g., `[b, {x}]=["abc", {x: 10}]` needs the default typed as
                //   a tuple `[string, {x: number}]`, not `(string | {x: number})[]`
                let request = if element_type != TypeId::ANY
                    && element_type != TypeId::UNKNOWN
                    && element_type != TypeId::ERROR
                {
                    request.read().contextual(element_type)
                } else {
                    request.read().contextual_opt(None)
                };
                self.invalidate_expression_for_contextual_retry(element_data.initializer);
                // Under `const`/`readonly`, tsc does not widen the default's literal
                // type (`widenTypeInferredFromInitializer` skips the `Constant` case),
                // so `const [first = 0] = [10, 20]` yields `0 | 10`, not `number`.
                // `let`/`var`/parameter defaults widen, matching the standard path.
                // Gated to tuple sources so non-tuple sources (e.g. a `number[]` or a
                // `RegExpMatchArray | never[]` union) keep their widening behavior.
                let prev_preserve = self.ctx.preserve_literal_types;
                if source_is_tuple && self.binding_pattern_is_const_declaration(pattern_idx) {
                    self.ctx.preserve_literal_types = true;
                }
                let init_type =
                    self.get_type_of_node_with_request(element_data.initializer, &request);
                self.ctx.preserve_literal_types = prev_preserve;

                // When the destructuring SOURCE is genuinely `any`, every element is
                // `any` and stays `any` (tsc's `isTypeAny(parentType)` short-circuit):
                // do NOT fold the default initializer's type onto it, or a nested
                // computed-key would then be checked against the init type and fire a
                // spurious TS2537. The initializer is still evaluated above for its own
                // checks; only the type override is skipped.
                if parent_type != TypeId::ANY {
                    if element_type == TypeId::ANY || element_type == TypeId::UNKNOWN {
                        element_type = init_type;
                    } else if !self
                        .destructuring_relation_outcome(init_type, element_type)
                        .related
                    {
                        element_type = binding_patterns::binding_pattern_initializer_union_type(
                            self.ctx.types,
                            element_type,
                            init_type,
                        );
                    }
                }
            }

            let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                continue;
            };

            // Identifier binding: cache the inferred type on the symbol.
            if name_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name)
            {
                // A bare `unique symbol` binding element widens to `symbol`, matching
                // tsc's `widenTypeForVariableLikeDeclaration` (the `isBindingElement`
                // branch always widens — a binding element's pattern annotation types
                // the *source*, not the element binding, so `const [db]: [typeof cs] = t`
                // still yields `db: symbol`). A binding element is never the unique
                // symbol's mint site, so no ref guard is needed; `is_unique_symbol_type`
                // is bare-only, so a `typeof a | typeof b` element is preserved. This
                // widens the *element* only — nested patterns recurse below with the
                // un-widened `element_type` and widen at their own leaf identifiers.
                let element_type =
                    if common_query::is_unique_symbol_type(self.ctx.types, element_type) {
                        TypeId::SYMBOL
                    } else {
                        element_type
                    };
                // Route null/undefined widening through the flow observation boundary.
                let final_type = flow_boundary::widen_null_undefined_to_any(
                    self.ctx.types,
                    element_type,
                    self.ctx.strict_null_checks(),
                );
                self.cache_symbol_type(sym_id, final_type);
            }

            // Nested binding patterns: check iterability for array patterns, then recurse
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                // Check iterability for nested array destructuring
                self.check_destructuring_iterability(
                    element_data.name,
                    element_type,
                    NodeIndex::NONE,
                );
                let nested_request = if element_type != TypeId::ANY
                    && element_type != TypeId::UNKNOWN
                    && element_type != TypeId::ERROR
                {
                    request.read().contextual(element_type)
                } else {
                    request.read().contextual_opt(None)
                };
                self.assign_binding_pattern_symbol_types_with_request(
                    element_data.name,
                    element_type,
                    &nested_request,
                );
            } else if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                let nested_request = if element_type != TypeId::ANY
                    && element_type != TypeId::UNKNOWN
                    && element_type != TypeId::ERROR
                {
                    request.read().contextual(element_type)
                } else {
                    request.read().contextual_opt(None)
                };
                self.assign_binding_pattern_symbol_types_with_request(
                    element_data.name,
                    element_type,
                    &nested_request,
                );
            }
        }
    }

    /// Get the expected type for a binding element from its parent type.
    pub(crate) fn get_binding_element_type_with_request(
        &mut self,
        pattern_idx: NodeIndex,
        element_index: usize,
        parent_type: TypeId,
        element_data: &tsz_parser::parser::node::BindingElementData,
        request: &TypingRequest,
    ) -> TypeId {
        let parent_type =
            self.normalize_parameter_binding_pattern_source_type(pattern_idx, parent_type);
        let pattern_kind = self.ctx.arena.get(pattern_idx).map_or(0, |n| n.kind);
        // Resolve binding-parent shapes without forcing full assignability
        // normalization on recursive alias unions. Cases like
        // `['and', ...Expression[]] | ['not', Expression]` can overflow when
        // union simplification tries to compare recursive tuple members.
        let has_recursive_alias_shape = common_query::contains_lazy_or_recursive(
            self.ctx.types.as_type_database(),
            parent_type,
        );
        let normalized_parent_type = parent_type;
        let parent_type = if has_recursive_alias_shape {
            let parent_type = self.resolve_lazy_type(parent_type);
            let parent_type = self.resolve_type_for_property_access(parent_type);
            self.evaluate_application_type(parent_type)
        } else {
            self.evaluate_type_for_assignability(parent_type)
        };
        let parent_type = self
            .preserve_actual_lib_namespace_binding_parent_type(normalized_parent_type, parent_type);
        let defer_property_not_found = self
            .should_defer_property_not_found_for_contextual_destructuring(pattern_idx, parent_type);
        let suppress_missing_property_for_literal_default = self
            .should_suppress_missing_property_for_literal_default(
                pattern_idx,
                element_data,
                request,
            );

        // Array binding patterns use the element position.
        if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            if parent_type == TypeId::UNKNOWN || parent_type == TypeId::ERROR {
                return parent_type;
            }

            // For union types of tuples/arrays, resolve element type from each member
            if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                // Rest element: distribute `sliceTupleType` over an all-tuple
                // union, otherwise bind a single array of the union's element
                // type — matching tsc's `getBindingElementTypeFromParentType`.
                if element_data.dot_dot_dot_token {
                    return self.union_rest_binding_type(&members, parent_type, element_index);
                }
                let mut elem_types = Vec::new();
                let mut saw_non_array_like = false;
                for &member in &members {
                    let member = query::unwrap_readonly_deep(self.ctx.types, member);
                    if let Some(elem) = query::array_element_type(self.ctx.types, member) {
                        let mut elem = elem;
                        if self.ctx.no_unchecked_indexed_access() {
                            elem = self.add_undefined_if_missing_for_destructuring(elem);
                        }
                        elem_types.push(elem);
                    } else if let Some(telems) = query::tuple_elements(self.ctx.types, member) {
                        let elem = self.get_element_access_type(
                            member,
                            TypeId::NUMBER,
                            Some(element_index),
                        );
                        if elem != TypeId::ERROR {
                            let has_rest = telems.iter().any(|e| e.rest);
                            if self.ctx.no_unchecked_indexed_access() && has_rest {
                                let non_rest_count = telems.iter().filter(|e| !e.rest).count();
                                if element_index >= non_rest_count {
                                    elem_types.push(
                                        self.add_undefined_if_missing_for_destructuring(elem),
                                    );
                                } else {
                                    elem_types.push(elem);
                                }
                            } else {
                                elem_types.push(elem);
                            }
                        }
                    } else {
                        saw_non_array_like = true;
                    }
                }
                // When some member is not array-like (string, `Iterable<T>`,
                // `Set`, ...), tsc types every position from the iterated
                // element type of the whole union rather than distributing
                // indexed access over the members. ANY/ERROR mean the union
                // could not be iterated, so keep the distributed result.
                if saw_non_array_like {
                    let iterated = self.for_of_element_type(parent_type, false);
                    if iterated != TypeId::ANY && iterated != TypeId::ERROR {
                        return iterated;
                    }
                }
                if elem_types.is_empty() {
                    // All members are tuples that are out of bounds for this index.
                    // Emit TS2339 "Property 'N' does not exist on type 'X'".
                    let all_tuples_oob = members.iter().all(|&m| {
                        let m = query::unwrap_readonly_deep(self.ctx.types, m);
                        if let Some(elems) = query::tuple_elements(self.ctx.types, m) {
                            let has_rest = elems.iter().any(|e| e.rest);
                            !has_rest && element_index >= elems.len()
                        } else {
                            false
                        }
                    });
                    if all_tuples_oob {
                        let type_str = self.format_type(parent_type);
                        self.error_at_node(
                            element_data.name,
                            &format!(
                                "Property '{element_index}' does not exist on type '{type_str}'.",
                            ),
                            crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        );
                    }
                    return TypeId::ANY;
                }
                return if elem_types.len() == 1 {
                    elem_types[0]
                } else {
                    binding_patterns::binding_pattern_member_union_type(self.ctx.types, elem_types)
                };
            }

            // Unwrap readonly wrappers for destructuring element access
            let array_like = query::unwrap_readonly_deep(self.ctx.types, parent_type);
            // Rest element: ...rest
            if element_data.dot_dot_dot_token {
                if let Some(elem) = query::array_element_type(self.ctx.types, array_like) {
                    // Array source `E[]` → `E[]`.
                    return self.rest_binding_array_type(elem);
                } else if let Some(elems) = query::tuple_elements(self.ctx.types, array_like) {
                    // Tuple sources slice the residual, matching tsc's
                    // `sliceTupleType`. Two fresh array-literal adjustments:
                    // a leading rest (`[...r] = [0, 1]`) never puts the
                    // literal in tuple context, so it binds the widened
                    // element array; and a trailing rest slices the
                    // *widened* literal tuple (tsz keeps unwidened literal
                    // elements for positional `const` literal-default
                    // precision, e.g. `const [first = 0] = [10, 20]` → `0 | 10`).
                    let is_fresh = self.array_binding_source_is_fresh_array_literal(pattern_idx);
                    if is_fresh && element_index == 0 {
                        let elem = self.get_element_access_type(array_like, TypeId::NUMBER, None);
                        return self.rest_binding_array_type(elem);
                    }
                    let sliced = self.tuple_rest_binding_type(&elems, element_index);
                    if is_fresh {
                        return self.widen_initializer_type_for_mutable_binding(sliced);
                    }
                    return sliced;
                }
                // Non-array-like iterable sources (string, `Iterable<T>`,
                // `Map`, generators) bind an array of the iterated element
                // type, mirroring tsc's `checkIteratedTypeOrElementType`.
                let iterated = self.for_of_element_type(array_like, false);
                if iterated == TypeId::ERROR {
                    return self.rest_binding_array_type(TypeId::ANY);
                }
                return self.rest_binding_array_type(iterated);
            }

            return if let Some(elem) = query::array_element_type(self.ctx.types, array_like) {
                if self.ctx.no_unchecked_indexed_access() {
                    self.add_undefined_if_missing_for_destructuring(elem)
                } else {
                    elem
                }
            } else if let Some(elems) = query::tuple_elements(self.ctx.types, array_like) {
                // Compute element types directly from the tuple structure rather
                // than using get_element_access_type, which applies
                // noUncheckedIndexedAccess globally to ALL elements.
                // Destructuring knows exact positions, so only rest-region
                // elements need `| undefined`.

                // Single pass: find rest element position/type and count non-rest.
                let mut rest_pos = None;
                let mut rest_type_id = None;
                let mut non_rest_count = 0usize;
                for (i, e) in elems.iter().enumerate() {
                    if e.rest {
                        rest_pos = Some(i);
                        rest_type_id = Some(e.type_id);
                    } else {
                        non_rest_count += 1;
                    }
                }
                let has_rest = rest_pos.is_some();
                let leading_fixed = rest_pos.unwrap_or(elems.len());

                // Determine the element type based on position in the tuple.
                // Also track whether the element is optional (e.g., `string?`
                // in `[string, string?]`). Optional tuple elements include
                // `undefined` in their type.
                let (raw_elem, is_optional) = if !has_rest {
                    // Fixed-length tuple: direct element access or out-of-bounds.
                    let elem = elems.get(element_index);
                    (elem.map(|e| e.type_id), elem.is_some_and(|e| e.optional))
                } else if element_index < leading_fixed {
                    // In the leading fixed region — guaranteed to exist.
                    let elem = &elems[element_index];
                    (Some(elem.type_id), elem.optional)
                } else {
                    // In the rest region or trailing fixed region. At
                    // destructuring time, we can't distinguish them, so use
                    // the rest element type (unwrapped from Array<T>).
                    (
                        rest_type_id
                            .map(|ty| query::array_element_type(self.ctx.types, ty).unwrap_or(ty)),
                        false,
                    )
                };

                if let Some(elem_type) = raw_elem {
                    // Optional tuple elements (e.g., `string?` in `[string, string?]`)
                    // include `undefined` in their type. Add it if not already present.
                    let elem_type = if is_optional {
                        self.add_undefined_if_missing_for_destructuring(elem_type)
                    } else {
                        elem_type
                    };
                    // With noUncheckedIndexedAccess, elements at indices >=
                    // the minimum guaranteed tuple length (non_rest_count =
                    // leading_fixed + trailing_fixed) may not exist at runtime.
                    if self.ctx.no_unchecked_indexed_access()
                        && has_rest
                        && element_index >= non_rest_count
                    {
                        self.add_undefined_if_missing_for_destructuring(elem_type)
                    } else {
                        elem_type
                    }
                } else {
                    let has_rest_tail = elems.last().is_some_and(|element| element.rest);
                    // When a binding element has a default value (e.g., `[a, b = a] = [1]`),
                    // accessing beyond the tuple length is allowed — the default covers
                    // the missing element. tsc does not emit TS2493 in this case.
                    // Also skip when the index is in bounds — ERROR may just mean the
                    // element type itself is an error (e.g. from an unresolved property),
                    // not that the index is out of range.
                    //
                    // Also skip TS2493 when the binding pattern is a PARAMETER whose
                    // type was inferred from a default initializer (e.g. `function
                    // f([x, y] = []) {}`). tsc treats the binding elements as
                    // implicitly any (TS7031) rather than tuple-out-of-bounds here.
                    let in_parameter_with_default =
                        self.binding_pattern_in_parameter_with_default(pattern_idx);
                    if !has_rest_tail
                        && element_data.initializer.is_none()
                        && element_index >= elems.len()
                        && !in_parameter_with_default
                    {
                        let tuple_type_str = self.format_type(array_like);
                        self.error_at_node(
                            element_data.name,
                            &format!(
                                "Tuple type '{}' of length '{}' has no element at index '{}'.",
                                tuple_type_str,
                                elems.len(),
                                element_index
                            ),
                            crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                        );
                        // Out-of-bounds tuple access yields `undefined` at
                        // runtime. Returning UNDEFINED here lets nested
                        // destructuring's iterability check fire TS2488 on
                        // inner array binding patterns — matching tsc's
                        // emission of both TS2493 and TS2488 for cases like
                        // `var [[a0]] = []`. ANY would short-circuit the
                        // check_destructuring_iterability ANY/ERROR fast path
                        // and silently swallow the inner error.
                        return TypeId::UNDEFINED;
                    }
                    TypeId::ANY
                }
            } else {
                // Non-array-like iterable sources (string, `Iterable<T>`,
                // `Map`, generators) bind the iterated element type at every
                // position, mirroring tsc's `checkIteratedTypeOrElementType`.
                let iterated = self.for_of_element_type(array_like, false);
                if iterated == TypeId::ERROR {
                    TypeId::ANY
                } else {
                    iterated
                }
            };
        }

        let computed_expr = self
            .ctx
            .arena
            .get(element_data.property_name)
            .and_then(|prop_node| self.ctx.arena.get_computed_property(prop_node))
            .map(|computed| computed.expression);

        if let Some(computed_expr) = computed_expr {
            let key_type = self.get_binding_element_computed_key_type_with_request(
                pattern_idx,
                computed_expr,
                request,
            );
            if let Some(property_type) = self.get_binding_element_literal_key_type(
                parent_type,
                key_type,
                element_data,
                defer_property_not_found,
                suppress_missing_property_for_literal_default,
            ) {
                // Check accessibility (TS2341/TS2445) for computed literal key destructuring.
                // e.g. `const { ["p"]: p1 } = new C();` where `p` is private.
                if let Some((string_keys, _)) = self.get_literal_key_union_from_type(key_type) {
                    let error_node = if element_data.property_name != NodeIndex::NONE {
                        element_data.property_name
                    } else if element_data.name != NodeIndex::NONE {
                        element_data.name
                    } else {
                        NodeIndex::NONE
                    };
                    for key in &string_keys {
                        let key_str = self.ctx.types.resolve_atom(*key);
                        self.check_property_accessibility(
                            NodeIndex::NONE,
                            &key_str,
                            error_node,
                            parent_type,
                        );
                    }
                }
                return property_type;
            }
        }

        // Extract the static property name from binding element.
        // Handles: { x }, { x: a }, { 'b': a }, { ['b']: a }, { [ident]: a }.
        let property_name = self.extract_binding_property_name(element_data);

        // Tuple-out-of-bounds check for object binding patterns whose property
        // name is a numeric literal (`{ 0: a, 3: d }: [T0, T1, T2]`). For
        // fixed-length tuples — those without a rest element — accessing an
        // index beyond the declared length is a TS2493. Element-access via
        // `x[3]` already emits TS2493 in the generic property/element-access
        // path; the destructuring path needs the same check applied to
        // numeric property keys.
        //
        // Rest-bearing tuples (e.g. `[T, ...T[]]`) accept any non-negative
        // index by design, so they are not bounds-checked here.
        if let Some(prop_name_str) = property_name.as_deref()
            && let Ok(idx) = prop_name_str.parse::<usize>()
            && let Some(elems) = query::tuple_elements(self.ctx.types, parent_type)
            && !elems.iter().any(|e| e.rest)
            && idx >= elems.len()
        {
            let tuple_type_str = self.format_type(parent_type);
            let error_node = if element_data.property_name.is_some() {
                element_data.property_name
            } else {
                element_data.name
            };
            self.error_at_node(
                error_node,
                &format!(
                    "Tuple type '{}' of length '{}' has no element at index '{}'.",
                    tuple_type_str,
                    elems.len(),
                    idx
                ),
                crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
            );
            return TypeId::UNDEFINED;
        }

        // Unique symbol keys (e.g. `const s = Symbol(); { [s]: v }`) resolve to
        // `__unique_N` via `get_property_name_resolved`.  Keep them as named
        // properties so normal property resolution can find matching symbol-keyed
        // properties on the parent type (or its type-parameter constraint).
        // Previously these were zeroed out and treated as dynamic keys, which
        // caused false TS2538 errors when the parent type actually had a matching
        // symbol property (e.g. `T extends { [sa]: string }`).

        // For computed keys in object binding patterns (e.g. `{ [k]: v }`),
        // check index signatures when the key resolves to a dynamic type
        // (string or number, not a literal matching a known property).
        if element_data.property_name.is_some() {
            // Only check index signatures for truly dynamic keys (not identifiers
            // or string/numeric literals that resolve to known properties).
            // Unique symbol keys are also treated as dynamic.
            if computed_expr.is_some() && property_name.is_none() {
                let key_type = computed_expr.map_or(TypeId::ANY, |expr_idx| {
                    self.get_binding_element_computed_key_type_with_request(
                        pattern_idx,
                        expr_idx,
                        request,
                    )
                });
                let key_is_string = key_type == TypeId::STRING;
                let key_is_number = key_type == TypeId::NUMBER;

                // TS2538: Reject invalid index types (any/void/boolean/etc.) and
                // symbol/unique-symbol types (can't match string/number index sigs;
                // matching symbol properties resolved earlier).
                // ERROR types from failed expressions are treated as `any`.
                let key_is_type_param = crate::query_boundaries::common::is_type_parameter_like(
                    self.ctx.types,
                    key_type,
                );
                if !key_is_string
                    && !key_is_number
                    && !key_is_type_param
                    && key_type != TypeId::NEVER
                {
                    let check_key = if key_type == TypeId::ERROR {
                        TypeId::ANY
                    } else {
                        self.resolve_lazy_type(key_type)
                    };
                    // A genuine `any` is a valid index type for a value-position
                    // destructuring computed key: `{ [k]: v } = obj` desugars to
                    // `v = obj[k]`, and element access permits an `any` index.
                    // Only the strict type-level `isValidIndexType` (keyof/mapped/
                    // `T[K]`) rejects `any`; that helper must not gate this
                    // value-position check. But an ERROR key (e.g. `[foo()]` where
                    // `foo` is not callable) is remapped to ANY above precisely so
                    // it still reports TS2538 (tsc does too) — so exempt only when
                    // the ORIGINAL key type is `any`, never the ERROR remap.
                    let is_invalid = if key_type == TypeId::ANY && check_key == TypeId::ANY {
                        None
                    } else {
                        crate::query_boundaries::type_checking_utilities::get_invalid_index_type_member_strict(self.ctx.types, check_key)
                    };
                    // Symbol types pass the general validity check but can't
                    // index into objects through string/number index signatures,
                    // UNLESS the parent type (or its constraint for generics)
                    // has the specific unique symbol as a declared property.
                    let is_symbol = key_type != TypeId::ERROR
                        && common_query::is_symbol_or_unique_symbol(self.ctx.types, key_type)
                        && !common_query::contains_type_parameters(
                            self.ctx.types.as_type_database(),
                            parent_type,
                        )
                        && !crate::query_boundaries::type_computation::access::literal_property_name(
                            self.ctx.types,
                            key_type,
                        )
                        .is_some_and(|atom| {
                            crate::query_boundaries::property_access::type_has_property(
                                self.ctx.types,
                                parent_type,
                                atom,
                            )
                        });
                    let ts2538_type = is_invalid.or(if is_symbol { Some(key_type) } else { None });
                    if let Some(err_type) = ts2538_type {
                        let key_type_str = self.format_type(err_type);
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                            &[&key_type_str],
                        );
                        let error_node = computed_expr.unwrap_or(element_data.property_name);
                        self.error_at_node(
                            error_node,
                            &message,
                            crate::diagnostics::diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                        );
                    }
                }

                // TS2538 (secondary): symbol/unique symbol types that reach here
                // can't index via string/number index signatures. Suppress when
                // parent type contains type parameters (deferred to instantiation)
                // or when the parent type has a matching unique symbol property.
                let parent_has_type_params = common_query::contains_type_parameters(
                    self.ctx.types.as_type_database(),
                    parent_type,
                );
                if !key_is_string
                    && !key_is_number
                    && !key_is_type_param
                    && !parent_has_type_params
                    && key_type != TypeId::NEVER
                    && key_type != TypeId::ERROR
                    && common_query::is_symbol_or_unique_symbol(self.ctx.types, key_type)
                {
                    let parent_has_symbol_prop =
                        crate::query_boundaries::type_computation::access::literal_property_name(
                            self.ctx.types,
                            key_type,
                        )
                        .is_some_and(|atom| {
                            crate::query_boundaries::property_access::type_has_property(
                                self.ctx.types,
                                parent_type,
                                atom,
                            )
                        });
                    if !parent_has_symbol_prop {
                        let key_type_str = self.format_type(key_type);
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                            &[&key_type_str],
                        );
                        let error_node = computed_expr.unwrap_or(element_data.property_name);
                        self.error_at_node(
                            error_node,
                            &message,
                            crate::diagnostics::diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                        );
                    }
                }

                if key_is_string || key_is_number {
                    let has_matching_index = |ty: TypeId| {
                        query::object_shape(self.ctx.types, ty).is_some_and(|shape| {
                            if key_is_string {
                                shape.string_index.is_some()
                            } else {
                                shape.number_index.is_some() || shape.string_index.is_some()
                            }
                        })
                    };

                    let has_index_signature =
                        if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                            members.into_iter().all(has_matching_index)
                        } else {
                            has_matching_index(parent_type)
                        };

                    if !has_index_signature
                        && parent_type != TypeId::ANY
                        && parent_type != TypeId::ERROR
                        && parent_type != TypeId::UNKNOWN
                    {
                        let mut formatter = self.ctx.create_type_formatter();
                        let object_str = formatter.format(parent_type);
                        let index_str = formatter.format(key_type);
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                            &[&object_str, &index_str],
                        );
                        let error_node = self
                            .ctx
                            .arena
                            .get(element_data.property_name)
                            .and_then(|prop_node| self.ctx.arena.get_computed_property(prop_node))
                            .map_or(element_data.property_name, |computed| computed.expression);
                        self.error_at_node(
                            error_node,
                            &message,
                            crate::diagnostics::diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                        );
                    }
                }
            }
        }

        if element_data.dot_dot_dot_token {
            if self.is_untyped_parameter_binding_pattern_without_context(pattern_idx, request) {
                return TypeId::ANY;
            }
            return self.compute_object_rest_type(pattern_idx, parent_type);
        }

        if parent_type == TypeId::UNKNOWN {
            let error_node = if element_data.property_name.is_some() {
                element_data.property_name
            } else if element_data.name.is_some() {
                element_data.name
            } else {
                NodeIndex::NONE
            };
            if let Some(prop_name_str) = property_name.as_deref() {
                // Suppress TS2339 when:
                // 1. Contextual destructuring defers the check, OR
                // 2. The source is a literal with defaults, OR
                // 3. The binding element has a default initializer.
                //    In tsc, `{x = val} = {}` doesn't error even though `{}` has no `x`,
                //    because the default handles the missing property. This applies to
                //    for-of patterns like `for (let {x = true} of [{}])`.
                if !defer_property_not_found
                    && !suppress_missing_property_for_literal_default
                    && element_data.initializer.is_none()
                {
                    self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
                }
            } else if element_data.initializer.is_none()
                && !defer_property_not_found
                && !suppress_missing_property_for_literal_default
            {
                self.error_at_node(
                    error_node,
                    "Object is of type 'unknown'.",
                    crate::diagnostics::diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                );
            }
            // Return ERROR to suppress cascading diagnostics in nested patterns.
            // TSC only reports errors at the outermost destructuring level for
            // unknown types (e.g., `{ a: { x } }` from catch clause only reports
            // TS2339 for `a`, not for nested `x`).
            return TypeId::ERROR;
        }

        if let Some(ref prop_name_str) = property_name {
            if self.binding_pattern_direct_source_is_this(pattern_idx)
                && self.ctx.directly_in_class_member_body()
                && let Some(class_info) = self.ctx.enclosing_class.as_ref()
                && class_info.in_constructor
                && let Some(declaring_class_name) =
                    self.find_abstract_property_declaring_class(class_info.class_idx, prop_name_str)
            {
                let error_node = if element_data.property_name.is_some() {
                    element_data.property_name
                } else if element_data.name.is_some() {
                    element_data.name
                } else {
                    NodeIndex::NONE
                };
                self.error_abstract_property_in_constructor(
                    prop_name_str,
                    &declaring_class_name,
                    error_node,
                );
            }

            use crate::query_boundaries::common::PropertyAccessResult;
            let prop_access_result =
                self.resolve_property_access_with_env(parent_type, prop_name_str);
            match prop_access_result {
                PropertyAccessResult::Success { type_id, .. } => {
                    // Check accessibility (TS2341/TS2445) — destructuring still
                    // respects private/protected modifiers.
                    let error_node = if element_data.property_name != NodeIndex::NONE {
                        element_data.property_name
                    } else if element_data.name != NodeIndex::NONE {
                        element_data.name
                    } else {
                        NodeIndex::NONE
                    };
                    self.check_property_accessibility(
                        NodeIndex::NONE, // no direct object expr in destructuring
                        prop_name_str,
                        error_node,
                        parent_type,
                    );
                    type_id
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    // tsc's getTypeOfDestructuredProperty uses mapType for
                    // unions where all non-empty members have the property.
                    // When a union contains empty object members (`{}`), those
                    // members naturally lack every property. In tsc, an empty
                    // object member contributes `undefined` for any property
                    // instead of failing the entire lookup. This commonly
                    // arises from `x ?? {}` patterns where the right-hand
                    // `{}` produces an empty member in the union.
                    //
                    // We only apply this per-member resolution when EVERY
                    // member that lacks the property is an empty object. If a
                    // non-empty member is missing the property, the standard
                    // TS2339 error should still fire.
                    if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                        let mut member_types = Vec::new();
                        let mut any_found = false;
                        let mut non_empty_missing = false;
                        for &member in &members {
                            let member_result =
                                self.resolve_property_access_with_env(member, prop_name_str);
                            match member_result {
                                PropertyAccessResult::Success { type_id, .. } => {
                                    member_types.push(type_id);
                                    any_found = true;
                                }
                                PropertyAccessResult::PossiblyNullOrUndefined {
                                    property_type,
                                    ..
                                } => {
                                    member_types.push(property_type.unwrap_or(TypeId::UNDEFINED));
                                    any_found = true;
                                }
                                PropertyAccessResult::PropertyNotFound { .. } => {
                                    // Empty `{}` or fresh object-literal members lacking the
                                    // property contribute implicit `undefined` (tsc
                                    // getTypeOfDestructuredProperty); named, call-return, and
                                    // freshness-widened const-bound members lack FRESH and error.
                                    use crate::query_boundaries::common;
                                    let db = self.ctx.types.as_type_database();
                                    if common::is_empty_object_type(db, member)
                                        || common::is_fresh_object_type(db, member)
                                    {
                                        member_types.push(TypeId::UNDEFINED);
                                    } else {
                                        non_empty_missing = true;
                                        break;
                                    }
                                }
                                PropertyAccessResult::IsUnknown => {
                                    member_types.push(TypeId::UNDEFINED);
                                }
                            }
                        }
                        if any_found && !non_empty_missing {
                            return binding_patterns::binding_pattern_member_union_type(
                                self.ctx.types,
                                member_types,
                            );
                        }
                    }

                    let error_node = if element_data.property_name.is_some() {
                        element_data.property_name
                    } else if element_data.name.is_some() {
                        element_data.name
                    } else {
                        NodeIndex::NONE
                    };
                    if computed_expr.is_none()
                        && !defer_property_not_found
                        && !suppress_missing_property_for_literal_default
                        && self.require_ts2305(pattern_idx, prop_name_str, error_node)
                    {
                        return TypeId::ERROR;
                    }
                    if !defer_property_not_found && !suppress_missing_property_for_literal_default {
                        // When the computed key is a unique symbol that doesn't exist
                        // on the parent type, emit TS2538 ("Type 'X' cannot be used as
                        // an index type") instead of TS2339 ("Property does not exist").
                        // tsc treats unique symbol keys that don't match a declared
                        // property as index-type errors, not property-not-found errors.
                        let emitted_ts2538 = if let Some(ce) = computed_expr {
                            let key_type = self.get_binding_element_computed_key_type_with_request(
                                pattern_idx,
                                ce,
                                request,
                            );
                            if common_query::is_symbol_or_unique_symbol(self.ctx.types, key_type) {
                                let key_type_str = self.format_type(key_type);
                                let message = crate::diagnostics::format_message(
                                    crate::diagnostics::diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                                    &[&key_type_str],
                                );
                                self.error_at_node(
                                    ce,
                                    &message,
                                    crate::diagnostics::diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                                );
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if !emitted_ts2538 {
                            // In tsc, destructuring uses the *apparent* type in the
                            // error message: `object` → `{}`, and primitives widen
                            // to their wrapper class (`string` → `String`,
                            // `number` → `Number`, etc.). Match that so binding
                            // patterns like `var { a } = "s"` report `type 'String'`
                            // rather than the raw `type 'string'`.
                            let apparent_type_display =
                                apparent_type_display_for_destructuring(parent_type);
                            if let Some(ce) = computed_expr {
                                let type_str = apparent_type_display.clone().unwrap_or_else(|| {
                                    self.format_type_for_assignability_message(parent_type)
                                });
                                let message = format!(
                                    "Property '{prop_name_str}' does not exist on type '{type_str}'."
                                );
                                self.error_at_node(
                                    ce,
                                    &message,
                                    crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                                );
                            } else if let Some(type_str) = apparent_type_display {
                                self.error_property_not_exist_with_apparent_type(
                                    prop_name_str,
                                    &type_str,
                                    error_node,
                                );
                            } else {
                                self.error_property_not_exist_at(
                                    prop_name_str,
                                    parent_type,
                                    error_node,
                                );
                            }
                        }
                    }
                    TypeId::ANY
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    if !defer_property_not_found && !suppress_missing_property_for_literal_default {
                        let error_node = if element_data.property_name.is_some() {
                            element_data.property_name
                        } else if element_data.name.is_some() {
                            element_data.name
                        } else {
                            NodeIndex::NONE
                        };
                        self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
                    }
                    property_type.unwrap_or(TypeId::ANY)
                }
                PropertyAccessResult::IsUnknown => TypeId::ANY,
            }
        } else {
            TypeId::ANY
        }
    }
}

mod commonjs_require;
mod recording;
mod tail;
