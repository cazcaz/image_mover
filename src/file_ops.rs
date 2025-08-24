//! File operations for copying and deleting media files.
//!
//! This module handles the core file operations including parallel copying,
//! deletion of original files, path validation, and handling file name conflicts.

use rayon::prelude::*;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::directory::{cleanup_empty_directories, create_unique_directory_structure};
use crate::media::{collect_media_files, collect_media_files_with_size_and_progress};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

pub fn validate_folder_paths(source: &PathBuf, destination: &PathBuf) -> io::Result<()> {
    // Canonicalize paths to resolve any symbolic links and get absolute paths
    let canonical_source = match source.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            eprintln!(
                "Warning: Cannot access source folder '{}': {}",
                source.display(),
                e
            );
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Unable to access source folder",
            ));
        }
    };

    let canonical_dest = match destination.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            eprintln!(
                "Warning: Cannot access destination folder '{}': {}",
                destination.display(),
                e
            );
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Unable to access destination folder",
            ));
        }
    };

    // Check if source and destination are the same
    if canonical_source == canonical_dest {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source and destination folders cannot be the same",
        ));
    }

    // Check if source is within destination (would cause infinite recursion)
    if canonical_source.starts_with(&canonical_dest) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Source folder cannot be within the destination folder",
        ));
    }

    // Check if destination is within source - allow this but warn the user
    if canonical_dest.starts_with(&canonical_source) {
        println!("Warning: Destination folder is within the source folder.");
        println!(
            "Files from the destination folder will be skipped to prevent infinite recursion."
        );
    }

    Ok(())
}

/// Get available disk space for a given path in bytes
#[cfg(windows)]
pub fn get_available_disk_space(path: &PathBuf) -> io::Result<u64> {
    // Get the root of the drive for the path
    let root = if let Some(root) = path.ancestors().last() {
        root
    } else {
        path
    };

    // Convert to wide string for Windows API
    let wide_path: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        let mut free_bytes_available = 0u64;
        let result = GetDiskFreeSpaceExW(
            windows::core::PCWSTR::from_raw(wide_path.as_ptr()),
            Some(&mut free_bytes_available),
            None,
            None,
        );

        if result.is_ok() {
            Ok(free_bytes_available)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to get disk space information",
            ))
        }
    }
}

/// Get available disk space for a given path in bytes (non-Windows fallback)
#[cfg(not(windows))]
pub fn get_available_disk_space(_path: &PathBuf) -> io::Result<u64> {
    // For non-Windows platforms, we could use statvfs or similar
    // For now, return an error indicating this feature is Windows-only
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Disk space calculation is only supported on Windows",
    ))
}

/// Calculate total size of all media files in bytes and collect them in one pass
/// Returns a tuple of (media_files, total_size_bytes)
pub fn collect_media_files_and_calculate_size(
    source: &PathBuf,
    exclude_path: Option<&PathBuf>,
) -> io::Result<(Vec<PathBuf>, u64)> {
    let mut media_files = Vec::new();
    let mut total_size = 0u64;

    collect_media_files_with_size_and_progress(
        source,
        source,
        &mut media_files,
        &mut total_size,
        exclude_path,
    )?;

    Ok((media_files, total_size))
}

/// Format bytes into human readable string
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    const THRESHOLD: f64 = 1024.0;

    if bytes == 0 {
        return "0 B".to_string();
    }

    let bytes_f = bytes as f64;
    let unit_index = (bytes_f.log10() / THRESHOLD.log10()).floor() as usize;
    let unit_index = unit_index.min(UNITS.len() - 1);

    if unit_index == 0 {
        format!("{} B", bytes)
    } else {
        let size = bytes_f / THRESHOLD.powi(unit_index as i32);
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

pub fn get_unique_file_path(original_path: &PathBuf) -> io::Result<PathBuf> {
    if !original_path.exists() {
        return Ok(original_path.clone());
    }

    let mut counter = 1;
    let parent = original_path.parent().unwrap_or(original_path);
    let stem = original_path
        .file_stem()
        .unwrap_or(std::ffi::OsStr::new("file"));
    let extension = original_path.extension();

    loop {
        let new_name = if let Some(ext) = extension {
            format!(
                "{}_{}.{}",
                stem.to_string_lossy(),
                counter,
                ext.to_string_lossy()
            )
        } else {
            format!("{}_{}", stem.to_string_lossy(), counter)
        };

        let new_path = parent.join(new_name);

        if !new_path.exists() {
            return Ok(new_path);
        }

        counter += 1;

        // Prevent infinite loops by limiting attempts
        if counter > 10000 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Could not find unique filename after 10000 attempts",
            ));
        }
    }
}

