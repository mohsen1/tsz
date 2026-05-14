use tsz_checker::test_utils::{check_source_strict_codes, check_source_strict_messages};

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2322).then_some(message))
        .collect()
}

#[test]
fn setter_write_does_not_narrow_getter_read_to_null() {
    let source = r#"
class Container {
  private _value: string | null = null;

  get value(): string {
    if (this._value === null) {
      throw new Error("Not set");
    }
    return this._value;
  }

  set value(v: string | null) {
    this._value = v;
  }
}

const c = new Container();
c.value = "hello";
c.value = null;
const v: string = c.value;
"#;
    let messages = ts2322_messages(source);
    assert!(
        messages.is_empty(),
        "setter writes must not narrow later getter reads below the getter type; got: {messages:?}"
    );
}

#[test]
fn renamed_accessor_write_keeps_getter_read_surface() {
    let source = r#"
class BoxedFlag {
  private stored: boolean | undefined = true;

  get ready(): boolean {
    return this.stored === true;
  }

  set ready(next: boolean | undefined) {
    this.stored = next;
  }
}

const flag = new BoxedFlag();
flag.ready = undefined;
const ready: boolean = flag.ready;
"#;
    let messages = ts2322_messages(source);
    assert!(
        messages.is_empty(),
        "the rule must be structural, not tied to property or class names; got: {messages:?}"
    );
}

#[test]
fn writable_property_assignment_still_narrows_later_read() {
    let source = r#"
class Container {
  value: string | null = "set";
}

const c = new Container();
c.value = null;
const v: string = c.value;
"#;
    let codes = check_source_strict_codes(source);
    let ts2322_count = codes.into_iter().filter(|code| *code == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "ordinary writable properties should still narrow to the assigned value"
    );
}
