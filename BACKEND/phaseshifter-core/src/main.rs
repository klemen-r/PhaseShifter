use phaseshifter_core::run_cli;

fn main() {
    if let Err(err) = run_cli() {
        tracing::error!(error = %err, "Application error");
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
