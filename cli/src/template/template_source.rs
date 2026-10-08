// Copyright Rob Gage 2026

use super::template_parameters::TemplateParameters;
use std::{
    error::Error,
    fs,
    io,
    path::{
        Component,
        Path,
        PathBuf,
    },
};

static EMBEDDED_TEMPLATE: include_dir::Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../template");

/// Reads, renders, and generates projects from a template source
pub struct TemplateSource {
    /// Optional filesystem root used instead of the embedded snapshot
    root: Option<PathBuf>,
}

impl TemplateSource {

    /// Creates a source that reads the editable repository template when available
    pub fn canonical() -> Self {
        let template_path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("template");
        if template_path.is_dir() {
            Self::filesystem(template_path)
        } else {
            Self::embedded()
        }
    }

    /// Creates a source that reads files from a filesystem directory
    pub fn filesystem(root: PathBuf) -> Self { Self { root: Some(root) } }

    /// Creates a source that reads the packaged template snapshot
    pub const fn embedded() -> Self { Self { root: None } }

    /// Loads template files with their relative paths
    pub fn files(&self) -> Result<Vec<(PathBuf, Vec<u8>)>, Box<dyn Error>> {
        if let Some(root) = &self.root {
            let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
            Self::read_filesystem_directory(root, root, &mut files)?;
            files.sort_by(|left, right| left.0.cmp(&right.0));
            return Ok(files);
        }
        let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        Self::read_embedded_directory(&EMBEDDED_TEMPLATE, &mut files);
        files.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(files)
    }

    /// Renders every canonical text file using the supplied template parameters
    pub fn rendered_files(
        &self,
        parameters: &TemplateParameters,
    ) -> Result<Vec<(PathBuf, Vec<u8>)>, Box<dyn Error>> {
        let files: Vec<(PathBuf, Vec<u8>)> = self.files()?;
        if files.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the canonical template contains no files",
            )
            .into());
        }
        let mut rendered_files: Vec<(PathBuf, Vec<u8>)> = Vec::with_capacity(files.len());
        for file_entry in files {
            let relative_path: PathBuf = file_entry.0;
            let template_contents: Vec<u8> = file_entry.1;
            if relative_path.as_os_str().is_empty()
                || relative_path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsafe template path: {}", relative_path.display()),
                )
                .into());
            }
            let text_file: bool = matches!(
                relative_path.extension().and_then(|extension| extension.to_str()),
                Some("rs" | "toml" | "md" | "txt" | "ron")
            );
            let contents: Vec<u8> = if text_file {
                let text: String = String::from_utf8(template_contents)?;
                let mut rendered: String = String::with_capacity(text.len());
                let mut remaining: &str = &text;
                while let Some(start_index) = remaining.find("{{") {
                    rendered.push_str(&remaining[..start_index]);
                    let expression: &str = &remaining[start_index + 2..];
                    let end_index: usize = expression.find("}}").ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("unclosed template placeholder in {}", relative_path.display()),
                        )
                    })?;
                    let field_name: &str = &expression[..end_index];
                    if field_name.is_empty()
                        || !field_name.chars().all(|character| {
                            character.is_ascii_alphanumeric() || character == '_'
                        })
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "invalid template placeholder '{{{{{field_name}}}}}' in {}",
                                relative_path.display()
                            ),
                        )
                        .into());
                    }
                    let field_value: &String = parameters.values.get(field_name).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!(
                                "unknown or missing template parameter '{field_name}' in {}",
                                relative_path.display()
                            ),
                        )
                    })?;
                    rendered.push_str(field_value);
                    remaining = &expression[end_index + 2..];
                }
                rendered.push_str(remaining);
                rendered.into_bytes()
            } else {
                template_contents
            };
            rendered_files.push((relative_path, contents));
        }
        Ok(rendered_files)
    }

    /// Creates an independent project at a destination that must not already exist
    pub fn generate(
        &self,
        destination: &Path,
        parameters: &TemplateParameters,
    ) -> Result<(), Box<dyn Error>> {
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("destination already exists: {}", destination.display()),
            )
            .into());
        }
        let files: Vec<(PathBuf, Vec<u8>)> = self.rendered_files(parameters)?;
        let destination_parent: &Path = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(destination_parent)?;
        fs::create_dir(destination)?;
        let result: Result<(), Box<dyn Error>> = (|| {
            for file_entry in files {
                let relative_path: PathBuf = file_entry.0;
                let contents: Vec<u8> = file_entry.1;
                let target_path: PathBuf = destination.join(relative_path);
                let parent: &Path = target_path.parent().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "template file has no parent directory",
                    )
                })?;
                fs::create_dir_all(parent)?;
                fs::write(target_path, contents)?;
            }
            Ok(())
        })();
        if result.is_err() {
            fs::remove_dir_all(destination)?;
        }
        result
    }

    /// Appends files found below a filesystem template directory
    fn read_filesystem_directory(
        root: &Path,
        directory: &Path,
        files: &mut Vec<(PathBuf, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        for entry in fs::read_dir(directory)? {
            let entry: fs::DirEntry = entry?;
            let path: PathBuf = entry.path();
            let file_type: fs::FileType = entry.file_type()?;
            if file_type.is_dir() {
                Self::read_filesystem_directory(root, &path, files)?;
            } else if file_type.is_file() {
                let relative_path: PathBuf = path.strip_prefix(root)?.to_path_buf();
                let contents: Vec<u8> = fs::read(path)?;
                files.push((relative_path, contents));
            }
        }
        Ok(())
    }

    /// Appends files found below an embedded template directory
    fn read_embedded_directory(
        directory: &include_dir::Dir<'_>,
        files: &mut Vec<(PathBuf, Vec<u8>)>,
    ) {
        for entry in directory.entries() {
            match entry {
                include_dir::DirEntry::Dir(child_directory) => {
                    Self::read_embedded_directory(child_directory, files);
                }
                include_dir::DirEntry::File(file) => {
                    let relative_path: PathBuf = file.path().to_path_buf();
                    let contents: Vec<u8> = file.contents().to_vec();
                    files.push((relative_path, contents));
                }
            }
        }
    }

}
