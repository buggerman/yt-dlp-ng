use std::path::PathBuf;

pub fn sanitize_filename(filename: &str) -> String {
    // Remove or replace characters that are invalid in filenames
    filename
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            '/' | '\\' => '-',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

pub fn generate_output_filename(template: &str, metadata: &crate::core::VideoMetadata) -> PathBuf {
    // Get the best format for determining extension
    let best_format = metadata
        .formats
        .iter()
        .filter(|f| f.vcodec.is_some() && f.acodec.is_some())
        .max_by_key(|f| f.tbr.unwrap_or(0.0) as i32)
        .or_else(|| metadata.formats.first());

    let ext = best_format.map(|f| f.ext.as_str()).unwrap_or("mp4");

    // Simple template replacement
    let filename = template
        .replace("%(title)s", &sanitize_filename(&metadata.title))
        .replace("%(id)s", &metadata.id)
        .replace(
            "%(uploader)s",
            &metadata.uploader.as_deref().unwrap_or("Unknown"),
        )
        .replace("%(ext)s", ext);

    PathBuf::from(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello/world"), "hello-world");
        assert_eq!(sanitize_filename("test<>file"), "test__file");
        assert_eq!(sanitize_filename("normal_file.mp4"), "normal_file.mp4");
    }
}
