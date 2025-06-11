use std::collections::hash_map::DefaultHasher;
use std::default::Default;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;
use image::{self, DynamicImage, GenericImageView};
use image_hasher::HasherConfig;
use tempdir::TempDir;

use dezoomify_rs::{Arguments, ZoomError, dezoomify, process_bulk};

/// Dezoom a file locally
#[tokio::test(flavor = "multi_thread")]
pub async fn custom_size_local_zoomify_tiles() {
    // Get absolute path to avoid working directory issues
    let workspace_root = get_workspace_root();
    let input_path = workspace_root.join("testdata/zoomify/test_custom_size/ImageProperties.xml");
    let expected_path =
        workspace_root.join("testdata/zoomify/test_custom_size/expected_result.jpg");

    test_image(
        input_path.to_str().unwrap(),
        expected_path.to_str().unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
pub async fn local_generic_tiles() {
    // Get absolute path to avoid working directory issues
    let workspace_root = get_workspace_root();
    let input_path = workspace_root.join("testdata/generic/map_{{X}}_{{Y}}.jpg");
    let expected_path = workspace_root.join("testdata/generic/map_expected.png");

    test_image(
        input_path.to_str().unwrap(),
        expected_path.to_str().unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
pub async fn bulk_mode_local_tiles() {
    test_bulk_processing().await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
pub async fn bulk_mode_end_to_end_cli() {
    test_bulk_mode_cli_end_to_end().await.unwrap()
}

/// Get the workspace root directory (where Cargo.toml is located)
fn get_workspace_root() -> PathBuf {
    let mut current_dir = std::env::current_dir().expect("Failed to get current directory");

    // Walk up the directory tree until we find Cargo.toml
    loop {
        if current_dir.join("Cargo.toml").exists() {
            return current_dir;
        }
        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            break;
        }
    }

    // If we can't find Cargo.toml by walking up, try using the CARGO_MANIFEST_DIR environment variable
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest_path = PathBuf::from(manifest_dir);
        if manifest_path.join("Cargo.toml").exists() {
            return manifest_path;
        }
    }

    // Last resort: try relative path from where tests typically run
    let test_workspace = PathBuf::from("../");
    if test_workspace.join("Cargo.toml").exists() {
        return test_workspace
            .canonicalize()
            .expect("Failed to canonicalize path");
    }

    // If all else fails, return current directory and let tests fail with better error message
    std::env::current_dir().expect("Failed to get current directory")
}

#[allow(clippy::needless_lifetimes)]
#[allow(clippy::field_reassign_with_default)]
pub async fn dezoom_image<'a>(input: &str, expected: &'a str) -> Result<TmpFile<'a>, ZoomError> {
    let mut args: Arguments = Default::default();
    args.input_uri = Some(input.into());
    args.largest = true;
    args.retries = 0;
    args.logging = "error".into();

    let tmp_file = TmpFile(expected);
    args.outfile = Some(tmp_file.to_path_buf());
    dezoomify(&args).await.expect("Dezooming failed");
    Ok(tmp_file)
}

// Unused in benchmarks
#[allow(dead_code)]
pub async fn test_image(input: &str, expected: &str) -> Result<(), ZoomError> {
    let tmp_file = dezoom_image(input, expected).await?;
    let tmp_path = tmp_file.to_path_buf();
    let actual = match image::open(&tmp_path) {
        Ok(actual) => actual,
        Err(e) => {
            std::fs::copy(&tmp_path, "err.png")?;
            eprintln!(
                "Unable to open the dezoomified image {:?}; copied it to err.png",
                tmp_path.display()
            );
            return Err(e.into());
        }
    };
    let expected = image::open(expected)?;
    assert_images_equal(actual, expected);
    Ok(())
}

fn assert_images_equal(a: DynamicImage, b: DynamicImage) {
    assert_eq!(
        a.dimensions(),
        b.dimensions(),
        "image dimensions should match"
    );
    let hasher = HasherConfig::new().to_hasher();
    let dist = hasher.hash_image(&a).dist(&hasher.hash_image(&b));
    assert!(dist < 3, "The distance between the two images is {}", dist);
}

pub struct TmpFile<'a>(&'a str);

impl<'a> TmpFile<'a> {
    fn to_path_buf(&'a self) -> PathBuf {
        let mut out_file = std::env::temp_dir();
        out_file.push(format!("dezoomify-out-{}", hash(self.0)));
        let orig_path: &Path = self.0.as_ref();
        out_file.set_extension(orig_path.extension().expect("missing extension"));
        out_file
    }
}

impl<'a> Drop for TmpFile<'a> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.to_path_buf());
    }
}

fn hash<T: Hash>(v: T) -> u64 {
    let mut s = DefaultHasher::new();
    v.hash(&mut s);
    s.finish()
}

