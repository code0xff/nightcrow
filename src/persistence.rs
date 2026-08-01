use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io::{BufReader, Write};
use std::path::Path;

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("opening {}", path.display())),
    };
    serde_json::from_reader(BufReader::new(file))
        .map(Some)
        .with_context(|| format!("parsing {}", path.display()))
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let mut pending = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating a temporary file in {}", parent.display()))?;
    serde_json::to_writer(pending.as_file_mut(), value)
        .with_context(|| format!("serializing {}", path.display()))?;
    pending
        .as_file_mut()
        .flush()
        .with_context(|| format!("flushing {}", path.display()))?;
    pending
        .persist(path)
        .map_err(|err| err.error)
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_json_replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        write_json(&path, &vec!["old"]).unwrap();
        write_json(&path, &vec!["new"]).unwrap();

        assert_eq!(
            read_json::<Vec<String>>(&path).unwrap(),
            Some(vec!["new".into()])
        );
    }
}
