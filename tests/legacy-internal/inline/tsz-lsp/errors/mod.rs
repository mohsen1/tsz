//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/errors/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e854ce2244e1b1d9c5f64d7d3d7f8faaa493ba8f6326224b0d363c008b68a1e0 141 rename_error_messages_are_canonical
    #[test]
    fn rename_error_messages_are_canonical() {
        assert_eq!(
            RenameError::NotRenamable.to_string(),
            "You cannot rename this element."
        );
        assert_eq!(
            RenameError::ExternalModule.to_string(),
            "You cannot rename elements from external modules."
        );
        assert_eq!(
            RenameError::SymbolNotFound.to_string(),
            "Could not find symbol to rename"
        );
        assert_eq!(RenameError::FileNotFound.to_string(), "Could not find file");
        assert_eq!(
            RenameError::EmptyName.to_string(),
            "Rename target cannot be empty."
        );
        assert_eq!(
            RenameError::InvalidIdentifier("9bad".to_string()).to_string(),
            "'9bad' is not a valid identifier name"
        );
        assert_eq!(
            RenameError::InvalidPrivateIdentifier("9bad".to_string()).to_string(),
            "'9bad' is not a valid private identifier name"
        );
    }
// TSZ_INLINE_TEST_END e854ce2244e1b1d9c5f64d7d3d7f8faaa493ba8f6326224b0d363c008b68a1e0

// TSZ_INLINE_TEST_BEGIN 05d1a2b9c4c9af68613386540b4e93c82b425e63d8d062e35b7e994fdf5c590b 170 format_error_messages_are_canonical
    #[test]
    fn format_error_messages_are_canonical() {
        assert_eq!(
            FormatError::InvalidFilePath.to_string(),
            "Invalid file path"
        );
        assert_eq!(
            FormatError::SpawnFailed {
                tool: "prettier",
                source: "boom".to_string()
            }
            .to_string(),
            "Failed to spawn prettier: boom"
        );
        assert_eq!(
            FormatError::OpenStdin {
                message: "Failed to open stdin"
            }
            .to_string(),
            "Failed to open stdin"
        );
        assert_eq!(
            FormatError::Io {
                stage: "Failed to write to prettier stdin",
                source: "io".to_string()
            }
            .to_string(),
            "Failed to write to prettier stdin: io"
        );
        assert_eq!(
            FormatError::FormatterFailed {
                tool: "Prettier",
                stderr: "nope".to_string()
            }
            .to_string(),
            "Prettier failed: nope"
        );
        assert_eq!(
            FormatError::JsonParse {
                source: "eof".to_string()
            }
            .to_string(),
            "Failed to parse eslint JSON output: eof"
        );
    }
// TSZ_INLINE_TEST_END 05d1a2b9c4c9af68613386540b4e93c82b425e63d8d062e35b7e994fdf5c590b
