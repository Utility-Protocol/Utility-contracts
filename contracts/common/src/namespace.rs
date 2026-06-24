use alloc::vec::Vec;

pub trait StorageNamespace {
    const PREFIX: [u8; 4];

    fn scoped_key(&self, raw: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(4 + raw.len());
        key.extend_from_slice(&Self::PREFIX);
        key.extend_from_slice(raw);
        key
    }
}
