use crate::test_utils::check_source_code_messages;

// #13484 family: when a generic *constructor* reference (`new C<…>()` / an
// `extends` base) supplies fewer type arguments than parameters, the missing
// slots fill from `default -> constraint -> unknown`, matching tsc. All
// under-applied fill sites — the base-instance type and the construct-signature
// paths — route through the shared `missing_base_type_arg_fill` boundary helper,
// so none of them can bake tsz's internal `error` cycle sentinel into a
// type-argument slot. These exercise the construct-signature paths end to end
// and vary binder names.

fn ts2322_count(messages: &[(u32, String)]) -> usize {
    messages.iter().filter(|(code, _)| *code == 2322).count()
}

#[test]
fn new_expression_under_applied_fills_trailing_default() {
    // Both parameters have defaults, so `new Slot<boolean>(...)` is a valid
    // under-application: the trailing `Backing` slot fills from its default
    // `number`. The deliberate annotation mismatches pin the resolved arguments.
    let messages = check_source_code_messages(
        r#"
class Slot<Stored = string, Backing extends number = number> {
    constructor(public stored: Stored, public backing: Backing) {}
}
const made = new Slot<boolean>(true, 5);
const wrongStored: number = made.stored;   // Stored = boolean (explicit arg)
const wrongBacking: string = made.backing; // Backing = number (filled default)
"#,
    );
    assert_eq!(
        ts2322_count(&messages),
        2,
        "expected boolean->number and number->string mismatches that prove the filled default, got: {messages:?}"
    );
}

#[test]
fn new_expression_under_applied_default_references_prior_param() {
    // The trailing default references an earlier parameter (`Echo = Head`), so
    // the fill must substitute the already-supplied argument into the default.
    let messages = check_source_code_messages(
        r#"
class Tandem<Head, Echo = Head> {
    constructor(public head: Head, public echo: Echo) {}
}
const pair = new Tandem<string>("a", "b");
const wrongEcho: number = pair.echo; // Echo = Head = string
"#,
    );
    assert_eq!(
        ts2322_count(&messages),
        1,
        "expected Echo to fill from a default referencing Head (string), got: {messages:?}"
    );
}
