use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

// Assuming these are correctly declared in lib.rs or main.rs
use crate::arguments::Arguments;
use crate::bulk_format::{BulkParser, BulkProcessedItem};
use crate::errors::ZoomError;
use crate::iiif_bulk_parser::IiifManifestBulkParser;
use crate::simple_text_parser::SimpleTextFileBulkParser;

// Placeholder for the actual single item processing function from your crate
// This would be your `dezoomify::dezoomify` or similar.
async fn process_single_item_args(_args: Arguments) -> Result<PathBuf, ZoomError> {
    // This is a stand-in. In a real scenario, this would call the core dezooming logic.
    // For now, let's pretend it always succeeds and returns the outfile path from args.
    // To make tests pass without the actual dezoomify function, we'll simulate success.
    // If _args.outfile is None, it means create_single_url_args wasn't called correctly or test setup is flawed.
    // Ok(_args.outfile.expect("outfile should be set for single item processing in mock"))
    Err(ZoomError::Io {
        source: std::io::Error::other("process_single_item_args mock called"),
    })
}

/// Reads a bulk input file, parses it, and returns a list of items to process.
/// This is a synchronous wrapper around the asynchronous `read_urls_from_content_with_parsers`.
pub async fn read_bulk_urls(path: &Path) -> Result<Vec<BulkProcessedItem>, ZoomError> {
    let mut file = File::open(path).map_err(|source| ZoomError::Io { source })?;
    let mut content_bytes = Vec::new();
    file.read_to_end(&mut content_bytes)
        .map_err(|source| ZoomError::Io { source })?;

    let source_description = path.to_string_lossy().into_owned();

    read_urls_from_content_with_parsers(&content_bytes, &source_description).await
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
    source_url: &str, // Can be a file path or URL
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

/// Renders a simple template string by replacing {key} with values from the map.
///
/// # Arguments
/// * `template_str`: The template string, e.g., "{manifest_label}_{page_number}".
/// * `vars`: A map of variable names to their string values.
///
/// # Returns
/// The rendered string or an error if a key is not found (currently returns template with missing keys).
fn render_template(template_str: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template_str.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{}}}", key);
        result = result.replace(&placeholder, value);
    }
    // Check for any unreplaced placeholders, could warn or error here if strictness is needed.
    // For now, allow unreplaced placeholders to remain.
    result
}

/// Generates the output file path for a single bulk item.
///
/// The generated path does not include an extension; it's expected that
/// the dezooming process will add an appropriate extension based on image content.
///
/// # Arguments
/// * `output_directory`: The base directory for output files.
/// * `output_template`: Optional user-defined filename template (relative to `output_directory`).
/// * `item`: The `BulkProcessedItem` containing `template_vars` and `default_filename_stem`.
/// * `item_index_0_based`: The 0-based index of the current item in the bulk list.
/// * `total_items`: Total number of items in the bulk list, for index padding.
///
/// # Returns
/// A `PathBuf` for the output file (stem only, no extension).
fn generate_output_path_for_item(
    output_directory: &Path,
    output_template: Option<&str>,
    item: &BulkProcessedItem,
    item_index_0_based: usize,
    total_items: usize,
) -> PathBuf {
    let filename_index_1_based = item_index_0_based + 1;
    let num_digits_in_total = if total_items == 0 {
        1
    } else {
        (total_items as f64).log10().floor() as usize + 1
    };
    let padding_width = num_digits_in_total.max(4); // Ensure at least 4 digits for index

    let padded_index = format!("{:0width$}", filename_index_1_based, width = padding_width);

    let filename_stem_str: String = match output_template {
        Some(template_str) => {
            let mut effective_vars = item.template_vars.clone();
            effective_vars.insert("index".to_string(), padded_index.clone());
            effective_vars.insert("item_index".to_string(), item_index_0_based.to_string());
            effective_vars.insert("item_index_1".to_string(), padded_index.clone());
            effective_vars.insert(
                "page_number".to_string(),
                filename_index_1_based.to_string(),
            ); // Common alias
            effective_vars.insert("total_items".to_string(), total_items.to_string());
            effective_vars.insert(
                "default_stem".to_string(),
                item.default_filename_stem.clone(),
            );

            let rendered = render_template(template_str, &effective_vars);
            if rendered.is_empty()
                || (rendered == template_str
                    && template_str.contains('{')
                    && !vars_can_render_template(template_str, &effective_vars))
            {
                // Fallback if template rendering fails to change anything meaningful (and it was a template) or is empty
                warn!(
                    "Template rendering for '{}' resulted in an empty or effectively unchanged string using available variables. Falling back to default naming with index: {} and default stem: {}",
                    template_str, padded_index, item.default_filename_stem
                );
                format!("{}_{}", item.default_filename_stem, padded_index)
            } else {
                // The rendered template might include path separators.
                // It's treated as a relative path from output_directory.
                rendered
            }
        }
        None => {
            // No template, use default stem + padded index
            if item.default_filename_stem.trim().is_empty() {
                format!("item_{}", padded_index)
            } else {
                format!("{}_{}", item.default_filename_stem, padded_index)
            }
        }
    };

    // Helper to check if any variable in the template string exists in the provided vars map
    fn vars_can_render_template(template_str: &str, vars: &HashMap<String, String>) -> bool {
        // A simple check: iterate through the string, find patterns like {key}, and check if key exists in vars
        let mut i = 0;
        while let Some(start) = template_str[i..].find('{') {
            if let Some(end) = template_str[i + start..].find('}') {
                let key = &template_str[i + start + 1..i + start + end];
                if vars.contains_key(key) {
                    return true; // Found at least one replaceable key
                }
                i += start + end + 1;
            } else {
                break; // No matching '}'
            }
        }
        false // No replaceable keys found
    }

    // Ensure the generated stem is not empty, which can happen if default_filename_stem is empty
    // and no template is used, or template renders empty.
    let final_filename_stem_str = if filename_stem_str.trim().is_empty() {
        format!("item_{}", padded_index)
    } else {
        filename_stem_str
    };

    // The `final_filename_stem_str` can be a simple stem "image_0001" or a relative path like "subdir/image_0001"
    // if the template included slashes.
    output_directory.join(final_filename_stem_str)
}

