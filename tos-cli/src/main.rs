use anyhow::Result;
use clap::{Parser, Subcommand};

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

fn main() -> Result<()> {
    tracing_subscriber::fmt::try_init().ok();
    let cli = Cli::parse();

    match cli.command {
        Command::Push { from, to, table } => {
            eprintln!("[scaffold] tos push --from {from} --to {to} --table {table:?}");
            eprintln!("[scaffold] real implementation in S3");
            Ok(())
        }
        Command::Sync { from, to, table, watch } => {
            eprintln!("[scaffold] tos sync --from {from} --to {to:?} --table {table:?} --watch {watch}");
            eprintln!("[scaffold] real implementation in S4");
            Ok(())
        }
        Command::Schema { action } => {
            eprintln!("[scaffold] tos schema {action:?}");
            eprintln!("[scaffold] real implementation in S5");
            Ok(())
        }
        Command::Topology { file, start } => {
            eprintln!("[scaffold] tos topology --file {file:?} --start {start}");
            eprintln!("[scaffold] real implementation in S5");
            Ok(())
        }
        Command::Node { action } => {
            eprintln!("[scaffold] tos node {action:?}");
            eprintln!("[scaffold] real implementation in S5");
            Ok(())
        }
        Command::Status => {
            eprintln!("[scaffold] tos status (no active sessions)");
            Ok(())
        }
        Command::Log { follow } => {
            eprintln!("[scaffold] tos log --follow {follow}");
            Ok(())
        }
    }
}
