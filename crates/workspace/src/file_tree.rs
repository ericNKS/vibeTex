use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    Latex,
    Bibtex,
    Image,
    Pdf,
    Style,
    Class,
    Other,
}

impl FileKind {
    pub fn from_path(path: &Path) -> Self {
        if path.is_dir() {
            return Self::Directory;
        }
        match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "tex" | "ltx" => Self::Latex,
            "bib" => Self::Bibtex,
            "png" | "jpg" | "jpeg" | "pdf" | "svg" | "eps" => Self::Image,
            "sty" => Self::Style,
            "cls" => Self::Class,
            _ => Self::Other,
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::Directory => "folder-symbolic",
            Self::Latex => "text-x-generic-symbolic",
            Self::Bibtex => "x-office-address-book-symbolic",
            Self::Image => "image-x-generic-symbolic",
            Self::Pdf => "application-pdf-symbolic",
            Self::Style | Self::Class => "system-run-symbolic",
            Self::Other => "text-plain-symbolic",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub rel_path: PathBuf,
    pub name: String,
    pub kind: FileKind,
    pub children: Vec<FileItem>,
}

pub struct FileTreeScanner;

impl FileTreeScanner {
    pub fn scan_project(root: &Path) -> Vec<FileItem> {
        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(root) {
            let mut entries_vec: Vec<_> = entries.flatten().collect();
            entries_vec.sort_by_key(|e| e.file_name());

            for entry in entries_vec {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden files and build output folders
                if name.starts_with('.') || name == "target" || name == ".build" {
                    continue;
                }

                let kind = FileKind::from_path(&path);
                let rel_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

                let children = if path.is_dir() {
                    Self::scan_project(&path)
                } else {
                    Vec::new()
                };

                items.push(FileItem {
                    path,
                    rel_path,
                    name,
                    kind,
                    children,
                });
            }
        }

        // Sort: directories first, then alphabetically
        items.sort_by(|a, b| {
            let a_is_dir = a.kind == FileKind::Directory;
            let b_is_dir = b.kind == FileKind::Directory;
            if a_is_dir != b_is_dir {
                b_is_dir.cmp(&a_is_dir)
            } else {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            }
        });

        items
    }

    pub fn scan_flat_files(root: &Path) -> Vec<FileItem> {
        let mut items = Vec::new();
        Self::collect_files_recursive(root, root, &mut items);
        items.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        items
    }

    fn collect_files_recursive(root: &Path, current: &Path, out: &mut Vec<FileItem>) {
        if let Ok(entries) = std::fs::read_dir(current) {
            let mut entries_vec: Vec<_> = entries.flatten().collect();
            entries_vec.sort_by_key(|e| e.file_name());

            for entry in entries_vec {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if name.starts_with('.') || name == "target" || name == ".build" {
                    continue;
                }

                let kind = FileKind::from_path(&path);
                let rel_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

                if path.is_dir() {
                    Self::collect_files_recursive(root, &path, out);
                } else {
                    out.push(FileItem {
                        path,
                        rel_path,
                        name,
                        kind,
                        children: Vec::new(),
                    });
                }
            }
        }
    }
}