pub fn copy_media_files(
    source: &PathBuf,
    destination: &PathBuf,
    media_files: &Vec<PathBuf>,
) -> io::Result<usize> {
    println!("Scanning for media files...");

    if media_files.is_empty() {
        println!("No media files found in the source directory.");
        return Ok(0);
    }

    // Use atomic counter for thread-safe counting
    let copied_count = Arc::new(AtomicUsize::new(0));

    // Create a custom thread pool to ensure proper cleanup
    let pool = rayon::ThreadPoolBuilder::new().build().map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to create thread pool: {}", e),
        )
    })?;

    // Process files in parallel using the custom thread pool
    let results: Vec<io::Result<()>> = pool.install(|| {
        media_files
            .par_iter()
            .map(|relative_path| {
                let source_file = source.join(relative_path);
                let mut dest_file = destination.join(relative_path);

                // Create destination directory structure if it doesn't exist, handling collisions
                if let Some(dest_dir) = dest_file.parent() {
                    if let Err(e) = create_unique_directory_structure(destination, dest_dir) {
                        eprintln!(
                            "Warning: Cannot create directory structure for '{}': {}",
                            dest_dir.display(),
                            e
                        );
                        return Err(e);
                    }

                    // The directory structure is now created, but we still need to check
                    // if the final file would collide and get a unique name for it
                }

                // Get unique file path to avoid overwriting existing files
                dest_file = match get_unique_file_path(&dest_file) {
                    Ok(path) => path,
                    Err(e) => {
                        eprintln!(
                            "Warning: Cannot determine unique file path for '{}': {}",
                            dest_file.display(),
                            e
                        );
                        return Err(e);
                    }
                };

                // Copy the file
                match fs::copy(&source_file, &dest_file) {
                    Ok(_) => {
                        // Thread-safe increment
                        let count = copied_count.fetch_add(1, Ordering::Relaxed) + 1;
                        println!(
                            "({}/{}) Copied: {} -> {}",
                            count,
                            media_files.len(),
                            source_file.display(),
                            dest_file.display()
                        );
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Cannot copy file '{}' to '{}': {}",
                            source_file.display(),
                            dest_file.display(),
                            e
                        );
                        Err(e)
                    }
                }
            })
            .collect()
    });

    // Check for any errors - but continue if some files failed
    let mut _successful_copies = 0;
    let mut failed_copies = 0;

    for result in results {
        match result {
            Ok(()) => _successful_copies += 1,
            Err(_) => failed_copies += 1,
        }
    }

    if failed_copies > 0 {
        println!(
            "Warning: {} files could not be copied due to access issues",
            failed_copies
        );
    }

    Ok(copied_count.load(Ordering::Relaxed))
}

pub fn delete_original_files(source_path: &PathBuf) -> io::Result<usize> {
    // First, collect all media files again (same as copy operation)
    let mut media_files = Vec::new();
    collect_media_files(source_path, source_path, &mut media_files, None)?;

    if media_files.is_empty() {
        return Ok(0);
    }

    let deleted_count = Arc::new(AtomicUsize::new(0));

    // Delete files in parallel
    let results: Vec<io::Result<()>> = media_files
        .par_iter()
        .map(|relative_path| {
            let file_path = source_path.join(relative_path);

            match fs::remove_file(&file_path) {
                Ok(()) => {
                    let count = deleted_count.fetch_add(1, Ordering::Relaxed) + 1;
                    println!(
                        "({}/{}) Deleted: {}",
                        count,
                        media_files.len(),
                        file_path.display()
                    );
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Warning: Failed to delete '{}': {}", file_path.display(), e);
                    Err(e)
                }
            }
        })
        .collect();

    // Check for any errors - but continue if some files failed
    let mut _successful_deletions = 0;
    let mut failed_deletions = 0;

    for result in results {
        match result {
            Ok(()) => _successful_deletions += 1,
            Err(_) => failed_deletions += 1,
        }
    }

    if failed_deletions > 0 {
        println!(
            "Warning: {} files could not be deleted due to access issues",
            failed_deletions
        );
    }

    // Clean up empty directories
    cleanup_empty_directories(source_path)?;

    Ok(deleted_count.load(Ordering::Relaxed))
}
