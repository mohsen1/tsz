macro_rules! expect_claimed_extension {
    () => {
        trait ExpectClaimed<T> {
            fn expect_claimed(self, message: &str) -> T;
        }

        impl<T> ExpectClaimed<T> for ServiceQuery<T> {
            #[track_caller]
            fn expect_claimed(self, message: &str) -> T {
                match self {
                    Self::Claimed(value) => value,
                    Self::Nonclaimed(nonclaim) => panic!(
                        "{message}: navigation query is {}",
                        nonclaim.completion().as_str()
                    ),
                }
            }
        }
    };
}
