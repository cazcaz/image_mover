//! Media file detection and collection functionality.
//!
//! This module provides functions for identifying media files (images and videos)
//! and recursively collecting them from directory structures.

use std::fs;
use std::io;
use std::path::PathBuf;

pub fn collect_media_files(
    current_dir: &PathBuf,
    source_root: &PathBuf,
    media_files: &mut Vec<PathBuf>,
    exclude_path: Option<&PathBuf>,
) -> io::Result<()> {
    let entries = match fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!(
                "Warning: Cannot access directory '{}': {}",
                current_dir.display(),
                e
            );
            return Ok(()); // Continue processing other directories
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!(
                    "Warning: Cannot read directory entry in '{}': {}",
                    current_dir.display(),
                    e
                );
                continue; // Skip this entry and continue with others
            }
        };
        let path = entry.path();

        if path.is_dir() {
            // Skip the destination directory if it's within the source to prevent infinite recursion
            if let Some(exclude) = exclude_path {
                if let (Ok(canonical_path), Ok(canonical_exclude)) =
                    (path.canonicalize(), exclude.canonicalize())
                {
                    if canonical_path == canonical_exclude {
                        println!("Skipping destination directory: {}", path.display());
                        continue;
                    }
                }
            }

            // Recursively process subdirectories
            if let Err(e) = collect_media_files(&path, source_root, media_files, exclude_path) {
                eprintln!(
                    "Warning: Cannot access subdirectory '{}': {}",
                    path.display(),
                    e
                );
                // Continue processing other directories
            }
        } else if path.is_file() {
            if let Some(extension) = path.extension() {
                let ext = extension.to_string_lossy().to_lowercase();

                // Check if it's an image or video file
                if is_media_file(&ext) {
                    // Calculate relative path from source root
                    let relative_path = path
                        .strip_prefix(source_root)
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

                    media_files.push(relative_path.to_path_buf());
                }
            }
        }
    }

    Ok(())
}

/// Determines if a file extension represents a media file (image or video).
///
/// This function supports a comprehensive list of media file formats including:
/// - Standard image formats (JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, etc.)
/// - RAW formats from all major camera manufacturers:
///   * Canon (CR2, CR3, CRW)
///   * Nikon (NEF, NRW)
///   * Sony (ARW, SRF, SR2)
///   * Olympus (ORF)
///   * Panasonic (RW2)
///   * Fujifilm (RAF)
///   * Pentax (PEF, PTX)
///   * Leica (RWL, DCS)
///   * Sigma (X3F)
///   * And many other manufacturers
/// - Adobe DNG (Digital Negative)
/// - Professional video formats (R3D, BRAW, ProRes, etc.)
/// - Standard video formats (MP4, MOV, AVI, MKV, etc.)
pub fn is_media_file(extension: &str) -> bool {
    match extension {
        // Standard image formats
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "tif" | "webp" | "svg" | "ico"
        | "heic" | "heif" => true,

        // RAW formats (comprehensive list for major camera manufacturers)
        // Generic RAW and Adobe DNG
        "raw" | "dng" => true,

        // Canon RAW formats
        "cr2" | "cr3" | "crw" | "1dx" | "1dc" => true,

        // Nikon RAW formats
        "nef" | "nrw" => true,

        // Sony RAW formats
        "arw" | "srf" | "sr2" => true,

        // Olympus RAW formats
        "orf" => true,

        // Panasonic RAW formats
        "rw2" => true,

        // Fujifilm RAW formats
        "raf" => true,

        // Pentax RAW formats
        "ptx" | "pef" => true,

        // Leica RAW formats
        "rwl" | "dcs" => true,

        // Sigma RAW formats
        "x3f" => true,

        // Mamiya RAW formats
        "mef" => true,

        // Phase One RAW formats
        "iiq" | "cap" => true,

        // Hasselblad RAW formats
        "3fr" | "fff" => true,

        // Kodak RAW formats
        "dcr" | "k25" | "kdc" => true,

        // Minolta/Konica Minolta RAW formats
        "mrw" => true,

        // Samsung RAW formats
        "srw" => true,

        // Epson RAW formats
        "erf" => true,

        // Other proprietary formats
        "bay" | "bmq" | "cs1" | "dc2" | "drf" | "dsc" | "dxo" | "ia" | "kc2" | "mdc" | "mos"
        | "mqv" | "ndd" | "obm" | "oti" | "pcd" | "pxn" | "qtk" | "ras" | "rdc" | "rwz" | "st4"
        | "st5" | "st6" | "st7" | "st8" | "stx" | "wdp" => true,

        // Video formats
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "3gp" | "3g2" | "f4v"
        | "asf" | "rm" | "rmvb" | "vob" | "ogv" | "drc" | "mng" | "qt" | "yuv" | "m2v" | "m4p"
        | "mpg" | "mp2" | "mpeg" | "mpe" | "mpv" | "m2ts" | "mts" | "ts" => true,

        // Professional video formats (removed duplicates)
        "mxf" | "r3d" | "braw" | "prores" | "dnxhd" | "cine" => true,

        _ => false,
    }
}

