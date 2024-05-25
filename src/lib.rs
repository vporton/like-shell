use std::env::set_current_dir;

use fs_extra::dir::CopyOptions;
use tempdir::TempDir;

/// Create a temporary directory filled with a copy of `source_dir`.
pub fn temp_dir_from_template(source_dir: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new("test")?;
    let options = CopyOptions::new();
    fs_extra::dir::copy(source_dir, temp_dir.path(), &options)?;
    set_current_dir(temp_dir.path())?;
    Ok(temp_dir)
}

