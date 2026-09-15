use crate::cache::ImageCache;
use crate::loader::decoder::{
    decode_full_image, decode_full_image_with_preview, decode_thumbnail, generate_thumbnail,
    DecodedImage,
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
    ImmediateWithPreview,
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
    Preview {
        path: PathBuf,
        index: usize,
        epoch: u64,
        width: u32,
        height: u32,
        image: Arc<egui::ColorImage>,
    },
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
    immediate_task_tx: Sender<FullImageTask>,
    prefetch_task_tx: Sender<FullImageTask>,
    thumb_task_tx: Sender<ThumbnailTask>,
    result_rx: Receiver<LoadResult>,
    pub current_epoch: Arc<AtomicU64>,
    cache: Arc<ImageCache>,
    pending_thumbnails: Arc<Mutex<HashSet<PathBuf>>>,
    pending_full: Arc<Mutex<HashSet<PathBuf>>>,
}

impl LoaderPipeline {
    pub fn new(
        cache: Arc<ImageCache>,
        num_threads: usize,
        repaint_ctx: egui::Context,
        thumbnail_size: u32,
    ) -> Self {
        let (immediate_task_tx, immediate_task_rx) = unbounded::<FullImageTask>();
        let (prefetch_task_tx, prefetch_task_rx) = unbounded::<FullImageTask>();
        // A bounded queue prevents a huge folder from turning one gallery open into
        // thousands of decodes and an out-of-memory crash.
        let (thumb_task_tx, thumb_task_rx) = bounded::<ThumbnailTask>(64);
        let (result_tx, result_rx) = unbounded::<LoadResult>();

        let current_epoch = Arc::new(AtomicU64::new(1));
        let pending_thumbnails = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));
        let pending_full = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));

        // Spawn full-image worker threads (fast decode, prioritizes active image)
        let full_workers = (num_threads / 2).clamp(1, 2);
        for worker_index in 0..full_workers {
            let immediate_rx = immediate_task_rx.clone();
            let prefetch_rx = prefetch_task_rx.clone();
            let tx = result_tx.clone();
            let epoch_atomic = Arc::clone(&current_epoch);
            let cache_ref = Arc::clone(&cache);
            let pending_ref = Arc::clone(&pending_full);
            let repaint = repaint_ctx.clone();
            let thumb_size = thumbnail_size;

            thread::Builder::new()
                .name("full-img-worker".to_string())
                .spawn(move || {
                    loop {
                        // Keep one worker reserved for images explicitly opened by the user.
                        // The other worker prefers immediate work but uses idle time to prefetch.
                        let task = if worker_index + 1 < full_workers {
                            match immediate_rx.recv() {
                                Ok(task) => task,
                                Err(_) => break,
                            }
                        } else {
                            crossbeam_channel::select_biased! {
                                recv(immediate_rx) -> task => match task {
                                    Ok(task) => task,
                                    Err(_) => break,
                                },
                                recv(prefetch_rx) -> task => match task {
                                    Ok(task) => task,
                                    Err(_) => break,
                                },
                            }
                        };
                        let task_path_for_pending = task.path.clone();
                        // Drop all obsolete navigation work, including old immediate tasks.
                        if task.epoch < epoch_atomic.load(Ordering::Relaxed) {
                            if let Ok(mut pending) = pending_ref.lock() {
                                pending.remove(&task_path_for_pending);
                            }
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
                            if let Ok(mut pending) = pending_ref.lock() {
                                pending.remove(&task_path_for_pending);
                            }
                            continue;
                        }

                        let result = if task.priority == Priority::ImmediateWithPreview {
                            let preview_path = task.path.clone();
                            let preview_tx = tx.clone();
                            let preview_repaint = repaint.clone();
                            decode_full_image_with_preview(
                                &task.path,
                                thumb_size.max(512),
                                move |width, height, image| {
                                    let _ = preview_tx.send(LoadResult::Preview {
                                        path: preview_path,
                                        index: task.index,
                                        epoch: task.epoch,
                                        width,
                                        height,
                                        image,
                                    });
                                    preview_repaint.request_repaint();
                                },
                            )
                        } else {
                            decode_full_image(&task.path)
                        };
                        if let Ok(ref img) = result {
                            cache_ref.insert_full(task.path.clone(), Arc::clone(img));
                        }

                        let task_path = task.path.clone();
                        let _ = tx.send(LoadResult::FullImage {
                            path: task.path,
                            index: task.index,
                            epoch: task.epoch,
                            result,
                        });
                        repaint.request_repaint();
                        if let Ok(mut pending) = pending_ref.lock() {
                            pending.remove(&task_path_for_pending);
                        }

                        // Generate thumbnail after sending the full image result
                        if cache_ref.get_thumbnail(&task_path).is_none() {
                            if let Some(full) = cache_ref.get_full(&task_path) {
                                if let Ok(thumb) = generate_thumbnail(&full, thumb_size) {
                                    cache_ref.insert_thumbnail(task_path, Arc::new(thumb));
                                    repaint.request_repaint();
                                }
                            }
                        }
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
            immediate_task_tx,
            prefetch_task_tx,
            thumb_task_tx,
            result_rx,
            current_epoch,
            cache,
            pending_thumbnails,
            pending_full,
        }
    }

    pub fn next_epoch(&self) -> u64 {
        let epoch = self.current_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut pending) = self.pending_full.lock() {
            pending.clear();
        }
        epoch
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
        if let Some(cached) = self.cache.get_full(&path) {
            return Some(cached);
        }

        // Immediate requests always go through (they reset epoch anyway).
        // Prefetch requests are deduped to avoid redundant work.
        if priority == Priority::Prefetch {
            let Ok(mut pending) = self.pending_full.lock() else {
                return None;
            };
            if !pending.insert(path.clone()) {
                return None;
            }
        } else if let Ok(mut pending) = self.pending_full.lock() {
            pending.insert(path.clone());
        }

        let epoch = self.current_epoch();
        let task = FullImageTask {
            path,
            index,
            epoch,
            priority,
        };
        let _ = match priority {
            Priority::Immediate | Priority::ImmediateWithPreview => {
                self.immediate_task_tx.send(task)
            }
            Priority::Prefetch => self.prefetch_task_tx.send(task),
        };
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
