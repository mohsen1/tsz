fn main() {
    let arguments = std::env::args_os().skip(1);
    match tsz_cli::driver::main_entry(arguments) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            println!("{error:#}");
            std::process::exit(1);
        }
    }
}
