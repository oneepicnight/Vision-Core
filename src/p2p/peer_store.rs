use anyhow::Result;
use std::collections::HashSet;

/// Persistent peer address store backed by a newline-delimited text file.
///
/// Addresses are stored as `"host:port"` strings. The file is loaded at startup
/// and written after every meaningful mutation. Duplicate addresses are silently
/// de-duplicated; blank lines and leading/trailing whitespace are ignored.
pub struct PeerStore {
    path: String,
    known: HashSet<String>,
}

impl PeerStore {
    /// Open the peer store at `path`, creating the file if it does not exist.
    pub fn open(path: &str) -> Result<Self> {
        let known = if let Ok(content) = std::fs::read_to_string(path) {
            content
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        } else {
            HashSet::new()
        };
        Ok(Self {
            path: path.to_string(),
            known,
        })
    }

    /// Return the number of known peer addresses.
    pub fn len(&self) -> usize {
        self.known.len()
    }

    /// `true` when the store is empty.
    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }

    /// Add an address.  No-op if already present.  Persists on insert.
    pub fn add(&mut self, addr: &str) -> Result<()> {
        if self.known.insert(addr.to_string()) {
            self.persist()?;
        }
        Ok(())
    }

    /// Remove an address.  No-op if not present.  Persists on removal.
    pub fn remove(&mut self, addr: &str) -> Result<()> {
        if self.known.remove(addr) {
            self.persist()?;
        }
        Ok(())
    }

    /// Check membership without modifying state.
    pub fn contains(&self, addr: &str) -> bool {
        self.known.contains(addr)
    }

    /// Return all known peer addresses in sorted order (deterministic).
    pub fn all(&self) -> Vec<String> {
        let mut v: Vec<String> = self.known.iter().cloned().collect();
        v.sort();
        v
    }

    /// Merge a slice of addresses (e.g. received from peer exchange).
    ///
    /// Returns the number of **new** addresses that were inserted.
    /// Only persists if at least one new address was added.
    pub fn merge(&mut self, addrs: &[String]) -> Result<usize> {
        let before = self.known.len();
        for addr in addrs {
            self.known.insert(addr.clone());
        }
        let added = self.known.len() - before;
        if added > 0 {
            self.persist()?;
        }
        Ok(added)
    }

    // ── Persistence ───────────────────────────────────────────────────────────

    fn persist(&self) -> Result<()> {
        let mut sorted: Vec<&str> = self.known.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        let content = sorted.join("\n");
        std::fs::write(&self.path, content)?;
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a PeerStore backed by a temporary file that is deleted on drop.
    fn temp_store() -> (PeerStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("peers.txt");
        let store = PeerStore::open(path.to_str().unwrap()).unwrap();
        (store, dir)
    }

    // ── open ──────────────────────────────────────────────────────────────────

    #[test]
    fn open_creates_empty_store_when_file_absent() {
        let (_store, _dir) = temp_store();
        // Just checking open() doesn't panic when file doesn't exist yet.
    }

    #[test]
    fn open_loads_existing_addresses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("peers.txt");
        std::fs::write(&path, "1.2.3.4:9000\n5.6.7.8:9000\n").unwrap();
        let store = PeerStore::open(path.to_str().unwrap()).unwrap();
        assert_eq!(store.len(), 2);
        assert!(store.contains("1.2.3.4:9000"));
        assert!(store.contains("5.6.7.8:9000"));
    }

    #[test]
    fn open_ignores_blank_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("peers.txt");
        std::fs::write(&path, "\n1.2.3.4:9000\n\n").unwrap();
        let store = PeerStore::open(path.to_str().unwrap()).unwrap();
        assert_eq!(store.len(), 1);
    }

    // ── add ───────────────────────────────────────────────────────────────────

    #[test]
    fn add_inserts_address() {
        let (mut store, _dir) = temp_store();
        store.add("10.0.0.1:9000").unwrap();
        assert!(store.contains("10.0.0.1:9000"));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn add_ignores_duplicate() {
        let (mut store, _dir) = temp_store();
        store.add("10.0.0.1:9000").unwrap();
        store.add("10.0.0.1:9000").unwrap();
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn add_persists_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("peers.txt");
        {
            let mut store = PeerStore::open(path.to_str().unwrap()).unwrap();
            store.add("10.0.0.1:9000").unwrap();
        }
        // Re-open and verify the address survived.
        let store2 = PeerStore::open(path.to_str().unwrap()).unwrap();
        assert!(store2.contains("10.0.0.1:9000"));
    }

    // ── remove ────────────────────────────────────────────────────────────────

    #[test]
    fn remove_deletes_address() {
        let (mut store, _dir) = temp_store();
        store.add("10.0.0.1:9000").unwrap();
        store.remove("10.0.0.1:9000").unwrap();
        assert!(!store.contains("10.0.0.1:9000"));
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let (mut store, _dir) = temp_store();
        store.remove("9.9.9.9:9000").unwrap();
        assert_eq!(store.len(), 0);
    }

    // ── all ───────────────────────────────────────────────────────────────────

    #[test]
    fn all_returns_sorted_list() {
        let (mut store, _dir) = temp_store();
        store.add("bbb:9000").unwrap();
        store.add("aaa:9000").unwrap();
        store.add("ccc:9000").unwrap();
        let list = store.all();
        assert_eq!(list, vec!["aaa:9000", "bbb:9000", "ccc:9000"]);
    }

    // ── merge ─────────────────────────────────────────────────────────────────

    #[test]
    fn merge_returns_count_of_new_addresses() {
        let (mut store, _dir) = temp_store();
        store.add("existing:9000").unwrap();
        let added = store
            .merge(&[
                "existing:9000".to_string(),
                "new1:9000".to_string(),
                "new2:9000".to_string(),
            ])
            .unwrap();
        assert_eq!(added, 2);
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn merge_empty_slice_returns_zero() {
        let (mut store, _dir) = temp_store();
        let added = store.merge(&[]).unwrap();
        assert_eq!(added, 0);
    }
}
