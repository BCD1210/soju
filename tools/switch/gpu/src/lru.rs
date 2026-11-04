//! Byte-accounted CPU caches. Captured command data owns its own copies.
use std::collections::BTreeMap;
pub(crate) struct Lru<K, V> {
    entries: BTreeMap<K, (u64, usize, V)>,
    clock: u64,
    bytes: usize,
    budget: usize,
}
impl<K: Ord + Clone, V> Default for Lru<K, V> {
    fn default() -> Self {
        Self::new(
            std::env::var("SOJU_GPU_CPU_CACHE_BYTES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(16 * 1024 * 1024),
        )
    }
}
impl<K: Ord + Clone, V> Lru<K, V> {
    fn new(budget: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            clock: 0,
            bytes: 0,
            budget,
        }
    }
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.clock += 1;
        self.entries.get_mut(key).map(|entry| {
            entry.0 = self.clock;
            &entry.2
        })
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        self.clock += 1;
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.1;
        }
        if bytes > self.budget {
            return;
        }
        while self.bytes > self.budget - bytes {
            let key = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.0)
                .unwrap()
                .0
                .clone();
            self.bytes -= self.entries.remove(&key).unwrap().1;
        }
        self.entries.insert(key, (self.clock, bytes, value));
        self.bytes += bytes;
    }
}
#[cfg(test)]
mod tests {
    use super::Lru;
    #[test]
    fn recent_entries_survive_and_oversize_entries_do_not_grow_cache() {
        let mut cache = Lru::new(12);
        cache.insert(1, "one", 6);
        cache.insert(2, "two", 6);
        assert_eq!(cache.get(&1), Some(&"one"));
        cache.insert(3, "three", 6);
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&1), Some(&"one"));
        cache.insert(4, "oversize", 13);
        assert_eq!(cache.get(&4), None);
        assert_eq!(cache.bytes, 12);
        cache.insert(1, "replaced", 3);
        assert_eq!(cache.bytes, 9);
        cache.clear();
        assert_eq!(cache.bytes, 0);
    }
}
