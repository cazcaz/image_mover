use windows::{
    core::*, Win32::System::Com::*,
};

mod dialogs;
mod file_ops;
mod media;
mod directory;

use dialogs::{select_folder, show_deletion_prompt};
use file_ops::{validate_folder_paths, copy_media_files, delete_original_files};

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
