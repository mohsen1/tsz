//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/diagnostic_push.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN caf7884e1f7df7a4bd89561246166cf29f8e46e1a18a1639c74554117af59bec 410 richer_related_information_is_preferred
    #[test]
    fn richer_related_information_is_preferred() {
        let lean = base();
        let rich = base().with_related("a.ts", 0, 1, "'B' is declared here.");
        // The richer elaboration wins regardless of which side is the incumbent.
        assert!(prefers_candidate_diagnostic(&lean, &rich));
        assert!(!prefers_candidate_diagnostic(&rich, &lean));
    }
// TSZ_INLINE_TEST_END caf7884e1f7df7a4bd89561246166cf29f8e46e1a18a1639c74554117af59bec

// TSZ_INLINE_TEST_BEGIN 87686f3f6d28f222337381b8c88214153e335482771530fc675c39ec7477fb5c 419 equal_length_related_breaks_tie_canonically
    #[test]
    fn equal_length_related_breaks_tie_canonically() {
        // Two equally rich elaborations that blame different "see also"
        // locations must resolve to the canonically smaller one, independent of
        // which was produced first.
        let earlier = base().with_related("a.ts", 5, 1, "'A' is declared here.");
        let later = base().with_related("a.ts", 99, 1, "'A' is declared here.");
        assert!(prefers_candidate_diagnostic(&later, &earlier));
        assert!(!prefers_candidate_diagnostic(&earlier, &later));
    }
// TSZ_INLINE_TEST_END 87686f3f6d28f222337381b8c88214153e335482771530fc675c39ec7477fb5c

// TSZ_INLINE_TEST_BEGIN da1efd65096ace2f9711f1d0c6e11df3b427be498200388513f76eb1a5f231ff 430 identical_diagnostics_do_not_replace
    #[test]
    fn identical_diagnostics_do_not_replace() {
        let a = base().with_related("a.ts", 5, 1, "'A' is declared here.");
        let b = base().with_related("a.ts", 5, 1, "'A' is declared here.");
        assert!(!prefers_candidate_diagnostic(&a, &b));
        assert!(!prefers_candidate_diagnostic(&b, &a));
    }
// TSZ_INLINE_TEST_END da1efd65096ace2f9711f1d0c6e11df3b427be498200388513f76eb1a5f231ff

// TSZ_INLINE_TEST_BEGIN fb820bac5cb2344d8f5ddc7c4869102ab16341771f04fda76229bd2064974130 438 selection_is_order_independent
    #[test]
    fn selection_is_order_independent() {
        // Whichever order three equivalent-but-distinct diagnostics arrive in,
        // the deterministic winner is the same one.
        let mut variants = [
            base().with_related("a.ts", 30, 1, "'A' is declared here."),
            base().with_related("a.ts", 10, 1, "'A' is declared here."),
            base().with_related("a.ts", 20, 1, "'A' is declared here."),
        ];
        let fold_winner = |order: &[Diagnostic]| -> Diagnostic {
            let mut winner = order[0].clone();
            for cand in &order[1..] {
                if prefers_candidate_diagnostic(&winner, cand) {
                    winner = cand.clone();
                }
            }
            winner
        };
        let baseline = fold_winner(&variants);
        variants.reverse();
        assert_eq!(fold_winner(&variants), baseline);
        variants.swap(0, 2);
        assert_eq!(fold_winner(&variants), baseline);
        // The canonical winner is the smallest related position.
        assert_eq!(baseline.related_information[0].start, 10);
    }
// TSZ_INLINE_TEST_END fb820bac5cb2344d8f5ddc7c4869102ab16341771f04fda76229bd2064974130
