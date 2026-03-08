//! TC39 (non-legacy) Decorator Transform
//!
//! Transforms decorated classes using the TC39 decorator protocol.
//! For ES2015 targets, outputs an IIFE with comma-separated decorator application.
//! For ES2022+ targets, uses static initializer blocks.

use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Information about a decorated member
#[derive(Debug, Clone)]
struct DecoratedMember {
    /// The member node index
    member_idx: NodeIndex,
    /// The member kind for the decorator context
    kind: MemberKind,
    /// Name of the member
    name: MemberName,
    /// Whether the member is static
    is_static: bool,
    /// Whether the member is private (#name)
    is_private: bool,
    /// Decorator expression texts (e.g. ["dec(1)"])
    decorator_exprs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum MemberKind {
    Method,
    Getter,
    Setter,
    Field,
    Accessor,
}

#[derive(Debug, Clone)]
enum MemberName {
    /// Simple identifier: `method1`
    Identifier(String),
    /// String literal in computed position: `["method2"]`
    StringLiteral(String),
    /// Computed expression: `[expr]` - needs `__propKey`
    Computed(NodeIndex),
    /// Private identifier: `#method1`
    Private(String),
}

/// TC39 Decorator Emitter
pub struct TC39DecoratorEmitter<'a> {
    arena: &'a NodeArena,
    source_text: Option<&'a str>,
    indent: usize,
}

