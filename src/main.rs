use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;
use std::sync::OnceLock;
use serde::Deserialize;
use teloxide::{prelude::*, types::{InputFile, UserId}};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use url::Url;

#[derive(Deserialize)]
struct Config {
    allowed_users: HashSet<u64>,
}

static ALLOWED_USERS: OnceLock<HashSet<UserId>> = OnceLock::new();

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // Read config
    let config_content = std::fs::read_to_string("config.json")
        .expect("Failed to read config.json. Please create it with an 'allowed_users' array.");
    let config: Config = serde_json::from_str(&config_content)
        .expect("Failed to parse config.json");

    let allowed_users_set: HashSet<UserId> = config.allowed_users.into_iter().map(UserId).collect();
    ALLOWED_USERS.set(allowed_users_set).unwrap();

    log::info!("Starting URL downloader bot...");

    let bot = Bot::from_env();

    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        // Check authorization
        let authorized = match msg.from {
            Some(ref user) => ALLOWED_USERS.get().unwrap().contains(&user.id),
            None => false,
        };

        if !authorized {
            log::info!("Unauthorized access attempt in chat {}", msg.chat.id);
            let _ = bot.send_message(msg.chat.id, "You are not authorized to use this bot.").await;
            return Ok(());
        }

        if let Some(text) = msg.text() {
            if text.starts_with("http://") || text.starts_with("https://") {
                let _ = bot.send_message(msg.chat.id, "Downloading...").await;
                
                match process_url(&bot, &msg, text).await {
                    Ok(_) => {
                        log::info!("Successfully processed URL for chat {}", msg.chat.id);
                    }
                    Err(e) => {
                        let _ = bot.send_message(msg.chat.id, format!("Error: {}", e)).await;
                    }
                }
            } else {
                let _ = bot.send_message(msg.chat.id, "Please send a valid HTTP/HTTPS URL.").await;
            }
        }
        Ok(())
    })
    .await;
}

async fn process_url(bot: &Bot, msg: &Message, url_str: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let url = Url::parse(url_str)?;
    let filename = get_filename_from_url(&url);

    // Create a temporary file
    let temp_dir = tempfile::tempdir()?;
    let file_path = temp_dir.path().join(&filename);
    
    // Download the file
    download_file(url_str, &file_path).await?;
    
    // Upload and send the file back
    let _ = bot.send_message(msg.chat.id, "Download complete. Uploading to Telegram...").await;
    
    bot.send_document(msg.chat.id, InputFile::file(&file_path))
        .await?;

    // temp_dir will be automatically deleted when it goes out of scope
    Ok(())
}

fn get_filename_from_url(url: &Url) -> String {
    if let Some(segments) = url.path_segments() {
        if let Some(last) = segments.last() {
            if !last.is_empty() {
                // Decode URL-encoded filename if any
                if let Ok(decoded) = urlencoding::decode(last) {
                    return decoded.into_owned();
                }
                return last.to_string();
            }
        }
    }
    "downloaded_file".to_string()
}

async fn download_file(url: &str, path: &PathBuf) -> Result<(), Box<dyn Error + Send + Sync>> {
    let response = reqwest::get(url).await?;
    
    if !response.status().is_success() {
        return Err(format!("Failed to download, status: {}", response.status()).into());
    }

    let mut file = File::create(path).await?;
    
    let bytes = response.bytes().await?;
    file.write_all(&bytes).await?;

    Ok(())
}
