# Design Document: Multi-Image Dezoomer Architecture Redesign

## Overview

This document outlines the redesign of the dezoomer architecture to better handle multi-image downloads by introducing a `ZoomableImage` abstraction and supporting both direct images and image URLs that need further processing.

## New Design

### Core Traits and Types

```rust
/// Represents a single zoomable image with multiple resolution levels
pub trait ZoomableImage: Send + Sync + std::fmt::Debug {
    /// Get all available zoom levels for this image
    fn zoom_levels(&self) -> Result<ZoomLevels, DezoomerError>;
    
    /// Get a human-readable title for this image
    fn title(&self) -> Option<String>;
}

/// A URL that can be processed by dezoomers to create ZoomableImages
#[derive(Debug, Clone)]
pub struct ZoomableImageUrl {
    pub url: String,
    pub title: Option<String>,
}

/// Result type for dezoomer operations
#[derive(Debug)]
pub enum DezoomerResult {
    /// Direct zoomable images (e.g., from IIIF manifests, krpano configs)
    Images(Vec<Box<dyn ZoomableImage>>),
    /// URLs that need further processing by other dezoomers
    ImageUrls(Vec<ZoomableImageUrl>),
}

/// Modified Dezoomer trait
pub trait Dezoomer {
    fn name(&self) -> &'static str;
    
    /// Extract images or image URLs from the input data
    fn dezoomer_result(&mut self, data: &DezoomerInput) -> Result<DezoomerResult, DezoomerError>;
}
```

### Picker Functions

```rust
/// Pick an image from multiple options (interactive or automatic)
pub fn image_picker(images: Vec<Box<dyn ZoomableImage>>) -> Result<Box<dyn ZoomableImage>, ZoomError>;

/// Existing level picker (unchanged)
pub fn level_picker(mut levels: ZoomLevels) -> Result<ZoomLevel, ZoomError>;
```

## Implementation Plan

### Step 1: Add New Traits and Types ✅ DONE
**Files to modify:** `src/dezoomer.rs`

**Tasks:**
1. ✅ Add `ZoomableImage` trait definition
2. ✅ Add `ZoomableImageUrl` struct  
3. ✅ Add `DezoomerResult` enum
4. ✅ Keep existing `Dezoomer` trait unchanged for now

**Tests to run:**
- ✅ `cargo clippy` - should pass
- ✅ `cargo test` - should pass (no functional changes yet)

**Remarks:** Successfully added all new types and traits. All existing functionality preserved, tests pass. Committed as 15693fa.

### Step 2: Create Simple ZoomableImage Implementation
**Files to modify:** `src/dezoomer.rs`

**Tasks:**
1. Create `SimpleZoomableImage` struct that wraps existing zoom levels
2. Implement `ZoomableImage` trait for `SimpleZoomableImage`

```rust
#[derive(Debug)]
pub struct SimpleZoomableImage {
    zoom_levels: ZoomLevels,
    title: Option<String>,
}

impl ZoomableImage for SimpleZoomableImage {
    fn zoom_levels(&self) -> Result<ZoomLevels, DezoomerError> {
        Ok(self.zoom_levels.clone())
    }
    
    fn title(&self) -> Option<String> {
        self.title.clone()
    }
}
```

**Tests to run:**
- `cargo clippy` - should pass
- `cargo test` - should pass
- Unit test the new `SimpleZoomableImage` implementation

### Step 3: Add New Dezoomer Method with Backward Compatibility
**Files to modify:** `src/dezoomer.rs`

**Tasks:**
1. Add `dezoomer_result` method to `Dezoomer` trait with default implementation
2. Default implementation calls existing `zoom_levels` method and wraps in `SimpleZoomableImage`

```rust
pub trait Dezoomer {
    fn name(&self) -> &'static str;
    fn zoom_levels(&mut self, data: &DezoomerInput) -> Result<ZoomLevels, DezoomerError>;
    
    // New method with default implementation for backward compatibility
    fn dezoomer_result(&mut self, data: &DezoomerInput) -> Result<DezoomerResult, DezoomerError> {
        let levels = self.zoom_levels(data)?;
        let image = SimpleZoomableImage {
            zoom_levels: levels,
            title: None,
        };
        Ok(DezoomerResult::Images(vec![Box::new(image)]))
    }
}
```

