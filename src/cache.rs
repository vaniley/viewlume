use crate::loader::decoder::DecodedImage;
use quick_cache::sync::Cache;
use quick_cache::Weighter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

impl FileFingerprint {
    fn read(path: &Path) -> Option<Self> {
        let metadata = path.metadata().ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

#[derive(Clone)]
struct CachedImage {
    fingerprint: FileFingerprint,
    image: Arc<DecodedImage>,
}

#[derive(Clone)]
struct CachedThumbnail {
    fingerprint: FileFingerprint,
    image: Arc<egui::ColorImage>,
}

#[derive(Clone, Copy)]
pub struct ImageWeighter;

impl Weighter<PathBuf, CachedImage> for ImageWeighter {
    fn weight(&self, _key: &PathBuf, val: &CachedImage) -> u64 {
        val.image.bytes_size as u64
    }
}

pub struct ImageCache {
    full_cache: Cache<PathBuf, CachedImage, ImageWeighter>,
    thumb_cache: Cache<PathBuf, CachedThumbnail>,
}

impl ImageCache {
    pub fn new(max_ram_mb: usize, max_thumbs: usize) -> Self {
        let max_bytes = (max_ram_mb * 1024 * 1024) as u64;
        let full_cache = Cache::with_weighter(128, max_bytes, ImageWeighter);

        let thumb_cache = Cache::new(max_thumbs);

        Self {
            full_cache,
            thumb_cache,
        }
    }

    pub fn get_full(&self, path: &Path) -> Option<Arc<DecodedImage>> {
        let cached = self.full_cache.get(path)?;
        if Some(cached.fingerprint) == FileFingerprint::read(path) {
            Some(Arc::clone(&cached.image))
        } else {
            self.full_cache.remove(path);
            self.thumb_cache.remove(path);
            None
        }
    }

    pub fn insert_full(&self, path: PathBuf, image: Arc<DecodedImage>) {
        if let Some(fingerprint) = FileFingerprint::read(&path) {
            self.full_cache
                .insert(path, CachedImage { fingerprint, image });
        }
    }

    pub fn get_thumbnail(&self, path: &Path) -> Option<Arc<egui::ColorImage>> {
        let cached = self.thumb_cache.get(path)?;
        if Some(cached.fingerprint) == FileFingerprint::read(path) {
            Some(Arc::clone(&cached.image))
        } else {
            self.thumb_cache.remove(path);
            self.full_cache.remove(path);
            None
        }
    }

    pub fn insert_thumbnail(&self, path: PathBuf, thumb: Arc<egui::ColorImage>) {
        if let Some(fingerprint) = FileFingerprint::read(&path) {
            self.thumb_cache.insert(
                path,
                CachedThumbnail {
                    fingerprint,
                    image: thumb,
                },
            );
        }
    }

    pub fn clear(&self) {
        self.full_cache.clear();
        self.thumb_cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded() -> Arc<DecodedImage> {
        Arc::new(DecodedImage {
            width: 1,
            height: 1,
            bytes_size: 4,
            color_image: Arc::new(egui::ColorImage::new([1, 1], egui::Color32::WHITE)),
            animation_frames: Arc::from([]),
        })
    }

    #[test]
    fn invalidates_entries_when_the_file_changes() {
        let path = std::env::temp_dir().join(format!(
            "viewlume-cache-invalidation-test-{}",
            std::process::id()
        ));
        std::fs::write(&path, b"old").expect("create cache fixture");
        let cache = ImageCache::new(1, 4);
        cache.insert_full(path.clone(), decoded());
        assert!(cache.get_full(&path).is_some());

        std::fs::write(&path, b"new-content").expect("modify cache fixture");
        assert!(cache.get_full(&path).is_none());
        let _ = std::fs::remove_file(path);
    }
}
