use crate::core::{Extractor, Thumbnail, VideoFormat, VideoMetadata};
use crate::extractors::youtube_signature::SignatureDecrypter;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use url::Url;

pub struct YouTubeExtractor {
    client: reqwest::Client,
    signature_decrypter: SignatureDecrypter,
}

impl YouTubeExtractor {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            signature_decrypter: SignatureDecrypter::new(),
        }
    }

    pub fn extract_video_id(&self, url: &Url) -> Option<String> {
        // Handle various YouTube URL formats
        if url.host_str() == Some("youtu.be") {
            return url.path_segments()?.next().map(|s| s.to_string());
        }

        if let Some(host) = url.host_str() {
            if host.contains("youtube.com") {
                if let Some(v) = url.query_pairs().find(|(key, _)| key == "v") {
                    return Some(v.1.to_string());
                }
            }
        }

        None
    }

    async fn extract_player_js(&self, html: &str) -> Result<String> {
        // Extract the player JavaScript URL from the HTML
        let re = Regex::new(r#"(/s/player/[^"]+\.js)"#)?;

        if let Some(captures) = re.captures(html) {
            let js_path = captures.get(1).unwrap().as_str();
            let js_url = format!("https://www.youtube.com{}", js_path);

            let response = self
                .client
                .get(&js_url)
                .header("Accept", "*/*")
                .header("Accept-Language", "en-US,en;q=0.9")
                .header("Accept-Encoding", "gzip, deflate, br")
                .header("Referer", "https://www.youtube.com/")
                .header("Origin", "https://www.youtube.com")
                .header("Sec-Fetch-Dest", "script")
                .header("Sec-Fetch-Mode", "no-cors")
                .header("Sec-Fetch-Site", "same-origin")
                .send()
                .await?;
            let js_content = response.text().await?;

            Ok(js_content)
        } else {
            anyhow::bail!("Could not find player JavaScript URL");
        }
    }

    fn decrypt_signature(&mut self, signature: &str, js_content: &str) -> Result<String> {
        // Use the proper signature decryption based on yt-dlp's approach
        self.signature_decrypter
            .decrypt_signature(signature, js_content)
    }

    fn parse_query_string(&self, query: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();

        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                params.insert(
                    urlencoding::decode(key).unwrap_or_default().to_string(),
                    urlencoding::decode(value).unwrap_or_default().to_string(),
                );
            }
        }

        params
    }

    async fn process_cipher_format(
        &mut self,
        format: &Value,
        js_content: &str,
    ) -> Result<Option<String>> {
        // Handle signatureCipher or cipher formats
        let cipher = format
            .get("signatureCipher")
            .or_else(|| format.get("cipher"))
            .and_then(|v| v.as_str());

        if let Some(cipher_str) = cipher {
            let params = self.parse_query_string(cipher_str);

            if let (Some(url), Some(signature)) = (params.get("url"), params.get("s")) {
                // Decrypt the signature
                let decrypted_sig = self.decrypt_signature(signature, js_content)?;

                // Construct the final URL
                let default_sp = "signature".to_string();
                let sp = params.get("sp").unwrap_or(&default_sp);
                let final_url = format!("{}&{}={}", url, sp, decrypted_sig);

                return Ok(Some(final_url));
            }
        }

        Ok(None)
    }

    async fn extract_metadata_with_js(
        &mut self,
        html: &str,
        video_id: &str,
        js_content: &str,
    ) -> Result<VideoMetadata> {
        // Extract ytInitialPlayerResponse JSON
        let player_response = self.extract_player_response(html)?;

        // Extract basic video details
        let video_details = player_response
            .get("videoDetails")
            .ok_or_else(|| anyhow::anyhow!("No video details found"))?;

        let title = video_details
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Title")
            .to_string();

        let description = video_details
            .get("shortDescription")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let duration = video_details
            .get("lengthSeconds")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        let uploader = video_details
            .get("author")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let view_count = video_details
            .get("viewCount")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        // Extract formats from streaming data with JS support
        let formats = self
            .extract_formats_with_js(&player_response, js_content)
            .await?;

        // Generate thumbnails
        let thumbnails = self.generate_thumbnails(video_id);

        Ok(VideoMetadata {
            id: video_id.to_string(),
            title,
            description,
            duration,
            uploader,
            upload_date: None, // TODO: Extract upload date
            view_count,
            like_count: None, // TODO: Extract like count
            formats,
            thumbnails,
            subtitles: std::collections::HashMap::new(), // TODO: Extract subtitles
        })
    }

    fn extract_player_response(&self, html: &str) -> Result<Value> {
        // Try multiple patterns for ytInitialPlayerResponse
        let patterns = [
            r#"ytInitialPlayerResponse\s*=\s*(\{.+?\});"#,
            r#"ytInitialPlayerResponse\s*=\s*(\{.+?\})\s*;"#,
            r#"ytInitialPlayerResponse":\s*(\{.+?\})"#,
            r#"var\s+ytInitialPlayerResponse\s*=\s*(\{.+?\});"#,
        ];

        for pattern in &patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(captures) = re.captures(html) {
                    let json_str = captures.get(1).unwrap().as_str();
                    // Try to parse the JSON
                    match serde_json::from_str::<Value>(json_str) {
                        Ok(parsed) => return Ok(parsed),
                        Err(_) => continue, // Try next pattern
                    }
                }
            }
        }

        anyhow::bail!("Could not find ytInitialPlayerResponse in HTML");
    }

    async fn extract_formats_with_js(
        &mut self,
        player_response: &Value,
        js_content: &str,
    ) -> Result<Vec<VideoFormat>> {
        let mut formats = Vec::new();

        let streaming_data = player_response
            .get("streamingData")
            .ok_or_else(|| anyhow::anyhow!("No streaming data found"))?;

        // Extract adaptive formats (separate video/audio)
        if let Some(adaptive_formats) = streaming_data
            .get("adaptiveFormats")
            .and_then(|v| v.as_array())
        {
            for format in adaptive_formats {
                if let Some(video_format) = self.parse_format_with_js(format, js_content).await? {
                    formats.push(video_format);
                }
            }
        }

        // Extract regular formats (combined video/audio)
        if let Some(regular_formats) = streaming_data.get("formats").and_then(|v| v.as_array()) {
            for format in regular_formats {
                if let Some(video_format) = self.parse_format_with_js(format, js_content).await? {
                    formats.push(video_format);
                }
            }
        }

        if formats.is_empty() {
            anyhow::bail!("No video formats found");
        }

        Ok(formats)
    }

    async fn parse_format_with_js(
        &mut self,
        format: &Value,
        js_content: &str,
    ) -> Result<Option<VideoFormat>> {
        // Try to get direct URL first
        let url = format
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // If no direct URL, try to process cipher
        let final_url = match url {
            Some(url) => url,
            None => match self.process_cipher_format(format, js_content).await? {
                Some(url) => url,
                None => return Ok(None),
            },
        };

        let itag = format
            .get("itag")
            .and_then(|v| v.as_i64())
            .map(|i| i.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let quality = format
            .get("quality")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let width = format
            .get("width")
            .and_then(|v| v.as_i64())
            .map(|i| i as u32);
        let height = format
            .get("height")
            .and_then(|v| v.as_i64())
            .map(|i| i as u32);

        let resolution = if let (Some(w), Some(h)) = (width, height) {
            Some(format!("{}x{}", w, h))
        } else {
            None
        };

        let fps = format.get("fps").and_then(|v| v.as_f64());

        let mime_type = format
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("video/mp4");

        let (vcodec, acodec, ext) = self.parse_mime_type(mime_type);

        let bitrate = format.get("bitrate").and_then(|v| v.as_f64());

        let filesize = format
            .get("contentLength")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        Ok(Some(VideoFormat {
            format_id: itag,
            url: final_url,
            quality,
            resolution,
            fps,
            vcodec,
            acodec,
            ext: ext.to_string(),
            filesize,
            tbr: bitrate,
            vbr: None, // TODO: Extract video bitrate
            abr: None, // TODO: Extract audio bitrate
        }))
    }

    fn parse_mime_type(&self, mime_type: &str) -> (Option<String>, Option<String>, &str) {
        if mime_type.contains("video/mp4") {
            (Some("h264".to_string()), Some("aac".to_string()), "mp4")
        } else if mime_type.contains("video/webm") {
            (Some("vp9".to_string()), Some("opus".to_string()), "webm")
        } else if mime_type.contains("audio/mp4") {
            (None, Some("aac".to_string()), "m4a")
        } else if mime_type.contains("audio/webm") {
            (None, Some("opus".to_string()), "webm")
        } else {
            (None, None, "unknown")
        }
    }

    fn generate_thumbnails(&self, video_id: &str) -> Vec<Thumbnail> {
        vec![
            Thumbnail {
                url: format!("https://i.ytimg.com/vi/{}/maxresdefault.jpg", video_id),
                width: Some(1280),
                height: Some(720),
                resolution: Some("1280x720".to_string()),
            },
            Thumbnail {
                url: format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id),
                width: Some(480),
                height: Some(360),
                resolution: Some("480x360".to_string()),
            },
            Thumbnail {
                url: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", video_id),
                width: Some(320),
                height: Some(180),
                resolution: Some("320x180".to_string()),
            },
        ]
    }
}

