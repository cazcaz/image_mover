use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn cleanup_empty_directories(source_path: &PathBuf) -> io::Result<()> {
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

pub fn collect_directories(current_dir: &PathBuf, directories: &mut Vec<PathBuf>) -> io::Result<()> {
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

pub fn create_unique_directory_structure(dest_root: &PathBuf, target_dir: &Path) -> io::Result<()> {
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
