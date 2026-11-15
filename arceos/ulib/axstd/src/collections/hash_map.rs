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
}
