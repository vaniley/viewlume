use crate::loader::decoder::DecodedImage;
use quick_cache::sync::Cache;
use quick_cache::Weighter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct ImageWeighter;

impl Weighter<PathBuf, Arc<DecodedImage>> for ImageWeighter {
    fn weight(&self, _key: &PathBuf, val: &Arc<DecodedImage>) -> u64 {
        val.bytes_size as u64
    }
}

pub struct ImageCache {
    full_cache: Cache<PathBuf, Arc<DecodedImage>, ImageWeighter>,
    thumb_cache: Cache<PathBuf, Arc<egui::ColorImage>>,
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
        self.full_cache.get(path)
    }

    pub fn insert_full(&self, path: PathBuf, image: Arc<DecodedImage>) {
        self.full_cache.insert(path, image);
    }

    pub fn get_thumbnail(&self, path: &Path) -> Option<Arc<egui::ColorImage>> {
        self.thumb_cache.get(path)
    }

    pub fn insert_thumbnail(&self, path: PathBuf, thumb: Arc<egui::ColorImage>) {
        self.thumb_cache.insert(path, thumb);
    }

    pub fn clear(&self) {
        self.full_cache.clear();
        self.thumb_cache.clear();
    }
}
