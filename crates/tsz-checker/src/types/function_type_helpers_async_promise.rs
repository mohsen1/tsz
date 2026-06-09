//! Async-function Promise diagnostics: TS2468/TS2705 missing Promise
//! constructor checks, TS1055/TS1064 async return-type-is-Promise checks
//! (including the JSDoc `@type` variant), and Promise-alias recognition.
//!
//! Child module of `function_type_helpers` so it can share that module's
//! private `CheckerState` helpers.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// TS2705/TS2468: Check that the Promise constructor is available for async functions.
    ///
    /// Reports the "downleveled async needs the global `Promise` constructor"
    /// diagnostics only where `tsc` does, when the Promise constructor *value* is
    /// missing from the loaded libs. Verified against `tsc` 6.0 with a lib that
    /// lacks the Promise constructor (`--lib es5`):
    ///
    /// * Expression-position async functions — function expressions, arrow
    ///   functions, object-literal methods — report **TS2468 + TS2705** at every
    ///   target.
    /// * Declaration-position async functions — function declarations and class
    ///   methods — report **TS2705** only when they carry an explicit return-type
    ///   annotation *and* the target downlevels async (`< ES2015`, i.e. ES3/ES5).
    ///   They never emit the program-level TS2468.
    /// * Everything else (bare declaration-position async, or any async at
    ///   `ES2015`+ in declaration position) reports nothing.
    pub(crate) fn check_async_promise_constructor_availability(
        &mut self,
        is_async: bool,
        is_generator: bool,
        has_type_annotation: bool,
        async_node_idx: NodeIndex,
        func_idx: NodeIndex,
    ) {
        if !is_async || is_generator {
            return;
        }

        // Run the cheap position/target gate before the lib scan: this helper is
        // called for every function-like node, and the common case (declaration
        // position, no annotation, or a modern target) bails out without the
        // `promise_constructor_diagnostics_required` lib walk.
        let is_expression_position = self.async_function_is_expression_position(func_idx);
        if !is_expression_position {
            // Declaration-position async (function declaration or class method):
            // only flagged when annotated and the target downlevels async.
            if !(has_type_annotation && self.ctx.compiler_options.target.is_es5()) {
                return;
            }
        }

        if !self.ctx.promise_constructor_diagnostics_required() {
            return;
        }

        // Find the `async` keyword position for error anchoring.
        // For async arrow functions (no name node), the node `pos` starts at
        // the first parameter, not the `async` keyword. We scan backward
        // in the source to locate the keyword.
        let async_keyword_span = if async_node_idx.is_none() {
            // Arrow function — scan backward from node start to find `async`
            self.ctx.arena.get(func_idx).and_then(|n| {
                let sf = self.ctx.arena.source_files.first()?;
                let text = sf.text.as_bytes();
                let node_pos = n.pos as usize;
                // Scan backward over whitespace to find end of `async`
                let mut end = node_pos;
                while end > 0 && text.get(end - 1).copied() == Some(b' ') {
                    end -= 1;
                }
                // Check that the 5 chars before `end` are "async"
                if end >= 5 && &text[end - 5..end] == b"async" {
                    Some((end as u32 - 5, 5u32))
                } else {
                    None
                }
            })
        } else {
            None
        };

        // TS2468: Cannot find global value 'Promise'.
        // tsc emits this program-level diagnostic (no file location) only for
        // expression-position async functions; declaration-position async
        // functions get TS2705 alone.
        if is_expression_position {
            let message =
                format_message(diagnostic_messages::CANNOT_FIND_GLOBAL_VALUE, &["Promise"]);
            self.error_program_level(message, diagnostic_codes::CANNOT_FIND_GLOBAL_VALUE);
        }

        // TS2705: anchored at the `async` keyword
        if let Some((start, length)) = async_keyword_span {
            self.error_at_position(
                start,
                length,
                diagnostic_messages::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
                diagnostic_codes::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
            );
        } else {
            let diagnostic_node = if async_node_idx.is_none() {
                func_idx
            } else {
                async_node_idx
            };
            self.error_at_node(
                diagnostic_node,
                diagnostic_messages::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
                diagnostic_codes::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
            );
        }
    }

    /// Whether an async function-like node sits in *expression* position:
    /// function expressions, arrow functions, and object-literal methods. `tsc`
    /// flags these for a missing Promise constructor at every target.
    ///
    /// Function declarations and class methods are statically-positioned
    /// declarations and return `false`; the caller flags them only in the
    /// narrower annotated/downlevel case. The class-vs-object distinction for
    /// method declarations is decided by the parent node kind (a method's parent
    /// is the class or object-literal node directly).
    fn async_function_is_expression_position(&self, func_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION => true,
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_extended(func_idx)
                .and_then(|ext| self.ctx.arena.get(ext.parent))
                .is_some_and(|parent| parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION),
            _ => false,
        }
    }

    /// TS2705/TS1055/TS1064: Check that an async function's return type annotation is Promise.
    /// Emits TS1055 (ES5) or TS1064 (ES6+) when the declared return type is not Promise<T>.
    pub(crate) fn check_async_return_type_is_promise(
        &mut self,
        has_type_annotation: bool,
        is_async: bool,
        is_generator: bool,
        return_type: TypeId,
        type_annotation: NodeIndex,
    ) {
        if !has_type_annotation || !is_async || is_generator {
            return;
        }
        use tsz_scanner::SyntaxKind;
        let should_emit = if self.is_global_promise_type(return_type) {
            // Return type is exactly the global Promise<T> - OK
            false
        } else if self.is_promise_type_through_alias(return_type) {
            // Return type is a type alias application that resolves to Promise
            // (e.g., `type MyPromise<T> = Promise<T>` with `declare var MyPromise: typeof Promise`).
            // The merged symbol prevents is_global_promise_type from recognizing it.
            false
        } else if self.return_type_annotation_is_exactly_promise(type_annotation) {
            // The declared annotation resolves to the lib Promise symbol. Some
            // evaluated Promise<T> forms lose the lazy base identity and arrive
            // as an Application over an object shape; tsc still accepts them.
            false
        } else if self.is_non_promise_application_type(return_type) {
            // Return type is an Application with a non-Promise base (e.g., MyPromise<T>).
            // TSC requires exactly Promise<T>, not subclasses.
            true
        } else if return_type != TypeId::ERROR {
            // Return type evaluated to a non-Application form (e.g., Object).
            // Fall back to strict syntactic check: only suppress TS1064 if the
            // annotation literally says `Promise<...>`. TSC uses `isReferenceToType`
            // which requires exactly the global Promise — not subclasses like
            // `MyPromise`, not qualified names like `X.MyPromise`, not type aliases.
            !self.return_type_annotation_is_exactly_promise(type_annotation)
        } else {
            // Return type is ERROR - use syntactic fallback
            // Check if the type annotation is a primitive keyword (never valid for async function)
            let type_node_result = self.ctx.arena.get(type_annotation);
            match type_node_result {
                Some(type_node) => {
                    // Primitives are definitely not valid async function return types
                    matches!(
                        type_node.kind as u32,
                        k if k == SyntaxKind::StringKeyword as u32
                            || k == SyntaxKind::NumberKeyword as u32
                            || k == SyntaxKind::BooleanKeyword as u32
                            || k == SyntaxKind::VoidKeyword as u32
                            || k == SyntaxKind::UndefinedKeyword as u32
                            || k == SyntaxKind::NullKeyword as u32
                            || k == SyntaxKind::NeverKeyword as u32
                            || k == SyntaxKind::ObjectKeyword as u32
                    )
                }
                None => false,
            }
        };
        if !should_emit {
            return;
        }
        use crate::context::ScriptTarget;
        // For ES5/ES3 targets, emit TS1055 instead of TS2705
        let is_es5_or_lower = matches!(
            self.ctx.compiler_options.target,
            ScriptTarget::ES3 | ScriptTarget::ES5
        );
        if is_es5_or_lower {
            let type_name = self.format_type(return_type);
            self.error_at_node(
                type_annotation,
                &format_message(
                    diagnostic_messages::TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER,
                    &[&type_name],
                ),
                diagnostic_codes::TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER,
            );
        } else {
            // TS1064: For ES6+ targets, the return type must be Promise<T>
            let type_name = self
                .promise_like_return_type_argument(return_type)
                .map_or_else(
                    || self.format_type(return_type),
                    |inner| self.format_type(inner),
                );
            self.error_at_node(
                type_annotation,
                &format_message(
                    diagnostic_messages::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
                    &[&type_name],
                ),
                diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
            );
        }
    }

    /// TS1064 for async functions in JS files with `@type {function(): ReturnType}`.
    ///
    /// When a variable in a JS file has `/** @type {function(): string} */` and the
    /// initializer is an async function, tsc emits TS1064 because `string` is not
    /// `Promise<string>`. The main `check_async_return_type_is_promise` only fires
    /// when there's an AST-level return type annotation, so this method handles the
    /// JSDoc-only case.
    pub(crate) fn check_async_return_type_from_jsdoc_type(
        &mut self,
        func_idx: NodeIndex,
        func_jsdoc: &Option<String>,
    ) {
        let Some(jsdoc) = func_jsdoc else {
            return;
        };
        let Some(ret_type_str) = Self::jsdoc_type_tag_function_return_type(jsdoc) else {
            return;
        };
        let trimmed = ret_type_str.trim();
        if Self::jsdoc_return_type_is_exact_promise_reference(trimmed)
            && self.jsdoc_promise_name_resolves_to_global(func_idx)
        {
            return;
        }

        let inner_type_name = trimmed;
        let sf = self.source_file_data_for_node(func_idx);
        let span = sf.and_then(|sf| {
            let source_text: &str = &sf.text;
            let comments = &sf.comments;
            let func_node = self.ctx.arena.get(func_idx)?;
            for comment in comments.iter().rev() {
                if comment.end <= func_node.pos {
                    if tsz_common::comments::is_jsdoc_comment(comment, source_text) {
                        return Self::jsdoc_type_tag_function_return_type_span_in_source(
                            source_text,
                            comment.pos,
                        );
                    }
                    break;
                }
            }
            self.try_jsdoc_with_ancestor_walk(func_idx, comments, source_text)
                .and_then(|_jsdoc_text| {
                    let mut current = func_idx;
                    for _ in 0..4 {
                        if let Some(ext) = self.ctx.arena.get_extended(current) {
                            let parent = ext.parent;
                            if parent.is_none() {
                                break;
                            }
                            if let Some(parent_node) = self.ctx.arena.get(parent) {
                                for comment in comments.iter().rev() {
                                    if (comment.end <= parent_node.pos
                                        || (comment.pos <= parent_node.pos
                                            && comment.end <= parent_node.end))
                                        && tsz_common::comments::is_jsdoc_comment(
                                            comment,
                                            source_text,
                                        ) {
                                            return Self::jsdoc_type_tag_function_return_type_span_in_source(
                                                source_text,
                                                comment.pos,
                                            );
                                        }
                                }
                                current = parent;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    None
                })
        });
        let msg = format_message(
            diagnostic_messages::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
            &[inner_type_name],
        );
        if let Some((start, length)) = span {
            self.error_at_position(
                start,
                length,
                &msg,
                diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
            );
        } else {
            self.error_at_node(
                func_idx,
                &msg,
                diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
            );
        }
    }

    fn jsdoc_promise_name_resolves_to_global(&self, func_idx: NodeIndex) -> bool {
        if self.jsdoc_has_prior_promise_typedef(func_idx) {
            return false;
        }

        for sym_id in self
            .ctx
            .binder
            .current_scope
            .get("Promise")
            .into_iter()
            .chain(self.ctx.binder.file_locals.get("Promise"))
        {
            if !self.ctx.sym_id_is_lib_promise(sym_id)
                && !self.ctx.sym_id_is_current_cloned_lib_promise(sym_id)
            {
                return false;
            }
        }
        true
    }

    fn jsdoc_has_prior_promise_typedef(&self, func_idx: NodeIndex) -> bool {
        let Some(sf) = self.source_file_data_for_node(func_idx) else {
            return false;
        };
        let Some(func_node) = self.ctx.arena.get(func_idx) else {
            return false;
        };

        sf.comments.iter().any(|comment| {
            comment.end <= func_node.pos
                && tsz_common::comments::is_jsdoc_comment(comment, &sf.text)
                && Self::jsdoc_comment_declares_promise_typedef(
                    &sf.text[comment.pos as usize..comment.end as usize],
                )
        })
    }

    fn jsdoc_comment_declares_promise_typedef(comment: &str) -> bool {
        comment.contains("@typedef")
            && Self::parse_jsdoc_typedefs(comment)
                .iter()
                .any(|(name, _)| name == "Promise")
    }

    fn jsdoc_return_type_is_exact_promise_reference(trimmed: &str) -> bool {
        let Some(rest) = trimmed.strip_prefix("Promise") else {
            return false;
        };
        let rest = rest.trim_start();
        rest.is_empty() || rest.starts_with('<') || rest.starts_with(".<")
    }

    /// Check if a type is a type alias application that resolves to Promise.
    ///
    /// For example, `type PromiseAlias<T> = Promise<T>; async function f(): PromiseAlias<void>`
    /// -- the return type `PromiseAlias<void>` is an Application whose base is a type alias.
    /// This method resolves the alias body and checks if it references the global Promise type.
    ///
    /// This handles tsc's `isReferenceToType` semantics for TS1064, where type aliases
    /// that ultimately resolve to Promise<T> are accepted as valid async return types.
    /// It also handles merged symbols (e.g., `type MyPromise<T> = Promise<T>` combined
    /// with `declare var MyPromise: typeof Promise`) by finding the type alias declaration
    /// among the symbol's declarations.
    pub(crate) fn is_promise_type_through_alias(&mut self, type_id: TypeId) -> bool {
        use crate::query_boundaries::checkers::promise as query;
        use tsz_binder::symbol_flags;

        // Must be an Application type
        let query::PromiseTypeKind::Application { base, .. } =
            query::classify_promise_type(self.ctx.types, type_id)
        else {
            return false;
        };

        // Check if the base is a Lazy(DefId) pointing to a type alias
        let def_id = match query::classify_promise_type(self.ctx.types, base) {
            query::PromiseTypeKind::Lazy(def_id) => def_id,
            _ => return false,
        };

        let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Only handle type aliases (not classes/interfaces)
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return false;
        }

        // Get the alias body type using type_reference_symbol_type_with_params which
        // correctly handles merged symbols (e.g., `type MyPromise<T> = Promise<T>`
        // merged with `declare var MyPromise: typeof Promise`). It finds the type
        // alias declaration in the symbol's declarations list.
        let (body_type, _params) = self.type_reference_symbol_type_with_params(sym_id);
        if self.is_global_promise_type(body_type) {
            return true;
        }

        // The body might itself be an Application (e.g., `Promise<T>`)
        // Check if the Application base refers to the global Promise type
        if let query::PromiseTypeKind::Application {
            base: body_base, ..
        } = query::classify_promise_type(self.ctx.types, body_type)
        {
            // Check if the body's base is Promise
            return self.is_global_promise_type(body_base);
        }

        false
    }
}
