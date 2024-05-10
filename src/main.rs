use anyhow::Result;
use clap::{Parser, Subcommand};
use gofile_api::{Api, ServerApi, UploadedFile};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task;
use tokio::task::JoinSet;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use gofile_api::UploadedMessage;

const UNWATNED_EXTENSION: [&'static str; 2] = ["sfv", "nfo"];
const MAX_CONCURRENT: usize = 4;

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

fn is_directory(entry: &DirEntry) -> bool {
    entry.path().is_dir()
}

fn is_unwanted(entry: &DirEntry) -> bool {
    UNWATNED_EXTENSION.contains(
        &entry
            .path()
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
            .as_str(),
    )
}

async fn upload(path: PathBuf) -> Result<()> {
    let api = Api::default();
    let mut map = HashMap::new();
    let (tx, rx) = mpsc::unbounded_channel::<UploadedMessage>();
    let mut total_size: u64 = 0;
    let mut set = JoinSet::new();
    let mut result = HashMap::new();

    let server_api = api.get_server(Uuid::new_v4()).await?;

    for e in WalkDir::new(&path).follow_links(true) {
        let entry = e?;
        if is_directory(&entry) || is_unwanted(&entry) {
            continue;
        }
        let size = entry.path().metadata()?.len();
        total_size += size;
        let uuid = Uuid::new_v4();
        map.insert(uuid, entry);
    }

    task::spawn(track_upload_progress(
        rx,
        path.clone(),
        total_size,
        server_api.base_url.clone(),
        map.clone(),
    ));
    let mut files_uploaded: usize = 0;
    let map_length = map.len();
    for (k, v) in &map {
        let file_path = v.path().to_path_buf();
        set.spawn(upload_file(
            server_api.clone(),
            k.clone(),
            file_path,
            tx.clone(),
        ));
        while set.len() >= MAX_CONCURRENT || files_uploaded != map_length {
            match set.join_next().await {
                Some(res) => {
                    let uploaded_file_info = res??;
                    files_uploaded += 1;
                    result.insert(k, uploaded_file_info.download_page);
                }
                None => {
                    break;
                }
            }
        }
    }

    for (k, v) in result {
        let entry = map.get(&k).unwrap();
        println!("{} {}", entry.path().display(), v);
    }

    Ok(())
}

async fn track_upload_progress(
    mut rx: mpsc::UnboundedReceiver<UploadedMessage>,
    input_path: PathBuf,
    total_size: u64,
    gofile_url: String,
    _map: HashMap<Uuid, DirEntry>, // for individual file progress tracking, not used rn
) {
    let mut bar = gofile_api::bar::WrappedBar::new(total_size, &gofile_url, false);
    let input = input_path.to_string_lossy();
    bar.set_length(total_size);
    let mut uploaded_map: HashMap<Uuid, u64> = HashMap::new();
    while let Some(message) = rx.recv().await {
        // let entry = map.get(&message.uuid).unwrap();
        uploaded_map.insert(message.uuid, message.uploaded);
        let total_uploaded: u64 = uploaded_map.values().into_iter().sum();
        bar.set_position(total_uploaded);
        if total_uploaded >= total_size {
            bar.finish_upload(&input, &gofile_url);
        }
    }
}

async fn upload_file(
    server_api: ServerApi,
    uuid: Uuid,
    path: PathBuf,
    tx: UnboundedSender<UploadedMessage>,
) -> Result<UploadedFile> {
    let mut modified_server_api = server_api;
    modified_server_api.uuid = uuid;
    let uploaded_file_info = modified_server_api.upload_file(path, tx).await?;
    Ok(uploaded_file_info)
}
