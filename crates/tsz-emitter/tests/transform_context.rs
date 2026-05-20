use super::*;

#[test]
fn test_transform_context_basic() {
    let mut ctx = TransformContext::new();
    let node = NodeIndex(42);

    // Initially empty
    assert!(ctx.is_empty());
    assert_eq!(ctx.len(), 0);
    assert!(!ctx.has_transform(node));

    // Insert a directive
    ctx.insert(node, TransformDirective::Identity);
    assert!(!ctx.is_empty());
    assert_eq!(ctx.len(), 1);
    assert!(ctx.has_transform(node));

    // Get the directive
    let directive = ctx.get(node);
    assert!(directive.is_some());
    assert!(matches!(
        directive.expect("directive is Some, verified by assertion"),
        TransformDirective::Identity
    ));

    // Clear
    ctx.clear();
    assert!(ctx.is_empty());
    assert!(!ctx.has_transform(node));
}

#[test]
fn test_es5_class_directive() {
    let mut ctx = TransformContext::new();
    let class_node = NodeIndex(10);

    ctx.insert(
        class_node,
        TransformDirective::ES5Class {
            class_node,
            heritage: None,
        },
    );

    let directive = ctx.get(class_node).expect("directive was just inserted");
    match directive {
        TransformDirective::ES5Class { class_node, .. } => {
            assert_eq!(*class_node, NodeIndex(10));
        }
        _ => panic!("Expected ES5Class directive"),
    }
}

#[test]
fn test_commonjs_export_chain() {
    let mut ctx = TransformContext::new();
    let class_node = NodeIndex(10);
    let name_id: IdentifierId = 11;

    // Chain ES5 class transform with CommonJS export
    let directive = TransformDirective::CommonJSExport {
        names: Arc::from(vec![name_id]),
        is_default: false,
        inner: Box::new(TransformDirective::ES5Class {
            class_node,
            heritage: None,
        }),
    };

    ctx.insert(class_node, directive);

    let retrieved = ctx.get(class_node).expect("directive was just inserted");
    match retrieved {
        TransformDirective::CommonJSExport { names, inner, .. } => {
            assert_eq!(names.as_ref(), &[name_id]);
            assert!(matches!(**inner, TransformDirective::ES5Class { .. }));
        }
        _ => panic!("Expected CommonJSExport directive"),
    }
}

#[test]
fn test_commonjs_export_names_shared() {
    let names: Arc<[IdentifierId]> = Arc::from(vec![1, 2]);
    let directive = TransformDirective::CommonJSExport {
        names: names.clone(),
        is_default: false,
        inner: Box::new(TransformDirective::Identity),
    };

    let cloned = directive;
    match cloned {
        TransformDirective::CommonJSExport {
            names: cloned_names,
            ..
        } => {
            assert!(Arc::ptr_eq(&names, &cloned_names));
        }
        _ => panic!("Expected CommonJSExport directive"),
    }
}
