#[macro_use]
extern crate lazy_static;

use std::env;

use anyhow::Result;
use clap::{ArgEnum, Parser, Subcommand};
use reqwest::Url;
use resources::objects;
use strum::Display;

mod create;
mod delete;
mod describe;
mod exec;
mod get;
mod logs;
mod patch;
mod utils;

struct AppConfig {
    base_url: Url,
}

lazy_static! {
    static ref CONFIG: AppConfig = AppConfig {
        base_url: match env::var("API_SERVER_URL") {
            Ok(url) => Url::parse(url.as_str()).unwrap(),
            Err(_) => Url::parse("http://127.0.0.1:8080/").unwrap(),
        }
    };
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a resource using configuration file.
    Create(create::Arg),
    /// Delete a resource by name.
    Delete(delete::Arg),
    /// Get resources list.
    Get(get::Arg),
    /// Patch a resource.
    Patch(patch::Arg),
    /// Describe a resource.
    Describe(describe::Arg),
    /// Print pod container logs.
    Logs(logs::Arg),
    /// Execute commands in a pod container.
    Exec(exec::Arg),
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Display)]
#[strum(serialize_all = "lowercase")]
enum ResourceKind {
    Pods,
    ReplicaSets,
    Services,
    Ingresses,
    HorizontalPodAutoscalers,
    GpuJobs,
    Nodes,
    Functions,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Create(arg) => arg.handle().await?,
        Commands::Delete(arg) => arg.handle().await?,
        Commands::Get(arg) => arg.handle().await?,
        Commands::Patch(arg) => arg.handle().await?,
        Commands::Describe(arg) => arg.handle().await?,
        Commands::Logs(arg) => arg.handle().await?,
        Commands::Exec(arg) => arg.handle().await?,
    }

    Ok(())
}
