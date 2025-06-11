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
    test_image(
        "testdata/zoomify/test_custom_size/ImageProperties.xml",
        "testdata/zoomify/test_custom_size/expected_result.jpg",
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
pub async fn local_generic_tiles() {
    test_image(
        "testdata/generic/map_{{X}}_{{Y}}.jpg",
        "testdata/generic/map_expected.png",
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
    // Create a temporary directory for the test
    let temp_dir = TempDir::new("dezoomify-rs-bulk-test").unwrap();

    // Create a bulk URLs file
    let bulk_file_path = temp_dir.path().join("urls.txt");
    let mut bulk_file = File::create(&bulk_file_path).unwrap();
    writeln!(bulk_file, "# Bulk test URLs").unwrap();
    writeln!(
        bulk_file,
        "testdata/zoomify/test_custom_size/ImageProperties.xml"
    )
    .unwrap();
    writeln!(bulk_file, "testdata/generic/map_{{{{X}}}}_{{{{Y}}}}.jpg").unwrap();
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
    assert_eq!(stats.total_images, 2, "Should process exactly 2 images from bulk file");
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

    // Create a temporary directory for the test
    let temp_dir = TempDir::new("dezoomify-rs-cli-bulk-test").unwrap();

    // Create a bulk URLs file
    let bulk_file_path = temp_dir.path().join("test_urls.txt");
    let mut bulk_file = File::create(&bulk_file_path).unwrap();
    writeln!(bulk_file, "# Test URLs for bulk CLI processing").unwrap();
    writeln!(
        bulk_file,
        "testdata/zoomify/test_custom_size/ImageProperties.xml"
    )
    .unwrap();
    writeln!(bulk_file).unwrap(); // Empty line should be ignored
    writeln!(bulk_file, "testdata/generic/map_{{{{X}}}}_{{{{Y}}}}.jpg").unwrap();
    writeln!(bulk_file, "# Another comment").unwrap();
    drop(bulk_file);

    // Set up output file path in temp directory
    let output_file = temp_dir.path().join("cli_bulk_test.jpg");

    // Save current directory and change to the project root for the test
    let _original_dir = env::current_dir().unwrap(); // Keep reference for safety

    // Create CLI arguments as they would come from command line
    // Note: input_uri and outfile are positional arguments, so they come after flags
    let args = vec![
        "dezoomify-rs".to_string(),
        "--bulk".to_string(),
        bulk_file_path.to_string_lossy().to_string(),
        "--logging".to_string(),
        "error".to_string(),
        "--retries".to_string(),
        "0".to_string(),
        // When using --bulk, we don't provide input_uri as positional arg
        // but we do provide outfile as positional arg
        output_file.to_string_lossy().to_string(),
    ];

    // Test CLI argument parsing
    let mut parsed_args =
        Arguments::try_parse_from(args.clone()).expect("CLI parsing should succeed");

    // Verify the arguments were parsed correctly
    assert!(parsed_args.is_bulk_mode(), "Should be in bulk mode");
    assert_eq!(
        parsed_args.bulk.as_ref().expect("bulk should be Some"),
        &bulk_file_path.to_string_lossy().to_string(),
        "Bulk file path should match"
    );

    // In bulk mode, the positional argument gets parsed as input_uri instead of outfile
    // We need to move it to the correct place for our test
    let actual_output_file = if parsed_args.outfile.is_none() && parsed_args.input_uri.is_some() {
        let output_from_input = PathBuf::from(parsed_args.input_uri.take().unwrap());
        parsed_args.outfile = Some(output_from_input.clone());
        output_from_input
    } else {
        parsed_args
            .outfile
            .as_ref()
            .expect("outfile should be Some")
            .clone()
    };

    assert_eq!(actual_output_file, output_file, "Output file should match");
    assert_eq!(
        parsed_args.logging, "error",
        "Logging level should be error"
    );
    assert_eq!(parsed_args.retries, 0, "Retries should be 0");

    // Test the complete bulk processing flow using the new unified architecture
    let stats = process_bulk(&parsed_args).await.expect("Bulk processing should succeed");
    
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
