//! Declaration emitter - expression/node emission, import management, and utility helpers.
//!
//! Type syntax emission (type references, unions, mapped types, etc.) is in `type_emission.rs`.

use super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
use crate::emitter::type_printer::TypePrinter;
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
use tsz_parser::parser::ParserState;
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Escape a cooked string value for embedding in a double-quoted string literal.
///
/// The scanner stores "cooked" (unescaped) text for string literals. When
/// writing strings back into `.d.ts` output we must re-escape characters
/// that cannot appear raw inside double-quoted string literals.
pub(crate) fn escape_string_for_double_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

type JsFoldedNamedExports = (
    FxHashSet<String>,
    FxHashMap<NodeIndex, Vec<NodeIndex>>,
    FxHashSet<NodeIndex>,
);
type JsNamespaceExportAliases = FxHashMap<String, Vec<(String, String)>>;
type JsCommonjsSyntheticStatements = FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>;
type JsCommonjsNamedExports = (
    FxHashSet<String>,
    JsCommonjsSyntheticStatements,
    JsCommonjsSyntheticStatements,
);

#[derive(Clone, Copy)]
pub(in crate::declaration_emitter) enum JsCommonjsExpandoDeclKind {
    Function,
    Value,
    PrototypeMethod,
}

#[derive(Default)]
pub(crate) struct JsCommonjsExpandoDeclarations {
    pub(crate) function_statements: FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>,
    pub(crate) value_statements: FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>,
    pub(crate) prototype_methods: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
}

#[derive(Clone)]
pub(crate) struct JsStaticMethodAugmentationGroup {
    pub(crate) class_idx: NodeIndex,
    pub(crate) method_idx: NodeIndex,
    pub(crate) class_is_exported: bool,
    pub(crate) properties: Vec<(NodeIndex, NodeIndex)>,
}

#[derive(Default)]
pub(crate) struct JsStaticMethodAugmentations {
    pub(crate) statements: FxHashMap<NodeIndex, JsStaticMethodAugmentationGroup>,
    pub(crate) skipped_statements: FxHashSet<NodeIndex>,
    pub(crate) augmented_method_nodes: FxHashSet<NodeIndex>,
}

/// Collected prototype member assignments for JS class-like heuristic variables.
/// e.g. `let A; A.prototype.b = {};` → variable `A` becomes `declare class A { ... }`.
#[derive(Default)]
pub(crate) struct JsClassLikePrototypeMembers {
    /// Maps variable name → list of (`member_name_idx`, `initializer_idx`) pairs.
    pub(crate) members: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
    /// Statement indices consumed by the class-like heuristic (to skip during normal emit).
    pub(crate) consumed_stmts: FxHashSet<NodeIndex>,
}

type JsStaticMethodKey = (String, String);
type JsStaticMethodInfo = (NodeIndex, NodeIndex, bool);
type JsStaticMethodAugmentationEntry = (
    NodeIndex,
    NodeIndex,
    NodeIndex,
    bool,
    Vec<(NodeIndex, NodeIndex)>,
);

pub(in crate::declaration_emitter) struct JsdocTypeAliasDecl {
    pub(in crate::declaration_emitter) name: String,
    pub(in crate::declaration_emitter) type_params: Vec<String>,
    pub(in crate::declaration_emitter) type_text: String,
    pub(in crate::declaration_emitter) description_lines: Vec<String>,
    pub(in crate::declaration_emitter) render_verbatim: bool,
}

pub(in crate::declaration_emitter) struct JsDefinedPropertyDecl {
    pub(in crate::declaration_emitter) name: String,
    pub(in crate::declaration_emitter) type_text: String,
    pub(in crate::declaration_emitter) readonly: bool,
}

#[derive(Clone)]
pub(crate) struct JsdocParamDecl {
    pub(crate) name: String,
    pub(crate) type_text: String,
    pub(crate) optional: bool,
    pub(crate) rest: bool,
}

/// Lightweight `TypeResolver` backed by `TypeCacheView` data for DTS emit.
#[allow(dead_code)]
pub(crate) struct DtsCacheResolver<'a> {
    pub(crate) cache: &'a crate::type_cache_view::TypeCacheView,
}

impl tsz_solver::def::resolver::TypeResolver for DtsCacheResolver<'_> {
    fn resolve_ref(
        &self,
        _symbol: tsz_solver::types::SymbolRef,
        _interner: &dyn tsz_solver::TypeDatabase,
    ) -> Option<tsz_solver::types::TypeId> {
        None
    }

    fn resolve_lazy(
        &self,
        def_id: tsz_solver::DefId,
        interner: &dyn tsz_solver::TypeDatabase,
    ) -> Option<tsz_solver::types::TypeId> {
        let &type_id = self.cache.def_types.get(&def_id.0)?;
        use tsz_solver::types::TypeData;
        match interner.lookup(type_id) {
            Some(TypeData::Union(_))
            | Some(TypeData::Intersection(_))
            | Some(TypeData::Lazy(_))
            | Some(TypeData::KeyOf(_))
            | Some(TypeData::TemplateLiteral(_)) => Some(type_id),
            _ if type_id.is_intrinsic() => Some(type_id),
            _ if tsz_solver::visitor::literal_value(interner, type_id).is_some() => Some(type_id),
            _ => None,
        }
    }

    fn get_lazy_type_params(
        &self,
        def_id: tsz_solver::DefId,
    ) -> Option<Vec<tsz_solver::types::TypeParamInfo>> {
        self.cache.def_type_params.get(&def_id.0).cloned()
    }
}

mod comments_source;
mod emit_node;
mod function_analysis;
mod js_exports;
mod jsdoc;
mod portability_check;
mod portability_resolve;
mod type_inference;
mod type_printing;
mod variable_decl;
mod visibility;
