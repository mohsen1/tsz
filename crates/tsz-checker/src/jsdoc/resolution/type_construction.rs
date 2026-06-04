use super::super::types::{JsdocCallbackInfo, JsdocTypedefInfo};

use crate::state::CheckerState;

use std::sync::Arc;

use tsz_binder::symbol_flags;

use tsz_parser::parser::NodeIndex;

use tsz_solver::{
    FunctionShape, IndexSignature, ObjectShape, ParamInfo, PropertyInfo, TupleElement, TypeId,
    TypePredicate, TypePredicateTarget, Visibility,
};

include!("type_construction_parts/part1.rs");
include!("type_construction_parts/part2.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::construction::TypeInterner;

    #[test]
    fn resolve_jsdoc_assigned_value_type_sees_prototype_property_statement() {
        let source = r#"
function C() { this.x = false; };
/** @type {number} */
C.prototype.x;
new C().x;
"#;
        let options = crate::context::CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..crate::context::CheckerOptions::default()
        };
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.js".to_string(),
            options,
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);
        assert_eq!(
            checker
                .resolve_jsdoc_assigned_value_type("C.prototype.x")
                .map(|ty| checker.format_type(ty)),
            Some("number".to_string())
        );
    }
}
