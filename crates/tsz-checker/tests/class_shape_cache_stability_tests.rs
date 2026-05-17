use tsz_checker::test_utils::{check_source_code_messages, check_source_codes};

fn codes_for(source: &str) -> Vec<u32> {
    check_source_codes(source)
        .into_iter()
        .filter(|code| !matches!(*code, 2318 | 2583))
        .collect()
}

#[test]
fn shape_stable_class_preserves_implements_and_static_constructor_type() {
    let codes = codes_for(
        r#"
interface ConfigA {
    readonly id: number;
    name: string;
    enabled: boolean;
    getId(): number;
}

class ServiceA implements ConfigA {
    readonly id: number = 1;
    name: string;
    enabled: boolean = true;
    private token: number = 0;

    constructor(name: string) {
        this.name = name;
    }

    getId(): number {
        return this.id;
    }

    static create(name: string): ServiceA {
        return new ServiceA(name);
    }
}

const service: ServiceA = ServiceA.create("ok");
const id: number = service.getId();
"#,
    );

    assert!(
        codes.is_empty(),
        "shape-stable class should keep implements/static/member typing clean, got {codes:?}"
    );
}

#[test]
fn shape_stable_class_keeps_member_return_mismatch_diagnostics() {
    let messages = check_source_code_messages(
        r#"
interface ConfigB {
    getId(): string;
}

class ServiceB implements ConfigB {
    readonly id: number = 1;

    constructor(name: string) {
        name;
    }

    getId(): number {
        return this.id;
    }

    static create(name: string): ServiceB {
        return new ServiceB(name);
    }
}
"#,
    );

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2416
                && message.contains(
                    "Property 'getId' in type 'ServiceB' is not assignable to the same property in base type 'ConfigB'"
                )
        }),
        "expected TS2416 for incompatible method return, got {messages:?}"
    );
}

#[test]
fn excluded_generic_class_still_reports_implements_mismatch() {
    let messages = check_source_code_messages(
        r#"
interface BoxLike<T> {
    value(): T;
}

class Boxed<T> implements BoxLike<T> {
    value(): number {
        return 1;
    }
}
"#,
    );

    assert!(
        messages.iter().any(|(code, message)| {
            *code == 2416
                && message.contains(
                    "Property 'value' in type 'Boxed<T>' is not assignable to the same property in base type 'BoxLike<T>'"
                )
        }),
        "expected generic class to keep full member compatibility checking, got {messages:?}"
    );
}
