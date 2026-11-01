#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("Jynx Harness {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help" | "-h") => {
            println!(
                "Jynx Harness {}\nUsage: jynx-harness [serve]\nRuns the JSON-line agent server on standard input/output.\nUse jynx-cli for interactive chat.",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
        None | Some("serve") => jynx_server::serve_stdio().await,
        Some(other) => anyhow::bail!("Unknown argument: {other}; use --help"),
    }
}
