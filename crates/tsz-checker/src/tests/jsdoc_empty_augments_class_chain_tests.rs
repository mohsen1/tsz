//! Empty checked-JS `@augments`/`@extends` tags suppress inherited instance
//! members in class-chain recovery, matching the class instance merge owner.

use crate::test_utils::check_js_source_code_messages;

fn codes(source: &str) -> Vec<u32> {
    check_js_source_code_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

#[test]
fn empty_augments_keeps_base_instance_member_missing_but_own_member_visible() {
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
        codes.contains(&2339),
        "expected TS2339 for inherited member suppressed by empty @augments, got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&code| code == 2339).count(),
        1,
        "own checked-JS member should remain visible while inherited member is suppressed: {codes:?}"
    );
}
