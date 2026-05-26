use super::*;

// =============================================================================
// Template literal type parameter inference (issue #6147)
// =============================================================================

/// Build a `T extends string`-constrained inference variable + placeholder TypeId.
///
/// This mirrors what the generic call resolver does: `fresh_type_param` + `set_declared_constraint`
/// + `mark_declared_constraint_preserves_literals` + `add_upper_bound`.
fn make_string_param(
    interner: &TypeInterner,
    ctx: &mut InferenceContext<'_>,
    name: &str,
) -> (crate::inference::infer::InferenceVar, TypeId) {
    let atom = interner.intern_string(name);
    let var = ctx.fresh_type_param(atom, false);
    ctx.set_declared_constraint(var, TypeId::STRING);
    ctx.mark_declared_constraint_preserves_literals(var);
    ctx.add_upper_bound(var, TypeId::STRING);
    let type_id = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: atom,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    (var, type_id)
}

/// f(x: prefix-T) where T extends string: calling with "prefix-hello" infers T = "hello".
#[test]
fn test_infer_type_param_from_template_literal_trailing() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (var_t, t_type) = make_string_param(&interner, &mut ctx, "T");

    let prefix_atom = interner.intern_string("prefix-");
    let param_type = interner.template_literal(vec![
        TemplateSpan::Text(prefix_atom),
        TemplateSpan::Type(t_type),
    ]);

    let source = interner.literal_string("prefix-hello");
    ctx.infer_from_types(
        source,
        param_type,
        crate::types::InferencePriority::NakedTypeVariable,
    )
    .unwrap();

    let resolved = ctx.resolve_with_constraints(var_t).unwrap_or(TypeId::ERROR);
    let expected = interner.literal_string("hello");
    assert_eq!(
        resolved,
        expected,
        "T should be inferred as literal \"hello\", got {:?}",
        interner.lookup(resolved)
    );
}

/// Same rule using a renamed type parameter (`K` instead of `T`) to verify
/// no identifier is hardcoded.
#[test]
fn test_infer_type_param_from_template_literal_trailing_renamed() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (var_k, k_type) = make_string_param(&interner, &mut ctx, "K");

    let prefix_atom = interner.intern_string("get-");
    let param_type = interner.template_literal(vec![
        TemplateSpan::Text(prefix_atom),
        TemplateSpan::Type(k_type),
    ]);

    let source = interner.literal_string("get-name");
    ctx.infer_from_types(
        source,
        param_type,
        crate::types::InferencePriority::NakedTypeVariable,
    )
    .unwrap();

    let resolved = ctx.resolve_with_constraints(var_k).unwrap_or(TypeId::ERROR);
    let expected = interner.literal_string("name");
    assert_eq!(
        resolved,
        expected,
        "K should be inferred as literal \"name\", got {:?}",
        interner.lookup(resolved)
    );
}

/// f(x: pre-T-suf) where T extends string: calling with "pre-mid-suf" infers T = "mid".
#[test]
fn test_infer_type_param_from_template_literal_surrounded() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (var_t, t_type) = make_string_param(&interner, &mut ctx, "T");

    let pre_atom = interner.intern_string("pre-");
    let suf_atom = interner.intern_string("-suf");
    let param_type = interner.template_literal(vec![
        TemplateSpan::Text(pre_atom),
        TemplateSpan::Type(t_type),
        TemplateSpan::Text(suf_atom),
    ]);

    let source = interner.literal_string("pre-mid-suf");
    ctx.infer_from_types(
        source,
        param_type,
        crate::types::InferencePriority::NakedTypeVariable,
    )
    .unwrap();

    let resolved = ctx.resolve_with_constraints(var_t).unwrap_or(TypeId::ERROR);
    let expected = interner.literal_string("mid");
    assert_eq!(
        resolved,
        expected,
        "T should be inferred as literal \"mid\", got {:?}",
        interner.lookup(resolved)
    );
}

/// f(x: T-U) where T, U extend string: calling with "hello-world" infers T = "hello", U = "world".
#[test]
fn test_infer_two_type_params_from_template_literal() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (var_t, t_type) = make_string_param(&interner, &mut ctx, "T");
    let (var_u, u_type) = make_string_param(&interner, &mut ctx, "U");

    let sep_atom = interner.intern_string("-");
    let param_type = interner.template_literal(vec![
        TemplateSpan::Type(t_type),
        TemplateSpan::Text(sep_atom),
        TemplateSpan::Type(u_type),
    ]);

    let source = interner.literal_string("hello-world");
    ctx.infer_from_types(
        source,
        param_type,
        crate::types::InferencePriority::NakedTypeVariable,
    )
    .unwrap();

    let resolved_t = ctx.resolve_with_constraints(var_t).unwrap_or(TypeId::ERROR);
    let resolved_u = ctx.resolve_with_constraints(var_u).unwrap_or(TypeId::ERROR);
    let expected_t = interner.literal_string("hello");
    let expected_u = interner.literal_string("world");
    assert_eq!(
        resolved_t,
        expected_t,
        "T should be \"hello\", got {:?}",
        interner.lookup(resolved_t)
    );
    assert_eq!(
        resolved_u,
        expected_u,
        "U should be \"world\", got {:?}",
        interner.lookup(resolved_u)
    );
}
