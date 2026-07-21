use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::{Arc, Mutex, OnceLock},
};

use sha2::{Digest, Sha256};

const FONT_DIGEST_CACHE_CAPACITY: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontBlobDigest([u8; 32]);

impl FontBlobDigest {
    pub fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn to_hex(self) -> String {
        format!("{self}")
    }
}

impl fmt::Debug for FontBlobDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for FontBlobDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontFaceKey {
    blob_id: u64,
    blob_len: usize,
    face_index: u32,
}

impl FontFaceKey {
    pub fn new(blob_id: u64, blob_len: usize, face_index: u32) -> Self {
        Self {
            blob_id,
            blob_len,
            face_index,
        }
    }

    pub fn blob_id(self) -> u64 {
        self.blob_id
    }

    pub fn blob_len(self) -> usize {
        self.blob_len
    }

    pub fn face_index(self) -> u32 {
        self.face_index
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontVariationSettingKey {
    tag: [u8; 4],
    value_bits: u32,
}

impl FontVariationSettingKey {
    pub fn new(tag: [u8; 4], value: f32) -> Self {
        Self {
            tag,
            value_bits: value.to_bits(),
        }
    }

    pub fn tag(self) -> [u8; 4] {
        self.tag
    }

    pub fn value(self) -> f32 {
        f32::from_bits(self.value_bits)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontSynthesisKey {
    variation_settings: Arc<[FontVariationSettingKey]>,
    embolden: bool,
    skew_bits: Option<u32>,
}

impl FontSynthesisKey {
    pub fn new(
        variation_settings: Vec<FontVariationSettingKey>,
        embolden: bool,
        skew: Option<f32>,
    ) -> Self {
        Self {
            variation_settings: variation_settings.into(),
            embolden,
            skew_bits: skew.map(f32::to_bits),
        }
    }

    pub fn variation_settings(&self) -> &[FontVariationSettingKey] {
        &self.variation_settings
    }

    pub fn embolden(&self) -> bool {
        self.embolden
    }

    pub fn skew(&self) -> Option<f32> {
        self.skew_bits.map(f32::from_bits)
    }

    pub fn any(&self) -> bool {
        !self.variation_settings.is_empty() || self.embolden || self.skew_bits.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontInstanceKey {
    face: FontFaceKey,
    normalized_coords: Arc<[i16]>,
    synthesis: FontSynthesisKey,
}

impl FontInstanceKey {
    pub fn new(
        face: FontFaceKey,
        normalized_coords: Vec<i16>,
        synthesis: FontSynthesisKey,
    ) -> Self {
        Self {
            face,
            normalized_coords: normalized_coords.into(),
            synthesis,
        }
    }

    pub fn face(&self) -> FontFaceKey {
        self.face
    }

    pub fn normalized_coords(&self) -> &[i16] {
        &self.normalized_coords
    }

    pub fn synthesis(&self) -> &FontSynthesisKey {
        &self.synthesis
    }
}

#[derive(Default)]
struct FontDigestCache {
    digests: HashMap<u64, FontBlobDigest>,
    order: VecDeque<u64>,
}

impl FontDigestCache {
    fn get(&mut self, blob_id: u64) -> Option<FontBlobDigest> {
        if let Some(digest) = self.digests.get(&blob_id).copied() {
            self.touch(blob_id);
            Some(digest)
        } else {
            None
        }
    }

    fn insert(&mut self, blob_id: u64, digest: FontBlobDigest) {
        self.digests.insert(blob_id, digest);
        self.touch(blob_id);
        while self.order.len() > FONT_DIGEST_CACHE_CAPACITY {
            if let Some(evicted) = self.order.pop_front() {
                self.digests.remove(&evicted);
            }
        }
    }

    fn touch(&mut self, blob_id: u64) {
        if let Some(index) = self
            .order
            .iter()
            .position(|candidate| *candidate == blob_id)
        {
            self.order.remove(index);
        }
        self.order.push_back(blob_id);
    }
}

pub(crate) fn font_blob_digest(blob_id: u64, data: &[u8]) -> FontBlobDigest {
    static CACHE: OnceLock<Mutex<FontDigestCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(FontDigestCache::default()));
    if let Some(digest) = cache
        .lock()
        .expect("font digest cache lock poisoned")
        .get(blob_id)
    {
        return digest;
    }

    let digest = FontBlobDigest(Sha256::digest(data).into());
    let mut cache = cache.lock().expect("font digest cache lock poisoned");
    if let Some(existing) = cache.get(blob_id) {
        existing
    } else {
        cache.insert(blob_id, digest);
        digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_key_distinguishes_blob_and_collection_index() {
        let first_digest = font_blob_digest(10, b"first font");
        let same_digest = font_blob_digest(10, b"ignored after cache hit");
        let second_digest = font_blob_digest(11, b"second font");

        assert_eq!(first_digest, same_digest);
        assert_ne!(first_digest, second_digest);
        assert_ne!(FontFaceKey::new(10, 100, 0), FontFaceKey::new(10, 100, 1));
        assert_ne!(FontFaceKey::new(10, 100, 0), FontFaceKey::new(11, 100, 0));
        assert_ne!(FontFaceKey::new(10, 100, 0), FontFaceKey::new(10, 101, 0));
        assert_eq!(first_digest.to_hex().len(), 64);
    }

    #[test]
    fn instance_key_distinguishes_variations_and_synthesis() {
        let face = FontFaceKey::new(12, b"variable font".len(), 0);
        let plain = FontInstanceKey::new(
            face,
            vec![0],
            FontSynthesisKey::new(Vec::new(), false, None),
        );
        let varied = FontInstanceKey::new(
            face,
            vec![8_000],
            FontSynthesisKey::new(Vec::new(), false, None),
        );
        let synthesized = FontInstanceKey::new(
            face,
            vec![0],
            FontSynthesisKey::new(
                vec![FontVariationSettingKey::new(*b"wght", 700.0)],
                true,
                Some(14.0),
            ),
        );

        assert_ne!(plain, varied);
        assert_ne!(plain, synthesized);
        assert!(synthesized.synthesis().any());
        assert_eq!(
            synthesized.synthesis().variation_settings()[0].tag(),
            *b"wght"
        );
        assert_eq!(
            synthesized.synthesis().variation_settings()[0].value(),
            700.0
        );
        assert_eq!(synthesized.synthesis().skew(), Some(14.0));
    }
}
