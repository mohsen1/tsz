use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
#[test]
fn test_conditional_infer_template_literal_with_prefix_constrained_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // [T] extends [`foo${infer R extends string}`] ? R : never, with T = "foo1" | "bar" (no distribution).
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(infer_r),
    ]);
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_template,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_match = interner.literal_string("foo1");
    let lit_other = interner.literal_string("bar");
    subst.insert(t_name, interner.union(vec![lit_match, lit_other]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_template_literal_two_infers_with_constraint_non_distributive_union_input()
{
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // [T] extends [`${infer A extends string}-${infer B extends string}`] ? A | B : never,
    // with T = "foo-bar" | "baz-qux" (no distribution).
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_template,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: interner.union(vec![infer_a, infer_b]),
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_left = interner.literal_string("foo-bar");
    let lit_right = interner.literal_string("baz-qux");
    subst.insert(t_name, interner.union(vec![lit_left, lit_right]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.literal_string("foo"),
        interner.literal_string("baz"),
        interner.literal_string("bar"),
        interner.literal_string("qux"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_template_literal_two_infers_with_constraint_non_distributive_non_matching_union_branch()
 {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // [T] extends [`${infer A extends string}-${infer B extends string}`] ? A | B : never,
    // with T = "foo-bar" | "baz" (no distribution).
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_template,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: interner.union(vec![infer_a, infer_b]),
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_match = interner.literal_string("foo-bar");
    let lit_other = interner.literal_string("baz");
    subst.insert(t_name, interner.union(vec![lit_match, lit_other]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_template_literal_two_infers_with_constraint_non_distributive_union_branch()
 {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // [T] extends [`${infer A extends string}-${infer B extends string}`] ? A | B : never,
    // with T = "foo-bar" | number (no distribution).
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);
    let cond = ConditionalType {
        check_type: interner.tuple(vec![TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        }]),
        extends_type: interner.tuple(vec![TupleElement {
            type_id: extends_template,
            name: None,
            optional: false,
            rest: false,
        }]),
        true_type: interner.union(vec![infer_a, infer_b]),
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_match = interner.literal_string("foo-bar");
    subst.insert(t_name, interner.union(vec![lit_match, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_template_literal_union_input_distributive() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `foo${infer R}` ? R : never, with T = `foo${string}` | `bar${string}`.
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(infer_r),
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let foo_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    let bar_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("bar")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    subst.insert(t_name, interner.union(vec![foo_template, bar_template]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_template_literal_from_string_input() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `${infer R}` ? R : never, with T = string.
    let extends_template = interner.template_literal(vec![TemplateSpan::Type(infer_r)]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, TypeId::STRING);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_template_literal_from_template_string_input() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `${infer R}` ? R : never, with T = `${string}`.
    let extends_template = interner.template_literal(vec![TemplateSpan::Type(infer_r)]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let template_string = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);
    subst.insert(t_name, template_string);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_template_literal_with_middle_infer_distributive() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `foo${infer R}bar` ? R : never, with T = "foobazbar" | "bar".
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("bar")),
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_match = interner.literal_string("foobazbar");
    let lit_other = interner.literal_string("bar");
    subst.insert(t_name, interner.union(vec![lit_match, lit_other]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.literal_string("baz");
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_template_literal_two_infers_distributive() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_a_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_b_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends `${infer A}-${infer B}` ? A | B : never, with T = "foo-bar" | "baz-qux".
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: interner.union(vec![infer_a, infer_b]),
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_left = interner.literal_string("foo-bar");
    let lit_right = interner.literal_string("baz-qux");
    subst.insert(t_name, interner.union(vec![lit_left, lit_right]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.literal_string("foo"),
        interner.literal_string("baz"),
        interner.literal_string("bar"),
        interner.literal_string("qux"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_template_literal_with_constrained_infer_distributive() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // T extends `foo${infer R extends string}` ? R : never, with T = "foo1" | "foo2".
    let extends_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo")),
        TemplateSpan::Type(infer_r),
    ]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_template,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let lit_foo1 = interner.literal_string("foo1");
    let lit_foo2 = interner.literal_string("foo2");
    subst.insert(t_name, interner.union(vec![lit_foo1, lit_foo2]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![
        interner.literal_string("1"),
        interner.literal_string("2"),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_distributive() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string } } | { a: { b: number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string } } | { a: { b: number } } (no distribution).
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string } } | number (no distribution).
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_nested_object_property_with_constraint() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    // T extends { a: { b: infer R extends string } } ? R : never, with T = { a: { b: string } } | { a: { b: number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_readonly() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { readonly a: { b: infer R } } ? R : never, with T = { readonly a: { b: string } } | { a: { b: number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_readonly_wrapper() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { a: Readonly<{ b: infer R }> } ? R : never,
    // with T = { a: Readonly<{ b: string }> } | { a: { b: number } }.
    let extends_inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_inner = interner.intern(TypeData::ReadonlyType(extends_inner_obj));
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_string = interner.intern(TypeData::ReadonlyType(obj_a_string_inner));
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_readonly_wrapper_non_distributive_union_input() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { a: Readonly<{ b: infer R }> } ? R : never,
    // with T = { a: Readonly<{ b: string }> } | { a: { b: number } } (no distribution).
    let extends_inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_inner = interner.intern(TypeData::ReadonlyType(extends_inner_obj));
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_string = interner.intern(TypeData::ReadonlyType(obj_a_string_inner));
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, obj_number]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_infer_nested_object_property_readonly_wrapper_non_distributive_union_branch() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { a: Readonly<{ b: infer R }> } ? R : never,
    // with T = { a: Readonly<{ b: string }> } | number (no distribution).
    let extends_inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_inner = interner.intern(TypeData::ReadonlyType(extends_inner_obj));
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_string = interner.intern(TypeData::ReadonlyType(obj_a_string_inner));
    let obj_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    subst.insert(t_name, interner.union(vec![obj_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_conditional_infer_nested_object_property_non_matching_branch() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string } } | { a: { c: number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let obj_a_string = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);
    let obj_a_number = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::NUMBER,
    )]);
    let obj_match = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_string,
    )]);
    let obj_non_match = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        obj_a_number,
    )]);
    subst.insert(t_name, interner.union(vec![obj_match, obj_non_match]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);
    // For { a: { b: string } }: matches, R = string
    // For { a: { c: number } }: doesn't match (no 'b' property), goes to false branch = never
    // Union: string | never = string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_conditional_infer_nested_object_property_union_value() {
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: infer_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { a: { b: infer R } } ? R : never, with T = { a: { b: string | number } }.
    let extends_inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        infer_r,
    )]);
    let extends_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        extends_inner,
    )]);
    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    let b_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        b_union,
    )]);
    let obj = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), obj_a)]);
    subst.insert(t_name, obj);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, b_union);
}

