use std::collections::HashMap;

/// Represents a single item to be processed in a bulk operation.
/// This struct is generic and not tied to any specific input format (like IIIF or plain text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkProcessedItem {
    /// The direct URL to download.
    pub download_url: String,
    /// A map of variables that can be used for filename templating.
    /// Keys are variable names (e.g., "manifest_label", "page_number", "filename_from_url").
    /// Values are the corresponding string values.
    pub template_vars: HashMap<String, String>,
    /// A default filename stem (without extension) to be used if no output template is provided
    /// or if template rendering fails.
    pub default_filename_stem: String,
}

/// A trait for parsers that can interpret different bulk input formats
/// (e.g., IIIF Manifests, plain text URL lists) and convert them into
/// a list of `BulkProcessedItem`s.
#[allow(async_fn_in_trait)]
pub trait BulkInputParser: Send + Sync {
    /// Parses the given content string into a list of `BulkProcessedItem`s.
    ///
    /// # Arguments
    /// * `content`: The string content to parse (e.g., content of a file or HTTP response).
    /// * `source_url`: An optional URL from which the content was fetched. This can be used
    ///   by parsers (e.g., IIIF) to resolve relative URLs within the content.
    ///
    /// # Returns
    /// A `Result` containing either a vector of `BulkProcessedItem`s on success,
    /// or a `String` error message on failure.
    async fn parse(&self, content: &str, source_url: Option<&str>) -> Result<Vec<BulkProcessedItem>, String>;

    /// A human-readable name for the parser, used for logging or debugging.
    fn name(&self) -> &str;
}

/// An enum that holds concrete parser types to work around the async trait object limitation
#[derive(Debug)]
pub enum BulkParser {
    IiifManifest(crate::iiif_bulk_parser::IiifManifestBulkParser),
    SimpleText(crate::simple_text_parser::SimpleTextFileBulkParser),
}

impl BulkParser {
    pub fn name(&self) -> &str {
        match self {
            BulkParser::IiifManifest(parser) => parser.name(),
            BulkParser::SimpleText(parser) => parser.name(),
        }
    }

    pub async fn parse(&self, content: &str, source_url: Option<&str>) -> Result<Vec<BulkProcessedItem>, String> {
        match self {
            BulkParser::IiifManifest(parser) => parser.parse(content, source_url).await,
            BulkParser::SimpleText(parser) => parser.parse(content, source_url).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bulk_processed_item_creation() {
        let mut vars = HashMap::new();
        vars.insert("key1".to_string(), "value1".to_string());
        vars.insert("key2".to_string(), "value2".to_string());

        let item = BulkProcessedItem {
            download_url: "http://example.com/image.jpg".to_string(),
            template_vars: vars.clone(),
            default_filename_stem: "image_default".to_string(),
        };

        assert_eq!(item.download_url, "http://example.com/image.jpg");
        assert_eq!(item.template_vars.get("key1"), Some(&"value1".to_string()));
        assert_eq!(item.default_filename_stem, "image_default");
    }
}