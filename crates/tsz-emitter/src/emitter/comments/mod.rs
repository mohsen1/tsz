mod core;
mod helpers;

#[cfg(test)]
mod name_paren_seam_tests;

pub use self::core::{
    CommentKind, CommentRange, get_leading_comment_ranges, get_trailing_comment_ranges,
};
