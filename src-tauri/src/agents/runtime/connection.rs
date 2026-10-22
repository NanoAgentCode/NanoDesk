use super::*;

impl RuntimeStore {
    /// Attach another frontend without treating the live owner's work as crashed.
    pub(crate) fn connect(path: PathBuf) -> AppResult<Self> {
        let store = Self { conn: Connection::open(path)? };
        store.init()?;
        Ok(store)
    }

    pub fn open(path: PathBuf) -> AppResult<Self> {
        let store = Self::connect(path)?;
        store.recover_interrupted_work()?;
        Ok(store)
    }
}