/// Collect media files and calculate total size in one pass with progress display
pub fn collect_media_files_with_size_and_progress(
    current_dir: &PathBuf,
    source_root: &PathBuf,
    media_files: &mut Vec<PathBuf>,
    total_size: &mut u64,
    exclude_path: Option<&PathBuf>,
) -> io::Result<()> {
    let result = collect_media_files_with_size_progress(
        current_dir,
        source_root,
        media_files,
        total_size,
        exclude_path,
        true,
    );

    // Print a newline after progress to move to next line
    if !media_files.is_empty() {
        println!(); // Move to next line after progress display
    }

    result
}

/// Collect media files and calculate total size in one pass with progress reporting
fn collect_media_files_with_size_progress(
    current_dir: &PathBuf,
    source_root: &PathBuf,
    media_files: &mut Vec<PathBuf>,
    total_size: &mut u64,
    exclude_path: Option<&PathBuf>,
    show_progress: bool,
) -> io::Result<()> {
    let entries = match fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!(
                "Warning: Cannot access directory '{}': {}",
                current_dir.display(),
                e
            );
            return Ok(()); // Continue processing other directories
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!(
                    "Warning: Cannot read directory entry in '{}': {}",
                    current_dir.display(),
                    e
                );
                continue; // Skip this entry and continue with others
            }
        };
        let path = entry.path();

        if path.is_dir() {
            // Skip the destination directory if it's within the source to prevent infinite recursion
            if let Some(exclude) = exclude_path {
                if let (Ok(canonical_path), Ok(canonical_exclude)) =
                    (path.canonicalize(), exclude.canonicalize())
                {
                    if canonical_path == canonical_exclude {
                        println!("Skipping destination directory: {}", path.display());
                        continue;
                    }
                }
            }

            // Recursively process subdirectories
            if let Err(e) = collect_media_files_with_size_progress(
                &path,
                source_root,
                media_files,
                total_size,
                exclude_path,
                show_progress,
            ) {
                eprintln!(
                    "Warning: Cannot access subdirectory '{}': {}",
                    path.display(),
                    e
                );
                // Continue processing other directories
            }
        } else if path.is_file() {
            if let Some(extension) = path.extension() {
                let ext = extension.to_string_lossy().to_lowercase();

                // Check if it's an image or video file
                if is_media_file(&ext) {
                    // Get file size
                    match fs::metadata(&path) {
                        Ok(metadata) => {
                            *total_size += metadata.len();

                            // Calculate relative path from source root
                            let relative_path = path
                                .strip_prefix(source_root)
                                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

                            media_files.push(relative_path.to_path_buf());

                            // Show progress if requested
                            if show_progress {
                                print!("\rFiles found: {}", media_files.len());
                                use std::io::Write;
                                std::io::stdout().flush().unwrap_or(());
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: Cannot get file size for '{}': {}",
                                path.display(),
                                e
                            );
                            // Still add the file to the list even if we can't get its size
                            let relative_path = path
                                .strip_prefix(source_root)
                                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

                            media_files.push(relative_path.to_path_buf());

                            // Show progress if requested
                            if show_progress {
                                print!("\rFiles found: {}", media_files.len());
                                use std::io::Write;
                                std::io::stdout().flush().unwrap_or(());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
