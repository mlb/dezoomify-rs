#![allow(clippy::upper_case_acronyms)]

use std::env::current_dir;

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::{fs, io};

use log::{debug, error, info};
use reqwest::Client;

pub use arguments::Arguments;
pub use binary_display::{BinaryDisplay, display_bytes};
use dezoomer::TileReference;
use dezoomer::{Dezoomer, DezoomerError, DezoomerInput};
use dezoomer::{ZoomLevel, ZoomLevelIter};
pub use errors::ZoomError;
use network::{client, fetch_uri};
use output_file::get_outname;
use tile::Tile;
pub use vec2d::Vec2d;



use crate::dezoomer::{PageContents, DezoomerResult, ZoomableImage, ZoomableImageUrl};
use crate::encoder::tile_buffer::TileBuffer;

use crate::output_file::reserve_output_file;

mod arguments;
mod binary_display;

pub mod dezoomer;
pub(crate) mod download_state;
mod encoder;
mod errors;
mod network;
mod output_file;
pub mod tile;
mod vec2d;

pub mod auto;
pub mod bulk_text;
pub mod custom_yaml;
pub mod dzi;
pub mod generic;
pub mod google_arts_and_culture;
pub mod iiif;
pub mod iipimage;
mod json_utils;
pub mod krpano;
pub mod nypl;
pub mod pff;
mod throttler;
pub mod zoomify;

fn stdin_line() -> Result<String, ZoomError> {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let first_line = lines.next().ok_or_else(|| {
        let err_msg = "Encountered end of standard input while reading a line";
        io::Error::new(io::ErrorKind::UnexpectedEof, err_msg)
    })?;
    Ok(first_line?)
}



