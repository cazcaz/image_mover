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
    let entries = fs::read_dir(current_dir)?;

    for entry in entries {
        let entry = entry?;
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
            collect_media_files(&path, source_root, media_files, exclude_path)?;
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

pub fn is_media_file(extension: &str) -> bool {
    match extension {
        // Image formats
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "tif" | "webp" | "svg" | "ico"
        | "heic" | "heif" | "raw" | "cr2" | "nef" | "arw" | "dng" | "orf" | "rw2" => true,

        // Video formats
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "3gp" | "3g2" | "f4v"
        | "asf" | "rm" | "rmvb" | "vob" | "ogv" | "drc" | "mng" | "qt" | "yuv" | "m2v" | "m4p"
        | "mpg" | "mp2" | "mpeg" | "mpe" | "mpv" | "m2ts" | "mts" | "ts" => true,

        _ => false,
    }
}