/// Creates `Arguments` for processing a single URL in bulk mode.
fn create_single_url_args(
    base_args: &Arguments,
    item: &BulkProcessedItem,
    item_index: usize,
    total_items: usize,
    bulk_output_directory: &Path,
) -> Arguments {
    let mut single_args = base_args.clone();
    single_args.input_uri = Some(item.download_url.clone());
    single_args.bulk = None; // Disable bulk mode for the individual processing

    if base_args.should_use_largest() {
        // Assuming should_use_largest() exists on Arguments
        single_args.largest = true;
    }

    // Since output_template doesn't exist in Arguments, we'll generate the outfile directly
    single_args.outfile = Some(generate_output_path_for_item(
        bulk_output_directory,
        None, // No template support for now since Arguments doesn't have this field
        item,
        item_index,
        total_items,
    ));

    single_args
}

/// Handles the result of processing a single URL and updates counters.
fn handle_single_url_result(
    result: Result<PathBuf, ZoomError>,
    url_desc: &str, // Could be the download_url or a more descriptive name
    index: usize,
    total_urls: usize,
    successful_count: &mut usize,
    error_count: &mut usize,
) {
    match result {
        Ok(saved_as) => {
            info!(
                "[{}/{}] Image from '{}' successfully saved to '{}'",
                index + 1,
                total_urls,
                url_desc,
                saved_as.to_string_lossy()
            );
            *successful_count += 1;
        }
        Err(err @ ZoomError::PartialDownload { .. }) => {
            warn!(
                "[{}/{}] Partial download for '{}': {}",
                index + 1,
                total_urls,
                url_desc,
                err
            );
            *successful_count += 1; // Partial downloads might still be considered a success for counting
        }
        Err(err) => {
            error!(
                "[{}/{}] ERROR processing '{}': {}",
                index + 1,
                total_urls,
                url_desc,
                err
            );
            *error_count += 1;
        }
    }
}

/// Prints the final bulk processing summary.
fn print_bulk_summary(successful_count: usize, error_count: usize, total_urls: usize) {
    info!("\nBulk processing completed:");
    info!("  Successfully processed: {}", successful_count);
    info!("  Errors: {}", error_count);
    info!("  Total items attempted: {}", total_urls);
}

/// Creates an error result for bulk processing if there were errors.
fn create_bulk_error_result(error_count: usize) -> Result<(), ZoomError> {
    if error_count > 0 {
        Err(ZoomError::Image {
            source: image::ImageError::from(std::io::Error::other(format!(
                "Bulk processing completed with {} error(s).",
                error_count
            ))),
        })
    } else {
        Ok(())
    }
}

