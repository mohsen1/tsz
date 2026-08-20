fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        println!(
            "tsz-server {}\n\nUsage: tsz-server [--protocol legacy]\n\nWithout --protocol, the server speaks the framed tsserver protocol on stdio.",
            env!("CARGO_PKG_VERSION")
        );
        return;
    }
    let legacy = arguments.iter().any(|argument| argument == "legacy");
    let result = if legacy {
        tsz_cli::tsserver::run_legacy_server(std::io::stdin().lock(), std::io::stdout().lock())
    } else {
        tsz_cli::tsserver::run_tsserver(std::io::stdin().lock(), std::io::stdout().lock())
    };
    if result.is_err() {
        std::process::exit(1);
    }
}