#[async_trait]
impl Extractor for YouTubeExtractor {
    fn name(&self) -> &'static str {
        "YouTube"
    }

    fn suitable(&self, url: &Url) -> bool {
        if let Some(host) = url.host_str() {
            host.contains("youtube.com") || host == "youtu.be"
        } else {
            false
        }
    }

    async fn extract(&mut self, url: &Url) -> Result<VideoMetadata> {
        let video_id = self
            .extract_video_id(url)
            .ok_or_else(|| anyhow::anyhow!("Could not extract video ID from URL"))?;

        // Fetch the YouTube page with enhanced headers
        let video_url = format!("https://www.youtube.com/watch?v={}", video_id);
        let response = self
            .client
            .get(&video_url)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("DNT", "1")
            .header("Connection", "keep-alive")
            .header("Upgrade-Insecure-Requests", "1")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1")
            .header(
                "Sec-Ch-Ua",
                "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"",
            )
            .header("Sec-Ch-Ua-Mobile", "?0")
            .header("Sec-Ch-Ua-Platform", "\"Windows\"")
            .header("Cache-Control", "max-age=0")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to fetch YouTube page: HTTP {}", response.status());
        }

        let html = response.text().await?;

        // Extract player JavaScript for signature decryption
        let js_content = self.extract_player_js(&html).await?;

        // Extract video metadata from the page
        let metadata = self
            .extract_metadata_with_js(&html, &video_id, &js_content)
            .await?;

        Ok(metadata)
    }
}
