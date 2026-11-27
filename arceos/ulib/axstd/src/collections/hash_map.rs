use alloc::vec::Vec;
use core::hash::{Hash, Hasher};
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

const BUCKET_COUNT: usize = 0x1000;

struct SimpleHasher {
    state: u64,
}

impl SimpleHasher {
    fn new() -> Self {
        Self { state: FNV_OFFSET }
    }
}

impl Hasher for SimpleHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let byte = byte as u64;
            self.state = (self.state ^ byte).wrapping_mul(FNV_PRIME);
        }
    }
    fn finish(&self) -> u64 {
        self.state
    }
}

pub struct HashMap<K, V> {
    buckets: Vec<Vec<(K, V)>>,
    size: usize,
}

impl<K, V> HashMap<K, V> {
    pub fn new() -> Self {
        let mut buckets = Vec::new();
        for _ in 0..BUCKET_COUNT {
            buckets.push(Vec::new());
        }
        HashMap { buckets, size: 0 }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        K: Hash + Eq,
    {
        let mut hasher = SimpleHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();

        let index = (hash % self.buckets.len() as u64) as usize;
        let bucket = &mut self.buckets[index];
        for (k, v) in bucket.iter_mut() {
            if *k == key {
                let old_v = core::mem::replace(v, value);
                return Some(old_v);
            }
        }
        bucket.push((key, value));
        self.size += 1;
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> + '_ {
        self.buckets
            .iter()
            .flat_map(|bucket| bucket.iter())
            .map(|(key, value)| (key, value))
    }

    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: Hash + Eq,
    {
        let mut hasher = SimpleHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let index = hash as usize % self.buckets.len();
        for (k, v) in self.buckets[index].iter() {
            if k == key {
                return Some(v);
            }
        }
        None
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V>
    where
        K: Hash + Eq,
    {
        let mut hasher = SimpleHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let index = hash as usize % self.buckets.len();
        for (k, v) in self.buckets[index].iter_mut() {
            if *k == *key {
                return Some(v);
            }
        }
        None
    }

    pub fn remove(&mut self, key: &K) -> Option<V>
    where
        K: Hash + Eq,
    {
        let mut hasher = SimpleHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let index = hash as usize % self.buckets.len();
        let mut idx = 0;
        let idx = self.buckets[index].iter().position(|(k, _)| k == key)?;

        let (old_k, old_v) = self.buckets[index].remove(idx);
        self.size -= 1;
        Some(old_v)
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}
