use clap::Parser;
use anyhow::Result;
use std::path::PathBuf;
use crate::core::{ExtractorEngine, Downloader};
use crate::extractors::YouTubeExtractor;
use crate::utils::generate_output_filename;

#[derive(Parser)]
#[command(name = "yt-dlp-ng")]
#[command(about = "Modern video downloader with enhanced performance")]
#[command(version)]
pub struct Cli {
    /// URL to download
    #[arg(value_name = "URL")]
    pub url: String,
    
    /// Output directory
    #[arg(short, long, default_value = ".")]
    pub output: String,
    
    /// Output filename template
    #[arg(short = 't', long)]
    pub output_template: Option<String>,
    
    /// Video format to download
    #[arg(short, long, default_value = "best")]
    pub format: String,
    
    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
    
    /// Number of concurrent downloads
    #[arg(short = 'j', long, default_value = "1")]
    pub concurrent: usize,
}

impl Cli {
    pub async fn run(&self) -> Result<()> {
        if self.verbose {
            println!("Verbose mode enabled");
        }
        
        println!("Downloading: {}", self.url);
        println!("Output directory: {}", self.output);
        println!("Format: {}", self.format);
        
        // Initialize extractor engine
        let mut extractor_engine = ExtractorEngine::new();
        extractor_engine.register_extractor(Box::new(YouTubeExtractor::new()));
        
        // Extract video metadata
        println!("Extracting video information...");
        let metadata = extractor_engine.extract(&self.url).await?;
        
        println!("Title: {}", metadata.title);
        if let Some(uploader) = &metadata.uploader {
            println!("Uploader: {}", uploader);
        }
        if let Some(duration) = metadata.duration {
            println!("Duration: {}s", duration);
        }
        if let Some(view_count) = metadata.view_count {
            println!("Views: {}", view_count);
        }
        
        println!("Available formats: {}", metadata.formats.len());
        for (i, format) in metadata.formats.iter().enumerate().take(5) {
            println!("  {}: {} - {} ({})", 
                i + 1, 
                format.format_id, 
                format.resolution.as_deref().unwrap_or("unknown"),
                format.ext
            );
        }
        
        // Generate output filename
        let template = self.output_template
            .as_deref()
            .unwrap_or("%(title)s.%(ext)s");
        let filename = generate_output_filename(template, &metadata);
        let output_path = PathBuf::from(&self.output).join(filename);
        
        println!("Output file: {}", output_path.display());
        
        // Initialize downloader
        let downloader = Downloader::new(self.concurrent);
        
        // Download the video
        println!("Starting download...");
        downloader.download(&metadata, output_path).await?;
        
        println!("Download completed!");
        
        Ok(())
    }
}