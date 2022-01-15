use std::collections::HashMap;
use std::path::{PathBuf, Path};

use actix_multipart::Multipart;
use anyhow::{anyhow, Context, Result};
use futures_util::TryStreamExt;

use financial_planning_lib::input::{Config, FileLoader};
use financial_planning_lib::model::ModelReport;

pub async fn extract_files(mut payload: Multipart) -> Result<HashMap<PathBuf, String>> {
    let mut files: HashMap<PathBuf, String> = HashMap::new();

    while let Some(mut field) = payload.try_next().await? {
        let content_disposition = field
            .content_disposition()
            .ok_or_else(|| anyhow!("field was missing content disposition"))?;

        let filename = match content_disposition.get_filename() {
            Some(name) => name,
            None => {
                return Err(anyhow!("Filenames must be provided"));
            }
        };

        let mut bytes = Vec::new();
        while let Some(chunk) = field.try_next().await? {
            bytes.extend(chunk);
        }
        let content = match std::str::from_utf8(&bytes) {
            Ok(content) => content,
            Err(_) => {
                return Err(anyhow!("File {} contained invalid utf-8", filename));
            }
        };

        files.insert(Path::new("./").join(filename), content.to_string());
    }

    Ok(files)
}

pub fn run(config: Config) -> Result<ModelReport> {
    let (range, mut model) = config
        .build_model()
        .context("Failed to build model from configs")?;
    model.run(range.clone()).context("failed to run model")
}

pub struct MapFileLoader {
    files: HashMap<PathBuf, String>,
}

impl MapFileLoader {
    pub fn new(files: HashMap<PathBuf, String>) -> Self {
        Self { files }
    }
}

impl FileLoader for MapFileLoader {
    fn load(&self, path: &Path) -> Result<String> {
        match self.files.get(path) {
            Some(content) => Ok(content.to_string()),
            None => Err(anyhow!("No file called {:?} found", path))
        }
    }
}
