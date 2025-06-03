// download_state.rs
use crate::arguments::Arguments;
use crate::dezoomer::{TileFetchResult, TileReference, ZoomLevel, ZoomLevelIter};
use crate::encoder::tile_buffer::TileBuffer;
use crate::errors::{self, ZoomError}; // `self` imports the errors module itself
use crate::max_size_in_rect;
use crate::network::{TileDownloader, client as network_client};
use crate::throttler::Throttler;
use crate::tile::Tile;
use crate::vec2d::Vec2d; // This is a public function from lib.rs

use futures::stream::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use log::debug;
use std::default::Default;

// --- DownloadState ---
#[derive(Debug, Default)]
pub(crate) struct DownloadState {
    pub(crate) total_tiles: u64,
    pub(crate) successful_tiles: u64,
    pub(crate) last_batch_count: u64,
    pub(crate) last_batch_successes: u64,
    tile_size: Option<Vec2d>,
}

impl DownloadState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add_batch(&mut self, count: u64) {
        self.last_batch_count = count;
        self.total_tiles += count;
        self.last_batch_successes = 0;
    }

    pub(crate) fn record_success(&mut self) {
        self.last_batch_successes += 1;
        self.successful_tiles += 1;
    }

    fn set_tile_size(&mut self, size: Vec2d) {
        self.tile_size = Some(size);
    }

    pub(crate) fn create_fetch_result(&self) -> TileFetchResult {
        TileFetchResult {
            count: self.last_batch_count,
            successes: self.last_batch_successes,
            tile_size: self.tile_size,
        }
    }

    pub(crate) fn is_successful(&self) -> bool {
        self.successful_tiles > 0
    }

    pub(crate) fn has_partial_failure(&self) -> bool {
        self.last_batch_successes < self.last_batch_count
    }
}

// --- ProgressManager ---
#[derive(Debug)]
pub(crate) struct ProgressManager {
    progress: ProgressBar,
}

impl ProgressManager {
    pub(crate) fn new() -> Self {
        Self {
            progress: progress_bar(10), // Default initial size, will be updated
        }
    }

    pub(crate) fn set_total_tiles(&self, total: u64) {
        self.progress.set_length(total);
    }

    pub(crate) fn set_computing_urls(&self) {
        self.progress
            .set_message("Computing the URLs of the image tiles...");
    }

    pub(crate) fn set_requesting_tiles(&self) {
        self.progress.set_message("Requesting the tiles...");
    }

    pub(crate) fn set_finalizing(&self) {
        self.progress
            .set_message("Downloaded all tiles. Finalizing the image file.");
    }

    pub(crate) fn increment(&self) {
        self.progress.inc(1);
    }

    pub(crate) fn update_for_tile(&self, tile: &Option<Tile>, success: bool) {
        if success {
            if let Some(tile) = tile {
                self.progress
                    .set_message(format!("Loaded tile at {}", tile.position()));
            }
        } else {
            self.progress
                .set_message("Failed to load tile, using empty replacement");
        }
    }

    pub(crate) fn finish(&self) {
        self.progress.finish_with_message("Finished tile download");
    }
}

// Helper function, private to this module
fn progress_bar(n: usize) -> ProgressBar {
    let progress = ProgressBar::new(n as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[ETA:{eta}] {bar:40.cyan/blue} {pos:>4}/{len:4} {msg}")
            .expect("Invalid indicatif progress bar template")
            .progress_chars("##-"),
    );
    progress
}

// --- TileDownloadCoordinator ---
// Not deriving Debug because Throttler doesn't derive Debug
pub(crate) struct TileDownloadCoordinator<'a> {
    downloader: TileDownloader,
    throttler: Throttler,
    args: &'a Arguments,
}

impl<'a> TileDownloadCoordinator<'a> {
    pub(crate) fn new(zoom_level: &ZoomLevel, args: &'a Arguments) -> Result<Self, ZoomError> {
        let downloader = create_tile_downloader(zoom_level, args)?;
        let throttler = Throttler::new(args.min_interval);

        Ok(Self {
            downloader,
            throttler,
            args,
        })
    }

    pub(crate) async fn download_batch(
        &mut self,
        tile_refs: Vec<TileReference>,
        canvas: &mut TileBuffer,
        state: &mut DownloadState,
        progress: &ProgressManager,
        zoom_level_iter: &ZoomLevelIter<'_>,
    ) -> Result<(), ZoomError> {
        state.add_batch(tile_refs.len() as u64);
        progress.set_total_tiles(state.total_tiles); // Update progress bar length with cumulative total
        progress.set_requesting_tiles();

        prepare_canvas_size(canvas, zoom_level_iter).await?;

        let mut stream = futures::stream::iter(tile_refs)
            .map(|tile_ref: TileReference| self.downloader.download_tile(tile_ref))
            .buffer_unordered(self.args.parallelism);

        while let Some(tile_result) = stream.next().await {
            debug!("Received tile result: {:?}", tile_result); // Tile and TileDownloadError need Debug
            progress.increment();

            let (tile, success) = process_tile_result(
                tile_result,
                &mut state.tile_size,
                zoom_level_iter.size_hint(),
            );

            progress.update_for_tile(&tile, success);

            if success {
                state.record_success();
                if let Some(ref tile) = tile {
                    state.set_tile_size(tile.size());
                }
            }

            if let Some(tile) = tile {
                canvas.add_tile(tile).await;
            }
            self.throttler.wait().await;
        }
        Ok(())
    }
}

