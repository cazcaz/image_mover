use windows::{core::*, Win32::System::Com::*};

mod dialogs;
mod directory;
mod file_ops;
mod media;

use dialogs::{
    select_folder, show_completion_dialog, show_copy_confirmation_dialog, show_deletion_prompt,
};
use file_ops::{
    collect_media_files_and_calculate_size, copy_media_files, delete_original_files, format_bytes,
    get_available_disk_space, validate_folder_paths,
};

fn main() -> Result<()> {
    run_with_com_initialization()
}

fn run_with_com_initialization() -> Result<()> {
    // Initialize COM
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }

    // Ensure COM is cleaned up even if we return early
    let result = run_image_mover();

    // Cleanup COM
    unsafe {
        CoUninitialize();
    }

    result
}

fn run_image_mover() -> Result<()> {
    // Bring up a folder selector to choose where to copy files from
    println!("Select source folder:");

    let source_path = match select_folder("Select Source Folder")? {
        Some(path) => path,
        None => {
            println!("No source selected.");
            return Ok(());
        }
    };

    let dest_path = match select_folder("Select Destination Folder")? {
        Some(path) => path,
        None => {
            println!("No destination selected.");
            return Ok(());
        }
    };

    println!("Source: {:?}", source_path);
    println!("Destination: {:?}", dest_path);

    // Check for invalid folder relationships
    if let Err(e) = validate_folder_paths(&source_path, &dest_path) {
        eprintln!("Error: {}", e);
        return Ok(());
    }

    // Calculate total size and collect media files in one pass
    println!("Scanning media files and calculating total size...");
    let (media_files, total_size) =
        match collect_media_files_and_calculate_size(&source_path, Some(&dest_path)) {
            Ok((files, size)) => (files, size),
            Err(e) => {
                eprintln!("Error scanning files and calculating size: {}", e);
                return Ok(());
            }
        };

    if total_size == 0 || media_files.is_empty() {
        println!("No media files found in the source directory.");
        return Ok(());
    }

    let file_count = media_files.len();

    // Get available disk space on destination drive
    let available_space = match get_available_disk_space(&dest_path) {
        Ok(space) => space,
        Err(e) => {
            eprintln!("Warning: Could not determine available disk space: {}", e);
            // Continue with operation but warn user
            u64::MAX // Set to max so we don't show space warning
        }
    };

    // Format the sizes for display
    let formatted_total_size = format_bytes(total_size);
    let formatted_available_space = format_bytes(available_space);

    // Show confirmation dialog with size and space information
    let should_proceed = match show_copy_confirmation_dialog(
        file_count,
        total_size,
        available_space,
        &formatted_total_size,
        &formatted_available_space,
    ) {
        Ok(proceed) => proceed,
        Err(e) => {
            eprintln!("Error showing confirmation dialog: {}", e);
            return Ok(());
        }
    };

    if !should_proceed {
        println!("Copy operation cancelled by user.");
        return Ok(());
    }

    let count = match copy_media_files(&source_path, &dest_path, &media_files) {
        Ok(count) => count,
        Err(e) => {
            eprintln!("Error copying files: {}", e);
            return Ok(());
        }
    };

    println!("Successfully copied {} files!", count);

    // Ask user if they want to delete original files
    if count == 0 {
        return Ok(());
    }

    let should_delete = match show_deletion_prompt(count) {
        Ok(delete) => delete,
        Err(e) => {
            eprintln!("Error showing deletion prompt: {}", e);
            return Ok(());
        }
    };

    if !should_delete {
        println!("Original files kept as requested.");
        // Show completion dialog
        if let Err(e) = show_completion_dialog() {
            eprintln!("Error showing completion dialog: {}", e);
        }
        return Ok(());
    }

    println!("Deleting original files...");
    match delete_original_files(&source_path) {
        Ok(deleted_count) => {
            println!("Successfully deleted {} original files!", deleted_count);
        }
        Err(e) => {
            eprintln!("Error deleting original files: {}", e);
        }
    }

    // Show completion dialog
    if let Err(e) = show_completion_dialog() {
        eprintln!("Error showing completion dialog: {}", e);
    }

    Ok(())
}
