use crate::dezoomer::*;

/// A dezoomer for text files containing lists of URLs
/// Parses text files where each line is a URL and returns them as ZoomableImageUrl objects
#[derive(Default)]
pub struct BulkTextDezoomer;

impl Dezoomer for BulkTextDezoomer {
    fn name(&self) -> &'static str {
        "bulk_text"
    }

    fn zoom_levels(&mut self, _data: &DezoomerInput) -> Result<ZoomLevels, DezoomerError> {
        // BulkTextDezoomer returns URLs that need further processing, not direct zoom levels
        // This method is only provided for backward compatibility but will always error
        Err(DezoomerError::DownloadError {
            msg: "BulkTextDezoomer produces URLs that need further processing by other dezoomers. Use dezoomer_result() instead.".to_string()
        })
    }

    fn dezoomer_result(&mut self, data: &DezoomerInput) -> Result<DezoomerResult, DezoomerError> {
        let DezoomerInputWithContents { uri: _, contents } = data.with_contents()?;
        
        // Parse the text content to extract URLs
        let content = std::str::from_utf8(contents)
            .map_err(|e| DezoomerError::DownloadError { 
                msg: format!("Failed to parse text file as UTF-8: {}", e) 
            })?;

        let urls = parse_text_urls(content);
        
        if urls.is_empty() {
            return Err(DezoomerError::DownloadError {
                msg: "No valid URLs found in text file".to_string()
            });
        }

        Ok(DezoomerResult::ImageUrls(urls))
    }
}

/// Parse a text file content and extract URLs
/// Each non-empty, non-comment line is treated as a URL
fn parse_text_urls(content: &str) -> Vec<ZoomableImageUrl> {
    let mut urls = Vec::new();
    
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        
        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        
        // Extract a simple title from the URL for better identification
        let title = extract_title_from_url(trimmed, line_num + 1);
        
        urls.push(ZoomableImageUrl {
            url: trimmed.to_string(),
            title,
        });
    }
    
    urls
}

/// Extract a title from a URL for better identification
fn extract_title_from_url(url: &str, line_number: usize) -> Option<String> {
    // Try to extract filename from URL
    if let Ok(parsed_url) = url::Url::parse(url) {
        if let Some(segments) = parsed_url.path_segments() {
            let segments: Vec<&str> = segments.collect();
            if let Some(last_segment) = segments.iter().rev().find(|s| !s.is_empty()) {
                // Remove file extension for a cleaner title
                let title = if let Some(dot_pos) = last_segment.rfind('.') {
                    &last_segment[..dot_pos]
                } else {
                    last_segment
                };
                
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }
    }
    
    // Fallback to line number if we can't extract a good title
    Some(format!("URL_{}", line_number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_content() {
        let urls = parse_text_urls("");
        assert!(urls.is_empty());
    }

    #[test]
    fn test_parse_comments_and_empty_lines() {
        let content = "# This is a comment\n\n   \n# Another comment";
        let urls = parse_text_urls(content);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_parse_valid_urls() {
        let content = "http://example.com/image1.jpg\nhttps://example.org/manifest.json";
        let urls = parse_text_urls(content);
        
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].url, "http://example.com/image1.jpg");
        assert_eq!(urls[0].title, Some("image1".to_string()));
        assert_eq!(urls[1].url, "https://example.org/manifest.json");
        assert_eq!(urls[1].title, Some("manifest".to_string()));
    }

    #[test]
    fn test_parse_mixed_content() {
        let content = "# IIIF manifests\nhttp://example.com/manifest1.json\n\n# Images\nhttps://example.org/info.json\n# End";
        let urls = parse_text_urls(content);
        
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].url, "http://example.com/manifest1.json");
        assert_eq!(urls[0].title, Some("manifest1".to_string()));
        assert_eq!(urls[1].url, "https://example.org/info.json");
        assert_eq!(urls[1].title, Some("info".to_string()));
    }

    #[test]
    fn test_extract_title_from_url() {
        assert_eq!(extract_title_from_url("http://example.com/image.jpg", 1), Some("image".to_string()));
        assert_eq!(extract_title_from_url("https://example.org/path/manifest.json", 2), Some("manifest".to_string()));
        assert_eq!(extract_title_from_url("http://example.com/", 3), Some("URL_3".to_string()));
        assert_eq!(extract_title_from_url("not_a_url", 4), Some("URL_4".to_string()));
    }

    #[test]
    fn test_dezoomer_result() {
        let mut dezoomer = BulkTextDezoomer::default();
        let content = "http://example.com/image1.jpg\nhttps://example.org/manifest.json".as_bytes();
        
        let input = DezoomerInput {
            uri: "file://test.txt".to_string(),
            contents: PageContents::Success(content.to_vec()),
        };
        
        let result = dezoomer.dezoomer_result(&input).unwrap();
        match result {
            DezoomerResult::ImageUrls(urls) => {
                assert_eq!(urls.len(), 2);
                assert_eq!(urls[0].url, "http://example.com/image1.jpg");
                assert_eq!(urls[1].url, "https://example.org/manifest.json");
            }
            _ => panic!("Expected ImageUrls result"),
        }
    }

    #[test]
    fn test_dezoomer_result_empty_file() {
        let mut dezoomer = BulkTextDezoomer::default();
        let content = "# Only comments\n\n# Nothing else".as_bytes();
        
        let input = DezoomerInput {
            uri: "file://empty.txt".to_string(),
            contents: PageContents::Success(content.to_vec()),
        };
        
        let result = dezoomer.dezoomer_result(&input);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No valid URLs found"));
    }
} 