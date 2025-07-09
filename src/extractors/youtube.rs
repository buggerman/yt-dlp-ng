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
        // Use a basic user agent that might bypass some restrictions
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
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
        // Try multiple patterns for player JavaScript URL extraction
        let patterns = [
            r#"(/s/player/[^"]+\.js)"#,
            r#""jsUrl":"(/s/player/[^"]+\.js)"#,
            r#"'jsUrl':'(/s/player/[^']+\.js)"#,
            r#"jsUrl\s*:\s*"(/s/player/[^"]+\.js)"#,
            r#"player_url":"(/s/player/[^"]+\.js)"#,
            r#"PLAYER_JS_URL":"(/s/player/[^"]+\.js)"#,
        ];

        for pattern in &patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(captures) = re.captures(html) {
                    let js_path = captures.get(1).unwrap().as_str();
                    let js_url = format!("https://www.youtube.com{}", js_path);

                    let response = self
                        .client
                        .get(&js_url)
                        .header("Accept", "*/*")
                        .header("Accept-Language", "en-US,en;q=0.9")
                        .header("Accept-Encoding", "identity") // Request no compression
                        .header("Referer", "https://www.youtube.com/")
                        .header("Origin", "https://www.youtube.com")
                        .header("Sec-Fetch-Dest", "script")
                        .header("Sec-Fetch-Mode", "no-cors")
                        .header("Sec-Fetch-Site", "same-origin")
                        .send()
                        .await?;

                    if response.status().is_success() {
                        // Debug: Check response headers for compression info
                        tracing::debug!("JavaScript response status: {}", response.status());
                        tracing::debug!("JavaScript response headers: {:?}", response.headers());
                        
                        let js_content = response.text().await?;
                        tracing::debug!("JavaScript content length: {}", js_content.len());
                        
                        // Check if content looks like valid JavaScript
                        let sample: String = js_content.chars().take(100).collect();
                        let is_text = sample.chars().all(|c| c.is_ascii() || c.is_ascii_whitespace());
                        tracing::debug!("JavaScript content appears to be text: {}", is_text);
                        tracing::debug!("JavaScript content sample: {:?}", sample);
                        
                        return Ok(js_content);
                    }
                }
            }
        }

        // Debug: Show what we're actually getting
        tracing::debug!("HTML content sample: {}", &html[..std::cmp::min(1000, html.len())]);
        
        // Look for any js files in the HTML
        let js_re = Regex::new(r#"(/[^"]*\.js)"#)?;
        let mut js_files = Vec::new();
        for captures in js_re.captures_iter(html) {
            if let Some(js_path) = captures.get(1) {
                js_files.push(js_path.as_str());
            }
        }
        tracing::debug!("Found JS files: {:?}", js_files);

        anyhow::bail!("Could not find player JavaScript URL");
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
        // Handle signatureCipher or cipher formats - this is based on yt-dlp's approach
        let cipher = format
            .get("signatureCipher")
            .or_else(|| format.get("cipher"))
            .and_then(|v| v.as_str());

        if let Some(cipher_str) = cipher {
            tracing::debug!("Processing cipher: {}", cipher_str);
            let params = self.parse_query_string(cipher_str);

            if let (Some(url), Some(signature)) = (params.get("url"), params.get("s")) {
                tracing::debug!("Found signature in cipher: {}", signature);
                // Decrypt the signature using yt-dlp's method
                match self.decrypt_signature(signature, js_content) {
                    Ok(decrypted_sig) => {
                        let default_sp = "signature".to_string();
                        let sp = params.get("sp").unwrap_or(&default_sp);
                        let mut final_url = format!("{}&{}={}", url, sp, decrypted_sig);
                        
                        // Handle n-sig parameter to prevent throttling (critical step from yt-dlp)
                        if let Some(n_param) = params.get("n") {
                            // Decrypt n-sig if present
                            match self.signature_decrypter.decrypt_nsig(n_param, js_content) {
                                Ok(decrypted_nsig) => {
                                    final_url = format!("{}&n={}", final_url, decrypted_nsig);
                                    tracing::debug!("Added decrypted n-sig parameter");
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to decrypt n-sig, using original: {}", e);
                                    final_url = format!("{}&n={}", final_url, n_param);
                                }
                            }
                        }
                        
                        tracing::debug!("Using decrypted signature URL");
                        return Ok(Some(final_url));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to decrypt signature, trying raw signature: {}", e);
                        
                        // Fallback: use raw signature
                        let default_sp = "signature".to_string();
                        let sp = params.get("sp").unwrap_or(&default_sp);
                        let raw_url = format!("{}&{}={}", url, sp, signature);
                        return Ok(Some(raw_url));
                    }
                }
            }
            
            // Check if there's a URL without signature (sometimes cipher has just URL)
            if let Some(url) = params.get("url") {
                return Ok(Some(url.clone()));
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

        // Debug: Check what we actually got
        tracing::debug!("Player response keys: {:?}", player_response.as_object().map(|o| o.keys().collect::<Vec<_>>()));

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

        tracing::debug!("Streaming data keys: {:?}", streaming_data.as_object().map(|o| o.keys().collect::<Vec<_>>()));

        // Extract adaptive formats (separate video/audio)
        if let Some(adaptive_formats) = streaming_data
            .get("adaptiveFormats")
            .and_then(|v| v.as_array())
        {
            tracing::debug!("Found {} adaptive formats", adaptive_formats.len());
            for format in adaptive_formats {
                if let Some(video_format) = self.parse_format_with_js(format, js_content).await? {
                    formats.push(video_format);
                }
            }
        }

        // Extract regular formats (combined video/audio)
        if let Some(regular_formats) = streaming_data.get("formats").and_then(|v| v.as_array()) {
            tracing::debug!("Found {} regular formats", regular_formats.len());
            for format in regular_formats {
                if let Some(video_format) = self.parse_format_with_js(format, js_content).await? {
                    formats.push(video_format);
                }
            }
        }

        tracing::debug!("Successfully extracted {} formats", formats.len());

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
        // Try to get direct URL first (these don't need signature decryption)
        let url = format
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // If no direct URL, try to process cipher
        let mut final_url = match url {
            Some(url) => {
                tracing::debug!("Found direct URL (no signature needed): {}", &url[..100.min(url.len())]);
                url
            },
            None => match self.process_cipher_format(format, js_content).await? {
                Some(url) => {
                    tracing::debug!("Processed cipher URL: {}", &url[..100.min(url.len())]);
                    url
                },
                None => {
                    tracing::debug!("No URL available for format");
                    return Ok(None);
                },
            },
        };

        // Try to remove problematic n parameter that causes throttling
        if let Ok(mut url_obj) = url::Url::parse(&final_url) {
            let query_pairs: Vec<(String, String)> = url_obj.query_pairs()
                .filter(|(k, _)| k != "n")  // Remove n parameter
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            
            url_obj.query_pairs_mut().clear();
            for (key, value) in query_pairs {
                url_obj.query_pairs_mut().append_pair(&key, &value);
            }
            
            final_url = url_obj.to_string();
            tracing::debug!("Removed n parameter from direct URL");
        }

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

    async fn extract_metadata_direct(&self, player_response: &Value, video_id: &str) -> Result<VideoMetadata> {
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

        // Try to extract formats without signature decryption
        let formats = self.extract_formats_direct(player_response).await?;

        // Generate thumbnails
        let thumbnails = self.generate_thumbnails(video_id);

        Ok(VideoMetadata {
            id: video_id.to_string(),
            title,
            description,
            duration,
            uploader,
            upload_date: None,
            view_count,
            like_count: None,
            formats,
            thumbnails,
            subtitles: std::collections::HashMap::new(),
        })
    }

    async fn extract_formats_direct(&self, player_response: &Value) -> Result<Vec<VideoFormat>> {
        let mut formats = Vec::new();

        let streaming_data = player_response
            .get("streamingData")
            .ok_or_else(|| anyhow::anyhow!("No streaming data found"))?;

        // Only try formats that have direct URLs (no signature decryption needed)
        if let Some(adaptive_formats) = streaming_data.get("adaptiveFormats").and_then(|v| v.as_array()) {
            tracing::debug!("Found {} adaptive formats to check", adaptive_formats.len());
            for format in adaptive_formats {
                if let Some(url) = format.get("url").and_then(|v| v.as_str()) {
                    tracing::debug!("Found direct URL format: {}", format.get("itag").unwrap_or(&serde_json::Value::Null));
                    if let Some(video_format) = self.parse_format_direct(format, url).await? {
                        formats.push(video_format);
                    }
                } else {
                    tracing::debug!("Format {} has no direct URL - requires signature decryption", format.get("itag").unwrap_or(&serde_json::Value::Null));
                }
            }
        }

        if let Some(regular_formats) = streaming_data.get("formats").and_then(|v| v.as_array()) {
            tracing::debug!("Found {} regular formats to check", regular_formats.len());
            for format in regular_formats {
                if let Some(url) = format.get("url").and_then(|v| v.as_str()) {
                    tracing::debug!("Found direct URL regular format: {}", format.get("itag").unwrap_or(&serde_json::Value::Null));
                    if let Some(video_format) = self.parse_format_direct(format, url).await? {
                        formats.push(video_format);
                    }
                } else {
                    tracing::debug!("Regular format {} has no direct URL - requires signature decryption", format.get("itag").unwrap_or(&serde_json::Value::Null));
                }
            }
        }

        if formats.is_empty() {
            anyhow::bail!("No direct URL formats found");
        }

        Ok(formats)
    }

    async fn parse_format_direct(&self, format: &Value, url: &str) -> Result<Option<VideoFormat>> {
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
            url: url.to_string(),
            quality,
            resolution,
            fps,
            vcodec,
            acodec,
            ext: ext.to_string(),
            filesize,
            tbr: bitrate,
            vbr: None,
            abr: None,
        }))
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

        // Fetch the YouTube page with yt-dlp compatible headers
        let video_url = format!("https://www.youtube.com/watch?v={}", video_id);
        let response = self
            .client
            .get(&video_url)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Accept-Encoding", "identity")
            .header("DNT", "1")
            .header("Connection", "keep-alive")
            .header("Upgrade-Insecure-Requests", "1")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1")
            .header("Cache-Control", "max-age=0")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to fetch YouTube page: HTTP {}", response.status());
        }

        let html = response.text().await?;
        
        // Debug: Check if we got valid HTML
        if html.is_empty() {
            anyhow::bail!("Empty response from YouTube");
        }
        
        if !html.contains("html") && !html.contains("HTML") {
            anyhow::bail!("Response doesn't appear to be HTML: {}", &html[..std::cmp::min(200, html.len())]);
        }

        // Try extracting metadata without JS first (some videos don't need signature decryption)
        let player_response = self.extract_player_response(&html)?;
        
        // Check if we can extract formats without signature decryption
        if let Ok(metadata) = self.extract_metadata_direct(&player_response, &video_id).await {
            tracing::info!("Successfully extracted metadata without signature decryption");
            return Ok(metadata);
        }

        // Fallback to JS-based signature decryption
        tracing::debug!("Direct extraction failed, trying JS-based signature decryption");
        let js_content = self.extract_player_js(&html).await?;
        
        // Initialize JavaScript interpreter with player code
        if let Err(e) = self.signature_decrypter.init_js_interpreter(js_content.clone()) {
            tracing::warn!("Failed to initialize JavaScript interpreter: {}", e);
        }
        
        let metadata = self
            .extract_metadata_with_js(&html, &video_id, &js_content)
            .await?;

        Ok(metadata)
    }
}
