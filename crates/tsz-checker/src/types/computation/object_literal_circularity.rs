use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{CallSignature, CallableShape, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    pub(super) fn build_object_literal_method_synthetic_this_type(
        &mut self,
        properties: &rustc_hash::FxHashMap<tsz_common::interner::Atom, tsz_solver::PropertyInfo>,
        obj_all_method_names: &rustc_hash::FxHashMap<tsz_common::interner::Atom, (NodeIndex, u32)>,
        current_method_idx: NodeIndex,
        current_method_name: &str,
        current_method_type_override: Option<TypeId>,
    ) -> TypeId {
        let mut this_props: Vec<tsz_solver::PropertyInfo> = properties.values().cloned().collect();

        if self.ctx.in_const_assertion {
            for prop in &mut this_props {
                prop.readonly = true;
            }
        }

        let current_method_name_atom = self.ctx.types.intern_string(current_method_name);
        for (&method_name_atom, &(other_elem_idx, decl_order)) in obj_all_method_names {
            if this_props.iter().any(|p| p.name == method_name_atom) {
                continue;
            }

            let method_type = if method_name_atom == current_method_name_atom {
                if let Some(override_type) = current_method_type_override {
                    override_type
                } else {
                    let Some(current_method_node) = self.ctx.arena.get(current_method_idx) else {
                        continue;
                    };
                    let Some(current_method) = self.ctx.arena.get_method_decl(current_method_node)
                    else {
                        continue;
                    };
                    let (_, tp_updates) =
                        self.push_type_parameters(&current_method.type_parameters);
                    let params = current_method
                        .parameters
                        .nodes
                        .iter()
                        .filter_map(|&param_idx| {
                            let param =
                                self.ctx.arena.get(param_idx).and_then(|param_node| {
                                    self.ctx.arena.get_parameter(param_node)
                                })?;
                            Some(tsz_solver::ParamInfo {
                                name: self
                                    .ctx
                                    .arena
                                    .get(param.name)
                                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                                    .map(|ident| self.ctx.types.intern_string(&ident.escaped_text)),
                                type_id: if param.type_annotation.is_some() {
                                    self.get_type_from_type_node(param.type_annotation)
                                } else {
                                    TypeId::ANY
                                },
                                optional: param.question_token || param.initializer.is_some(),
                                rest: param.dot_dot_dot_token,
                            })
                        })
                        .collect();
                    let placeholder = self.ctx.types.factory().callable(CallableShape {
                        call_signatures: vec![CallSignature {
                            type_params: Vec::new(),
                            params,
                            this_type: None,
                            return_type: TypeId::VOID,
                            type_predicate: None,
                            is_method: true,
                        }],
                        construct_signatures: Vec::new(),
                        properties: Vec::new(),
                        string_index: None,
                        number_index: None,
                        symbol: None,
                        is_abstract: false,
                    });
                    self.pop_type_parameters(tp_updates);
                    placeholder
                }
            } else {
                let (other_params, other_return_type) = self
                    .ctx
                    .arena
                    .get(other_elem_idx)
                    .and_then(|n| self.ctx.arena.get_method_decl(n))
                    .map(|other_method| {
                        let params: Vec<tsz_solver::ParamInfo> = other_method
                            .parameters
                            .nodes
                            .iter()
                            .filter_map(|&param_idx| {
                                let param = self
                                    .ctx
                                    .arena
                                    .get(param_idx)
                                    .and_then(|pn| self.ctx.arena.get_parameter(pn))?;
                                if let Some(name_node) = self.ctx.arena.get(param.name)
                                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                    && ident.escaped_text == "this"
                                {
                                    return None;
                                }
                                Some(tsz_solver::ParamInfo {
                                    name: self
                                        .ctx
                                        .arena
                                        .get(param.name)
                                        .and_then(|name_node| {
                                            self.ctx.arena.get_identifier(name_node)
                                        })
                                        .map(|ident| {
                                            self.ctx.types.intern_string(&ident.escaped_text)
                                        }),
                                    type_id: if param.type_annotation.is_some() {
                                        self.get_type_from_type_node(param.type_annotation)
                                    } else {
                                        TypeId::ANY
                                    },
                                    optional: param.question_token || param.initializer.is_some(),
                                    rest: param.dot_dot_dot_token,
                                })
                            })
                            .collect();
                        let return_type = if other_method.type_annotation.is_some() {
                            self.get_type_from_type_node(other_method.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        (params, return_type)
                    })
                    .unwrap_or_else(|| {
                        (
                            vec![tsz_solver::ParamInfo {
                                name: None,
                                type_id: TypeId::ANY,
                                optional: false,
                                rest: true,
                            }],
                            TypeId::ANY,
                        )
                    });

                self.ctx.types.factory().callable(CallableShape {
                    call_signatures: vec![CallSignature {
                        type_params: Vec::new(),
                        params: other_params,
                        this_type: None,
                        return_type: other_return_type,
                        type_predicate: None,
                        is_method: true,
                    }],
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                })
            };

            this_props.push(tsz_solver::PropertyInfo {
                name: method_name_atom,
                type_id: method_type,
                write_type: method_type,
                optional: false,
                readonly: self.ctx.in_const_assertion,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: decl_order,
            });
        }

        self.ctx.types.factory().object(this_props)
    }

    /// Build a synthetic `this` type for a function expression that is a property
    /// initializer in an object literal. Similar to `build_object_literal_method_synthetic_this_type`
    /// but for property assignments like `{ prop: function() { this.n } }`.
    ///
    /// The synthetic type includes:
    /// - All already-processed properties from the object literal
    /// - Placeholder signatures for pre-scanned method declarations
    pub(super) fn build_object_literal_fn_property_synthetic_this_type(
        &mut self,
        properties: &rustc_hash::FxHashMap<tsz_common::interner::Atom, tsz_solver::PropertyInfo>,
        obj_all_method_names: &rustc_hash::FxHashMap<tsz_common::interner::Atom, (NodeIndex, u32)>,
        _current_property_name: &str,
    ) -> TypeId {
        let mut this_props: Vec<tsz_solver::PropertyInfo> = properties.values().cloned().collect();

        if self.ctx.in_const_assertion {
            for prop in &mut this_props {
                prop.readonly = true;
            }
        }

        // Add placeholder callable types for pre-scanned method declarations
        for (&method_name_atom, &(other_elem_idx, decl_order)) in obj_all_method_names {
            if this_props.iter().any(|p| p.name == method_name_atom) {
                continue;
            }

            let (other_params, other_return_type) = self
                .ctx
                .arena
                .get(other_elem_idx)
                .and_then(|n| self.ctx.arena.get_method_decl(n))
                .map(|other_method| {
                    let params: Vec<tsz_solver::ParamInfo> = other_method
                        .parameters
                        .nodes
                        .iter()
                        .filter_map(|&param_idx| {
                            let param = self
                                .ctx
                                .arena
                                .get(param_idx)
                                .and_then(|pn| self.ctx.arena.get_parameter(pn))?;
                            if let Some(name_node) = self.ctx.arena.get(param.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && ident.escaped_text == "this"
                            {
                                return None;
                            }
                            Some(tsz_solver::ParamInfo {
                                name: self
                                    .ctx
                                    .arena
                                    .get(param.name)
                                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                                    .map(|ident| self.ctx.types.intern_string(&ident.escaped_text)),
                                type_id: if param.type_annotation.is_some() {
                                    self.get_type_from_type_node(param.type_annotation)
                                } else {
                                    TypeId::ANY
                                },
                                optional: param.question_token || param.initializer.is_some(),
                                rest: param.dot_dot_dot_token,
                            })
                        })
                        .collect();
                    let return_type = if other_method.type_annotation.is_some() {
                        self.get_type_from_type_node(other_method.type_annotation)
                    } else {
                        TypeId::ANY
                    };
                    (params, return_type)
                })
                .unwrap_or_else(|| {
                    (
                        vec![tsz_solver::ParamInfo {
                            name: None,
                            type_id: TypeId::ANY,
                            optional: false,
                            rest: true,
                        }],
                        TypeId::ANY,
                    )
                });

            this_props.push(tsz_solver::PropertyInfo {
                name: method_name_atom,
                type_id: self.ctx.types.factory().callable(CallableShape {
                    call_signatures: vec![CallSignature {
                        type_params: Vec::new(),
                        params: other_params,
                        this_type: None,
                        return_type: other_return_type,
                        type_predicate: None,
                        is_method: true,
                    }],
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                }),
                write_type: TypeId::ANY,
                optional: false,
                readonly: self.ctx.in_const_assertion,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: decl_order,
            });
        }

        self.ctx.types.factory().object(this_props)
    }

    pub(super) fn widen_primitive_literal_type_display(display: &str) -> String {
        let bytes = display.as_bytes();
        let mut out = String::with_capacity(display.len());
        let mut i = 0usize;

        while i < bytes.len() {
            if bytes[i] != b':' {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }

            out.push(':');
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                out.push(bytes[i] as char);
                i += 1;
            }

            if i >= bytes.len() {
                break;
            }

            if bytes[i] == b'"' {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.push_str("string");
                continue;
            }

            let rest = &display[i..];
            let literal_match = if rest.starts_with("true")
                && rest[4..]
                    .chars()
                    .next()
                    .is_none_or(|ch| matches!(ch, ';' | ',' | '}' | ']' | ')' | ' '))
            {
                Some((4usize, "boolean"))
            } else if rest.starts_with("false")
                && rest[5..]
                    .chars()
                    .next()
                    .is_none_or(|ch| matches!(ch, ';' | ',' | '}' | ']' | ')' | ' '))
            {
                Some((5usize, "boolean"))
            } else {
                let mut end = i;
                if bytes[end] == b'-' {
                    end += 1;
                }
                while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
                    end += 1;
                }
                if end > i
                    && display[end..]
                        .chars()
                        .next()
                        .is_none_or(|ch| matches!(ch, ';' | ',' | '}' | ']' | ')' | ' '))
                {
                    Some((end - i, "number"))
                } else {
                    None
                }
            };

            if let Some((len, widened)) = literal_match {
                i += len;
                out.push_str(widened);
                continue;
            }

            out.push(bytes[i] as char);
            i += 1;
        }

        out
    }
}
