use std::{fs::File, path::PathBuf};

use clap::Parser;
use pod::Pod;
use resources::objects::KubeObject;

mod config;
mod docker;
mod pod;
mod volume;

#[derive(Parser)]
#[clap(version, about, long_about = None)]
struct Args {
    /// Path to pod configuration file.
    #[clap(short, long, parse(from_os_str))]
    path: PathBuf,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let path = args.path;
    let file = File::open(path).unwrap();
    let object: KubeObject = serde_yaml::from_reader(file).unwrap();

    let mut pod = Pod::create(object).await.unwrap();
    pod.start().await.unwrap();
    pod.update_status().await.unwrap();
    println!("{:#?}", pod);
}
