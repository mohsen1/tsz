fn submit_discussion_or_print_fallback(report_path: &Path) -> Result<()> {
    let body = fs::read_to_string(report_path)?;
    let title = "[try-tsz] compatibility report";
    let output = Command::new("gh")
        .args([
            "api",
            "graphql",
            "-f",
            "query=mutation($repositoryId:ID!,$categoryId:ID!,$title:String!,$body:String!){createDiscussion(input:{repositoryId:$repositoryId,categoryId:$categoryId,title:$title,body:$body}){discussion{url}}}",
            "-f",
            "repositoryId=R_kgDOQ7o9zQ",
            "-f",
            "categoryId=DIC_kwDOQ7o9zc4C-QRC",
            "-f",
            &format!("title={title}"),
            "-f",
            &format!("body={body}"),
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        _ => {
            println!("Could not submit with gh. Open a Discussion and paste:");
            println!("{}", report_path.display());
            println!(
                "https://github.com/tsz-org/tsz/discussions/new?category=general&title={}&body={}",
                percent_encode_url_component(title),
                percent_encode_url_component(&body)
            );
        }
    }
    Ok(())
}

fn percent_encode_url_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn normalize_path_label(cwd: &Path, path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let normalized_cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let normalized_path = fs::canonicalize(&absolute).unwrap_or(absolute);
    relative_path(&normalized_cwd, &normalized_path)
}

fn relative_path(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let mut path = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should work")
                .as_nanos();
            let unique = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            path.push(format!(
                "try_tsz_test_{}_{}_{}",
                std::process::id(),
                nanos,
                unique
            ));
            fs::create_dir_all(&path).expect("temp dir should be created");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent should be created");
        }
        fs::write(path, text).expect("file should be written");
    }

    fn diag(code: u32, file: &str, message: &str) -> ComparableDiagnostic {
        ComparableDiagnostic {
            file: Some(file.to_string()),
            start: Some(1),
            length: Some(2),
            line: Some(1),
            column: Some(2),
            code,
            category: "error".to_string(),
            message: message.to_string(),
        }
    }

    fn report_for_config(config: &str) -> ConfigReport {
        ConfigReport {
            config: config.to_string(),
            state: ResultState::Mismatch,
            metadata: ProjectMetadata {
                try_tsz_version: "test".to_string(),
                tsz_version: "test".to_string(),
                typescript_version: Some("6.0.3".to_string()),
                node_version: None,
                os: "test".to_string(),
                arch: "test".to_string(),
                package_manager: None,
                project_references: true,
                file_count: 0,
                approx_loc: 0,
            },
            tsc: None,
            tsz: None,
            extra_tsz_diagnostics: Vec::new(),
            missing_tsc_diagnostics: Vec::new(),
            order_mismatches: 0,
            setup_error: None,
        }
    }

    #[test]
    fn discover_nearest_tsconfig() {
        let temp = TempDir::new();
        write_file(&temp.path.join("tsconfig.json"), "{}");
        fs::create_dir_all(temp.path.join("src/nested")).expect("nested dir");

        let configs = discover_configs(&temp.path.join("src/nested"), None, false)
            .expect("config should be discovered");

        assert_eq!(configs, vec![temp.path.join("tsconfig.json")]);
    }

    #[test]
    fn explicit_project_directory_resolves_tsconfig() {
        let temp = TempDir::new();
        write_file(&temp.path.join("pkg/tsconfig.json"), "{}");

        let configs = discover_configs(&temp.path, Some(Path::new("pkg")), false)
            .expect("project dir should resolve");

        assert_eq!(configs, vec![temp.path.join("pkg/tsconfig.json")]);
    }

    #[test]
    fn all_skips_generated_directories() {
        let temp = TempDir::new();
        write_file(&temp.path.join("packages/a/tsconfig.json"), "{}");
        write_file(&temp.path.join("node_modules/pkg/tsconfig.json"), "{}");

        let configs = discover_configs(&temp.path, None, true).expect("all should find configs");

        assert_eq!(configs, vec![temp.path.join("packages/a/tsconfig.json")]);
    }

    #[test]
    fn typescript_oracle_preflight_accepts_hoisted_workspace_tsc() {
        let temp = TempDir::new();
        let package_dir = temp.path.join("packages/foo");
        let config = package_dir.join("tsconfig.json");
        write_file(&config, "{}");
        write_file(&temp.path.join(local_tsc_relative_path()), "");

        ensure_typescript_oracle(&package_dir, &config)
            .expect("hoisted workspace TypeScript should satisfy preflight");
    }

    #[test]
    fn typescript_oracle_preflight_rejects_missing_tsc() {
        let temp = TempDir::new();
        let package_dir = temp.path.join("packages/foo");
        let config = package_dir.join("tsconfig.json");
        write_file(&config, "{}");

        let error = ensure_typescript_oracle(&package_dir, &config)
            .expect_err("missing local TypeScript should be rejected")
            .to_string();

        assert!(error.contains("TypeScript 6.0.3 or newer"));
        assert!(error.contains("node_modules/.bin/tsc"));
    }

    #[test]
    fn tsz_timeout_env_value_must_be_positive_seconds() {
        assert_eq!(
            tsz_timeout_from_env_value(None),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("45")),
            Duration::from_secs(45)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("0")),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
        assert_eq!(
            tsz_timeout_from_env_value(Some("nope")),
            Duration::from_secs(DEFAULT_TSZ_TIMEOUT_SECS)
        );
    }

    #[test]
    fn tsconfig_context_collects_local_extends_and_references() {
        let temp = TempDir::new();
        write_file(
            &temp.path.join("tsconfig.base.json"),
            "{ // jsonc is accepted\n  \"compilerOptions\": { \"strict\": true }\n}\n",
        );
        write_file(
            &temp.path.join("packages/shared/tsconfig.json"),
            "{ \"compilerOptions\": { \"composite\": true } }\n",
        );
        write_file(
            &temp.path.join("packages/app/tsconfig.json"),
            "{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"references\": [{ \"path\": \"../shared\" }]\n}\n",
        );

        let snapshots =
            collect_tsconfig_context(&temp.path, &report_for_config("packages/app/tsconfig.json"));
        let labels = snapshots
            .iter()
            .map(|snapshot| snapshot.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "packages/app/tsconfig.json",
                "tsconfig.base.json",
                "packages/shared/tsconfig.json"
            ]
        );
        assert!(snapshots.iter().all(|snapshot| !snapshot.truncated));
    }

    #[test]
    fn diagnostic_diff_detects_extra_missing_and_order() {
        let first = diag(2322, "a.ts", "A");
        let second = diag(2339, "b.ts", "B");

        let diff = diff_diagnostics(
            std::slice::from_ref(&first),
            &[first.clone(), second.clone()],
        );
        assert_eq!(diff.extra_tsz, vec![second.clone()]);
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 0);

        let diff = diff_diagnostics(&[first.clone(), second.clone()], &[second, first]);
        assert!(diff.extra_tsz.is_empty());
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 2);
    }

    #[test]
    fn config_deprecation_diagnostics_ignore_location_for_try_tsz_diff() {
        let message = concat!(
            "Option 'moduleResolution=node10' is deprecated and will stop functioning in TypeScript 7.0.",
            " Specify compilerOption '\"ignoreDeprecations\": \"6.0\"' to silence this error.",
            "\n  Visit https://aka.ms/ts6 for migration information.",
        );
        let mut tsc = ComparableDiagnostic {
            file: None,
            start: None,
            length: None,
            line: None,
            column: None,
            code: 5107,
            category: "error".to_string(),
            message: message.to_string(),
        };
        let mut tsz = diag(5107, "tsconfig.json", message);

        normalize_config_deprecation_location(&mut tsc);
        normalize_config_deprecation_location(&mut tsz);
        let diff = diff_diagnostics(&[tsc], &[tsz]);

        assert!(diff.extra_tsz.is_empty());
        assert!(diff.missing_tsc.is_empty());
        assert_eq!(diff.order_mismatches, 0);
    }

    #[test]
    fn line_window_returns_context_around_offset() {
        let source = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";
        let snippet = enclosing_line_window(source, 14);

        assert!(snippet.contains("three"));
        assert!(snippet.contains("five"));
    }
}
