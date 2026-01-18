//! Character codes matching TypeScript's CharacterCodes enum.
//!
//! This module provides character code constants used by the scanner.
//! Not all constants are currently used, but they are kept for TypeScript
//! compatibility and future scanner extensions.

#![allow(dead_code)] // Character code tables are intentionally complete for TypeScript compatibility

/// Character codes used throughout the scanner.
pub struct CharacterCodes;

impl CharacterCodes {
    // Line terminators
    pub const LINE_FEED: u32 = 0x0A; // \n
    pub const CARRIAGE_RETURN: u32 = 0x0D; // \r
    pub const LINE_SEPARATOR: u32 = 0x2028;
    pub const PARAGRAPH_SEPARATOR: u32 = 0x2029;
    pub const NEXT_LINE: u32 = 0x0085;

    // Whitespace
    pub const SPACE: u32 = 0x0020;
    pub const TAB: u32 = 0x09;
    pub const VERTICAL_TAB: u32 = 0x0B;
    pub const FORM_FEED: u32 = 0x0C;
    pub const NON_BREAKING_SPACE: u32 = 0x00A0;
    pub const OGHAM: u32 = 0x1680;
    pub const EN_QUAD: u32 = 0x2000;
    pub const EM_QUAD: u32 = 0x2001;
    pub const EN_SPACE: u32 = 0x2002;
    pub const EM_SPACE: u32 = 0x2003;
    pub const THREE_PER_EM_SPACE: u32 = 0x2004;
    pub const FOUR_PER_EM_SPACE: u32 = 0x2005;
    pub const SIX_PER_EM_SPACE: u32 = 0x2006;
    pub const FIGURE_SPACE: u32 = 0x2007;
    pub const PUNCTUATION_SPACE: u32 = 0x2008;
    pub const THIN_SPACE: u32 = 0x2009;
    pub const HAIR_SPACE: u32 = 0x200A;
    pub const ZERO_WIDTH_SPACE: u32 = 0x200B;
    pub const NARROW_NO_BREAK_SPACE: u32 = 0x202F;
    pub const MATHEMATICAL_SPACE: u32 = 0x205F;
    pub const IDEOGRAPHIC_SPACE: u32 = 0x3000;
    pub const BYTE_ORDER_MARK: u32 = 0xFEFF;

    // Digits
    pub const _0: u32 = 0x30;
    pub const _1: u32 = 0x31;
    pub const _2: u32 = 0x32;
    pub const _3: u32 = 0x33;
    pub const _4: u32 = 0x34;
    pub const _5: u32 = 0x35;
    pub const _6: u32 = 0x36;
    pub const _7: u32 = 0x37;
    pub const _8: u32 = 0x38;
    pub const _9: u32 = 0x39;

    // Uppercase letters
    pub const UPPER_A: u32 = 0x41;
    pub const UPPER_B: u32 = 0x42;
    pub const UPPER_C: u32 = 0x43;
    pub const UPPER_D: u32 = 0x44;
    pub const UPPER_E: u32 = 0x45;
    pub const UPPER_F: u32 = 0x46;
    pub const UPPER_G: u32 = 0x47;
    pub const UPPER_H: u32 = 0x48;
    pub const UPPER_I: u32 = 0x49;
    pub const UPPER_J: u32 = 0x4A;
    pub const UPPER_K: u32 = 0x4B;
    pub const UPPER_L: u32 = 0x4C;
    pub const UPPER_M: u32 = 0x4D;
    pub const UPPER_N: u32 = 0x4E;
    pub const UPPER_O: u32 = 0x4F;
    pub const UPPER_P: u32 = 0x50;
    pub const UPPER_Q: u32 = 0x51;
    pub const UPPER_R: u32 = 0x52;
    pub const UPPER_S: u32 = 0x53;
    pub const UPPER_T: u32 = 0x54;
    pub const UPPER_U: u32 = 0x55;
    pub const UPPER_V: u32 = 0x56;
    pub const UPPER_W: u32 = 0x57;
    pub const UPPER_X: u32 = 0x58;
    pub const UPPER_Y: u32 = 0x59;
    pub const UPPER_Z: u32 = 0x5A;

    // Lowercase letters
    pub const LOWER_A: u32 = 0x61;
    pub const LOWER_B: u32 = 0x62;
    pub const LOWER_C: u32 = 0x63;
    pub const LOWER_D: u32 = 0x64;
    pub const LOWER_E: u32 = 0x65;
    pub const LOWER_F: u32 = 0x66;
    pub const LOWER_G: u32 = 0x67;
    pub const LOWER_H: u32 = 0x68;
    pub const LOWER_I: u32 = 0x69;
    pub const LOWER_J: u32 = 0x6A;
    pub const LOWER_K: u32 = 0x6B;
    pub const LOWER_L: u32 = 0x6C;
    pub const LOWER_M: u32 = 0x6D;
    pub const LOWER_N: u32 = 0x6E;
    pub const LOWER_O: u32 = 0x6F;
    pub const LOWER_P: u32 = 0x70;
    pub const LOWER_Q: u32 = 0x71;
    pub const LOWER_R: u32 = 0x72;
    pub const LOWER_S: u32 = 0x73;
    pub const LOWER_T: u32 = 0x74;
    pub const LOWER_U: u32 = 0x75;
    pub const LOWER_V: u32 = 0x76;
    pub const LOWER_W: u32 = 0x77;
    pub const LOWER_X: u32 = 0x78;
    pub const LOWER_Y: u32 = 0x79;
    pub const LOWER_Z: u32 = 0x7A;

    // Punctuation and operators
    pub const EXCLAMATION: u32 = 0x21; // !
    pub const DOUBLE_QUOTE: u32 = 0x22; // "
    pub const HASH: u32 = 0x23; // #
    pub const DOLLAR: u32 = 0x24; // $
    pub const PERCENT: u32 = 0x25; // %
    pub const AMPERSAND: u32 = 0x26; // &
    pub const SINGLE_QUOTE: u32 = 0x27; // '
    pub const OPEN_PAREN: u32 = 0x28; // (
    pub const CLOSE_PAREN: u32 = 0x29; // )
    pub const ASTERISK: u32 = 0x2A; // *
    pub const PLUS: u32 = 0x2B; // +
    pub const COMMA: u32 = 0x2C; // ,
    pub const MINUS: u32 = 0x2D; // -
    pub const DOT: u32 = 0x2E; // .
    pub const SLASH: u32 = 0x2F; // /
    pub const COLON: u32 = 0x3A; // :
    pub const SEMICOLON: u32 = 0x3B; // ;
    pub const LESS_THAN: u32 = 0x3C; // <
    pub const EQUALS: u32 = 0x3D; // =
    pub const GREATER_THAN: u32 = 0x3E; // >
    pub const QUESTION: u32 = 0x3F; // ?
    pub const AT: u32 = 0x40; // @
    pub const OPEN_BRACKET: u32 = 0x5B; // [
    pub const BACKSLASH: u32 = 0x5C; // \
    pub const CLOSE_BRACKET: u32 = 0x5D; // ]
    pub const CARET: u32 = 0x5E; // ^
    pub const UNDERSCORE: u32 = 0x5F; // _
    pub const BACKTICK: u32 = 0x60; // `
    pub const OPEN_BRACE: u32 = 0x7B; // {
    pub const BAR: u32 = 0x7C; // |
    pub const CLOSE_BRACE: u32 = 0x7D; // }
    pub const TILDE: u32 = 0x7E; // ~
}
