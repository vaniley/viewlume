use crate::cache::ImageCache;
use crate::loader::decoder::{
    decode_full_image, decode_thumbnail, generate_thumbnail, DecodedImage,
};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Immediate,
    Prefetch,
}

pub struct FullImageTask {
    pub path: PathBuf,
    pub index: usize,
    pub epoch: u64,
    pub priority: Priority,
}

pub struct ThumbnailTask {
    pub path: PathBuf,
    pub index: usize,
    pub max_dim: u32,
}

pub enum LoadResult {
    FullImage {
        path: PathBuf,
        index: usize,
        epoch: u64,
        result: Result<Arc<DecodedImage>, String>,
    },
    Thumbnail {
        path: PathBuf,
        index: usize,
        result: Result<Arc<egui::ColorImage>, String>,
    },
}

pub struct LoaderPipeline {
    full_task_tx: Sender<FullImageTask>,
    thumb_task_tx: Sender<ThumbnailTask>,
    result_rx: Receiver<LoadResult>,
    pub current_epoch: Arc<AtomicU64>,
    cache: Arc<ImageCache>,
    pending_thumbnails: Arc<Mutex<HashSet<PathBuf>>>,
}

impl LoaderPipeline {
    pub fn new(cache: Arc<ImageCache>, num_threads: usize, repaint_ctx: egui::Context) -> Self {
        let (full_task_tx, full_task_rx) = unbounded::<FullImageTask>();
        // A bounded queue prevents a huge folder from turning one gallery open into
        // thousands of decodes and an out-of-memory crash.
        let (thumb_task_tx, thumb_task_rx) = bounded::<ThumbnailTask>(64);
        let (result_tx, result_rx) = unbounded::<LoadResult>();

        let current_epoch = Arc::new(AtomicU64::new(1));
        let pending_thumbnails = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));

        // Spawn full-image worker threads (fast decode, prioritizes active image)
        let full_workers = (num_threads / 2).clamp(1, 2);
        for _ in 0..full_workers {
            let rx = full_task_rx.clone();
            let tx = result_tx.clone();
            let epoch_atomic = Arc::clone(&current_epoch);
            let cache_ref = Arc::clone(&cache);
            let repaint = repaint_ctx.clone();

            thread::Builder::new()
                .name("full-img-worker".to_string())
                .spawn(move || {
                    while let Ok(task) = rx.recv() {
                        // Drop all obsolete navigation work, including old immediate tasks.
                        if task.epoch < epoch_atomic.load(Ordering::Relaxed) {
                            continue;
                        }

                        // Check cache first
                        if let Some(cached) = cache_ref.get_full(&task.path) {
                            let _ = tx.send(LoadResult::FullImage {
                                path: task.path,
                                index: task.index,
                                epoch: task.epoch,
                                result: Ok(cached),
                            });
                            repaint.request_repaint();
                            continue;
                        }

                        let result = decode_full_image(&task.path);
                        if let Ok(ref img) = result {
                            cache_ref.insert_full(task.path.clone(), Arc::clone(img));
                            // Also populate thumbnail if missing
                            if cache_ref.get_thumbnail(&task.path).is_none() {
                                if let Ok(thumb) = generate_thumbnail(img, 128) {
                                    cache_ref.insert_thumbnail(task.path.clone(), Arc::new(thumb));
                                }
                            }
                        }

                        let _ = tx.send(LoadResult::FullImage {
                            path: task.path,
                            index: task.index,
                            epoch: task.epoch,
                            result,
                        });
                        repaint.request_repaint();
                    }
                })
                .expect("Failed to spawn full image worker thread");
        }

        // Spawn thumbnail worker threads (background thumbnails)
        let thumb_workers = (num_threads / 4).clamp(1, 2);
        for _ in 0..thumb_workers {
            let rx = thumb_task_rx.clone();
            let tx = result_tx.clone();
            let cache_ref = Arc::clone(&cache);
            let pending_ref = Arc::clone(&pending_thumbnails);
            let repaint = repaint_ctx.clone();

            thread::Builder::new()
                .name("thumb-worker".to_string())
                .spawn(move || {
                    while let Ok(task) = rx.recv() {
                        let task_path = task.path.clone();
                        // Check cache
                        if let Some(cached) = cache_ref.get_thumbnail(&task.path) {
                            let _ = tx.send(LoadResult::Thumbnail {
                                path: task.path,
                                index: task.index,
                                result: Ok(cached),
                            });
                            repaint.request_repaint();
                            if let Ok(mut pending) = pending_ref.lock() {
                                pending.remove(&task_path);
                            }
                            continue;
                        }

                        // If full image is in cache, generate thumbnail from it
                        let thumb_res = if let Some(full) = cache_ref.get_full(&task.path) {
                            generate_thumbnail(&full, task.max_dim).map(Arc::new)
                        } else {
                            decode_thumbnail(&task.path, task.max_dim).map(Arc::new)
                        };

                        if let Ok(ref thumb) = thumb_res {
                            cache_ref.insert_thumbnail(task.path.clone(), Arc::clone(thumb));
                        }

                        let _ = tx.send(LoadResult::Thumbnail {
                            path: task.path,
                            index: task.index,
                            result: thumb_res,
                        });
                        repaint.request_repaint();
                        if let Ok(mut pending) = pending_ref.lock() {
                            pending.remove(&task_path);
                        }
                    }
                })
                .expect("Failed to spawn thumbnail worker thread");
        }

        Self {
            full_task_tx,
            thumb_task_tx,
            result_rx,
            current_epoch,
            cache,
            pending_thumbnails,
        }
    }

    pub fn next_epoch(&self) -> u64 {
        self.current_epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current_epoch(&self) -> u64 {
        self.current_epoch.load(Ordering::SeqCst)
    }

    pub fn request_full_image(
        &self,
        path: PathBuf,
        index: usize,
        priority: Priority,
    ) -> Option<Arc<DecodedImage>> {
        // If already cached, return immediately
        if let Some(cached) = self.cache.get_full(&path) {
            return Some(cached);
        }

        let epoch = self.current_epoch();
        let _ = self.full_task_tx.send(FullImageTask {
            path,
            index,
            epoch,
            priority,
        });
        None
    }

    pub fn request_thumbnail(
        &self,
        path: PathBuf,
        index: usize,
        max_dim: u32,
    ) -> Option<Arc<egui::ColorImage>> {
        if let Some(cached) = self.cache.get_thumbnail(&path) {
            return Some(cached);
        }

        let Ok(mut pending) = self.pending_thumbnails.lock() else {
            return None;
        };
        if !pending.insert(path.clone()) {
            return None;
        }
        drop(pending);

        let task = ThumbnailTask {
            path: path.clone(),
            index,
            max_dim,
        };
        if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
            self.thumb_task_tx.try_send(task)
        {
            if let Ok(mut pending) = self.pending_thumbnails.lock() {
                pending.remove(&path);
            }
        }
        None
    }

    pub fn try_recv_result(&self) -> Option<LoadResult> {
        self.result_rx.try_recv().ok()
    }
}
