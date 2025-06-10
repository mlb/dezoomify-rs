use crate::{Arguments, ZoomError, dezoomify};
use log::{error, info, warn};
use std::path::{Path, PathBuf};

/// Reads URLs from the bulk file specified in arguments.
pub fn read_bulk_urls(bulk_file_path: &Path) -> Result<Vec<String>, ZoomError> {
    let content = std::fs::read_to_string(bulk_file_path)?;
    let urls: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect();
    if urls.is_empty() {
        let bulk_file_path = bulk_file_path.to_string_lossy().to_string();
        return Err(ZoomError::NoBulkUrl { bulk_file_path });
    }
    Ok(urls)
}

/// Determines the bulk output file path from arguments
fn resolve_bulk_output_file(args: &Arguments) -> Option<PathBuf> {
    if args.outfile.is_some() {
        args.outfile.clone()
    } else if args.input_uri.is_some() {
        // The output file was parsed as input_uri, convert it to PathBuf
        Some(PathBuf::from(args.input_uri.as_ref().unwrap()))
    } else {
        None
    }
}

/// Creates arguments for processing a single URL in bulk mode
fn create_single_url_args(
    base_args: &Arguments,
    url: &str,
    index: usize,
    bulk_output_file: &Option<PathBuf>,
) -> Arguments {
    let mut single_args = base_args.clone();
    single_args.input_uri = Some(url.to_string());
    single_args.bulk = None; // Disable bulk mode for the individual processing

    // In bulk mode, if no level-specifying arguments are provided, imply --largest
    // This logic is now handled by args.should_use_largest() in Arguments.
    // We ensure `largest` is set based on `should_use_largest`.
    if base_args.should_use_largest() {
        single_args.largest = true;
    }

    // Handle output file naming for bulk mode
    if let Some(outfile) = bulk_output_file {
        single_args.outfile = Some(generate_bulk_output_name(outfile, index));
    }

    single_args
}

/// Handles the result of processing a single URL and updates counters
fn handle_single_url_result(
    result: Result<PathBuf, ZoomError>,
    index: usize,
    total_urls: usize,
    successful_count: &mut usize,
    error_count: &mut usize,
) {
    match result {
        Ok(saved_as) => {
            info!(
                "[{}/{}] Image successfully saved to '{}'",
                index + 1,
                total_urls,
                saved_as.to_string_lossy()
            );
            *successful_count += 1;
        }
        Err(err @ ZoomError::PartialDownload { .. }) => {
            warn!("[{}/{}] Partial download: {}", index + 1, total_urls, err);
            *successful_count += 1; // Partial downloads are still considered successful
        }
        Err(err) => {
            error!("[{}/{}] ERROR: {}", index + 1, total_urls, err);
            *error_count += 1;
        }
    }
}

/// Prints the final bulk processing summary
fn print_bulk_summary(successful_count: usize, error_count: usize, total_urls: usize) {
    info!("\nBulk processing completed:");
    info!("  Successful: {}", successful_count);
    info!("  Errors: {}", error_count);
    info!("  Total: {}", total_urls);
}

/// Creates an error result for bulk processing if there were errors
fn create_bulk_error_result(error_count: usize) -> Result<(), ZoomError> {
    if error_count > 0 {
        Err(ZoomError::Io {
            source: std::io::Error::other(format!(
                "Bulk processing completed with {} errors",
                error_count
            )),
        })
    } else {
        Ok(())
    }
}

/// Generates a unique output file name for bulk processing.
/// `base_outfile` is the user-provided output (or a fallback).
/// `index` is the 0-based index of the current URL being processed.
pub fn generate_bulk_output_name(base_outfile: &Path, index: usize) -> PathBuf {
    let stem = base_outfile.file_stem().unwrap_or_default();
    let extension = base_outfile.extension().unwrap_or_default();
    let parent = base_outfile.parent().unwrap_or_else(|| Path::new("."));

    let mut new_name = std::ffi::OsString::from(stem);
    new_name.push(format!("_{:04}", index + 1)); // Use 1-based indexing for filenames
    if !extension.is_empty() {
        new_name.push(".");
        new_name.push(extension);
    }

    parent.join(new_name)
}

