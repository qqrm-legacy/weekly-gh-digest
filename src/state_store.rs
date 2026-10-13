use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::{fs, io::Write, path::Path};

pub struct StateStore<'a> {
    path: &'a Path,
}

impl<'a> StateStore<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self { path }
    }

    pub fn load<T: DeserializeOwned + Default>(&self) -> Result<T> {
        match fs::read(self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .context("stored state is invalid; refusing empty dedup memory"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(error) => {
                Err(error).context("stored state cannot be read; refusing empty dedup memory")
            }
        }
    }

    pub fn save(&self, value: &impl Serialize) -> Result<()> {
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec_pretty(value)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    type Data = BTreeMap<String, i64>;

    #[test]
    fn missing_state_bootstraps_but_corruption_and_io_errors_fail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let store = StateStore::new(&path);
        assert!(store.load::<Data>().unwrap().is_empty());
        fs::write(&path, b"broken").unwrap();
        assert!(store.load::<Data>().is_err());
        assert!(StateStore::new(dir.path()).load::<Data>().is_err());
    }

    #[test]
    fn atomic_save_replaces_an_existing_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/state.json");
        let store = StateStore::new(&path);
        store.save(&Data::from([("sent".into(), 1)])).unwrap();
        store.save(&Data::from([("sent".into(), 2)])).unwrap();
        assert_eq!(store.load::<Data>().unwrap()["sent"], 2);
    }

    #[test]
    fn serialization_failure_preserves_previous_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let store = StateStore::new(&path);
        store.save(&Data::from([("sent".into(), 1)])).unwrap();
        let invalid_keys = BTreeMap::from([(vec![1, 2], "invalid JSON key")]);
        assert!(store.save(&invalid_keys).is_err());
        assert_eq!(store.load::<Data>().unwrap()["sent"], 1);
    }
}
