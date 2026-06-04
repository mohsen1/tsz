#[test]
fn test_variance_mixed_direct_and_callback_method_param_is_reliable() {
    // interface C<T> { m(x: T, cb: (x: T) => void): void }
    // T appears both directly in m's params (method-bivariant) AND inside a
    // non-method callback (strict covariant). The strict occurrence pins the
    // variance — the result must be COVARIANT and reliably rejecting
    // (matches tsc, which errors on `C<Foo>` -> `C<Bar>` for unrelated Foo,
    // Bar despite the direct-param T being method-bivariant).
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let callback = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let method = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("cb")),
                type_id: callback,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });
    let body = interner.object(vec![PropertyInfo::new(interner.intern_string("m"), method)]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, body, t_atom);

    assert!(
        variance.is_covariant(),
        "Mixed direct/callback method params should resolve to COVARIANT"
    );
    assert!(
        !variance.rejection_unreliable(),
        "A strict (callback) occurrence should clear REJECTION_UNRELIABLE set by sibling direct method params"
    );
}

#[test]
fn test_variance_merged_promise_like_overloads_is_covariant() {
    // Mimics the variance contribution of a declaration-merged
    // `Promise<T>` interface: a callable shape with two method-style
    // `then` overloads, each using T inside a callback parameter.
    //
    // Shape (conceptual):
    //   {
    //     then(onfulfilled: (value: T) => any): any;   // lib overload
    //     then(cb: (x: T) => any): any;                // user overload
    //   }
    //
    // Both occurrences of T are inside non-method callbacks nested under
    // method-bivariant parameters. Each leaf occurrence is a strict
    // covariant signal (double-contravariant = covariant), so the merged
    // variance must be COVARIANT and reliable, with no
    // REJECTION_UNRELIABLE bit. This locks in tsz's variance computation
    // for the Promise pattern after declaration merging — the path
    // exercised by `tests/cases/compiler/promisesWithConstraints.ts`.
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let make_callback_with_t = |param_name: &str| {
        interner.function(FunctionShape {
            params: vec![ParamInfo {
                name: Some(interner.intern_string(param_name)),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::ANY,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let lib_then_sig = CallSignature {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("onfulfilled")),
            type_id: make_callback_with_t("value"),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_method: true,
    };
    let user_then_sig = CallSignature {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: make_callback_with_t("x"),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_method: true,
    };

    let then_callable = interner.callable(CallableShape {
        call_signatures: vec![lib_then_sig, user_then_sig],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let body = interner.object(vec![PropertyInfo::new(
        interner.intern_string("then"),
        then_callable,
    )]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, body, t_atom);

    assert!(
        variance.is_covariant(),
        "Merged Promise-like interface with multiple `then` overloads should be COVARIANT in T (got {variance:?})"
    );
    assert!(
        !variance.is_contravariant(),
        "T does not appear in any contravariant-only position"
    );
    assert!(
        !variance.rejection_unreliable(),
        "Strict callback occurrences pin the variance signal — REJECTION_UNRELIABLE must be cleared"
    );
}
