use crate::bulk::types::{BulkInputParser, BulkProcessedItem};
use std::collections::HashMap;
use std::path::Path;
use url::Url;

fn simple_percent_decode(input: &str) -> String {
    input
        .replace("%20", " ")
        .replace("%21", "!")
        .replace("%22", "\"")
        .replace("%23", "#")
        .replace("%24", "$")
        .replace("%25", "%")
        .replace("%26", "&")
        .replace("%27", "'")
        .replace("%28", "(")
        .replace("%29", ")")
        .replace("%2A", "*")
        .replace("%2B", "+")
        .replace("%2C", ",")
        .replace("%2D", "-")
        .replace("%2E", ".")
        .replace("%2F", "/")
}

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
        _source_url: Option<&str>,
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
                Ok(parsed_url) => {
                    if let Some(segments) = parsed_url.path_segments() {
                        let segments: Vec<&str> = segments.collect();
                        let last_non_empty = segments.iter().rev().find(|s| !s.is_empty());

                        if let Some(name) = last_non_empty {
                            let decoded_name = simple_percent_decode(name);
                            Path::new(&decoded_name).file_stem().map_or_else(
                                || decoded_name.to_string(),
                                |s| s.to_string_lossy().into_owned(),
                            )
                        } else {
                            format!("image_{}", index)
                        }
                    } else {
                        format!("image_{}", index)
                    }
                }
                Err(_) => format!("image_{}", index),
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

        assert_eq!(result[0].download_url, "http://example.com/image1.jpg");
        assert_eq!(result[0].default_filename_stem, "image1");
        assert_eq!(result[0].template_vars.get("index"), Some(&"1".to_string()));
        assert_eq!(
            result[0].template_vars.get("url"),
            Some(&"http://example.com/image1.jpg".to_string())
        );
        assert_eq!(
            result[0].template_vars.get("filename_from_url"),
            Some(&"image1".to_string())
        );

        assert_eq!(
            result[1].download_url,
            "https://example.org/data/archive.zip"
        );
        assert_eq!(result[1].default_filename_stem, "archive");
        assert_eq!(result[1].template_vars.get("index"), Some(&"2".to_string()));
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
            "http://example.com/image_no_extension\n",
            "http://example.com/archive.tar.gz\n",
            "http://example.com/.hiddenfile\n",
            "http://example.com/path/\n",
            "http://example.com/\n",
            "not_a_valid_url_at_all\n",
            "http://example.com/with space.jpg"
        );
        let result = parser.parse(content, None).await.unwrap();

        assert_eq!(result.len(), 7);

        assert_eq!(result[0].default_filename_stem, "image_no_extension");
        assert_eq!(
            result[0].template_vars["filename_from_url"],
            "image_no_extension"
        );

        assert_eq!(result[1].default_filename_stem, "archive.tar");
        assert_eq!(result[1].template_vars["filename_from_url"], "archive.tar");

        assert_eq!(result[2].default_filename_stem, ".hiddenfile");
        assert_eq!(result[2].template_vars["filename_from_url"], ".hiddenfile");

        assert_eq!(result[3].default_filename_stem, "path");
        assert_eq!(result[3].template_vars["filename_from_url"], "path");

        assert_eq!(result[4].default_filename_stem, "image_5");
        assert_eq!(result[4].template_vars["filename_from_url"], "image_5");

        assert_eq!(result[5].default_filename_stem, "image_6");
        assert_eq!(result[5].template_vars["filename_from_url"], "image_6");

        assert_eq!(result[6].default_filename_stem, "with space");
        assert_eq!(result[6].template_vars["filename_from_url"], "with space");
    }

    #[tokio::test]
    async fn test_url_with_query_and_fragment() {
        let parser = SimpleTextFileBulkParser::new();
        let content = "http://example.com/file.pdf?param=value#section";
        let result = parser.parse(content, None).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].default_filename_stem, "file");
        assert_eq!(result[0].template_vars["filename_from_url"], "file");
    }
}
