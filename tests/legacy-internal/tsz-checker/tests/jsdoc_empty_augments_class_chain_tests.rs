//! A malformed empty checked-JS `@augments`/`@extends` tag reports
//! TS8023+TS1003 but does NOT override a syntactic `extends` clause: tsc
//! 7.0.2 still resolves the base class, so inherited instance members stay
//! visible on the derived class (oracle: `class Root{constructor(){this.y=0}}
//! /** @augments */ class Leaf extends Root { probe(){this.y} }` emits only
//! TS8023+TS1003, no TS2339).

use crate::test_utils::check_js_source_code_messages;

fn codes(source: &str) -> Vec<u32> {
    check_js_source_code_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

#[test]
fn empty_augments_keeps_inherited_and_own_members_visible() {
    let codes = codes(
        r#"
class Root {
  constructor() {
    this.y = 0;
  }
}
/** @augments */
class Leaf extends Root {
  constructor() {
    super();
    this.z = 1;
  }
  probe() {
    this.y;
    this.z;
  }
}
"#,
    );

    assert!(
        !codes.contains(&2339),
        "a malformed empty @augments must not suppress the syntactic extends edge; \
         inherited `y` and own `z` both stay visible (tsc: TS8023+TS1003 only), got {codes:?}"
    );
    assert!(
        codes.contains(&8023),
        "the malformed tag itself still reports TS8023, got {codes:?}"
    );
}
