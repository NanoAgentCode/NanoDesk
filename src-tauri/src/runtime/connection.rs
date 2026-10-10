use super::*;

impl RuntimeStore {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init()?;
        store.recover_interrupted_work()?;
        Ok(store)
    }
}
