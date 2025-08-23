use rayon::prelude::*;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use windows::{
    core::*, Win32::Foundation::*, Win32::System::Com::*, Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

fn main() -> Result<()> {
    // Initialize COM
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }

    // Bring up a folder selector to choose where to copy files from
    println!("Select source folder:");
    if let Some(source_path) = select_folder("Select Source Folder")? {
        if let Some(dest_path) = select_folder("Select Destination Folder")? {
            println!("Source: {:?}", source_path);
            println!("Destination: {:?}", dest_path);

            // Check for invalid folder relationships
            if let Err(e) = validate_folder_paths(&source_path, &dest_path) {
                eprintln!("Error: {}", e);
                return Ok(());
            }

            println!("Copying image and video files...");
            match copy_media_files(&source_path, &dest_path) {
                Ok(count) => {
                    println!("Successfully copied {} files!", count);

                    // Ask user if they want to delete original files
                    if count > 0 {
                        match show_deletion_prompt(count) {
                            Ok(true) => {
                                println!("Deleting original files...");
                                match delete_original_files(&source_path) {
                                    Ok(deleted_count) => {
                                        println!(
                                            "Successfully deleted {} original files!",
                                            deleted_count
                                        );
                                    }
                                    Err(e) => eprintln!("Error deleting original files: {}", e),
                                }
                            }
                            Ok(false) => println!("Original files kept as requested."),
                            Err(e) => eprintln!("Error showing deletion prompt: {}", e),
                        }
                    }
                }
                Err(e) => eprintln!("Error copying files: {}", e),
            }
        } else {
            println!("No destination selected.");
        }
    } else {
        println!("No source selected.");
    }

    // Cleanup COM
    unsafe {
        CoUninitialize();
    }

    Ok(())
}

fn select_folder(title: &str) -> Result<Option<PathBuf>> {
    unsafe {
        // Create the file dialog
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL)?;

        // Set dialog options to select folders only
        let options = FOS_PICKFOLDERS | FOS_PATHMUSTEXIST;
        dialog.SetOptions(options)?;

        // Set the title
        let title_wide = HSTRING::from(title);
        dialog.SetTitle(&title_wide)?;

        // Show the dialog
        match dialog.Show(None) {
            Ok(()) => {
                // Get the selected folder
                let item = dialog.GetResult()?;
                let path = item.GetDisplayName(SIGDN_FILESYSPATH)?;

                // Convert to Rust PathBuf
                let path_str = path.to_string()?;
                Ok(Some(PathBuf::from(path_str)))
            }
            Err(err) if err.code() == E_ABORT => {
                // User cancelled the dialog
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }
}

fn validate_folder_paths(source: &PathBuf, destination: &PathBuf) -> io::Result<()> {
    // Canonicalize paths to resolve any symbolic links and get absolute paths
    let canonical_source = source
        .canonicalize()
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "Unable to access source folder"))?;
    let canonical_dest = destination.canonicalize().map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Unable to access destination folder",
        )
    })?;

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

fn show_deletion_prompt(file_count: usize) -> Result<bool> {
    unsafe {
        let title = HSTRING::from("Delete Original Files");
        let message = HSTRING::from(&format!(
            "All {} files have been successfully copied to the destination folder.\n\nWould you like to delete the original files from the source folder?\n\nWarning: This action cannot be undone!",
            file_count
        ));

        let result = MessageBoxW(
            None,
            &message,
            &title,
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2, // Default to "No" for safety
        );

        Ok(result == IDYES)
    }
}

fn delete_original_files(source_path: &PathBuf) -> io::Result<usize> {
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
                    eprintln!("Failed to delete {}: {}", file_path.display(), e);
                    Err(e)
                }
            }
        })
        .collect();

    // Check for any errors
    for result in results {
        result?;
    }

    // Clean up empty directories
    cleanup_empty_directories(source_path)?;

    Ok(deleted_count.load(Ordering::Relaxed))
}

fn cleanup_empty_directories(source_path: &PathBuf) -> io::Result<()> {
    // Get all directories in reverse order (deepest first)
    let mut directories = Vec::new();
    collect_directories(source_path, &mut directories)?;
    directories.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

    for dir in directories {
        // Skip the root source directory
        if dir == *source_path {
            continue;
        }

        // Try to remove directory if it's empty
        match fs::remove_dir(&dir) {
            Ok(()) => println!("Removed empty directory: {}", dir.display()),
            Err(e) if e.kind() == io::ErrorKind::Other => {
                // Directory not empty or other non-critical error, continue
            }
            Err(_) => {
                // Other errors, continue without failing
            }
        }
    }

    Ok(())
}

fn collect_directories(current_dir: &PathBuf, directories: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = fs::read_dir(current_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            directories.push(path.clone());
            collect_directories(&path, directories)?;
        }
    }

    Ok(())
}

fn get_unique_file_path(original_path: &PathBuf) -> io::Result<PathBuf> {
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

fn create_unique_directory_structure(dest_root: &PathBuf, target_dir: &Path) -> io::Result<()> {
    // If target directory doesn't exist, create it normally
    if !target_dir.exists() {
        return fs::create_dir_all(target_dir);
    }

    // If it exists, we need to create the path with potential renames
    let relative_path = target_dir
        .strip_prefix(dest_root)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid path relationship"))?;

    let mut current_path = dest_root.clone();

    // Build the path component by component, handling collisions
    for component in relative_path.components() {
        if let std::path::Component::Normal(name) = component {
            let next_path = current_path.join(name);

            if next_path.exists() {
                // Directory already exists, continue with existing one
                current_path = next_path;
            } else {
                // Create the directory
                fs::create_dir(&next_path)?;
                current_path = next_path;
            }
        }
    }

    Ok(())
}

fn copy_media_files(source: &PathBuf, destination: &PathBuf) -> io::Result<usize> {
    println!("Scanning for media files...");

    // First, collect all media files to be copied
    let mut media_files = Vec::new();
    collect_media_files(source, source, &mut media_files, Some(destination))?;

    if media_files.is_empty() {
        println!("No media files found in the source directory.");
        return Ok(0);
    }

    println!(
        "Found {} media files. Starting parallel copy...",
        media_files.len()
    );

    // Use atomic counter for thread-safe counting
    let copied_count = Arc::new(AtomicUsize::new(0));

    // Process files in parallel
    let results: Vec<io::Result<()>> = media_files
        .par_iter()
        .map(|relative_path| {
            let source_file = source.join(relative_path);
            let mut dest_file = destination.join(relative_path);

            // Create destination directory structure if it doesn't exist, handling collisions
            if let Some(dest_dir) = dest_file.parent() {
                create_unique_directory_structure(destination, dest_dir)?;

                // The directory structure is now created, but we still need to check
                // if the final file would collide and get a unique name for it
            }

            // Get unique file path to avoid overwriting existing files
            dest_file = get_unique_file_path(&dest_file)?;

            // Copy the file
            fs::copy(&source_file, &dest_file)?;

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
        })
        .collect();

    // Check for any errors
    for result in results {
        result?;
    }

    Ok(copied_count.load(Ordering::Relaxed))
}

fn collect_media_files(
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

fn is_media_file(extension: &str) -> bool {
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