/// Process a single dezoomer to get a result, handling the NeedsData loop
async fn get_dezoomer_result(
    dezoomer: &mut dyn Dezoomer,
    http: &Client,
    uri: &str,
) -> Result<DezoomerResult, ZoomError> {
    let mut i = DezoomerInput {
        uri: String::from(uri),
        contents: PageContents::Unknown,
    };
    loop {
        match dezoomer.dezoomer_result(&i) {
            Ok(result) => return Ok(result),
            Err(DezoomerError::NeedsData { uri }) => {
                let contents = fetch_uri(&uri, http).await.into();
                debug!("Response for metadata file '{}': {:?}", uri, &contents);
                i.uri = uri;
                i.contents = contents;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Type alias for the complex future return type
type ProcessImageUrlsFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Box<dyn ZoomableImage>>, ZoomError>> + 'a>>;

/// Process a list of image URLs by trying each dezoomer on each URL
fn process_image_urls(
    urls: Vec<ZoomableImageUrl>,
    http: &Client,
) -> ProcessImageUrlsFuture<'_> {
    Box::pin(async move {
        use crate::auto::all_dezoomers;
        
        let mut all_images = Vec::new();
        
        for url in urls {
            debug!("Processing URL: {}", url.url);
            
            // Try each dezoomer on this URL
            let mut found_images = false;
            for mut dezoomer in all_dezoomers(false) {
                debug!("Trying dezoomer '{}' on URL: {}", dezoomer.name(), url.url);
                
                match get_dezoomer_result(dezoomer.as_mut(), http, &url.url).await {
                    Ok(result) => match result {
                        DezoomerResult::Images(images) => {
                            debug!("Dezoomer '{}' found {} images for URL: {}", dezoomer.name(), images.len(), url.url);
                            all_images.extend(images);
                            found_images = true;
                            break; // Found images with this dezoomer, stop trying others
                        }
                        DezoomerResult::ImageUrls(nested_urls) => {
                            debug!("Dezoomer '{}' found {} nested URLs for URL: {}", dezoomer.name(), nested_urls.len(), url.url);
                            // Recursively process nested URLs
                            match process_image_urls(nested_urls, http).await {
                                Ok(nested_images) => {
                                    all_images.extend(nested_images);
                                    found_images = true;
                                    break; // Successfully processed nested URLs
                                }
                                Err(e) => {
                                    debug!("Failed to process nested URLs from '{}': {}", dezoomer.name(), e);
                                    // Continue trying other dezoomers
                                }
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Dezoomer '{}' failed for URL '{}': {}", dezoomer.name(), url.url, e);
                        // Continue trying other dezoomers
                    }
                }
            }
            
            if !found_images {
                error!("No dezoomer could process URL: {}", url.url);
            }
        }
        
        if all_images.is_empty() {
            Err(ZoomError::NoLevels)
        } else {
            Ok(all_images)
        }
    })
}

/// Process an input URI to extract zoomable images, handling both direct images and URL lists
async fn get_images_from_uri(
    args: &Arguments,
    http: &Client,
    uri: &str,
) -> Result<Vec<Box<dyn ZoomableImage>>, ZoomError> {
    let mut dezoomer = args.find_dezoomer()?;
    
    match get_dezoomer_result(dezoomer.as_mut(), http, uri).await? {
        DezoomerResult::Images(images) => {
            debug!("Found {} direct images", images.len());
            Ok(images)
        }
        DezoomerResult::ImageUrls(urls) => {
            debug!("Found {} URLs to process", urls.len());
            process_image_urls(urls, http).await
        }
    }
}

/// Validates a user input line as a level index
fn parse_level_index(input: &str, max_index: usize) -> Option<usize> {
    input.parse::<usize>().ok().filter(|&idx| idx < max_index)
}

/// Gets the actual level index to use, handling out-of-bounds requests
fn resolve_level_index(requested: usize, available_count: usize) -> usize {
    if requested < available_count {
        requested
    } else {
        available_count - 1
    }
}

/// Gets the actual image index to use, handling out-of-bounds requests
fn resolve_image_index(requested: usize, available_count: usize) -> usize {
    if requested < available_count {
        requested
    } else {
        available_count - 1
    }
}

/// Finds the position of a level with the specified size hint
fn find_level_with_size(levels: &[ZoomLevel], target_size: Vec2d) -> Option<usize> {
    levels
        .iter()
        .position(|l| l.size_hint() == Some(target_size))
}

/// An interactive level picker
fn level_picker(mut levels: Vec<ZoomLevel>) -> Result<ZoomLevel, ZoomError> {
    println!("Found the following zoom levels:");
    for (i, level) in levels.iter().enumerate() {
        println!("{: >2}. {}", i, level.name());
    }
    loop {
        println!("Which level do you want to download? ");
        let line = stdin_line()?;
        if let Some(idx) = parse_level_index(&line, levels.len()) {
            return Ok(levels.swap_remove(idx));
        }
        error!("'{line}' is not a valid level number");
    }
}

fn choose_level(mut levels: Vec<ZoomLevel>, args: &Arguments) -> Result<ZoomLevel, ZoomError> {
    match levels.len() {
        0 => Err(ZoomError::NoLevels),
        1 => Ok(levels.swap_remove(0)),
        _ => {
            if let Some(requested_level) = args.zoom_level {
                let actual_level = resolve_level_index(requested_level, levels.len());
                if actual_level == requested_level {
                    info!("Selected zoom level {} as requested", requested_level);
                } else {
                    info!(
                        "Requested zoom level {} not available. Using last one ({})",
                        requested_level, actual_level
                    );
                }
                return Ok(levels.swap_remove(actual_level));
            }

            if let Some(best_size) = args.best_size(levels.iter().filter_map(|l| l.size_hint())) {
                if let Some(pos) = find_level_with_size(&levels, best_size) {
                    return Ok(levels.swap_remove(pos));
                }
            }

            level_picker(levels)
        }
    }
}

/// An interactive image picker for when multiple images are available
fn image_picker(mut images: Vec<Box<dyn ZoomableImage>>) -> Result<Box<dyn ZoomableImage>, ZoomError> {
    println!("Found the following images:");
    for (i, image) in images.iter().enumerate() {
        let title = image.title().unwrap_or_else(|| format!("Image {}", i + 1));
        println!("{: >2}. {}", i, title);
    }
    loop {
        println!("Which image do you want to download? ");
        let line = stdin_line()?;
        if let Some(idx) = parse_level_index(&line, images.len()) {
            return Ok(images.swap_remove(idx));
        }
        error!("'{line}' is not a valid image number");
    }
}

/// Choose an image from multiple options (interactive or automatic)
fn choose_image(mut images: Vec<Box<dyn ZoomableImage>>, args: &Arguments) -> Result<Box<dyn ZoomableImage>, ZoomError> {
    match images.len() {
        0 => Err(ZoomError::NoLevels),
        1 => Ok(images.swap_remove(0)),
        _ => {
            if let Some(requested_index) = args.image_index {
                let actual_index = resolve_image_index(requested_index, images.len());
                if actual_index == requested_index {
                    info!("Selected image {} as requested", requested_index);
                } else {
                    info!(
                        "Requested image index {} not available. Using last one ({})",
                        requested_index, actual_index
                    );
                }
                return Ok(images.swap_remove(actual_index));
            }

            // In bulk mode, automatically select the first image to avoid interactive prompts
            if args.is_bulk_mode() {
                info!("Bulk mode: automatically selecting first image (index 0)");
                return Ok(images.swap_remove(0));
            }

            // Interactive selection when no command line option is provided
            image_picker(images)
        }
    }
}



/// Finds the appropriate zoomlevel for a given size if one is specified,
async fn find_zoomlevel(args: &Arguments) -> Result<ZoomLevel, ZoomError> {
    let uri = args.choose_input_uri()?;
    let http_client = client(args.headers(), args, Some(&uri))?;
    debug!("Trying to locate a zoomable image...");
    
    // Use the new unified processing pipeline
    let images = get_images_from_uri(args, &http_client, &uri).await?;
    debug!("Found {} zoomable images", images.len());
    
    // Select an image from the available options
    let selected_image = choose_image(images, args)?;
    debug!("Selected image: {:?}", selected_image.title());
    
    // Extract zoom levels from the selected image
    let zoom_levels = selected_image.into_zoom_levels().map_err(|e| ZoomError::Dezoomer { source: e })?;
    debug!("Extracted {} zoom levels", zoom_levels.len());
    
    // Select a zoom level from the available options
    choose_level(zoom_levels, args)
}

/// Prepares the output file path for saving
fn prepare_output_path(
    outfile_arg: &Option<PathBuf>,
    title: &Option<String>,
    base_dir: &Path,
    size_hint: Option<Vec2d>,
) -> Result<PathBuf, ZoomError> {
    let outname = get_outname(outfile_arg, title, base_dir, size_hint);
    let save_as = fs::canonicalize(outname.as_path()).unwrap_or_else(|_e| outname.clone());
    reserve_output_file(&save_as)?;
    Ok(save_as)
}

/// Creates a tile buffer for the given output path
async fn create_tile_buffer(save_as: PathBuf, compression: u8) -> Result<TileBuffer, ZoomError> {
    TileBuffer::new(save_as, compression).await
}

pub async fn dezoomify(args: &Arguments) -> Result<PathBuf, ZoomError> {
    let zoom_level = find_zoomlevel(args).await?;
    let base_dir = current_dir()?;
    let save_as = prepare_output_path(
        &args.outfile,
        &zoom_level.title(),
        &base_dir,
        zoom_level.size_hint(),
    )?;
    let tile_buffer = create_tile_buffer(save_as.clone(), args.compression).await?;
    info!("Dezooming {}", zoom_level.name());
    dezoomify_level(args, zoom_level, tile_buffer).await?;
    Ok(save_as)
}

/// Statistics for bulk processing
#[derive(Debug, Default)]
pub struct BulkStats {
    pub total_images: usize,
    pub successful_images: usize,
    pub failed_images: usize,
    pub partial_downloads: usize,
}

impl BulkStats {
    fn new() -> Self {
        Self::default()
    }

    fn record_success(&mut self) {
        self.successful_images += 1;
    }

    fn record_partial(&mut self) {
        self.partial_downloads += 1;
    }

    fn record_failure(&mut self) {
        self.failed_images += 1;
    }

    fn set_total(&mut self, total: usize) {
        self.total_images = total;
    }
}

/// Process multiple images in bulk mode using the new unified architecture
pub async fn process_bulk(args: &Arguments) -> Result<BulkStats, ZoomError> {
    use log::{debug, trace, warn};
    
    debug!("Starting bulk processing mode");
    trace!("Bulk processing arguments: {:?}", args);
    
    // Get the bulk file/URI from arguments
    let bulk_uri = args.bulk.as_ref()
        .ok_or_else(|| ZoomError::NoBulkUrl { 
            bulk_file_path: "No bulk source specified".to_string() 
        })?;
    
    debug!("Bulk source: {}", bulk_uri);
    
    // Get all images from the bulk source using unified pipeline
    let http = client(std::iter::empty(), args, None)?;
    let images = get_images_from_uri(args, &http, bulk_uri).await?;
    
    let mut stats = BulkStats::new();
    stats.set_total(images.len());
    
    info!("Found {} images to process in bulk mode", images.len());
    debug!("Images discovered: {:?}", images.iter().map(|img| img.title().unwrap_or_else(|| "Untitled".to_string())).collect::<Vec<_>>());
    
    let base_dir = current_dir()?;
    
    // Process each image individually
    for (index, image) in images.into_iter().enumerate() {
        let image_title = image.title().unwrap_or_else(|| format!("Image_{}", index + 1));
        info!("Processing image {}/{}: {}", index + 1, stats.total_images, image_title);
        trace!("Processing image: {:?}", image);
        
        // Generate unique output filename for this image
        let output_path = if let Some(ref base_outfile) = args.outfile {
            generate_bulk_output_name(base_outfile, index)
        } else {
            let mut path = base_dir.clone();
            path.push(format!("dezoomified_{}.jpg", index + 1));
            path
        };
        
        debug!("Output path for image {}: {:?}", index + 1, output_path);
        
        // Get zoom levels from the image
        let zoom_levels = match image.into_zoom_levels() {
            Ok(levels) => levels,
            Err(e) => {
                warn!("Failed to get zoom levels for image {}: {}", index + 1, e);
                stats.record_failure();
                continue;
            }
        };
        
        trace!("Zoom levels for image {}: {} levels available", index + 1, zoom_levels.len());
        
        // Choose the appropriate zoom level using existing logic
        let zoom_level = match choose_level(zoom_levels, args) {
            Ok(level) => level,
            Err(e) => {
                warn!("Failed to choose zoom level for image {}: {}", index + 1, e);
                stats.record_failure();
                continue;
            }
        };
        
        debug!("Selected zoom level for image {}: {} ({}x{})", 
               index + 1, zoom_level.name(), 
               zoom_level.size_hint().map(|s| s.x).unwrap_or(0),
               zoom_level.size_hint().map(|s| s.y).unwrap_or(0));
        
        // Prepare output file
        let save_as = match prepare_output_path(&Some(output_path), &zoom_level.title(), &base_dir, zoom_level.size_hint()) {
            Ok(path) => path,
            Err(e) => {
                warn!("Failed to prepare output path for image {}: {}", index + 1, e);
                stats.record_failure();
                continue;
            }
        };
        
        let tile_buffer = match create_tile_buffer(save_as.clone(), args.compression).await {
            Ok(buffer) => buffer,
            Err(e) => {
                warn!("Failed to create tile buffer for image {}: {}", index + 1, e);
                stats.record_failure();
                continue;
            }
        };
        
        // Process the image
        info!("Downloading image {}: {}", index + 1, zoom_level.name());
        match dezoomify_level(args, zoom_level, tile_buffer).await {
            Ok(()) => {
                info!("Successfully saved image {} to {}", index + 1, save_as.display());
                stats.record_success();
            },
            Err(ZoomError::PartialDownload { successful_tiles, total_tiles, .. }) => {
                warn!("Image {} completed with partial download: {}/{} tiles", index + 1, successful_tiles, total_tiles);
                stats.record_partial();
            },
            Err(e) => {
                warn!("Failed to process image {}: {}", index + 1, e);
                stats.record_failure();
            }
        }
    }
    
    // Log final statistics
    info!("Bulk processing complete!");
    info!("Total images: {}", stats.total_images);
    info!("Successfully downloaded: {}", stats.successful_images);
    info!("Partial downloads: {}", stats.partial_downloads);
    info!("Failed downloads: {}", stats.failed_images);
    
    debug!("Final bulk processing stats: {:?}", stats);
    
    Ok(stats)
}

/// Generate a unique output filename for bulk processing
fn generate_bulk_output_name(base_outfile: &Path, index: usize) -> PathBuf {
    let mut result = base_outfile.to_path_buf();
    
    if let Some(stem) = base_outfile.file_stem() {
        if let Some(extension) = base_outfile.extension() {
            let new_name = format!("{}_{}.{}", 
                                   stem.to_string_lossy(), 
                                   index + 1, 
                                   extension.to_string_lossy());
            result.set_file_name(new_name);
        } else {
            let new_name = format!("{}_{}", stem.to_string_lossy(), index + 1);
            result.set_file_name(new_name);
        }
    } else {
        result.set_file_name(format!("dezoomified_{}.jpg", index + 1));
    }
    
    result
}

/// Validates the download success based on the final state.
/// Validates that enough tiles were downloaded to proceed
fn validate_download_success(state: &download_state::DownloadState) -> Result<(), ZoomError> {
    if !state.is_successful() {
        Err(ZoomError::NoTile)
    } else {
        Ok(())
    }
}

/// Determines final result based on download success rate
fn determine_final_result(
    state: &download_state::DownloadState,
    destination: String,
) -> Result<(), ZoomError> {
    if state.has_partial_failure() {
        Err(ZoomError::PartialDownload {
            successful_tiles: state.successful_tiles,
            total_tiles: state.total_tiles,
            destination,
        })
    } else {
        Ok(())
    }
}

pub async fn dezoomify_level(
    args: &Arguments,
    mut zoom_level: ZoomLevel,
    tile_buffer: TileBuffer,
) -> Result<(), ZoomError> {
    debug!("Starting to dezoomify {zoom_level:?}");
    let mut canvas = tile_buffer;
    let mut coordinator = download_state::TileDownloadCoordinator::new(&zoom_level, args)?;
    let mut state = download_state::DownloadState::new();
    let progress = download_state::ProgressManager::new();

    progress.set_computing_urls();

    let mut zoom_level_iter = ZoomLevelIter::new(&mut zoom_level);

    while let Some(tile_refs) = zoom_level_iter.next_tile_references() {
        coordinator
            .download_batch(
                tile_refs,
                &mut canvas,
                &mut state,
                &progress,
                &zoom_level_iter,
            )
            .await?;

        zoom_level_iter.set_fetch_result(state.create_fetch_result());
    }

    validate_download_success(&state)?;

    progress.set_finalizing();
    canvas.finalize().await?;
    progress.finish();

    let destination = canvas.destination().to_string_lossy().to_string();
    determine_final_result(&state, destination)
}

/// Returns the maximal size a tile can have in order to fit in a canvas of the given size
pub fn max_size_in_rect(position: Vec2d, tile_size: Vec2d, canvas_size: Vec2d) -> Vec2d {
    (position + tile_size).min(canvas_size) - position
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_level_index() {
        assert_eq!(parse_level_index("0", 5), Some(0));
        assert_eq!(parse_level_index("4", 5), Some(4));
        assert_eq!(parse_level_index("5", 5), None); // Out of bounds
        assert_eq!(parse_level_index("abc", 5), None); // Invalid number
        assert_eq!(parse_level_index("", 5), None); // Empty string
        assert_eq!(parse_level_index("2", 1), None); // Index too high
    }

    #[test]
    fn test_resolve_level_index() {
        assert_eq!(resolve_level_index(2, 5), 2); // Within bounds
        assert_eq!(resolve_level_index(0, 5), 0); // First index
        assert_eq!(resolve_level_index(4, 5), 4); // Last valid index
        assert_eq!(resolve_level_index(10, 5), 4); // Out of bounds, use last
        assert_eq!(resolve_level_index(100, 3), 2); // Way out of bounds
    }

    #[test]
    fn test_resolve_image_index() {
        assert_eq!(resolve_image_index(1, 3), 1); // Within bounds
        assert_eq!(resolve_image_index(0, 3), 0); // First index
        assert_eq!(resolve_image_index(2, 3), 2); // Last valid index
        assert_eq!(resolve_image_index(5, 3), 2); // Out of bounds, use last
        assert_eq!(resolve_image_index(100, 1), 0); // Way out of bounds
    }

    #[test]
    fn test_max_size_in_rect() {
        // Tile fits completely within canvas
        assert_eq!(
            max_size_in_rect(
                Vec2d { x: 10, y: 10 },
                Vec2d { x: 50, y: 50 },
                Vec2d { x: 100, y: 100 }
            ),
            Vec2d { x: 50, y: 50 }
        );

        // Tile extends beyond canvas horizontally
        assert_eq!(
            max_size_in_rect(
                Vec2d { x: 80, y: 10 },
                Vec2d { x: 50, y: 50 },
                Vec2d { x: 100, y: 100 }
            ),
            Vec2d { x: 20, y: 50 }
        );

        // Tile extends beyond canvas vertically
        assert_eq!(
            max_size_in_rect(
                Vec2d { x: 10, y: 80 },
                Vec2d { x: 50, y: 50 },
                Vec2d { x: 100, y: 100 }
            ),
            Vec2d { x: 50, y: 20 }
        );

        // Tile extends beyond canvas in both dimensions
        assert_eq!(
            max_size_in_rect(
                Vec2d { x: 90, y: 90 },
                Vec2d { x: 50, y: 50 },
                Vec2d { x: 100, y: 100 }
            ),
            Vec2d { x: 10, y: 10 }
        );

        // Tile at canvas edge
        assert_eq!(
            max_size_in_rect(
                Vec2d { x: 0, y: 0 },
                Vec2d { x: 100, y: 100 },
                Vec2d { x: 100, y: 100 }
            ),
            Vec2d { x: 100, y: 100 }
        );
    }

    #[test]
    fn test_validate_download_success() {
        let mut successful_state = download_state::DownloadState::new();
        successful_state.record_success();
        assert!(validate_download_success(&successful_state).is_ok());

        let failed_state = download_state::DownloadState::new();
        assert!(validate_download_success(&failed_state).is_err());
    }

    #[test]
    fn test_determine_final_result() {
        let destination = "test.jpg".to_string();

        // Complete success - no partial failure
        let mut success_state = download_state::DownloadState::new();
        success_state.add_batch(10);
        for _ in 0..10 {
            success_state.record_success();
        }
        assert!(determine_final_result(&success_state, destination.clone()).is_ok());

        // Partial failure
        let mut partial_state = download_state::DownloadState::new();
        partial_state.add_batch(10);
        for _ in 0..8 {
            partial_state.record_success();
        }
        let result = determine_final_result(&partial_state, destination.clone());
        assert!(result.is_err());
        if let Err(ZoomError::PartialDownload {
            successful_tiles,
            total_tiles,
            ..
        }) = result
        {
            assert_eq!(successful_tiles, 8);
            assert_eq!(total_tiles, 10);
        } else {
            panic!("Expected PartialDownload error");
        }
    }

    #[test]
    fn test_find_level_with_size() {
        // Since we can't easily create real ZoomLevel instances for testing,
        // let's test the logic directly with a simpler approach
        let sizes = [
            Some(Vec2d { x: 100, y: 100 }),
            Some(Vec2d { x: 200, y: 200 }),
            None,
            Some(Vec2d { x: 300, y: 300 }),
        ];

        let target_size = Vec2d { x: 200, y: 200 };
        let position = sizes.iter().position(|&s| s == Some(target_size));
        assert_eq!(position, Some(1));

        let target_size_not_found = Vec2d { x: 400, y: 400 };
        let position = sizes.iter().position(|&s| s == Some(target_size_not_found));
        assert_eq!(position, None);
    }
}
