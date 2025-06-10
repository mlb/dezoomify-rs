use crate::bulk_format::{BulkInputParser, BulkProcessedItem};
use std::collections::HashMap;
use std::path::Path;
use url::Url;

/// A parser for simple text files where each non-empty, non-comment line is treated as a URL.
#[derive(Default, Debug)]
pub struct SimpleTextFileBulkParser;

impl SimpleTextFileBulkParser {
    pub fn new() -> Self {
        SimpleTextFileBulkParser
    }
}

impl BulkInputParser for SimpleTextFileBulkParser {
    fn name(&self) -> &str {
        "SimpleTextFileBulkParser"
    }

    async fn parse(
        &self,
        content: &str,
        _source_url: Option<&str>, // Not used by this parser
    ) -> Result<Vec<BulkProcessedItem>, String> {
        let mut items = Vec::new();
        let mut index = 0;

        for line in content.lines() {
            let trimmed_line = line.trim();

            if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
                continue;
            }

            index += 1;
            let url_str = trimmed_line.to_string();

            let mut template_vars = HashMap::new();
            template_vars.insert("index".to_string(), index.to_string());
            template_vars.insert("url".to_string(), url_str.clone());

            let filename_stem_from_url = match Url::parse(&url_str) {
                Ok(parsed_url) => parsed_url
                    .path_segments()
                    .and_then(|segments| segments.last())
                    .filter(|s| !s.is_empty()) // Ensure segment is not empty (e.g. from "http://host.com/")
                    .map(|name| {
                        Path::new(name)
                            .file_stem()
                            .map_or_else(
                                || name.to_string(), // Use full segment if no stem (e.g. ".bashrc", "nodot")
                                |s| s.to_string_lossy().into_owned(),
                            )
                    })
                    .unwrap_or_else(|| format!("image_{}", index)), // Fallback if no path segment or empty
                Err(_) => format!("image_{}", index), // Fallback if URL parsing fails
            };

            template_vars.insert(
                "filename_from_url".to_string(),
                filename_stem_from_url.clone(),
            );

            items.push(BulkProcessedItem {
                download_url: url_str,
                template_vars,
                default_filename_stem: filename_stem_from_url,
            });
        }

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_empty_content() {
        let parser = SimpleTextFileBulkParser::new();
        let content = "";
        let result = parser.parse(content, None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_parse_comments_and_empty_lines() {
        let parser = SimpleTextFileBulkParser::new();
        let content = "# This is a comment\n\n   \n# Another comment";
        let result = parser.parse(content, None).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_parse_valid_urls() {
        let parser = SimpleTextFileBulkParser::new();
        let content = "http://example.com/image1.jpg\nhttps://example.org/data/archive.zip";
        let result = parser.parse(content, None).await.unwrap();

        assert_eq!(result.len(), 2);

        // Item 1
        assert_eq!(result[0].download_url, "http://example.com/image1.jpg");
        assert_eq!(result[0].default_filename_stem, "image1");
        assert_eq!(
            result[0].template_vars.get("index"),
            Some(&"1".to_string())
        );
        assert_eq!(
            result[0].template_vars.get("url"),
            Some(&"http://example.com/image1.jpg".to_string())
        );
        assert_eq!(
            result[0].template_vars.get("filename_from_url"),
            Some(&"image1".to_string())
        );

        // Item 2
        assert_eq!(
            result[1].download_url,
            "https://example.org/data/archive.zip"
        );
        assert_eq!(result[1].default_filename_stem, "archive");
        assert_eq!(
            result[1].template_vars.get("index"),
            Some(&"2".to_string())
        );
        assert_eq!(
            result[1].template_vars.get("url"),
            Some(&"https://example.org/data/archive.zip".to_string())
        );
        assert_eq!(
            result[1].template_vars.get("filename_from_url"),
            Some(&"archive".to_string())
        );
    }

    #[tokio::test]
    async fn test_parse_urls_with_tricky_filenames() {
        let parser = SimpleTextFileBulkParser::new();
        let content = concat!(
            "http://example.com/image_no_extension\n", // No extension
            "http://example.com/archive.tar.gz\n",      // Double extension
            "http://example.com/.hiddenfile\n",         // Hidden file
            "http://example.com/path/\n",               // Trailing slash
            "http://example.com/\n",                    // Host only with slash
            "not_a_valid_url_at_all\n",                 // Invalid URL
            "http://example.com/with space.jpg"       // URL with space (though technically needs encoding)
        );
        let result = parser.parse(content, None).await.unwrap();

        assert_eq!(result.len(), 7);

        assert_eq!(result[0].default_filename_stem, "image_no_extension");
        assert_eq!(result[0].template_vars["filename_from_url"], "image_no_extension");

        assert_eq!(result[1].default_filename_stem, "archive.tar");
        assert_eq!(result[1].template_vars["filename_from_url"], "archive.tar");
        
        assert_eq!(result[2].default_filename_stem, ".hiddenfile");
        assert_eq!(result[2].template_vars["filename_from_url"], ".hiddenfile");

        // Url: "http://example.com/path/"
        // path_segments().last() -> Some("path")
        // Path::new("path").file_stem() -> Some("path")
        assert_eq!(result[3].default_filename_stem, "path");
        assert_eq!(result[3].template_vars["filename_from_url"], "path");

        // Url: "http://example.com/"
        // path_segments().last() -> None (or Some("") which is filtered)
        assert_eq!(result[4].default_filename_stem, "image_5"); // Fallback
        assert_eq!(result[4].template_vars["filename_from_url"], "image_5");

        // Url: "not_a_valid_url_at_all"
        assert_eq!(result[5].default_filename_stem, "image_6"); // Fallback
        assert_eq!(result[5].template_vars["filename_from_url"], "image_6");

        // Url: "http://example.com/with space.jpg"
        assert_eq!(result[6].default_filename_stem, "with space");
        assert_eq!(result[6].template_vars["filename_from_url"], "with space");
    }

     #[tokio::test]
    async fn test_url_with_query_and_fragment() {
        let parser = SimpleTextFileBulkParser::new();
        let content = "http://example.com/image.jpg?query=123#fragment";
        let result = parser.parse(content, None).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].download_url, "http://example.com/image.jpg?query=123#fragment");
        assert_eq!(result[0].default_filename_stem, "image");
        assert_eq!(result[0].template_vars["filename_from_url"], "image");
    }
}