use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub struct ZipService;

impl ZipService {
    /// Exports a project directory into an Overleaf-compatible .zip archive.
    pub fn export_project(project_dir: &Path, output_zip_path: &Path) -> std::io::Result<()> {
        let file = File::create(output_zip_path)?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for entry in WalkDir::new(project_dir) {
            let entry = entry?;
            let path = entry.path();
            let rel_path = match path.strip_prefix(project_dir) {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Skip hidden build files and output caches
            let rel_str = rel_path.to_string_lossy();
            if rel_str.starts_with(".build") || rel_str.starts_with(".git") || rel_str.is_empty() {
                continue;
            }

            if path.is_file() {
                zip.start_file(rel_str.as_ref(), options)?;
                let mut f = File::open(path)?;
                let mut buffer = Vec::new();
                f.read_to_end(&mut buffer)?;
                zip.write_all(&buffer)?;
            } else if path.is_dir() {
                zip.add_directory(rel_str.as_ref(), options)?;
            }
        }

        zip.finish()?;
        Ok(())
    }

    /// Imports an Overleaf or external project .zip archive safely, protecting against zip slip attacks.
    pub fn import_project(zip_path: &Path, target_dir: &Path) -> Result<PathBuf, std::io::Error> {
        let file = File::open(zip_path)?;
        let mut archive = ZipArchive::new(file)?;
        std::fs::create_dir_all(target_dir)?;

        let canonical_target = target_dir.canonicalize()?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let enclosed = match file.enclosed_name() {
                Some(p) => p.to_owned(),
                None => {
                    // Path contains '..' or absolute components: skip for safety
                    continue;
                }
            };

            let out_path = canonical_target.join(&enclosed);

            // Double check that out_path starts with canonical_target (anti zip-slip)
            if !out_path.starts_with(&canonical_target) {
                continue;
            }

            if file.is_dir() {
                std::fs::create_dir_all(&out_path)?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut outfile = File::create(&out_path)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        Ok(canonical_target)
    }
}
