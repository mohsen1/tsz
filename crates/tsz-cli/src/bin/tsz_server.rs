fn main() {
    if std::env::args()
        .skip(1)
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        println!(
            "tsz-server {}\n\nUsage: tsz-server\n\nSpeaks the framed tsserver protocol on stdio.",
            env!("CARGO_PKG_VERSION")
        );
        return;
    }
    let result = tsz_cli::tsserver::run_tsserver(std::io::stdin().lock(), std::io::stdout().lock());
    if result.is_err() {
        std::process::exit(1);
    }
}