/// Processes a list of URLs in bulk mode.
pub async fn process_bulk(args: &Arguments) -> Result<(), ZoomError> {
    // args.bulk is Some, otherwise this function wouldn't be called.
    let bulk_file_path = args.bulk.as_ref().ok_or_else(|| ZoomError::Io {
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Bulk file path not provided for bulk processing.",
        ),
    })?;
    let urls = read_bulk_urls(bulk_file_path)?;
    let total_urls = urls.len();

    info!("Processing {total_urls} URLs in bulk mode");

    let mut successful_count = 0;
    let mut error_count = 0;
    let bulk_output_file = resolve_bulk_output_file(args);

    for (index, url) in urls.iter().enumerate() {
        info!("\n[{}/{}] Processing: {}", index + 1, total_urls, url);

        let single_args = create_single_url_args(args, url, index, &bulk_output_file);
        let result = dezoomify(&single_args).await;

        handle_single_url_result(
            result,
            index,
            total_urls,
            &mut successful_count,
            &mut error_count,
        );
    }

    print_bulk_summary(successful_count, error_count, total_urls);
    create_bulk_error_result(error_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arguments, ZoomError};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use tempdir::TempDir;

    // Helper function to create a temporary file with specified content
    fn create_temp_file_with_content(temp_dir: &Path, file_name: &str, content: &str) -> PathBuf {
        let file_path = temp_dir.join(file_name);
        let mut temp_file = std::fs::File::create(&file_path)
            .unwrap_or_else(|e| panic!("Failed to create temp file '{:?}': {}", file_path, e));
        write!(temp_file, "{}", content)
            .unwrap_or_else(|e| panic!("Failed to write to temp file '{:?}': {}", file_path, e));
        file_path
    }

    #[test]
    fn test_read_bulk_urls_success() {
        let temp_dir = TempDir::new("test_read_success").expect("Failed to create temp dir");
        let content = "https://example.com/image1\n  https://example.com/image2  \n# This is a comment\n\nhttps://example.com/image3";
        let bulk_file_path = create_temp_file_with_content(temp_dir.path(), "urls.txt", content);

        let urls = read_bulk_urls(&bulk_file_path).unwrap();
        assert_eq!(urls.len(), 3);
        assert_eq!(urls[0], "https://example.com/image1");
        assert_eq!(urls[1], "https://example.com/image2");
        assert_eq!(urls[2], "https://example.com/image3");
    }

    #[test]
    fn test_read_bulk_urls_empty_file() {
        let temp_dir = TempDir::new("test_read_empty").expect("Failed to create temp dir");
        let empty_file_path = create_temp_file_with_content(temp_dir.path(), "empty_urls.txt", "");

        let result = read_bulk_urls(&empty_file_path);
        let Err(ZoomError::NoBulkUrl { bulk_file_path }) = result else {
            panic!(
                "Expected NoBulkUrl error for empty bulk file, got {:?}",
                result
            )
        };
        assert!(
            bulk_file_path.ends_with("empty_urls.txt"),
            "Incorrect bulk file path in error: {}",
            bulk_file_path
        );
    }

    #[test]
    fn test_read_bulk_urls_only_comments_and_empty_lines() {
        let temp_dir = TempDir::new("test_read_comments").expect("Failed to create temp dir");
        let content = "# Comment 1\n\n # Comment 2\n";
        let comments_file_path =
            create_temp_file_with_content(temp_dir.path(), "comments_urls.txt", content);

        let result = read_bulk_urls(&comments_file_path);
        let Err(ZoomError::NoBulkUrl { bulk_file_path }) = result else {
            panic!(
                "Expected NoBulkUrl error for comments-only bulk file, got {:?}",
                result
            )
        };
        assert!(
            bulk_file_path.ends_with("comments_urls.txt"),
            "Incorrect bulk file path in error: {}",
            bulk_file_path
        );
    }

    #[test]
    fn test_read_bulk_urls_non_existent_file() {
        let temp_dir = TempDir::new("test_read_non_existent").expect("Failed to create temp dir");
        let non_existent_path = temp_dir.path().join("non_existent.txt");

        let result = read_bulk_urls(&non_existent_path);
        assert!(result.is_err());
        if let Err(ZoomError::Io { source }) = result {
            assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        } else {
            panic!(
                "Expected IoError with NotFound kind for non-existent bulk file, got {:?}",
                result
            );
        }
    }

    #[test]
    fn test_generate_bulk_output_name() {
        let base = Path::new("output.jpg");
        assert_eq!(
            generate_bulk_output_name(base, 0),
            PathBuf::from("output_0001.jpg")
        );
        assert_eq!(
            generate_bulk_output_name(base, 42),
            PathBuf::from("output_0043.jpg")
        );

        let base_with_path = Path::new("/tmp/images/photo.png");
        assert_eq!(
            generate_bulk_output_name(base_with_path, 9),
            PathBuf::from("/tmp/images/photo_0010.png")
        );

        let base_no_extension = Path::new("output");
        assert_eq!(
            generate_bulk_output_name(base_no_extension, 0),
            PathBuf::from("output_0001")
        );

        let base_with_dot_in_stem = Path::new("archive.tar.gz");
        assert_eq!(
            generate_bulk_output_name(base_with_dot_in_stem, 0),
            PathBuf::from("archive.tar_0001.gz")
        );

        let base_hidden_file = Path::new(".config");
        assert_eq!(
            generate_bulk_output_name(base_hidden_file, 0),
            PathBuf::from(".config_0001")
        );
    }

    #[test]
    fn test_resolve_bulk_output_file() {
        // Test with outfile set
        let mut args = Arguments::default();
        args.outfile = Some(std::path::PathBuf::from("output.jpg"));
        assert_eq!(
            resolve_bulk_output_file(&args),
            Some(std::path::PathBuf::from("output.jpg"))
        );

        // Test with input_uri set (fallback case)
        args.outfile = None;
        args.input_uri = Some("fallback.png".to_string());
        assert_eq!(
            resolve_bulk_output_file(&args),
            Some(std::path::PathBuf::from("fallback.png"))
        );

        // Test with neither set
        args.input_uri = None;
        assert_eq!(resolve_bulk_output_file(&args), None);
    }

    #[test]
    fn test_create_single_url_args() {
        let mut base_args = Arguments::default();
        base_args.bulk = Some(std::path::PathBuf::from("urls.txt"));
        base_args.parallelism = 8;
        assert!(base_args.should_use_largest()); // True for bulk mode if no level args

        let bulk_output_file_opt = Some(std::path::PathBuf::from("output.jpg"));
        let single_args = create_single_url_args(
            &base_args,
            "https://example.com/image",
            2, // 0-indexed, so output should be _0003
            &bulk_output_file_opt,
        );

        assert_eq!(
            single_args.input_uri,
            Some("https://example.com/image".to_string())
        );
        assert_eq!(single_args.bulk, None);
        assert_eq!(single_args.parallelism, 8);
        assert!(single_args.largest);
        assert_eq!(
            single_args.outfile,
            Some(std::path::PathBuf::from("output_0003.jpg"))
        );

        let mut base_args_with_level = base_args.clone();
        base_args_with_level.zoom_level = Some(1);
        assert!(!base_args_with_level.should_use_largest()); // False due to zoom_level

        let single_args_with_level = create_single_url_args(
            &base_args_with_level,
            "https://example.com/image2",
            0,
            &None,
        );
        assert!(!single_args_with_level.largest);
        assert_eq!(single_args_with_level.zoom_level, Some(1));
    }

    #[test]
    fn test_create_bulk_error_result() {
        assert!(create_bulk_error_result(0).is_ok());
        let err_result = create_bulk_error_result(1);
        assert!(err_result.is_err());
        if let Err(ZoomError::Io { source }) = err_result {
            assert_eq!(
                source.to_string(),
                "Bulk processing completed with 1 errors"
            );
        } else {
            panic!("Expected IoError for bulk error result");
        }

        let err_result_5 = create_bulk_error_result(5);
        assert!(err_result_5.is_err());
        if let Err(ZoomError::Io { source }) = err_result_5 {
            assert_eq!(
                source.to_string(),
                "Bulk processing completed with 5 errors"
            );
        } else {
            panic!("Expected IoError for bulk error result");
        }
    }

    #[test]
    fn test_handle_single_url_result() {
        let mut successful_count = 0;
        let mut error_count = 0;

        // Test successful result
        let ok_result = Ok(std::path::PathBuf::from("output.jpg"));
        handle_single_url_result(ok_result, 0, 5, &mut successful_count, &mut error_count);
        assert_eq!(successful_count, 1);
        assert_eq!(error_count, 0);

        // Test partial download (counts as success for the counter)
        let partial_result = Err(ZoomError::PartialDownload {
            successful_tiles: 90,
            total_tiles: 100,
            destination: "output.jpg".to_string(),
        });
        handle_single_url_result(
            partial_result,
            1,
            5,
            &mut successful_count,
            &mut error_count,
        );
        assert_eq!(successful_count, 2);
        assert_eq!(error_count, 0);

        // Test error result
        let error_result = Err(ZoomError::NoLevels);
        handle_single_url_result(error_result, 2, 5, &mut successful_count, &mut error_count);
        assert_eq!(successful_count, 2);
        assert_eq!(error_count, 1);
    }
}
