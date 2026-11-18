use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use serde::{Serialize, de::DeserializeOwned};
use tempfile::NamedTempFile;

use super::error::{AppError, Result};

pub fn read_document<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match File::open(path) {
        Ok(file) => Ok(serde_json::from_reader(BufReader::new(file))?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error.into()),
    }
}

pub fn write_document<T: Serialize + ?Sized>(
    directory: &Path,
    path: &Path,
    document: &T,
) -> Result<()> {
    fs::create_dir_all(directory)?;
    let mut temporary = NamedTempFile::new_in(directory)?;
    let mut writer = BufWriter::new(temporary.as_file_mut());
    serde_json::to_writer(&mut writer, document)?;
    writer.flush()?;
    drop(writer);
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub fn write_text(path: &Path, text: &str) -> Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| AppError::File("The export needs a destination folder.".into()))?;
    let mut temporary = NamedTempFile::new_in(directory)?;
    temporary.write_all(text.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}
