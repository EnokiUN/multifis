use std::{
    cmp::min,
    env,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use anyhow::{bail, Context, Result};
use reqwest::{
    header::HeaderMap,
    multipart::{Form, Part},
    Client,
};
use todel::models::{FileData, InstanceInfo};
use tokio::{fs, io::AsyncWriteExt, time};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let mut args = env::args().skip(1);

    let subcommand = args
        .next()
        .context("You must pass either `upload` or `download` as the subcommand")?;

    let instance_url = args
        .next()
        .context("You must pass an instance URL as the first argument")?;

    let client = Client::new();

    match subcommand.as_str() {
        "upload" => {
            let file_name = args
                .next()
                .context("You must pass a file path to be uploaded")?;
            let mut file = fs::read(&file_name)
                .await
                .with_context(|| format!("Could not read {}", file_name))?;
            let path = PathBuf::from(file_name);
            let file_name = path
                .file_name()
                .context("Could not find file name")?
                .to_str()
                .context("Could not convert to normal string")?;
            let instance_info = get_instance_info(&client, &instance_url).await?;
            if file.len() as u64 <= instance_info.attachment_file_size {
                bail!("File is not big enough to need splitting up, try normal uploading instead");
            }
            let mut file_ids = vec![];
            let parts =
                (file.len() as f64 / instance_info.attachment_file_size as f64).ceil() as u64;
            for index in 0..parts {
                eprintln!("Uploading part {}/{}", index + 1, parts);
                let part = file
                    .drain(0..min(file.len() as u64, instance_info.attachment_file_size) as usize)
                    .collect::<Vec<u8>>();
                let response = client
                    .post(&instance_info.effis_url)
                    .multipart(
                        Form::new().part(
                            "file",
                            Part::bytes(part)
                                .file_name(format!("{}-{}", file_name, index))
                                .mime_str("application/octet-stream")?,
                        ),
                    )
                    .send()
                    .await?;
                let headers = response.headers();
                rate_limit(
                    headers,
                    min(file.len() as u64, instance_info.attachment_file_size),
                )
                .await?;
                let part_data: FileData = response.json().await?;
                file_ids.push(part_data.id);
            }

            let meta_file_data: FileData = client
                .post(&instance_info.effis_url)
                .multipart(
                    Form::new().part(
                        "file",
                        Part::bytes(
                            format!(
                                "{}\0{}",
                                file_name,
                                file_ids
                                    .into_iter()
                                    .map(|i| i.to_string())
                                    .collect::<Vec<String>>()
                                    .join("\0")
                            )
                            .bytes()
                            .collect::<Vec<u8>>(),
                        )
                        .file_name(format!("{}-meta", file_name))
                        .mime_str("text/plain")?,
                    ),
                )
                .send()
                .await?
                .json()
                .await?;
            println!("{}", meta_file_data.id);
        }
        "download" => {
            let instance_info = get_instance_info(&client, &instance_url).await?;
            let meta_file_id = args
                .next()
                .context("You must pass a meta file id to download a multi-part file")?;
            let meta = client
                .get(format!("{}/{}", instance_info.effis_url, meta_file_id))
                .send()
                .await
                .context("Could not fetch meta file with provided ID")?
                .text()
                .await?;
            let mut parts = meta.split("\0");
            let file_name = parts.next().context("Invalid metadata format")?;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(file_name)
                .await
                .context("Could not create file")?;
            let part_count = parts.clone().count();
            for (index, id) in parts.enumerate() {
                eprintln!("Downloading part {}/{}...", index + 1, part_count);
                let part = client
                    .get(format!("{}/{}", instance_info.effis_url, id))
                    .send()
                    .await
                    .with_context(|| format!("Could not fetch part {}", id))?
                    .bytes()
                    .await?;
                file.write_all(&part).await?;
            }
            eprintln!("Finished installing {}", file_name);
        }
        _ => bail!("Unknown subcommand, expected either `upload` or `download`"),
    }

    Ok(())
}

async fn get_instance_info(client: &Client, instance_url: &str) -> Result<InstanceInfo> {
    Ok(client.get(instance_url).send().await?.json().await?)
}

async fn rate_limit(headers: &HeaderMap, next_upload: u64) -> Result<()> {
    let byte_max = get_header_value(headers, "X-RateLimit-Byte-Max")?;
    let sent_bytes = get_header_value(headers, "X-RateLimit-Sent-Bytes")?;
    let max = get_header_value(headers, "X-RateLimit-Max")?;
    let request_count = get_header_value(headers, "X-RateLimit-Request-Count")?;
    if max == request_count || sent_bytes + next_upload > byte_max {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64;
        let reset = get_header_value(headers, "X-RateLimit-Reset")?;
        let last_reset = get_header_value(headers, "X-RateLimit-Last-Reset")?;
        eprintln!("Rate limiting for {}ms", last_reset + reset - now);
        time::sleep(Duration::from_millis(last_reset + reset - now)).await;
    }
    Ok(())
}

fn get_header_value(headers: &HeaderMap, value: &str) -> Result<u64> {
    Ok(headers
        .get(value)
        .with_context(|| format!("Could not find {} header", value))?
        .to_str()?
        .parse()
        .with_context(|| format!("Could not parse {} header", value))?)
}