**Tests to run:**
- `cargo clippy` - should pass
- `cargo test` - should pass (all existing dezoomers use default implementation)

### Step 4: Transform IIIF Dezoomer
**Files to modify:** `src/iiif/mod.rs`

**Tasks:**
1. Create `IIIFZoomableImage` struct that wraps IIIF-specific zoom levels
2. Modify IIIF dezoomer to implement `dezoomer_result` method
3. Remove `zoom_levels` implementation (use trait default)

```rust
#[derive(Debug)]
pub struct IIIFZoomableImage {
    zoom_levels: ZoomLevels,
    title: Option<String>,
}

impl Dezoomer for IIIF {
    fn dezoomer_result(&mut self, data: &DezoomerInput) -> Result<DezoomerResult, DezoomerError> {
        let levels = zoom_levels(data.with_contents()?.uri, data.with_contents()?.contents)?;
        let image = IIIFZoomableImage {
            zoom_levels: levels,
            title: None, // Could extract from IIIF metadata later
        };
        Ok(DezoomerResult::Images(vec![Box::new(image)]))
    }
}
```

**Tests to run:**
- `cargo clippy` - should pass
- `cargo test` - IIIF tests should pass
- Test that IIIF dezoomer returns `DezoomerResult::Images`

### Step 5: Transform Krpano Dezoomer
**Files to modify:** `src/krpano/mod.rs`

**Tasks:**
1. Group krpano zoom levels by logical image (side/face)
2. Create `KrpanoZoomableImage` for each group
3. Implement `dezoomer_result` method

**Tests to run:**
- `cargo clippy` - should pass
- `cargo test` - Krpano tests should pass
- Test that Krpano returns multiple images for cube faces

### Step 6: Create Bulk Text Dezoomer
**Files to create:** `src/bulk_text/mod.rs`
**Files to modify:** `src/lib.rs` (to register new dezoomer)

**Tasks:**
1. Create new `BulkTextDezoomer` that parses text files
2. Returns `DezoomerResult::ImageUrls` with parsed URLs
3. Integrate into main dezoomer list

**Tests to run:**
- `cargo clippy` - should pass
- `cargo test` - should pass
- Test bulk text parsing with sample input

### Step 7: Update Main Processing Logic
**Files to modify:** `src/lib.rs`, main processing functions

**Tasks:**
1. Handle `DezoomerResult::ImageUrls` by recursively calling other dezoomers
2. Handle `DezoomerResult::Images` with new image picker
3. Update level picker to work with selected image

**Tests to run:**
- `cargo clippy` - should pass
- `cargo test` - should pass
- Integration tests with various input types

### Step 8: Remove Old Bulk Processing
**Files to modify/remove:** `src/bulk/` directory

**Tasks:**
1. Remove old bulk processing modules
2. Update imports and references
3. Remove `--bulk` CLI option

**Tests to run:**
- `cargo clippy` - should pass
- `cargo test` - should pass
- Manual testing of CLI to ensure bulk functionality works through new dezoomer

### Step 9: Remove Backward Compatibility
**Files to modify:** `src/dezoomer.rs`

**Tasks:**
1. Remove `zoom_levels` method from `Dezoomer` trait
2. Remove default implementation of `dezoomer_result`
3. Update any remaining dezoomers to implement the new method

**Tests to run:**
- `cargo clippy` - should pass
- `cargo test` - should pass
- All functionality preserved but with cleaner API

## Impacted Files

- `src/dezoomer.rs` - Core trait definitions and implementations
- `src/iiif/mod.rs` - IIIF dezoomer transformation
- `src/krpano/mod.rs` - Krpano dezoomer transformation  
- `src/bulk_text/mod.rs` - New bulk text dezoomer (created)
- `src/lib.rs` - Main processing logic updates
- `src/bulk/` - Removed in step 8
- Various dezoomer modules - Minor updates for new trait

## Success Criteria

After each step:
- ✅ `cargo clippy` passes with no warnings
- ✅ `cargo test` passes with no failures
- ✅ Specific functionality tests for the modified components
- ✅ No regression in existing functionality

The final result will be a unified architecture where all input types (single images, IIIF manifests, bulk text files) are handled through the same dezoomer interface, with proper separation between image discovery and zoom level generation.