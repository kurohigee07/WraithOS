use clap::{Parser, Subcommand};
use wraith_sdk::WraithClient;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "wraith", version = "0.1.0", about = "Wraith-V2 Orchestrator CLI")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:9090", global = true)]
    server: String,
    #[arg(long, global = true)]
    token: Option<String>,
    #[arg(long, global = true)]
    tenant: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all modules
    List,
    /// Upload a .wasm module
    Deploy {
        #[arg(short, long)]
        id: String,
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Execute a module with JSON payload
    Exec {
        id: String,
        #[arg(short, long, default_value = "{}")]
        payload: String,
    },
    /// Delete a module
    Delete {
        id: String,
    },
    /// View execution logs (last 50)
    Logs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut client = WraithClient::new(cli.server)?;
    if let Some(t) = cli.token {
        client = client.with_token(t);
    }
    let tenant = cli.tenant.as_deref();

    match cli.command {
        Commands::List => {
            let list = client.list(tenant).await?;
            println!("{:<15} {:<10} {:<10}", "ID", "TENANT", "VERSION");
            println!("{}", "-".repeat(35));
            for m in list.modules {
                println!("{:<15} {:<10} {:<10}", m.id, m.tenant, m.version);
            }
            println!("\nTotal: {}", list.count);
        }
        Commands::Deploy { id, file } => {
            let bytes = std::fs::read(&file)?;
            let res = client.upload(&id, tenant, bytes).await?;
            println!("Deployed: {}", res);
        }
        Commands::Exec { id, payload } => {
            let json_val: serde_json::Value = serde_json::from_str(&payload)?;
            let res = client.execute(&id, tenant, json_val).await?;
            println!("{:#}", res);
        }
        Commands::Delete { id } => {
            client.delete(&id, tenant).await?;
            println!("Deleted: {}", id);
        }
        Commands::Logs => {
            let logs = client.get_logs().await?;
            println!("{:<6} {:<15} {:<15} {:<8} {:<8} {}", "ID", "TENANT", "MODULE", "STATUS", "TIME(ms)", "EXECUTED_AT");
            println!("{}", "-".repeat(80));
            for l in logs {
                println!("{:<6} {:<15} {:<15} {:<8} {:<8} {}", l.id, l.tenant, l.module_id, l.status, l.duration_ms, l.executed_at);
            }
        }
    }
    Ok(())
}