/// Main function to process a list of URLs in bulk.
pub async fn process_bulk(args: &Arguments) -> Result<(), ZoomError> {
    let bulk_file_path = args.bulk.as_ref().ok_or_else(|| ZoomError::Image {
        source: image::ImageError::from(std::io::Error::other(
            "Bulk file path (--bulk) is required for bulk processing.",
        )),
    })?;

    info!(
        "Starting bulk processing from file: '{}'",
        bulk_file_path.to_string_lossy()
    );

    let items_to_process = read_bulk_urls(bulk_file_path).await?;

    if items_to_process.is_empty() {
        info!("No items found to process in the bulk file.");
        return Ok(());
    }

    let total_items = items_to_process.len();
    info!("Found {} item(s) to process.", total_items);

    let bulk_output_directory = PathBuf::from("."); // Default to current directory since Arguments doesn't have output_directory

    if !bulk_output_directory.exists() {
        std::fs::create_dir_all(&bulk_output_directory)
            .map_err(|source| ZoomError::Io { source })?;
        info!(
            "Created output directory: '{}'",
            bulk_output_directory.to_string_lossy()
        );
    } else if !bulk_output_directory.is_dir() {
        return Err(ZoomError::Image {
            source: image::ImageError::from(std::io::Error::other(format!(
                "Specified bulk output path '{}' exists but is not a directory.",
                bulk_output_directory.to_string_lossy()
            ))),
        });
    }

    let mut successful_count = 0;
    let mut error_count = 0;

    for (index, item) in items_to_process.iter().enumerate() {
        info!(
            "Processing item {}/{} (URL: {})",
            index + 1,
            total_items,
            item.download_url
        );

        let single_args =
            create_single_url_args(args, item, index, total_items, &bulk_output_directory);

        // This is where the actual download and processing for the single item happens.
        let result = process_single_item_args(single_args).await;

        handle_single_url_result(
            result,
            &item.download_url,
            index,
            total_items,
            &mut successful_count,
            &mut error_count,
        );
    }

    print_bulk_summary(successful_count, error_count, total_items);
    create_bulk_error_result(error_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bulk_format::BulkProcessedItem; // For constructing test items

    // Mock Arguments for testing
    // Ensure this mock aligns with the actual Arguments struct definition
    fn mock_base_args() -> Arguments {
        let mut args = Arguments::default();
        args.retries = 0; // Override for testing
        args
    }

    #[test]
    fn test_render_template_simple() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "world".to_string());
        vars.insert("num".to_string(), "123".to_string());
        let template = "Hello, {name}! Count: {num}.";
        assert_eq!(
            render_template(template, &vars),
            "Hello, world! Count: 123."
        );
    }

    #[test]
    fn test_render_template_missing_key() {
        let vars = HashMap::new(); // Empty vars
        let template = "Key: {missing_key}";
        assert_eq!(render_template(template, &vars), "Key: {missing_key}"); // Stays as is
    }

    #[test]
    fn test_generate_output_path_no_template() {
        let dir = PathBuf::from("output");
        let item = BulkProcessedItem {
            download_url: "http://example.com/image.jpg".to_string(),
            template_vars: HashMap::new(),
            default_filename_stem: "default_stem".to_string(),
        };
        let path = generate_output_path_for_item(&dir, None, &item, 0, 10);
        assert_eq!(path, dir.join("default_stem_0001"));

        let path_high_index = generate_output_path_for_item(&dir, None, &item, 9, 10);
        assert_eq!(path_high_index, dir.join("default_stem_0010"));

        let path_high_total = generate_output_path_for_item(&dir, None, &item, 0, 10000);
        assert_eq!(path_high_total, dir.join("default_stem_00001")); // 5 digits padding
    }

    #[test]
    fn test_generate_output_path_with_template() {
        let dir = PathBuf::from("custom_output");
        let mut vars = HashMap::new();
        vars.insert("id".to_string(), "item123".to_string());
        vars.insert("label".to_string(), "My Label".to_string());

        let item = BulkProcessedItem {
            download_url: "url".to_string(),
            template_vars: vars,
            default_filename_stem: "fallback".to_string(),
        };

        // Template uses item vars and auto index
        let template1 = "{label}_{id}_{index}";
        let path1 = generate_output_path_for_item(&dir, Some(template1), &item, 0, 1);
        assert_eq!(path1, dir.join("My Label_item123_0001"));

        // Template uses default_stem
        let template2 = "{default_stem}_extra_{item_index_1}";
        let path2 = generate_output_path_for_item(&dir, Some(template2), &item, 2, 5);
        assert_eq!(path2, dir.join("fallback_extra_0003"));

        // Template with path separators
        let template3 = "subdir/{id}/{index}";
        let path3 = generate_output_path_for_item(&dir, Some(template3), &item, 0, 1);
        assert_eq!(path3, dir.join("subdir/item123/0001"));
    }

    #[test]
    fn test_generate_output_path_empty_template_render_fallback() {
        let dir = PathBuf::from("output");
        let item = BulkProcessedItem {
            download_url: "url".to_string(),
            template_vars: HashMap::new(), // No vars to fill template
            default_filename_stem: "default_fallback".to_string(),
        };
        // Template that will render to itself because {unknown_var} is not in vars
        let template = "{unknown_var}";
        let path = generate_output_path_for_item(&dir, Some(template), &item, 0, 1);
        // Fallback because vars_can_render_template returns false for "{unknown_var}" with empty vars
        assert_eq!(path, dir.join("default_fallback_0001"));
    }

    #[test]
    fn test_generate_output_path_empty_default_stem_no_template() {
        let dir = PathBuf::from("output");
        let item = BulkProcessedItem {
            download_url: "url".to_string(),
            template_vars: HashMap::new(),
            default_filename_stem: "".to_string(), // Empty default stem
        };
        let path = generate_output_path_for_item(&dir, None, &item, 0, 1);
        assert_eq!(path, dir.join("item_0001")); // Fallback to "item_{index}"
    }

    #[tokio::test]
    async fn test_read_urls_from_content_plain_text() {
        let content = "http://example.com/1\n#comment\nhttp://example.com/2";
        let items = read_urls_from_content_with_parsers(content.as_bytes(), "test.txt")
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].download_url, "http://example.com/1");
        assert_eq!(items[0].default_filename_stem, "1"); // From SimpleTextFileBulkParser logic
        assert_eq!(items[1].download_url, "http://example.com/2");
        assert_eq!(items[1].default_filename_stem, "2");
    }

    #[tokio::test]
    async fn test_read_urls_from_content_iiif_manifest() {
        // A minimal valid IIIF manifest JSON structure that IiifManifestBulkParser would parse
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
        ); // As per IiifManifestBulkParser
        assert_eq!(items[0].default_filename_stem, "Test_Manifest_page_1");
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
        // Invalid JSON, should fallback to plain text if lines look like URLs
        let content = "this is not json\nhttp://example.com/fallback_url";
        let items = read_urls_from_content_with_parsers(content.as_bytes(), "test_fallback.txt")
            .await
            .unwrap();

        // SimpleTextFileBulkParser will try to parse each line.
        // "this is not json" will be parsed as a URL by it.
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
        // The SimpleTextFileBulkParser will treat this as a URL, so it should succeed with 1 item
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().len(), 1);
    }

    #[test]
    fn test_create_single_url_args_usage() {
        let base_args = mock_base_args();
        let test_output_dir_name = "./test_output_dir_bulk_create_args"; // Test specific dir
        let output_dir = PathBuf::from(test_output_dir_name);

        // Clean up before test if dir exists from previous failed run
        if output_dir.exists() {
            std::fs::remove_dir_all(&output_dir).expect("Failed to clean up test dir before test");
        }
        std::fs::create_dir_all(&output_dir).expect("Failed to create test dir");

        let mut item_vars = HashMap::new();
        item_vars.insert("id".to_string(), "foo".to_string());

        let item = BulkProcessedItem {
            download_url: "http://test.com/img.png".to_string(),
            template_vars: item_vars,
            default_filename_stem: "my_item".to_string(),
        };

        let new_args = create_single_url_args(
            &base_args,
            &item,
            0, // index
            1, // total_items
            &output_dir,
        );

        assert_eq!(
            new_args.input_uri,
            Some("http://test.com/img.png".to_string())
        );
        assert!(new_args.bulk.is_none());
        assert_eq!(new_args.outfile, Some(output_dir.join("my_item_0001")));

        // Clean up test directory
        std::fs::remove_dir_all(&output_dir).expect("Failed to clean up test dir after test");
    }
}
