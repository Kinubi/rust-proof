use anyhow::Result;
use clap::{ Parser, Subcommand };

const DEFAULT_RPC_URL: &str = "http://127.0.0.1:8545";

#[derive(Debug, Parser)]
#[command(name = "rust-proof-client", about = "Desktop client scaffold for rust-proof")]
struct Cli {
    #[arg(long, default_value = DEFAULT_RPC_URL)]
    rpc_url: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    Status,
    Keygen,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Status) {
        Command::Status => print_status(&cli.rpc_url),
        Command::Keygen => print_keygen_todo(),
    }

    Ok(())
}

fn print_status(rpc_url: &str) {
    println!("rust-proof desktop client");
    println!("rpc endpoint: {rpc_url}");
    println!(
        "status RPC is not wired yet; this crate is the CLI scaffold for future node interaction."
    );
}

fn print_keygen_todo() {
    println!("key generation is not wired yet;");
    println!("next step: add a wallet module once the node RPC and transaction format settle.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_cli_values() {
        let cli = Cli::parse_from(["rust-proof-client"]);

        assert_eq!(cli.rpc_url, DEFAULT_RPC_URL);
        assert!(cli.command.is_none());
    }
}
