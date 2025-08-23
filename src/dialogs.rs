//! Windows dialog functionality for the image mover application.
//!
//! This module provides functions for displaying Windows native dialogs,
//! including folder selection and user confirmation dialogs.

use std::path::PathBuf;
use windows::{
    core::*, Win32::Foundation::*, Win32::System::Com::*, Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

pub fn select_folder(title: &str) -> Result<Option<PathBuf>> {
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

pub fn show_deletion_prompt(file_count: usize) -> Result<bool> {
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

pub fn show_completion_dialog() -> Result<()> {
    unsafe {
        let title = HSTRING::from("Process Complete");
        let message = HSTRING::from("Done! All operations completed successfully.");

        MessageBoxW(None, &message, &title, MB_OK | MB_ICONINFORMATION);

        Ok(())
    }
}

pub fn show_copy_confirmation_dialog(
    file_count: usize,
    total_size: u64,
    available_space: u64,
    formatted_total_size: &str,
    formatted_available_space: &str,
) -> Result<bool> {
    unsafe {
        let title = HSTRING::from("Confirm Copy Operation");

        let space_warning = if total_size > available_space {
            "\n\n⚠️  WARNING: Not enough disk space available!"
        } else {
            ""
        };

        let message = HSTRING::from(&format!(
            "Ready to copy {} media files\n\nTotal size to copy: {}\nAvailable space on destination: {}{}\n\nDo you want to proceed with the copy operation?",
            file_count,
            formatted_total_size,
            formatted_available_space,
            space_warning
        ));

        let result = MessageBoxW(
            None,
            &message,
            &title,
            MB_YESNO
                | MB_ICONQUESTION
                | if total_size > available_space {
                    MB_ICONWARNING
                } else {
                    MB_ICONQUESTION
                },
        );

        Ok(result == IDYES)
    }
}