#[allow(dead_code)]
async fn test_bulk_processing() -> Result<(), ZoomError> {
    // Get workspace root to use absolute paths
    let workspace_root = get_workspace_root();

    // Create a temporary directory for the test
    let temp_dir = TempDir::new("dezoomify-rs-bulk-test").unwrap();

    // Create a bulk URLs file with absolute paths
    let bulk_file_path = temp_dir.path().join("urls.txt");
    let mut bulk_file = File::create(&bulk_file_path).unwrap();
    writeln!(bulk_file, "# Bulk test URLs").unwrap();

    // Use absolute paths to testdata
    let zoomify_path = workspace_root.join("testdata/zoomify/test_custom_size/ImageProperties.xml");
    let generic_path = workspace_root.join("testdata/generic/map_{{X}}_{{Y}}.jpg");

    writeln!(bulk_file, "{}", zoomify_path.to_string_lossy()).unwrap();
    writeln!(bulk_file, "{}", generic_path.to_string_lossy()).unwrap();
    writeln!(bulk_file).unwrap(); // Empty line
    writeln!(bulk_file, "# Comment line should be ignored").unwrap();
    drop(bulk_file);

    // Setup arguments for bulk processing
    let mut args: Arguments = Default::default();
    args.bulk = Some(bulk_file_path.to_string_lossy().to_string());
    args.largest = true; // This should be implied anyway in bulk mode
    args.retries = 0;
    args.logging = "error".into();

    // Set output file with a base name
    let output_base = temp_dir.path().join("bulk_test.jpg");
    args.outfile = Some(output_base.clone());

    // Execute bulk processing using the new unified architecture
    let stats = process_bulk(&args).await?;

    // Verify statistics
    assert_eq!(
        stats.total_images, 2,
        "Should process exactly 2 images from bulk file"
    );
    assert!(
        stats.successful_images + stats.partial_downloads > 0,
        "At least some images should succeed"
    );

    let successful_count = stats.successful_images + stats.partial_downloads;

    assert_eq!(
        successful_count, 2,
        "Both URLs should be processed successfully"
    );

    // Verify the output files have the expected naming pattern
    let expected_file1 = output_base.parent().unwrap().join("bulk_test_1.jpg");
    let expected_file2 = output_base.parent().unwrap().join("bulk_test_2.jpg");

    assert!(
        expected_file1.exists(),
        "First output file should exist with correct name"
    );
    assert!(
        expected_file2.exists(),
        "Second output file should exist with correct name"
    );

    // Verify files are different (different content)
    let file1_metadata = std::fs::metadata(&expected_file1).unwrap();
    let file2_metadata = std::fs::metadata(&expected_file2).unwrap();

    // Files should have some reasonable size
    assert!(
        file1_metadata.len() > 1000,
        "First file should be reasonably sized"
    );
    assert!(
        file2_metadata.len() > 1000,
        "Second file should be reasonably sized"
    );

    Ok(())
}

