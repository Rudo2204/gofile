use anyhow::Result;
use clap::{Parser, Subcommand};
use gofile_api::Api;
use std::path::PathBuf;
use tokio::fs::metadata;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Upload {
        #[arg()]
        path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    match args.command {
        Command::Upload { path } => upload(path).await?,
    }

    Ok(())
}

async fn upload(path: PathBuf) -> Result<()> {
    let api = Api::default();
    let metadata = metadata(&path).await?;

    if !metadata.is_file() {
        anyhow::bail!("Not a file");
    };

    let server_api = api.get_server().await?;
    let uploaded_file_info = server_api.upload_file(path).await?;

    println!("{}", uploaded_file_info.download_page);

    Ok(())
}
