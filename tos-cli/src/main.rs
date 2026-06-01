mod cmd;
mod daemon;
mod schema;
mod uri;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::cmd::{push, sync};
use crate::daemon::{load_config, Daemon};

#[derive(Parser, Debug)]
#[command(name = "tos", version, about = "Translation of Service: P2P data sync")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Push {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        table: Option<String>,
    },
    Sync {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: Vec<String>,
        #[arg(long)]
        table: Option<String>,
        #[arg(long, default_value_t = false)]
        watch: bool,
        #[arg(long, default_value_t = 5)]
        interval: u64,
    },
    Schema {
        #[command(subcommand)]
        action: SchemaAction,
    },
    Topology {
        #[arg(long)]
        file: Option<String>,
        #[arg(long, default_value_t = false)]
        start: bool,
    },
    Node {
        #[command(subcommand)]
        action: NodeAction,
    },
    Status,
    Log {
        #[arg(long, default_value_t = false)]
        follow: bool,
    },
}

#[derive(Subcommand, Debug)]
enum SchemaAction {
    Pull { uri: String },
    Push { file: String, #[arg(long)] to: String },
    Infer { #[arg(long)] from: String },
    Diff { file1: String, file2: String },
    Validate { file: String },
}

#[derive(Subcommand, Debug)]
enum NodeAction {
    Start,
    Stop,
    Status,
    Id,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::try_init().ok();
    let cli = Cli::parse();

    match cli.command {
        Command::Push { from, to, table } => {
            match push(&from, &to, table.as_deref()).await {
                Ok(stats) => {
                    println!(
                        "pushed {} records ({} batches, {} bytes) in {}ms",
                        stats.total_records,
                        stats.total_batches,
                        stats.bytes_sent,
                        stats.duration_ms
                    );
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Command::Sync { from, to, table, watch, interval } => {
            match sync(&from, &to, table.as_deref(), watch, interval).await {
                Ok(stats) => {
                    for (i, s) in stats.iter().enumerate() {
                        println!(
                            "[{i}] pushed {} records ({} batches, {} bytes) in {}ms",
                            s.total_records, s.total_batches, s.bytes_sent, s.duration_ms
                        );
                    }
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Command::Schema { action } => match action {
            SchemaAction::Pull { uri } => schema::pull(&uri).await,
            SchemaAction::Push { file, to } => schema::push(&PathBuf::from(file), &to).await,
            SchemaAction::Infer { from } => schema::infer(&PathBuf::from(from)).await,
            SchemaAction::Diff { file1, file2 } => {
                schema::diff(&PathBuf::from(file1), &PathBuf::from(file2))
            }
            SchemaAction::Validate { file } => schema::validate_file(&PathBuf::from(file)),
        },
        Command::Topology { file, start } => {
            if start {
                let path = file
                    .map(PathBuf::from)
                    .unwrap_or_else(crate::daemon::default_topology_path);
                let cfg = load_config(&path)?;
                let mut d = Daemon::new(cfg);
                d.start().await?;
                println!(
                    "node {} started, uptime 0s, {} pipelines",
                    d.node_id(),
                    d.config_ref().pipeline.len()
                );
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        println!("\nshutdown signal received, stopping pipelines");
                        d.abort_all();
                    }
                }
                Ok(())
            } else {
                let path = file
                    .map(PathBuf::from)
                    .unwrap_or_else(crate::daemon::default_topology_path);
                let cfg = load_config(&path)?;
                println!("loaded {} ({} pipelines)", path.display(), cfg.pipeline.len());
                for p in &cfg.pipeline {
                    println!(
                        "  - {}: {} -> {} (batch={}, watch={})",
                        p.name,
                        p.from,
                        p.to.join(","),
                        p.batch_size,
                        p.watch
                    );
                }
                Ok(())
            }
        }
        Command::Node { action } => match action {
            NodeAction::Start => {
                let path = crate::daemon::default_topology_path();
                let cfg = load_config(&path)?;
                let mut d = Daemon::new(cfg);
                d.start().await?;
                println!("node {} started, {} pipelines", d.node_id(), d.config_ref().pipeline.len());
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        println!("\nstopping");
                        d.abort_all();
                    }
                }
                Ok(())
            }
            NodeAction::Stop => {
                println!("no PID file: stop runs against an active foreground process (Ctrl+C)");
                Ok(())
            }
            NodeAction::Status => {
                println!("node status is foreground-only in v1.0");
                Ok(())
            }
            NodeAction::Id => {
                let id = crate::daemon::generate_node_id_pub();
                println!("{id}");
                Ok(())
            }
        },
        Command::Status => {
            println!("status is foreground-only in v1.0");
            Ok(())
        }
        Command::Log { follow } => {
            if follow {
                println!("log --follow is not implemented; use stdout from `tos topology --start`");
            } else {
                println!("log tail is not implemented; see stdout from `tos topology --start`");
            }
            Ok(())
        }
    }
}