#[allow(dead_code)]
async fn test_bulk_mode_cli_end_to_end() -> Result<(), ZoomError> {
    use std::env;

    // Get workspace root to use absolute paths
    let workspace_root = get_workspace_root();

    // Create a temporary directory for the test
    let temp_dir = TempDir::new("dezoomify-rs-cli-bulk-test").unwrap();

    // Create a bulk URLs file with absolute paths
    let bulk_file_path = temp_dir.path().join("test_urls.txt");
    let mut bulk_file = File::create(&bulk_file_path).unwrap();
    writeln!(bulk_file, "# Test URLs for bulk CLI processing").unwrap();

    // Use absolute paths to testdata
    let zoomify_path = workspace_root.join("testdata/zoomify/test_custom_size/ImageProperties.xml");
    let generic_path = workspace_root.join("testdata/generic/map_{{X}}_{{Y}}.jpg");

    writeln!(bulk_file, "{}", zoomify_path.to_string_lossy()).unwrap();
    writeln!(bulk_file).unwrap(); // Empty line should be ignored
    writeln!(bulk_file, "{}", generic_path.to_string_lossy()).unwrap();
    writeln!(bulk_file, "# Another comment").unwrap();
    drop(bulk_file);

    // Set up output file path in temp directory
    let output_file = temp_dir.path().join("cli_bulk_test.jpg");

    // Save current directory and change to the project root for the test
    let _original_dir = env::current_dir().unwrap(); // Keep reference for safety

    // Create CLI arguments as they would come from command line
    // Note: When using --bulk, outfile should not be provided as positional argument
    let args = vec![
        "dezoomify-rs".to_string(),
        "--bulk".to_string(),
        bulk_file_path.to_string_lossy().to_string(),
        "--logging".to_string(),
        "error".to_string(),
        "--retries".to_string(),
        "0".to_string(),
    ];

    // Parse arguments using clap
    let mut parsed_args = Arguments::try_parse_from(args).expect("CLI parsing should succeed");

    // Set the outfile after parsing (in bulk mode, this is typically how it's done)
    parsed_args.outfile = Some(output_file.clone());

    // Verify the arguments were parsed correctly
    assert!(parsed_args.bulk.is_some());
    assert_eq!(
        parsed_args.bulk.as_ref().unwrap(),
        &bulk_file_path.to_string_lossy().to_string()
    );
    assert!(parsed_args.outfile.is_some());

    // Test the complete bulk processing flow using the new unified architecture
    let stats = process_bulk(&parsed_args)
        .await
        .expect("Bulk processing should succeed");

    // Verify statistics
    assert_eq!(stats.total_images, 2, "Should process exactly 2 images");
    let successful_count = stats.successful_images + stats.partial_downloads;
    assert_eq!(
        successful_count, stats.total_images,
        "All images should be processed successfully"
    );

    // Verify the expected output files exist with correct naming
    let expected_file1 = temp_dir.path().join("cli_bulk_test_1.jpg");
    let expected_file2 = temp_dir.path().join("cli_bulk_test_2.jpg");

    assert!(
        expected_file1.exists(),
        "First bulk output file should exist: {:?}",
        expected_file1
    );
    assert!(
        expected_file2.exists(),
        "Second bulk output file should exist: {:?}",
        expected_file2
    );

    // Verify files have reasonable sizes
    let file1_size = std::fs::metadata(&expected_file1).unwrap().len();
    let file2_size = std::fs::metadata(&expected_file2).unwrap().len();

    assert!(
        file1_size > 1000,
        "First file should be reasonably sized (got {} bytes)",
        file1_size
    );
    assert!(
        file2_size > 1000,
        "Second file should be reasonably sized (got {} bytes)",
        file2_size
    );

    // Test edge case: verify that level-specifying arguments work correctly in bulk mode
    let args_with_max_width = vec![
        "dezoomify-rs".to_string(),
        "--bulk".to_string(),
        bulk_file_path.to_string_lossy().to_string(),
        "--max-width".to_string(),
        "1000".to_string(),
        "--logging".to_string(),
        "error".to_string(),
    ];

    let parsed_with_constraint =
        Arguments::try_parse_from(args_with_max_width).expect("CLI parsing should succeed");
    assert!(
        parsed_with_constraint.has_level_specifying_args(),
        "Should have level-specifying args"
    );
    assert!(
        !parsed_with_constraint.should_use_largest(),
        "Should not use largest when max-width is specified"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_bulk_mode_uses_image_titles_for_iiif_manifest() {
    // Get workspace root to use absolute paths
    let workspace_root = get_workspace_root();

    // Create a temporary directory for the test
    let temp_dir = TempDir::new("dezoomify-rs-bulk-title-test").unwrap();

    // Create a bulk URLs file with a simple test manifest
    let bulk_file_path = temp_dir.path().join("urls.txt");
    let mut bulk_file = File::create(&bulk_file_path).unwrap();

    // Use absolute path to testdata
    let zoomify_path = workspace_root.join("testdata/zoomify/test_custom_size/ImageProperties.xml");
    writeln!(bulk_file, "{}", zoomify_path.to_string_lossy()).unwrap();
    drop(bulk_file);

    // Setup arguments for bulk processing WITHOUT specifying outfile
    let mut args: Arguments = Default::default();
    let bulk_path_string = bulk_file_path.to_string_lossy().to_string();
    args.bulk = Some(bulk_path_string);
    args.largest = true;
    args.retries = 0;
    args.logging = "error".into();

    // Set the working directory to the test temp dir for output
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&temp_dir).unwrap();

    // Run bulk processing
    let result = process_bulk(&args).await;

    // Restore original directory
    std::env::set_current_dir(&original_dir).unwrap();

    let stats = result.unwrap();
    assert_eq!(stats.total_images, 1);
    assert_eq!(stats.successful_images, 1);

    // Check that the generated file uses the image title
    let entries: Vec<_> = std::fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "jpg") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(entries.len(), 1, "Expected exactly one output file");
    let output_file = &entries[0];
    let file_name_os = output_file.file_name().unwrap();
    let filename = file_name_os.to_string_lossy();

    // The filename should be based on the title (test_custom_size) from the Zoomify dezoomer
    // NOT "dezoomified"
    assert!(
        filename.starts_with("test_custom_size"),
        "Expected filename to start with 'test_custom_size', got: {}",
        filename
    );
    assert!(
        !filename.starts_with("dezoomified"),
        "Filename should not start with 'dezoomified', got: {}",
        filename
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_bulk_mode_with_outfile_specified_still_uses_titles_in_naming() {
    // Get workspace root to use absolute paths
    let workspace_root = get_workspace_root();

    // Test that even when outfile is specified, the title logic is preserved
    let temp_dir = TempDir::new("dezoomify-rs-bulk-outfile-test").unwrap();

    let bulk_file_path = temp_dir.path().join("urls.txt");
    let mut bulk_file = File::create(&bulk_file_path).unwrap();

    // Use absolute path to testdata
    let zoomify_path = workspace_root.join("testdata/zoomify/test_custom_size/ImageProperties.xml");
    writeln!(bulk_file, "{}", zoomify_path.to_string_lossy()).unwrap();
    drop(bulk_file);

    let mut args: Arguments = Default::default();
    let bulk_path_string = bulk_file_path.to_string_lossy().to_string();
    args.bulk = Some(bulk_path_string);
    args.largest = true;
    args.retries = 0;
    args.logging = "error".into();

    // Set a custom outfile - this should result in index-based naming but still use the title
    args.outfile = Some(temp_dir.path().join("my_collection.jpg"));

    let stats = process_bulk(&args).await.unwrap();
    assert_eq!(stats.total_images, 1);
    assert_eq!(stats.successful_images, 1);

    // When outfile is specified, it should use indexed naming with the base outfile name
    let entries: Vec<_> = std::fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "jpg") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(entries.len(), 1, "Expected exactly one output file");
    let output_file = &entries[0];
    let file_name_os = output_file.file_name().unwrap();
    let filename = file_name_os.to_string_lossy();

    // With outfile specified, should use indexed naming
    assert!(
        filename.starts_with("my_collection_1"),
        "Expected filename to start with 'my_collection_1', got: {}",
        filename
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_google_arts_and_culture_dezoomer_basic() {
    use dezoomify_rs::dezoomer::{Dezoomer, DezoomerError, DezoomerInput, PageContents};
    use dezoomify_rs::google_arts_and_culture::GAPDezoomer;
    use std::fs;

    let workspace_root = get_workspace_root();
    let test_html_path = workspace_root.join("testdata/google_arts_and_culture/page_source.html");
    let test_xml_path = workspace_root.join("testdata/google_arts_and_culture/tile_info.xml");

    // Check that test files exist
    assert!(test_html_path.exists(), "Test HTML file should exist");
    assert!(test_xml_path.exists(), "Test XML file should exist");

    let mut dezoomer = GAPDezoomer::default();

    // Test 1: Parse Google Arts page
    let page_html = fs::read(&test_html_path).unwrap();
    let input1 = DezoomerInput {
        uri: "https://artsandculture.google.com/asset/test".to_string(),
        contents: PageContents::Success(page_html),
    };

    let result1 = dezoomer.zoom_levels(&input1);
    let tile_info_uri = match result1 {
        Err(DezoomerError::NeedsData { uri }) => {
            assert!(uri.ends_with("=g"));
            uri
        }
        other => panic!("Expected NeedsData, got: {:?}", other),
    };

    // Test 2: Parse tile info XML
    let tile_info_xml = fs::read(&test_xml_path).unwrap();
    let input2 = DezoomerInput {
        uri: tile_info_uri,
        contents: PageContents::Success(tile_info_xml),
    };

    let result2 = dezoomer.zoom_levels(&input2);
    match result2 {
        Ok(levels) => {
            assert!(!levels.is_empty(), "Should have at least one zoom level");
            println!("Successfully parsed {} zoom levels", levels.len());
        }
        Err(e) => panic!("Failed to parse tile info: {:?}", e),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_google_arts_and_culture_url_validation() {
    use dezoomify_rs::dezoomer::{Dezoomer, DezoomerError, DezoomerInput, PageContents};
    use dezoomify_rs::google_arts_and_culture::GAPDezoomer;

    let mut dezoomer = GAPDezoomer::default();

    // Test 1: Valid Google Arts & Culture URL should be accepted
    let valid_input = DezoomerInput {
        uri: "https://artsandculture.google.com/asset/test".to_string(),
        contents: PageContents::Success(b"invalid html".to_vec()),
    };

    let result = dezoomer.zoom_levels(&valid_input);
    // Should not be rejected as wrong dezoomer
    assert!(!matches!(result, Err(DezoomerError::WrongDezoomer { .. })));

    // Test 2: Invalid URL should be rejected
    let mut dezoomer2 = GAPDezoomer::default();
    let invalid_input = DezoomerInput {
        uri: "https://example.com/test".to_string(),
        contents: PageContents::Success(vec![]),
    };

    let result = dezoomer2.zoom_levels(&invalid_input);
    assert!(matches!(result, Err(DezoomerError::WrongDezoomer { .. })));
}
