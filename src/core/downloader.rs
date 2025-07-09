use crate::core::{VideoFormat, VideoMetadata};
use anyhow::Result;
use futures::StreamExt;
use std::path::PathBuf;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tracing::{info, warn};
use std::time::Duration;

pub struct Downloader {
    client: reqwest::Client,
    pub concurrent_limit: usize,
}

impl Downloader {
    pub fn new(concurrent_limit: usize) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("Failed to create HTTP client");
            
        Self {
            client,
            concurrent_limit,
        }
    }
    
    pub async fn download(&self, metadata: &VideoMetadata, output_path: PathBuf) -> Result<()> {
        // Select best format
        let format = self.select_best_format(&metadata.formats)?;
        
        info!("Downloading: {} - {}", metadata.title, format.format_id);
        info!("URL: {}", format.url);
        
        // Download the video
        self.download_format(format, output_path).await?;
        
        Ok(())
    }
    
    pub fn select_best_format<'a>(&self, formats: &'a [VideoFormat]) -> Result<&'a VideoFormat> {
        // Simple selection: prefer mp4, then highest resolution
        let best = formats
            .iter()
            .filter(|f| f.vcodec.is_some() && f.acodec.is_some()) // Has both video and audio
            .max_by_key(|f| {
                let score = match f.ext.as_str() {
                    "mp4" => 1000,
                    "webm" => 500,
                    _ => 0,
                };
                score + f.tbr.unwrap_or(0.0) as i32
            });
            
        best.ok_or_else(|| anyhow::anyhow!("No suitable format found"))
    }
    
    async fn download_format(&self, format: &VideoFormat, output_path: PathBuf) -> Result<()> {
        // Check if partial file exists for resume capability
        let resume_from = if output_path.exists() {
            match tokio::fs::metadata(&output_path).await {
                Ok(metadata) => {
                    let size = metadata.len();
                    info!("Found partial file, resuming from {} bytes", size);
                    Some(size)
                }
                Err(_) => None
            }
        } else {
            None
        };
        
        // Retry logic with exponential backoff for 403 errors
        const MAX_RETRIES: u32 = 3;
        let mut attempt = 0;
        
        loop {
            attempt += 1;
            
            // Build request with enhanced anti-detection headers
            let mut request = self.client
                .get(&format.url)
                .header("Accept", "*/*")
                .header("Accept-Language", "en-US,en;q=0.9")
                .header("Accept-Encoding", "gzip, deflate, br")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .header("Pragma", "no-cache")
                .header("Referer", "https://www.youtube.com/")
                .header("Origin", "https://www.youtube.com")
                .header("Sec-Fetch-Dest", "video")
                .header("Sec-Fetch-Mode", "no-cors")
                .header("Sec-Fetch-Site", "cross-site")
                .header("Sec-Ch-Ua", "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"")
                .header("Sec-Ch-Ua-Mobile", "?0")
                .header("Sec-Ch-Ua-Platform", "\"Windows\"")
                .header("Upgrade-Insecure-Requests", "1")
                .header("X-Client-Data", "CgSLywE=")
                .header("X-Youtube-Client-Name", "1")
                .header("X-Youtube-Client-Version", "2.20231201.00.00");
            
            if let Some(resume_pos) = resume_from {
                request = request.header("Range", format!("bytes={}-", resume_pos));
            }
            
            let response = match request.send().await {
                Ok(response) => response,
                Err(e) => {
                    if attempt >= MAX_RETRIES {
                        return Err(e.into());
                    }
                    warn!("Request failed (attempt {}): {}", attempt, e);
                    tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt))).await;
                    continue;
                }
            };
            
            let status = response.status();
            
            if status.is_success() || status.as_u16() == 206 {
                // Success - proceed with download
                return self.perform_download(response, output_path, resume_from).await;
            } else if status.as_u16() == 403 && attempt < MAX_RETRIES {
                // 403 Forbidden - retry with backoff
                warn!("HTTP 403 error (attempt {}), retrying in {} seconds...", attempt, 2_u64.pow(attempt));
                tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt))).await;
                continue;
            } else {
                // Other errors or max retries exceeded
                anyhow::bail!("Failed to download after {} attempts: HTTP {}", attempt, status);
            }
        }
    }
    
    async fn perform_download(&self, response: reqwest::Response, output_path: PathBuf, resume_from: Option<u64>) -> Result<()> {
        let total_size = response.content_length();
        let mut downloaded = resume_from.unwrap_or(0);
        
        // Open file in append mode if resuming, create new otherwise
        let mut file = if resume_from.is_some() {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&output_path)
                .await?;
            file.seek(std::io::SeekFrom::End(0)).await?;
            file
        } else {
            File::create(&output_path).await?
        };
        
        let mut stream = response.bytes_stream();
        
        // Calculate total expected size
        let expected_total = if let Some(partial_size) = resume_from {
            total_size.map(|size| size + partial_size)
        } else {
            total_size
        };
        
        println!(
            "Downloading {} bytes...", 
            expected_total.map_or("unknown".to_string(), |s| s.to_string())
        );
        
        if resume_from.is_some() {
            println!("Resuming from {} bytes", resume_from.unwrap());
        }
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded += chunk.len() as u64;
            file.write_all(&chunk).await?;
            
            // Progress reporting
            if let Some(total) = expected_total {
                let progress = (downloaded as f64 / total as f64 * 100.0) as u32;
                print!("\rProgress: {}% ({}/{} bytes)", progress, downloaded, total);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            } else {
                if downloaded % (1024 * 1024) == 0 { // Report every MB
                    print!("\rDownloaded: {} bytes", downloaded);
                    std::io::Write::flush(&mut std::io::stdout()).unwrap();
                }
            }
        }
        
        println!(); // New line after progress
        file.flush().await?;
        info!("Downloaded to: {}", output_path.display());
        
        Ok(())
    }
}