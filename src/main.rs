use colour::{green_ln, red_ln, yellow_ln};
use human_panic::setup_panic;

use dezoomify_rs::{Arguments, ZoomError, dezoomify};

#[tokio::main]
async fn main() {
    setup_panic!();
    let has_args = std::env::args_os().count() > 1;
    let mut has_errors = false;
    let args: Arguments = clap::Parser::parse();
    init_log(&args);

    if args.is_bulk_mode() {
        // Bulk processing mode
        match process_bulk(&args).await {
            Ok(_) => {}
            Err(err) => {
                red_ln!("BULK ERROR {}", err);
                has_errors = true;
            }
        }
    } else {
        // Single processing mode (existing behavior)
        loop {
            match dezoomify(&args).await {
                Ok(saved_as) => {
                    green_ln!(
                        "Image successfully saved to '{}' (current working directory: {})",
                        saved_as.to_string_lossy(),
                        std::env::current_dir()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_e| "unknown".into())
                    );
                }
                Err(ZoomError::Io { source })
                    if source.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    // If we have reached the end of stdin, we exit
                    yellow_ln!("Reached end of input. Exiting...");
                    break;
                }
                Err(err @ ZoomError::PartialDownload { .. }) => {
                    yellow_ln!("{}", err);
                    has_errors = true;
                }
                Err(err) => {
                    red_ln!("ERROR {}", err);
                    has_errors = true;
                }
            }
            if has_args {
                // Command-line invocation
                break;
            }
        }
    }
    if has_errors {
        std::process::exit(1);
    }
}

async fn process_bulk(args: &Arguments) -> Result<(), ZoomError> {
    let urls = args.read_bulk_urls()?;
    let total_urls = urls.len();

    println!("Processing {} URLs in bulk mode", total_urls);

    let mut successful_count = 0;
    let mut error_count = 0;

    // In bulk mode, the output file might be in the input_uri field due to positional argument parsing
    let bulk_output_file = if args.outfile.is_some() {
        args.outfile.clone()
    } else if args.input_uri.is_some() {
        // The output file was parsed as input_uri, convert it to PathBuf
        Some(std::path::PathBuf::from(args.input_uri.as_ref().unwrap()))
    } else {
        None
    };

    for (index, url) in urls.iter().enumerate() {
        println!("\n[{}/{}] Processing: {}", index + 1, total_urls, url);

        // Create a modified args for this specific URL
        let mut single_args = args.clone();
        single_args.input_uri = Some(url.clone());
        single_args.bulk = None; // Disable bulk mode for the individual processing

        // In bulk mode, if no level-specifying arguments are provided, imply --largest
        if args.is_bulk_mode() && !args.has_level_specifying_args() {
            single_args.largest = true;
        }

        // Handle output file naming for bulk mode
        if let Some(ref outfile) = bulk_output_file {
            single_args.outfile = Some(generate_bulk_output_name(outfile, index));
        }

        match dezoomify(&single_args).await {
            Ok(saved_as) => {
                green_ln!(
                    "[{}/{}] Image successfully saved to '{}'",
                    index + 1,
                    total_urls,
                    saved_as.to_string_lossy()
                );
                successful_count += 1;
            }
            Err(err @ ZoomError::PartialDownload { .. }) => {
                yellow_ln!("[{}/{}] Partial download: {}", index + 1, total_urls, err);
                successful_count += 1; // Partial downloads are still considered successful
            }
            Err(err) => {
                red_ln!("[{}/{}] ERROR: {}", index + 1, total_urls, err);
                error_count += 1;
            }
        }
    }

    println!("\nBulk processing completed:");
    println!("  Successful: {}", successful_count);
    println!("  Errors: {}", error_count);
    println!("  Total: {}", total_urls);

    if error_count > 0 {
        return Err(ZoomError::Io {
            source: std::io::Error::other(format!(
                "Bulk processing completed with {} errors",
                error_count
            )),
        });
    }

    Ok(())
}

fn generate_bulk_output_name(base_outfile: &std::path::Path, index: usize) -> std::path::PathBuf {
    let stem = base_outfile.file_stem().unwrap_or_default();
    let extension = base_outfile.extension().unwrap_or_default();
    let parent = base_outfile
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let mut new_name = std::ffi::OsString::from(stem);
    new_name.push(format!("_{:04}", index + 1));
    if !extension.is_empty() {
        new_name.push(".");
        new_name.push(extension);
    }

    parent.join(new_name)
}

fn init_log(args: &Arguments) {
    let env = env_logger::Env::new().default_filter_or(&args.logging);
    env_logger::init_from_env(env);
}