// Helper function, private to this module
fn create_tile_downloader(
    zoom_level: &ZoomLevel,
    args: &Arguments,
) -> Result<TileDownloader, ZoomError> {
    let level_headers = zoom_level.http_headers();
    Ok(TileDownloader {
        http_client: network_client(level_headers.iter().chain(args.headers()), args, None)?,
        post_process_fn: zoom_level.post_process_fn(),
        retries: args.retries,
        retry_delay: args.retry_delay,
        tile_storage_folder: args.tile_storage_folder.clone(),
    })
}

// Helper function, private to this module
async fn prepare_canvas_size(
    canvas: &mut TileBuffer,
    zoom_level_iter: &ZoomLevelIter<'_>,
) -> Result<(), ZoomError> {
    if let Some(size) = zoom_level_iter.size_hint() {
        canvas.set_size(size).await?;
    }
    Ok(())
}

// Helper function, private to this module
fn process_tile_result(
    tile_result: Result<Tile, errors::TileDownloadError>,
    tile_size: &mut Option<Vec2d>,
    canvas_size: Option<Vec2d>,
) -> (Option<Tile>, bool) {
    match tile_result {
        Ok(tile) => {
            *tile_size = Some(tile.size()); // Update tile_size with the size of the successfully downloaded tile
            (Some(tile), true)
        }
        Err(err) => {
            let position = err.tile_reference.position;
            // Try to create an empty tile only if we know the expected tile_size and canvas_size
            let empty_tile = match (*tile_size, canvas_size) {
                (Some(current_tile_size), Some(current_canvas_size)) => {
                    let size = max_size_in_rect(position, current_tile_size, current_canvas_size);
                    Some(Tile::empty(position, size))
                }
                _ => None, // Not enough info to create a correctly sized empty tile
            };
            (empty_tile, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::process_tile_result; // From the parent module 'download_state'
    use crate::dezoomer::TileReference;
    use crate::errors::{TileDownloadError, ZoomError};
    use crate::max_size_in_rect;
    use crate::tile::Tile;
    use crate::vec2d::Vec2d; // Used by process_tile_result, ensure it's in scope for understanding test logic

    #[test]
    fn test_process_tile_result() {
        let mut tile_size: Option<Vec2d> = None;
        let canvas_size = Vec2d { x: 1000, y: 1000 };

        // Test successful tile result
        let tile_to_test = Tile::empty(Vec2d { x: 0, y: 0 }, Vec2d { x: 256, y: 256 });
        let ok_result: Result<Tile, TileDownloadError> = Ok(tile_to_test.clone());
        let (result_tile_opt, success) =
            process_tile_result(ok_result, &mut tile_size, Some(canvas_size));

        assert!(success, "Tile processing should succeed for Ok result");
        assert!(
            result_tile_opt.is_some(),
            "Result tile should be Some for Ok result"
        );
        if let Some(ref result_tile) = result_tile_opt {
            assert_eq!(
                result_tile.size(),
                Vec2d { x: 256, y: 256 },
                "Result tile size mismatch"
            );
        }
        assert_eq!(
            tile_size,
            Some(Vec2d { x: 256, y: 256 }),
            "tile_size variable mismatch after success"
        );

        // Test failed tile result
        // process_tile_result will use the current value of tile_size (if Some) to determine the size of the empty tile.
        // So, we set it to what a previously successful tile might have set.
        tile_size = Some(Vec2d { x: 256, y: 256 });

        let tile_ref = TileReference {
            url: "http://example.com/tile.jpg".to_string(),
            position: Vec2d { x: 100, y: 100 },
        };
        let error = TileDownloadError {
            tile_reference: tile_ref.clone(), // Clone if tile_ref is used later, or ensure it's not.
            cause: ZoomError::NoLevels,       // Using an arbitrary ZoomError variant
        };
        let err_result: Result<Tile, TileDownloadError> = Err(error);
        let (result_tile_opt_err, success_err) =
            process_tile_result(err_result, &mut tile_size, Some(canvas_size));

        assert!(!success_err, "Tile processing should fail for Err result");
        assert!(
            result_tile_opt_err.is_some(),
            "Result tile should be Some (empty tile) for Err result"
        );
        if let Some(ref empty_tile) = result_tile_opt_err {
            // The empty tile's size is determined by max_size_in_rect.
            // Given position (100,100), tile_size (256,256), canvas_size (1000,1000),
            // max_size_in_rect should return (256,256) as it fits.
            let expected_empty_size =
                max_size_in_rect(tile_ref.position, tile_size.unwrap(), canvas_size);
            assert_eq!(
                empty_tile.size(),
                expected_empty_size,
                "Empty tile size mismatch"
            );
            assert_eq!(
                empty_tile.position(),
                tile_ref.position,
                "Empty tile position mismatch"
            );
        }
        // tile_size should remain Some(Vec2d { x: 256, y: 256 }) as per logic in process_tile_result for Err case.
        assert_eq!(
            tile_size,
            Some(Vec2d { x: 256, y: 256 }),
            "tile_size variable mismatch after failure"
        );
    }
}
