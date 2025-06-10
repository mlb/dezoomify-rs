use crate::arguments::Arguments;
use crate::bulk::parsers::iiif_manifest::IiifManifestBulkParser;
use crate::bulk::parsers::simple_text::SimpleTextFileBulkParser;
use crate::bulk::types::{BulkParser, BulkProcessedItem};
use crate::errors::ZoomError;
use crate::network::{client, fetch_uri};
use log::{debug, info, warn};

/// Reads a bulk input source (file path or URL), parses it, and returns a list of items to process.
/// This function accepts both local file paths and URLs.
pub async fn read_bulk_urls(
    source: &str,
    args: &Arguments,
) -> Result<Vec<BulkProcessedItem>, ZoomError> {
    let http_client = client(args.headers(), args, Some(source))?;
    let content_bytes = fetch_uri(source, &http_client).await?;
    read_urls_from_content_with_parsers(&content_bytes, source).await
}

/// Parses content (e.g., from a file or HTTP response) to extract processable items.
///
/// Tries `IiifManifestBulkParser` first. If it fails or returns no items,
/// it falls back to `SimpleTextFileBulkParser`.
///
/// # Arguments
/// * `content_bytes`: The raw byte content (UTF-8 assumed for plain text).
/// * `source_url`: An optional URL from which the content was fetched. This can be used
///   by parsers (e.g., IIIF) to resolve relative URLs within the content. Can also be a file path.
///
/// # Returns
/// A `Result` containing a vector of `BulkProcessedItem`s on success, or a `ZoomError`.
pub async fn read_urls_from_content_with_parsers(
    content_bytes: &[u8],
    source_url: &str,
) -> Result<Vec<BulkProcessedItem>, ZoomError> {
    let content_str = std::str::from_utf8(content_bytes).map_err(|e| ZoomError::Io {
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Bulk content from '{}' is not valid UTF-8: {}",
                source_url, e
            ),
        ),
    })?;

    let parsers: Vec<BulkParser> = vec![
        BulkParser::IiifManifest(IiifManifestBulkParser::new()),
        BulkParser::SimpleText(SimpleTextFileBulkParser::new()),
    ];

    for parser in parsers {
        debug!(
            "Attempting to parse '{}' using {}",
            source_url,
            parser.name()
        );
        match parser.parse(content_str, Some(source_url)).await {
            Ok(items) => {
                if !items.is_empty() {
                    info!(
                        "Successfully parsed '{}' with {}: found {} item(s).",
                        source_url,
                        parser.name(),
                        items.len()
                    );
                    return Ok(items);
                } else {
                    debug!(
                        "Parser {} successfully parsed '{}' but found no items. Trying next parser.",
                        parser.name(),
                        source_url
                    );
                }
            }
            Err(e) => {
                debug!(
                    "Failed to parse '{}' with {}: {}. Trying next parser.",
                    source_url,
                    parser.name(),
                    e
                );
            }
        }
    }

    warn!(
        "No parser could successfully extract items from '{}'.",
        source_url
    );
    Err(ZoomError::NoBulkUrl {
        bulk_file_path: source_url.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_urls_from_content_plain_text() {
        let content = "http://example.com/1\n#comment\nhttp://example.com/2";
        let items = read_urls_from_content_with_parsers(content.as_bytes(), "test.txt")
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].download_url, "http://example.com/1");
        assert_eq!(items[0].default_filename_stem, "1");
        assert_eq!(items[1].download_url, "http://example.com/2");
        assert_eq!(items[1].default_filename_stem, "2");
    }

    #[tokio::test]
    async fn test_read_urls_from_content_iiif_manifest() {
        let manifest_content = r#"{
            "@context": "http://iiif.io/api/presentation/3/context.json",
            "id": "http://example.com/manifest",
            "type": "Manifest",
            "label": {"en": ["Test Manifest"]},
            "items": [
                {
                    "id": "http://example.com/canvas/1",
                    "type": "Canvas",
                    "label": {"en": ["Page 1"]},
                    "height": 100, "width": 100,
                    "items": [
                        {
                            "id": "http://example.com/annoPage/1",
                            "type": "AnnotationPage",
                            "items": [
                                {
                                    "id": "http://example.com/anno/1",
                                    "type": "Annotation",
                                    "motivation": "painting",
                                    "body": {
                                        "id": "http://example.com/image/1/full/full/0/default.jpg",
                                        "type": "Image",
                                        "format": "image/jpeg",
                                        "service": [{
                                            "@id": "http://example.com/image/1",
                                            "type": "ImageService3",
                                            "profile": "level2"
                                        }]
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;
        let items = read_urls_from_content_with_parsers(
            manifest_content.as_bytes(),
            "http://example.com/manifest.json",
        )
        .await
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].download_url,
            "http://example.com/image/1/info.json"
        );
        assert_eq!(items[0].default_filename_stem, "Test_Manifest_Page_1");
        assert_eq!(
            items[0].template_vars.get("manifest_label"),
            Some(&"Test Manifest".to_string())
        );
        assert_eq!(
            items[0].template_vars.get("canvas_label"),
            Some(&"Page 1".to_string())
        );
    }

    #[tokio::test]
    async fn test_read_urls_from_content_fallback_to_plain() {
        let content = "this is not json\nhttp://example.com/fallback_url";
        let items = read_urls_from_content_with_parsers(content.as_bytes(), "test_fallback.txt")
            .await
            .unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].download_url, "this is not json");
        assert_eq!(items[1].download_url, "http://example.com/fallback_url");
    }

    #[tokio::test]
    async fn test_read_urls_from_content_empty_or_no_urls() {
        let content = "# only comments\n\n";
        let result = read_urls_from_content_with_parsers(content.as_bytes(), "empty.txt").await;
        assert!(matches!(result, Err(ZoomError::NoBulkUrl { .. })));

        let invalid_iiif_and_no_urls = r#"{ "not": "a valid manifest structure" }"#;
        let result2 = read_urls_from_content_with_parsers(
            invalid_iiif_and_no_urls.as_bytes(),
            "invalid.json",
        )
        .await;
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().len(), 1);
    }
}
