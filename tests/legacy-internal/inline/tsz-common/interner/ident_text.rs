//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/interner/ident_text.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5a57038829e61006a75389de6dde5486d00c593112aada3e4987c3b78648c505 252 empty_is_shared_and_empty
    #[test]
    fn empty_is_shared_and_empty() {
        let a = IdentText::empty();
        let b = IdentText::default();
        assert!(a.is_empty());
        assert_eq!(a, b);
        assert!(Arc::ptr_eq(&a.0, &b.0));
    }
// TSZ_INLINE_TEST_END 5a57038829e61006a75389de6dde5486d00c593112aada3e4987c3b78648c505

// TSZ_INLINE_TEST_BEGIN db29023d2db9dce23046b6e1a85ef208b176c2ea7a5e69c768aa677cef54b485 261 compares_like_string
    #[test]
    fn compares_like_string() {
        let t = IdentText::from("foo");
        assert_eq!(t, "foo");
        assert_eq!(t, *"foo");
        assert_eq!("foo", t);
        assert_eq!(t, String::from("foo"));
        assert_eq!(String::from("foo"), t);
        assert_ne!(t, "bar");
    }
// TSZ_INLINE_TEST_END db29023d2db9dce23046b6e1a85ef208b176c2ea7a5e69c768aa677cef54b485

// TSZ_INLINE_TEST_BEGIN 8d3dac3f655718ee8078deffa93c9182146ba33efb5c166920d142529ef89960 272 ptr_sharing_and_content_equality
    #[test]
    fn ptr_sharing_and_content_equality() {
        let shared: Arc<str> = Arc::from("name");
        let a = IdentText::from_arc(Arc::clone(&shared));
        let b = IdentText::from_arc(shared);
        let c = IdentText::from("name");
        assert_eq!(a, b);
        assert_eq!(a, c); // different Arc, same content
    }
// TSZ_INLINE_TEST_END 8d3dac3f655718ee8078deffa93c9182146ba33efb5c166920d142529ef89960

// TSZ_INLINE_TEST_BEGIN a7c148d0340dbfb4c9f0abe0ee5a58f8e0e61f546c6673d4ea9becf74bb64e2d 282 debug_and_display_match_string
    #[test]
    fn debug_and_display_match_string() {
        let t = IdentText::from("x\\y");
        let s = String::from("x\\y");
        assert_eq!(format!("{t}"), format!("{s}"));
        assert_eq!(format!("{t:?}"), format!("{s:?}"));
    }
// TSZ_INLINE_TEST_END a7c148d0340dbfb4c9f0abe0ee5a58f8e0e61f546c6673d4ea9becf74bb64e2d

// TSZ_INLINE_TEST_BEGIN f87a7d61a730ee98ef202dd455ad16357195978f60d91076cfd958944781b53a 290 serde_is_string_compatible
    #[test]
    fn serde_is_string_compatible() {
        let t = IdentText::from("hello");
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"hello\"");
        let back: IdentText = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(back, t);
        // A String round-trips into IdentText and vice versa.
        let from_string: IdentText =
            serde_json::from_str(&serde_json::to_string(&String::from("s")).unwrap()).unwrap();
        assert_eq!(from_string, "s");
    }
// TSZ_INLINE_TEST_END f87a7d61a730ee98ef202dd455ad16357195978f60d91076cfd958944781b53a

// TSZ_INLINE_TEST_BEGIN e02d27d46847ef8a3b81a8e78652fdd93d80c3a2548029682fba0866ef367806 303 hashes_like_str
    #[test]
    fn hashes_like_str() {
        use std::collections::HashMap;
        let mut m: HashMap<IdentText, u32> = HashMap::new();
        m.insert(IdentText::from("k"), 1);
        // Borrow<str> lookup
        assert_eq!(m.get("k"), Some(&1));
    }
// TSZ_INLINE_TEST_END e02d27d46847ef8a3b81a8e78652fdd93d80c3a2548029682fba0866ef367806
