use crate::arguments::Arguments;
use crate::bulk::content_reader::read_bulk_urls;
use crate::bulk::output_path::generate_output_path_for_item;
use crate::bulk::types::BulkProcessedItem;
use crate::dezoomify;
use crate::errors::ZoomError;
use log::{error, info, warn};
use std::path::{Path, PathBuf};

async fn process_single_item_args(args: Arguments) -> Result<PathBuf, ZoomError> {
    dezoomify(&args).await
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
    single_args.bulk = None;

    if base_args.should_use_largest() {
        single_args.largest = true;
    }

    single_args.outfile = Some(generate_output_path_for_item(
        bulk_output_directory,
        None,
        item,
        item_index,
        total_items,
    ));

    single_args
}

/// Handles the result of processing a single URL and updates counters.
fn handle_single_url_result(
    result: Result<PathBuf, ZoomError>,
    url_desc: &str,
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
            *successful_count += 1;
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
    let bulk_source = args.bulk.as_ref().ok_or_else(|| ZoomError::Image {
        source: image::ImageError::from(std::io::Error::other(
            "Bulk source (--bulk) is required for bulk processing.",
        )),
    })?;

    info!("Starting bulk processing from source: '{}'", bulk_source);

    let items_to_process = read_bulk_urls(bulk_source, args).await?;

    if items_to_process.is_empty() {
        info!("No items found to process in the bulk file.");
        return Ok(());
    }

    let total_items = items_to_process.len();
    info!("Found {} item(s) to process.", total_items);

    let bulk_output_directory = PathBuf::from(".");

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
    use crate::bulk::types::BulkProcessedItem;
    use std::collections::HashMap;

    fn mock_base_args() -> Arguments {
        let mut args = Arguments::default();
        args.retries = 0;
        args
    }

    #[test]
    fn test_create_single_url_args_usage() {
        let base_args = mock_base_args();
        let test_output_dir_name = "./test_output_dir_bulk_create_args";
        let output_dir = PathBuf::from(test_output_dir_name);

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

        let new_args = create_single_url_args(&base_args, &item, 0, 1, &output_dir);

        assert_eq!(
            new_args.input_uri,
            Some("http://test.com/img.png".to_string())
        );
        assert!(new_args.bulk.is_none());
        assert_eq!(new_args.outfile, Some(output_dir.join("my_item_0001")));

        std::fs::remove_dir_all(&output_dir).expect("Failed to clean up test dir after test");
    }
}