impl<'a> TC39DecoratorEmitter<'a> {
    pub const fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            source_text: None,
            indent: 0,
        }
    }

    pub const fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    pub const fn set_indent_level(&mut self, level: usize) {
        self.indent = level;
    }

    /// Emit the TC39 decorator transform for a class declaration.
    pub fn emit_class(&self, class_idx: NodeIndex) -> String {
        let Some(class_node) = self.arena.get(class_idx) else {
            return String::new();
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return String::new();
        };

        let class_name = self
            .get_identifier_text(class_data.name)
            .unwrap_or_default();
        let class_decorators = self.collect_class_decorator_exprs(&class_data.modifiers);
        let decorated_members = self.collect_decorated_members(&class_data.members);
        let has_extends = self.has_extends_clause(&class_data.heritage_clauses);

        let has_any_instance = decorated_members.iter().any(|m| !m.is_static);
        let has_any_static = decorated_members.iter().any(|m| m.is_static);

        // Compute temp var allocation
        let mut temp_counter: u32 = 0;
        let class_alias = next_temp_var(&mut temp_counter); // _a

        // Compute propKey temp vars for computed members
        let mut computed_key_vars: Vec<(usize, String)> = Vec::new();
        for (i, member) in decorated_members.iter().enumerate() {
            if let MemberName::Computed(_) = &member.name {
                let var = next_temp_var(&mut temp_counter);
                computed_key_vars.push((i, var));
            }
        }

        // Compute member variable names
        let member_vars = self.compute_all_member_vars(&decorated_members);

        let mut out = String::new();
        let i1 = indent_str(self.indent + 1);
        let i2 = indent_str(self.indent + 2);
        let i3 = indent_str(self.indent + 3);
        let i4 = indent_str(self.indent + 4);

        // --- IIFE header ---
        out.push_str(&format!("let {class_name} = (() => {{\n"));

        // Var declarations: class alias on its own line, computed key vars combined
        out.push_str(&format!("{i1}var {class_alias};\n"));
        if !computed_key_vars.is_empty() {
            let key_names: Vec<&str> = computed_key_vars.iter().map(|(_, v)| v.as_str()).collect();
            out.push_str(&format!("{i1}var {};\n", key_names.join(", ")));
        }

        // Class decorator variables
        if !class_decorators.is_empty() {
            out.push_str(&format!(
                "{i1}let _classDecorators = [{}];\n",
                class_decorators.join(", ")
            ));
            out.push_str(&format!("{i1}let _classDescriptor;\n"));
            out.push_str(&format!("{i1}let _classExtraInitializers = [];\n"));
            out.push_str(&format!("{i1}let _classThis;\n"));
        }

        // Instance/static extra initializer arrays
        if has_any_instance {
            out.push_str(&format!("{i1}let _instanceExtraInitializers = [];\n"));
        }
        if has_any_static {
            out.push_str(&format!("{i1}let _staticExtraInitializers = [];\n"));
        }

        // Per-member decorator and initializer variables
        for var_info in &member_vars {
            out.push_str(&format!("{i1}let {};\n", var_info.decorators_var));
            if var_info.has_initializers {
                out.push_str(&format!(
                    "{i1}let {} = [];\n",
                    var_info.initializers_var.as_ref().unwrap()
                ));
                out.push_str(&format!(
                    "{i1}let {} = [];\n",
                    var_info.extra_initializers_var.as_ref().unwrap()
                ));
            }
            if var_info.has_descriptor {
                out.push_str(&format!(
                    "{i1}let {};\n",
                    var_info.descriptor_var.as_ref().unwrap()
                ));
            }
        }

        // --- Class expression ---
        out.push_str(&format!("{i1}return {class_alias} = class {class_name}"));
        if has_extends && let Some(extends_text) = self.get_extends_text(class_data) {
            out.push_str(&format!(" extends {extends_text}"));
        }
        out.push_str(" {\n");

        // --- Emit class members ---
        self.emit_class_body(
            class_node,
            class_data,
            &decorated_members,
            &member_vars,
            &computed_key_vars,
            has_any_instance,
            has_any_static,
            &class_alias,
            &i3,
            &i4,
            &mut out,
        );

        // Close class body at level 2
        out.push_str(&format!("{i2}}},\n"));

        // --- Decorator application IIFE ---
        out.push_str(&format!("{i2}(() => {{\n"));

        // Metadata
        if has_extends {
            if let Some(extends_text) = self.get_extends_text(class_data) {
                out.push_str(&format!("{i3}const _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create({extends_text}[Symbol.metadata] ?? null) : void 0;\n"));
            } else {
                out.push_str(&format!("{i3}const _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"));
            }
        } else {
            out.push_str(&format!("{i3}const _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"));
        }

        // __esDecorate calls for each member
        for (i, member) in decorated_members.iter().enumerate() {
            let var_info = &member_vars[i];
            self.emit_es_decorate_call(
                member,
                var_info,
                &class_alias,
                &computed_key_vars,
                i,
                &i3,
                &mut out,
            );
        }

        // Class-level __esDecorate if needed
        if !class_decorators.is_empty() {
            out.push_str(&format!("{i3}__esDecorate(null, _classDescriptor = {{ value: _classThis }}, _classDecorators, {{ kind: \"class\", name: _classThis.name, metadata: _metadata }}, null, _classExtraInitializers);\n"));
            out.push_str(&format!(
                "{i3}{class_name} = _classThis = _classDescriptor.value;\n"
            ));
        }

        // Metadata assignment
        out.push_str(&format!("{i3}if (_metadata) Object.defineProperty({class_alias}, Symbol.metadata, {{ enumerable: true, configurable: true, writable: true, value: _metadata }});\n"));

        // Static extra initializers
        if has_any_static {
            out.push_str(&format!(
                "{i3}__runInitializers({class_alias}, _staticExtraInitializers);\n"
            ));
        }

        out.push_str(&format!("{i2}}})(),\n"));

        // Return class alias
        out.push_str(&format!("{i2}{class_alias};\n"));

        // Close IIFE
        out.push_str("})();\n");

        out
    }

    /// Emit class body members.
    ///
    /// The key trick: ALL decorator assignment expressions are collected and placed
    /// inside the last computed member's name brackets as a comma expression.
    /// If no computed member exists, a synthetic `[(...)]() { }` is added.
    #[allow(clippy::too_many_arguments)]
    fn emit_class_body(
        &self,
        class_node: &tsz_parser::parser::node::Node,
        class_data: &tsz_parser::parser::node::ClassData,
        decorated_members: &[DecoratedMember],
        member_vars: &[MemberVarInfo],
        computed_key_vars: &[(usize, String)],
        has_any_instance: bool,
        _has_any_static: bool,
        _class_alias: &str,
        indent: &str,
        inner_indent: &str,
        out: &mut String,
    ) {
        // Build the decorator assignment comma expression that goes in the sink member
        let mut assignment_parts: Vec<String> = Vec::new();
        for (i, member) in decorated_members.iter().enumerate() {
            let var_info = &member_vars[i];
            let dec_exprs = member.decorator_exprs.join(", ");
            assignment_parts.push(format!("{} = [{}]", var_info.decorators_var, dec_exprs));
        }
        // Add computed key assignments
        for (member_i, var_name) in computed_key_vars {
            if let Some(member) = decorated_members.get(*member_i)
                && let MemberName::Computed(expr_idx) = &member.name
            {
                assignment_parts.push(format!(
                    "{var_name} = __propKey({})",
                    self.node_text(*expr_idx)
                ));
            }
        }

        let needs_sink = !assignment_parts.is_empty();
        let sink_expr = assignment_parts.join(", ");

        // Emit each member (excluding constructors and index signatures).
        // Use the NEXT sibling's pos as the end boundary for each member.
        // This avoids relying on member_node.end which includes trailing trivia.
        let all_members: Vec<_> = class_data
            .members
            .nodes
            .iter()
            .filter_map(|&idx| self.arena.get(idx).map(|n| (idx, n)))
            .collect();

        // Members with computed keys needing __propKey are folded into the sink
        let propkey_member_indices: Vec<NodeIndex> = decorated_members
            .iter()
            .filter(|m| matches!(m.name, MemberName::Computed(_)))
            .map(|m| m.member_idx)
            .collect();

        let emittable: Vec<usize> = all_members
            .iter()
            .enumerate()
            .filter(|(_, (idx, node))| {
                node.kind != syntax_kind_ext::CONSTRUCTOR
                    && node.kind != syntax_kind_ext::INDEX_SIGNATURE
                    && node.kind != syntax_kind_ext::SEMICOLON_CLASS_ELEMENT
                    && !propkey_member_indices.contains(idx)
            })
            .map(|(i, _)| i)
            .collect();

        for &emit_i in &emittable {
            let (_, member_node) = all_members[emit_i];
            // Find next sibling in the full member list (not just emittable ones)
            let next_boundary = if emit_i + 1 < all_members.len() {
                all_members[emit_i + 1].1.pos as usize
            } else {
                // Last member: use class_node.end and scan backwards for `}`
                self.find_class_close_brace(class_node)
            };
            let member_text = self.emit_member_bounded(member_node, next_boundary);
            out.push_str(&format!("{indent}{member_text}\n"));
        }

        // Emit the sink computed member with all decorator assignments
        if needs_sink {
            out.push_str(&format!("{indent}[({sink_expr})]() {{ }}\n"));
        }

        // Emit constructor
        out.push_str(&format!("{indent}constructor("));
        if let Some(ctor) = self.get_constructor_info(class_data) {
            out.push_str(&ctor.params);
            out.push_str(") {\n");
            for line in &ctor.body_lines {
                out.push_str(&format!("{inner_indent}{}\n", line.trim()));
            }
            if has_any_instance {
                out.push_str(&format!(
                    "{inner_indent}__runInitializers(this, _instanceExtraInitializers);\n"
                ));
            }
            out.push_str(&format!("{indent}}}\n"));
        } else {
            out.push_str(") {\n");
            if has_any_instance {
                out.push_str(&format!(
                    "{inner_indent}__runInitializers(this, _instanceExtraInitializers);\n"
                ));
            }
            out.push_str(&format!("{indent}}}\n"));
        }
    }

    /// Find the position of the class closing brace by scanning backwards from end.
    fn find_class_close_brace(&self, class_node: &tsz_parser::parser::node::Node) -> usize {
        let Some(source) = self.source_text else {
            return class_node.end as usize;
        };
        let bytes = source.as_bytes();
        let mut pos = class_node.end as usize;
        // Scan backwards past whitespace and the closing `}`
        while pos > 0 && matches!(bytes[pos - 1], b' ' | b'\t' | b'\n' | b'\r') {
            pos -= 1;
        }
        // Now pos should be at `}` (the class closing brace) - skip past it
        if pos > 0 && bytes[pos - 1] == b'}' {
            pos -= 1;
        }
        pos
    }

    /// Emit a single member with decorators stripped, bounded by the next member's start.
    /// Uses AST positions for the clean start and the next member's position as end boundary.
    fn emit_member_bounded(
        &self,
        member_node: &tsz_parser::parser::node::Node,
        next_boundary: usize,
    ) -> String {
        let Some(source) = self.source_text else {
            return String::new();
        };

        let clean_start = self.find_member_clean_start(member_node);
        // Use the minimum of member.end and next_boundary, then trim
        let raw_end = std::cmp::min(member_node.end as usize, next_boundary);

        if clean_start < source.len() && raw_end <= source.len() && clean_start < raw_end {
            let text = source[clean_start..raw_end].trim();
            if let Some(stripped) = text.strip_suffix("{}") {
                format!("{stripped}{{ }}")
            } else {
                text.to_string()
            }
        } else {
            String::new()
        }
    }

    /// Find the position in source text where the "clean" (non-decorator, non-TS-modifier)
    /// part of a class member begins.
    fn find_member_clean_start(&self, member_node: &tsz_parser::parser::node::Node) -> usize {
        let (modifiers, name_idx) = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node);
                (
                    data.as_ref().and_then(|m| m.modifiers.clone()),
                    data.map(|m| m.name),
                )
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let data = self.arena.get_property_decl(member_node);
                (
                    data.as_ref().and_then(|p| p.modifiers.clone()),
                    data.map(|p| p.name),
                )
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node);
                (
                    data.as_ref().and_then(|a| a.modifiers.clone()),
                    data.map(|a| a.name),
                )
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node);
                (
                    data.as_ref().and_then(|a| a.modifiers.clone()),
                    data.map(|a| a.name),
                )
            }
            _ => (None, None),
        };

        let Some(mods) = modifiers else {
            return member_node.pos as usize;
        };

        let ts_only_kinds: &[u16] = &[
            SyntaxKind::AbstractKeyword as u16,
            SyntaxKind::DeclareKeyword as u16,
            SyntaxKind::ReadonlyKeyword as u16,
            SyntaxKind::OverrideKeyword as u16,
            SyntaxKind::PublicKeyword as u16,
            SyntaxKind::PrivateKeyword as u16,
            SyntaxKind::ProtectedKeyword as u16,
            SyntaxKind::AccessorKeyword as u16,
        ];

        // Find the first JS-visible modifier (static, async, etc.)
        for &mod_idx in &mods.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind != syntax_kind_ext::DECORATOR
                && !ts_only_kinds.contains(&mod_node.kind)
            {
                // JS-visible modifier - start from its position
                return mod_node.pos as usize;
            }
        }

        // All modifiers are decorators/TS-only.
        // Use the name node position as the reliable anchor, but for GET_ACCESSOR
        // and SET_ACCESSOR we must include the `get`/`set` keyword which precedes
        // the name in the source text and is NOT stored as a modifier.
        if let Some(idx) = name_idx
            && let Some(name_node) = self.arena.get(idx)
        {
            let name_pos = name_node.pos as usize;
            let is_accessor = member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR;
            if is_accessor
                && let Some(source) = self.source_text {
                    // Scan backwards from name position to find 'get' or 'set' keyword
                    let keyword = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        "get"
                    } else {
                        "set"
                    };
                    // Allow generous whitespace between keyword and name
                    let search_start = name_pos.saturating_sub(keyword.len() + 20);
                    // Look for the keyword in the text before the name
                    if let Some(kw_offset) = source[search_start..name_pos].rfind(keyword) {
                        return search_start + kw_offset;
                    }
                }
            return name_pos;
        }

        member_node.pos as usize
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            self.arena
                .get_identifier(node)
                .map(|id| id.escaped_text.clone())
        } else {
            None
        }
    }

    fn node_text(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        let Some(source) = self.source_text else {
            return String::new();
        };
        let start = node.pos as usize;
        let end = node.end as usize;
        if start < source.len() && end <= source.len() && start < end {
            source[start..end].trim().to_string()
        } else {
            String::new()
        }
    }

    fn collect_class_decorator_exprs(&self, modifiers: &Option<NodeList>) -> Vec<String> {
        let Some(mods) = modifiers else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for &idx in &mods.nodes {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::DECORATOR
                && let Some(dec) = self.arena.get_decorator(node)
            {
                result.push(self.node_text(dec.expression));
            }
        }
        result
    }

    fn collect_decorated_members(&self, members: &NodeList) -> Vec<DecoratedMember> {
        let mut result = Vec::new();

        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let (modifiers, name_idx, kind) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (method.modifiers.clone(), method.name, MemberKind::Method)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let kind = if self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                    {
                        MemberKind::Accessor
                    } else {
                        MemberKind::Field
                    };
                    (prop.modifiers.clone(), prop.name, kind)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    let Some(acc) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (acc.modifiers.clone(), acc.name, MemberKind::Getter)
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(acc) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (acc.modifiers.clone(), acc.name, MemberKind::Setter)
                }
                _ => continue,
            };

            // Collect decorator expressions
            let mut decorator_exprs = Vec::new();
            if let Some(ref mods) = modifiers {
                for &mod_idx in &mods.nodes {
                    let Some(mod_node) = self.arena.get(mod_idx) else {
                        continue;
                    };
                    if mod_node.kind == syntax_kind_ext::DECORATOR
                        && let Some(dec) = self.arena.get_decorator(mod_node)
                    {
                        decorator_exprs.push(self.node_text(dec.expression));
                    }
                }
            }
            if decorator_exprs.is_empty() {
                continue;
            }

            let is_static = self
                .arena
                .has_modifier(&modifiers, SyntaxKind::StaticKeyword);
            let (name, is_private) = self.resolve_member_name(name_idx);

            result.push(DecoratedMember {
                member_idx,
                kind,
                name,
                is_static,
                is_private,
                decorator_exprs,
            });
        }

        result
    }

    fn resolve_member_name(&self, name_idx: NodeIndex) -> (MemberName, bool) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return (MemberName::Identifier(String::new()), false);
        };

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let text = self
                    .arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone())
                    .unwrap_or_default();
                (MemberName::Identifier(text), false)
            }
            k if k == SyntaxKind::PrivateIdentifier as u16 => {
                let text = self
                    .arena
                    .get_identifier(name_node)
                    .map(|id| id.escaped_text.clone())
                    .unwrap_or_default();
                (MemberName::Private(text), true)
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                let Some(computed) = self.arena.get_computed_property(name_node) else {
                    return (MemberName::Identifier(String::new()), false);
                };
                // Check if computed expression is a string literal
                if let Some(expr_node) = self.arena.get(computed.expression)
                    && expr_node.kind == SyntaxKind::StringLiteral as u16
                    && let Some(lit) = self.arena.get_literal(expr_node)
                {
                    return (MemberName::StringLiteral(lit.text.clone()), false);
                }
                (MemberName::Computed(computed.expression), false)
            }
            _ => (MemberName::Identifier(String::new()), false),
        }
    }

    fn has_extends_clause(&self, heritage: &Option<NodeList>) -> bool {
        let Some(clauses) = heritage else {
            return false;
        };
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            if let Some(h) = self.arena.get_heritage_clause(clause_node)
                && h.token == SyntaxKind::ExtendsKeyword as u16
            {
                return true;
            }
        }
        false
    }

    fn get_extends_text(&self, class_data: &tsz_parser::parser::node::ClassData) -> Option<String> {
        let clauses = class_data.heritage_clauses.as_ref()?;
        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage = self.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let first_type = heritage.types.nodes.first()?;
            let type_node = self.arena.get(*first_type)?;
            if let Some(expr_data) = self.arena.get_expr_type_args(type_node) {
                return Some(self.node_text(expr_data.expression));
            }
            return Some(self.node_text(*first_type));
        }
        None
    }

    fn compute_all_member_vars(&self, members: &[DecoratedMember]) -> Vec<MemberVarInfo> {
        let mut counter: u32 = 0;
        // Track the last seen computed/string member name to group getter/setter pairs.
        // tsc only increments the suffix counter between different member names.
        let mut last_computed_name: Option<String> = None;
        members
            .iter()
            .map(|m| self.compute_member_var_info(m, &mut counter, &mut last_computed_name))
            .collect()
    }

    fn compute_member_var_info(
        &self,
        member: &DecoratedMember,
        counter: &mut u32,
        last_computed_name: &mut Option<String>,
    ) -> MemberVarInfo {
        let base_name = match &member.name {
            MemberName::Identifier(name) => name.clone(),
            MemberName::Private(name) => format!("private_{}", name.trim_start_matches('#')),
            MemberName::StringLiteral(_) | MemberName::Computed(_) => "member".to_string(),
        };

        let prefix = if member.is_static { "static_" } else { "" };
        let kind_prefix = match member.kind {
            MemberKind::Getter => "get_",
            MemberKind::Setter => "set_",
            _ => "",
        };

        let var_base = format!("_{kind_prefix}{prefix}{base_name}");

        // For computed/string members, only increment counter on NEW member names.
        // Getter/setter pairs with the same name share the same suffix.
        let is_computed_or_string = matches!(
            member.name,
            MemberName::StringLiteral(_) | MemberName::Computed(_)
        );

        if is_computed_or_string {
            let current_name = match &member.name {
                MemberName::StringLiteral(s) => s.clone(),
                MemberName::Computed(idx) => self.node_text(*idx),
                _ => unreachable!(),
            };
            let is_new_name = last_computed_name
                .as_ref()
                .is_none_or(|prev| *prev != current_name);
            if is_new_name {
                if last_computed_name.is_some() {
                    *counter += 1;
                }
                *last_computed_name = Some(current_name);
            }
        }

        let suffix = if *counter > 0 && is_computed_or_string {
            format!("_{}", *counter)
        } else {
            String::new()
        };

        let decorators_var = format!("{var_base}_decorators{suffix}");
        let has_field_inits = matches!(member.kind, MemberKind::Field | MemberKind::Accessor);
        let has_descriptor = member.is_private && matches!(member.kind, MemberKind::Method);

        MemberVarInfo {
            decorators_var,
            has_initializers: has_field_inits,
            initializers_var: if has_field_inits {
                Some(format!("{var_base}_initializers{suffix}"))
            } else {
                None
            },
            extra_initializers_var: if has_field_inits {
                Some(format!("{var_base}_extraInitializers{suffix}"))
            } else {
                None
            },
            has_descriptor,
            descriptor_var: if has_descriptor {
                Some(format!("{var_base}_descriptor{suffix}"))
            } else {
                None
            },
        }
    }

    fn emit_es_decorate_call(
        &self,
        member: &DecoratedMember,
        var_info: &MemberVarInfo,
        class_alias: &str,
        computed_key_vars: &[(usize, String)],
        member_index: usize,
        indent: &str,
        out: &mut String,
    ) {
        let kind_str = match member.kind {
            MemberKind::Method => "method",
            MemberKind::Getter => "getter",
            MemberKind::Setter => "setter",
            MemberKind::Field => "field",
            MemberKind::Accessor => "accessor",
        };

        let name_str = self.member_name_for_context(member, computed_key_vars, member_index);
        let access_str = self.member_access_for_context(member, computed_key_vars, member_index);

        let ctor_arg = if member.is_private {
            "null".to_string()
        } else {
            class_alias.to_string()
        };

        let extra_init_arg = if member.is_static {
            "_staticExtraInitializers"
        } else {
            "_instanceExtraInitializers"
        };

        out.push_str(&format!(
            "{indent}__esDecorate({ctor_arg}, null, {}, {{ kind: \"{kind_str}\", name: {name_str}, static: {}, private: {}, access: {{ {access_str} }}, metadata: _metadata }}, null, {extra_init_arg});\n",
            var_info.decorators_var,
            member.is_static,
            member.is_private,
        ));
    }

    fn member_name_for_context(
        &self,
        member: &DecoratedMember,
        computed_key_vars: &[(usize, String)],
        member_index: usize,
    ) -> String {
        match &member.name {
            MemberName::Identifier(name)
            | MemberName::StringLiteral(name)
            | MemberName::Private(name) => format!("\"{name}\""),
            MemberName::Computed(_) => computed_key_vars
                .iter()
                .find(|(i, _)| *i == member_index)
                .map(|(_, var)| var.clone())
                .unwrap_or_else(|| "undefined".to_string()),
        }
    }

    fn member_access_for_context(
        &self,
        member: &DecoratedMember,
        computed_key_vars: &[(usize, String)],
        member_index: usize,
    ) -> String {
        let key_expr = match &member.name {
            MemberName::Identifier(name) | MemberName::StringLiteral(name) => {
                format!("\"{name}\"")
            }
            MemberName::Private(name) => name.clone(),
            MemberName::Computed(_) => computed_key_vars
                .iter()
                .find(|(i, _)| *i == member_index)
                .map(|(_, var)| var.clone())
                .unwrap_or_else(|| "undefined".to_string()),
        };

        let is_simple_ident = matches!(member.name, MemberName::Identifier(_));
        let prop_access = if is_simple_ident {
            if let MemberName::Identifier(name) = &member.name {
                format!("obj.{name}")
            } else {
                unreachable!()
            }
        } else {
            format!("obj[{key_expr}]")
        };

        let has_in = format!("{key_expr} in obj");

        match member.kind {
            MemberKind::Method | MemberKind::Getter => {
                format!("has: obj => {has_in}, get: obj => {prop_access}")
            }
            MemberKind::Setter => {
                format!("has: obj => {has_in}, set: (obj, value) => {{ {prop_access} = value; }}")
            }
            MemberKind::Field | MemberKind::Accessor => {
                format!(
                    "has: obj => {has_in}, get: obj => {prop_access}, set: (obj, value) => {{ {prop_access} = value; }}"
                )
            }
        }
    }

    fn get_constructor_info(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Option<ConstructorInfo> {
        for &member_idx in &class_data.members.nodes {
            let member_node = self.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let func = self.arena.get_function(member_node)?;
            let source = self.source_text?;

            let params = if func.parameters.nodes.is_empty() {
                String::new()
            } else {
                let mut param_texts = Vec::new();
                for &param_idx in &func.parameters.nodes {
                    let param_node = self.arena.get(param_idx)?;
                    let param_data = self.arena.get_parameter(param_node)?;
                    let name_text = self.node_text(param_data.name);
                    if param_data.initializer.is_some() {
                        let init_text = self.node_text(param_data.initializer);
                        param_texts.push(format!("{name_text} = {init_text}"));
                    } else if param_data.dot_dot_dot_token {
                        param_texts.push(format!("...{name_text}"));
                    } else {
                        param_texts.push(name_text);
                    }
                }
                param_texts.join(", ")
            };

            if func.body.is_none() {
                return Some(ConstructorInfo {
                    params,
                    body_lines: Vec::new(),
                });
            }
            let body_node = self.arena.get(func.body)?;
            let block = self.arena.get_block(body_node)?;
            let mut body_lines = Vec::new();
            for &stmt_idx in &block.statements.nodes {
                let stmt_node = self.arena.get(stmt_idx)?;
                let start = stmt_node.pos as usize;
                let end = stmt_node.end as usize;
                if start < source.len() && end <= source.len() && start < end {
                    body_lines.push(source[start..end].trim().to_string());
                }
            }
            return Some(ConstructorInfo { params, body_lines });
        }
        None
    }
}

fn indent_str(level: usize) -> String {
    "    ".repeat(level)
}

fn next_temp_var(counter: &mut u32) -> String {
    let name = format!("_{}", (b'a' + (*counter % 26) as u8) as char);
    *counter += 1;
    name
}

struct MemberVarInfo {
    decorators_var: String,
    has_initializers: bool,
    initializers_var: Option<String>,
    extra_initializers_var: Option<String>,
    has_descriptor: bool,
    descriptor_var: Option<String>,
}

struct ConstructorInfo {
    params: String,
    body_lines: Vec<String>,
}

#[cfg(test)]
#[path = "../../tests/es_decorators.rs"]
mod tests;
