use crate::{
    formats,
    raw::{self, Metadata},
    thumbnail_work::DecodePool,
};
use image::{DynamicImage, ImageReader, codecs::jpeg::JpegEncoder};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::Cursor,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock, Weak},
    time::{Instant, SystemTime},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub marked: bool,
    pub is_raw: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub photos: Vec<Photo>,
    pub label: String,
}
pub struct Media {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFailure {
    pub id: String,
    pub name: String,
    pub error: String,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReport {
    pub deleted: Vec<String>,
    pub failed: Vec<DeleteFailure>,
}
fn identity(path: &std::path::Path) -> Result<String, String> {
    let m = fs::metadata(path).map_err(|e| e.to_string())?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().to_lowercase().hash(&mut hash);
    m.len().hash(&mut hash);
    m.modified().ok().hash(&mut hash);
    Ok(format!("{:016x}", hash.finish()))
}
pub struct Engine {
    mutation_gate: Mutex<()>,
    pub collection: RwLock<Collection>,
    files: RwLock<HashMap<String, PathBuf>>,
    metadata: Mutex<HashMap<String, Metadata>>,
    db: Mutex<Connection>,
    cache_dir: PathBuf,
    raw_gate: Mutex<()>,
    preview_pool: DecodePool,
    raster_pool: DecodePool,
    thumb_locks: Mutex<HashMap<String, Weak<Mutex<()>>>>,
    raw_thumb_gate: Mutex<()>,
    raster_gate: Mutex<()>,
    last_raw: Mutex<Option<(String, Arc<Vec<u8>>)>>,
}
impl Engine {
    pub fn new(data: PathBuf) -> Result<Arc<Self>, String> {
        fs::create_dir_all(data.join("previews")).map_err(|e| e.to_string())?;
        let db = Connection::open(data.join("library.sqlite3")).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS marks(id TEXT PRIMARY KEY, path TEXT NOT NULL, marked INTEGER NOT NULL);").map_err(|e|e.to_string())?;
        let engine = Arc::new(Self {
            mutation_gate: Mutex::new(()),
            collection: RwLock::new(Collection {
                photos: vec![],
                label: String::new(),
            }),
            files: RwLock::new(HashMap::new()),
            metadata: Mutex::new(HashMap::new()),
            db: Mutex::new(db),
            cache_dir: data.join("previews"),
            raw_gate: Mutex::new(()),
            preview_pool: DecodePool::new(4, 4),
            raster_pool: DecodePool::new(
                std::thread::available_parallelism()
                    .map_or(2, usize::from)
                    .clamp(1, 4),
                768 * 1024 * 1024,
            ),
            thumb_locks: Mutex::new(HashMap::new()),
            raw_thumb_gate: Mutex::new(()),
            raster_gate: Mutex::new(()),
            last_raw: Mutex::new(None),
        });
        engine.prune_disk_cache();
        Ok(engine)
    }
    fn prune_disk_cache(&self) {
        // Derived previews only: never touches source photographs. Limit to 2 GiB.
        let mut entries: Vec<_> = fs::read_dir(&self.cache_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                if !m.is_file() {
                    return None;
                }
                Some((
                    m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    m.len(),
                    e.path(),
                ))
            })
            .collect();
        let mut total: u64 = entries.iter().map(|e| e.1).sum();
        entries.sort_by_key(|e| e.0);
        for (_, len, path) in entries {
            if total < 2 * 1024 * 1024 * 1024 {
                break;
            }
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(len)
            }
        }
    }
    pub fn select(&self, paths: Vec<String>) -> Result<Collection, String> {
        let _mutation = self.mutation_gate.lock().map_err(|e| e.to_string())?;
        let label = if paths.len() == 1 {
            paths[0].clone()
        } else {
            format!("已选择 {} 个文件", paths.len())
        };
        let mut candidates = Vec::new();
        for path in paths {
            let path = PathBuf::from(path);
            if path.is_dir() {
                for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    if entry.file_type().map_err(|e| e.to_string())?.is_file() {
                        candidates.push(entry.path())
                    }
                }
            } else {
                candidates.push(path)
            }
        }
        candidates.retain(|p| formats::supported(p));
        candidates.sort_by_cached_key(|p| p.to_string_lossy().to_lowercase());
        candidates.dedup();
        let mut photos = Vec::new();
        let mut files = HashMap::new();
        let db = self.db.lock().map_err(|e| e.to_string())?;
        for path in candidates {
            let path = fs::canonicalize(&path).map_err(|e| e.to_string())?;
            let m = fs::metadata(&path).map_err(|e| e.to_string())?;
            let id = identity(&path)?;
            if files.contains_key(&id) {
                continue;
            }
            let marked = db
                .query_row("SELECT marked FROM marks WHERE id=?1", [&id], |r| {
                    r.get::<_, bool>(0)
                })
                .unwrap_or(false);
            photos.push(Photo {
                id: id.clone(),
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path: path
                    .to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .to_string(),
                size: m.len(),
                marked,
                is_raw: formats::is_raw(&path),
            });
            files.insert(id, path);
        }
        drop(db);
        let collection = Collection { photos, label };
        *self.files.write().map_err(|e| e.to_string())? = files;
        *self.collection.write().map_err(|e| e.to_string())? = collection.clone();
        self.metadata.lock().map_err(|e| e.to_string())?.clear();
        *self.last_raw.lock().map_err(|e| e.to_string())? = None;
        Ok(collection)
    }
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        self.files
            .read()
            .map_err(|e| e.to_string())?
            .get(id)
            .cloned()
            .ok_or("照片不在当前选择范围内".to_string())
    }
    pub fn export_source(&self, id: &str) -> Result<PathBuf, String> {
        let path = self.path(id)?;
        if identity(&path)? != id {
            return Err("文件已被修改，请重新选择来源后再导出。".into());
        }
        Ok(path)
    }
    pub fn meta(&self, id: &str) -> Result<Metadata, String> {
        if let Some(m) = self.metadata.lock().map_err(|e| e.to_string())?.get(id) {
            return Ok(m.clone());
        }
        let path = self.path(id)?;
        let meta = if formats::is_raw(&path) {
            raw::load(&path, 0)?.meta
        } else {
            formats::metadata(&path)?
        };
        self.metadata
            .lock()
            .map_err(|e| e.to_string())?
            .insert(id.to_string(), meta.clone());
        Ok(meta)
    }
    pub fn mark(&self, id: &str, marked: bool) -> Result<(), String> {
        let _mutation = self.mutation_gate.lock().map_err(|e| e.to_string())?;
        let path = self.path(id)?;
        self.db.lock().map_err(|e|e.to_string())?.execute("INSERT INTO marks(id,path,marked) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET marked=excluded.marked",params![id,path.to_string_lossy(),marked]).map_err(|e|e.to_string())?;
        if let Some(p) = self
            .collection
            .write()
            .map_err(|e| e.to_string())?
            .photos
            .iter_mut()
            .find(|p| p.id == id)
        {
            p.marked = marked
        }
        Ok(())
    }
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let _mutation = self.mutation_gate.lock().map_err(|e| e.to_string())?;
        self.delete_one(id)
    }
    pub fn delete_many(&self, ids: Vec<String>) -> Result<DeleteReport, String> {
        // Freeze the source while the batch runs. Each file still gets its own
        // identity check; one failed file must not hide earlier successes.
        let _mutation = self.mutation_gate.lock().map_err(|e| e.to_string())?;
        let names: HashMap<_, _> = self
            .collection
            .read()
            .map_err(|e| e.to_string())?
            .photos
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        let mut seen = HashSet::new();
        let mut report = DeleteReport::default();
        for id in ids {
            if !seen.insert(id.clone()) {
                continue;
            }
            match self.delete_one(&id) {
                Ok(()) => report.deleted.push(id),
                Err(error) => report.failed.push(DeleteFailure {
                    name: names.get(&id).cloned().unwrap_or_else(|| id.clone()),
                    id,
                    error,
                }),
            }
        }
        Ok(report)
    }
    fn delete_one(&self, id: &str) -> Result<(), String> {
        let path = self.path(id)?;
        if identity(&path)? != id {
            return Err("文件已被其他程序修改，请重新选择来源后再删除。".into());
        }
        // No permanent-delete fallback. Failure is surfaced to the user.
        trash::delete(&path).map_err(|e| format!("无法移入回收站：{e}"))?;
        self.files.write().map_err(|e| e.to_string())?.remove(id);
        self.collection
            .write()
            .map_err(|e| e.to_string())?
            .photos
            .retain(|p| p.id != id);
        self.metadata.lock().map_err(|e| e.to_string())?.remove(id);
        let mut last = self.last_raw.lock().map_err(|e| e.to_string())?;
        if last.as_ref().is_some_and(|(key, _)| key == id) {
            *last = None
        }
        Ok(())
    }
    pub fn media(&self, id: &str, kind: &str) -> Result<Media, String> {
        if kind == "thumb" {
            // Deduplicate only the same photograph. A slow RAW must not hold a
            // hashed lane needed by unrelated, inexpensive previews.
            let lock = {
                let mut locks = self.thumb_locks.lock().map_err(|e| e.to_string())?;
                if locks.len() > 256 {
                    locks.retain(|_, v| v.strong_count() > 0);
                }
                if let Some(lock) = locks.get(id).and_then(Weak::upgrade) {
                    lock
                } else {
                    let lock = Arc::new(Mutex::new(()));
                    locks.insert(id.to_string(), Arc::downgrade(&lock));
                    lock
                }
            };
            let _same_photo = lock.lock().map_err(|e| e.to_string())?;
            self.media_inner(id, kind)
        } else {
            self.media_inner(id, kind)
        }
    }
    fn media_inner(&self, id: &str, kind: &str) -> Result<Media, String> {
        let path = self.path(id)?;
        if !formats::is_raw(&path) {
            return self.raster_media(id, &path, kind);
        }
        if kind == "raw" {
            let _gate = self.raw_gate.lock().map_err(|e| e.to_string())?;
            if let Some((key, bytes)) = &*self.last_raw.lock().map_err(|e| e.to_string())? {
                if key == id {
                    return Ok(Media {
                        bytes: bytes.as_ref().clone(),
                        mime: "application/octet-stream",
                    });
                }
            }
            let started = Instant::now();
            let d = raw::load(&path, 2)?;
            let pixels = (d.width as usize)
                .checked_mul(d.height as usize)
                .ok_or("图像尺寸溢出")?;
            if pixels > 150_000_000 || d.bytes.len() != pixels * 3 {
                return Err("RAW 输出尺寸无效或超过 1.5 亿像素限制".into());
            }
            let mut bytes = Vec::with_capacity(12 + pixels * 4);
            bytes.extend_from_slice(b"NRV1");
            bytes.extend_from_slice(&d.width.to_le_bytes());
            bytes.extend_from_slice(&d.height.to_le_bytes());
            for rgb in d.bytes.chunks_exact(3) {
                bytes.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255])
            }
            eprintln!(
                "raw {} {}x{} {}ms",
                path.file_name().unwrap_or_default().to_string_lossy(),
                d.width,
                d.height,
                started.elapsed().as_millis()
            );
            *self.last_raw.lock().map_err(|e| e.to_string())? =
                Some((id.to_string(), Arc::new(bytes.clone())));
            return Ok(Media {
                bytes,
                mime: "application/octet-stream",
            });
        }
        // Return the embedded JPEG byte-for-byte: no re-encoding or RAW development.
        if kind == "full" || kind == "preview" {
            if let Ok(bytes) = fs::read(self.cache_dir.join(format!("{id}-raw-fallback-v1.png"))) {
                return Ok(Media {
                    bytes,
                    mime: "image/png",
                });
            }
            let d = match raw::load(&path, 1) {
                Ok(d) => d,
                Err(_) => return self.raw_fallback(id, &path, kind),
            };
            self.metadata
                .lock()
                .map_err(|e| e.to_string())?
                .insert(id.to_string(), d.meta.clone());
            if d.jpeg {
                return Ok(Media {
                    bytes: formats::orient_preview_jpeg(d.bytes, d.meta.flip),
                    mime: "image/jpeg",
                });
            }
            let img = DynamicImage::ImageRgb8(
                image::RgbImage::from_raw(d.width, d.height, d.bytes).ok_or("预览尺寸无效")?,
            );
            let img = match d.meta.flip {
                3 => img.rotate180(),
                5 => img.rotate270(),
                6 => img.rotate90(),
                _ => img,
            };
            let mut bytes = Vec::new();
            JpegEncoder::new_with_quality(&mut bytes, 98)
                .encode_image(&img)
                .map_err(|e| e.to_string())?;
            return Ok(Media {
                bytes,
                mime: "image/jpeg",
            });
        }
        if kind != "thumb" {
            return Err("无效的图像类型".into());
        }
        let file = self.cache_dir.join(format!("{id}-{kind}-v1.jpg"));
        if let Ok(bytes) = fs::read(&file) {
            return Ok(Media {
                bytes,
                mime: "image/jpeg",
            });
        }
        let embedded = {
            let _permit = self.preview_pool.acquire(1)?;
            self.raw_embedded_thumbnail(id, &path)
        };
        let bytes = match embedded {
            Ok(bytes) => bytes,
            // The preview permit is released before expensive RAW development.
            Err(_) => return self.raw_fallback(id, &path, kind),
        };
        self.cache(&file, &bytes);
        Ok(Media {
            bytes,
            mime: "image/jpeg",
        })
    }
    fn raw_embedded_thumbnail(&self, id: &str, path: &std::path::Path) -> Result<Vec<u8>, String> {
        let d = raw::load(path, 3)?;
        self.metadata
            .lock()
            .map_err(|e| e.to_string())?
            .insert(id.to_string(), d.meta.clone());
        if d.jpeg {
            if let Ok(bytes) = formats::embedded_jpeg_thumbnail(&d.bytes, d.meta.flip) {
                return Ok(bytes);
            }
        }
        // Unusual embedded encodings use a bounded compatibility decoder.
        let _compat = self.raster_gate.lock().map_err(|e| e.to_string())?;
        let orientation =
            formats::raw_orientation(if d.jpeg { Some(&d.bytes) } else { None }, d.meta.flip);
        let mut img = if d.jpeg {
            ImageReader::new(Cursor::new(d.bytes))
                .with_guessed_format()
                .map_err(|e| e.to_string())?
                .decode()
                .map_err(|e| e.to_string())?
        } else {
            DynamicImage::ImageRgb8(
                image::RgbImage::from_raw(d.width, d.height, d.bytes).ok_or("预览尺寸无效")?,
            )
        };
        img.apply_orientation(orientation);
        formats::thumbnail(&img)
    }
    fn cache(&self, path: &std::path::Path, bytes: &[u8]) {
        let temp = path.with_extension("tmp");
        if fs::write(&temp, bytes).is_ok() {
            let _ = fs::rename(temp, path);
        }
    }
    fn raster_media(&self, id: &str, path: &std::path::Path, kind: &str) -> Result<Media, String> {
        if !matches!(kind, "full" | "preview" | "thumb") {
            return Err("此图片不需要 RAW 显影".into());
        }
        if kind != "thumb" {
            if let Some(mime) = formats::direct_mime(path) {
                // Preserve original pixels, EXIF and ICC profile; browser decodes off the UI thread.
                return Ok(Media {
                    bytes: fs::read(path).map_err(|e| e.to_string())?,
                    mime,
                });
            }
        }
        let thumb = kind == "thumb";
        let file = self.cache_dir.join(if thumb {
            format!("{id}-image-thumb-v1.jpg")
        } else {
            format!("{id}-image-full-v1.png")
        });
        let mime = if thumb { "image/jpeg" } else { "image/png" };
        if let Ok(bytes) = fs::read(&file) {
            return Ok(Media { bytes, mime });
        }
        if thumb && matches!(formats::extension(path).as_str(), "jpg" | "jpeg") {
            let _permit = self.preview_pool.acquire(1)?;
            let bytes = formats::jpeg_thumbnail(path).or_else(|_| {
                let _fallback = self.raster_gate.lock().map_err(|e| e.to_string())?;
                formats::thumbnail(&formats::decode(path)?)
            })?;
            self.cache(&file, &bytes);
            return Ok(Media { bytes, mime });
        }
        if thumb {
            let (w, h) = image::image_dimensions(path).map_err(|e| e.to_string())?;
            let estimated = u64::from(w)
                .saturating_mul(u64::from(h))
                .saturating_mul(8)
                .saturating_add(32 * 1024 * 1024);
            let _permit = self.raster_pool.acquire(estimated)?;
            let bytes = formats::raster_thumbnail(path)
                .or_else(|_| formats::thumbnail(&formats::decode(path)?))?;
            self.cache(&file, &bytes);
            return Ok(Media { bytes, mime });
        }
        // Independent gate: a long TIFF decode must not block direct JPEG/RAW preview reads.
        let _gate = self.raster_gate.lock().map_err(|e| e.to_string())?;
        if let Ok(bytes) = fs::read(&file) {
            return Ok(Media { bytes, mime });
        }
        let image = formats::decode(path)?;
        let bytes = formats::png_with_profile(&image, path)?;
        self.cache(&file, &bytes);
        Ok(Media { bytes, mime })
    }
    fn raw_fallback(&self, id: &str, path: &std::path::Path, kind: &str) -> Result<Media, String> {
        if kind == "thumb" {
            let _gate = self.raw_thumb_gate.lock().map_err(|e| e.to_string())?;
            let full_file = self.cache_dir.join(format!("{id}-raw-fallback-v1.png"));
            let image = if let Ok(bytes) = fs::read(full_file) {
                image::load_from_memory(&bytes).map_err(|e| e.to_string())?
            } else {
                let d = raw::load(path, 4).map_err(|e| format!("没有可读取的内嵌预览；{e}"))?;
                self.metadata
                    .lock()
                    .map_err(|e| e.to_string())?
                    .insert(id.to_string(), d.meta);
                DynamicImage::ImageRgb8(
                    image::RgbImage::from_raw(d.width, d.height, d.bytes)
                        .ok_or("RAW 预览尺寸无效")?,
                )
            };
            // Half-size development is already oriented. Cache ONLY the small
            // thumbnail; never let it satisfy a full-resolution/100% request.
            let bytes = formats::thumbnail(&image)?;
            self.cache(&self.cache_dir.join(format!("{id}-thumb-v1.jpg")), &bytes);
            return Ok(Media {
                bytes,
                mime: "image/jpeg",
            });
        }
        let _gate = self.raw_gate.lock().map_err(|e| e.to_string())?;
        let file = self.cache_dir.join(format!("{id}-raw-fallback-v1.png"));
        let bytes = if let Ok(bytes) = fs::read(&file) {
            bytes
        } else {
            let d = raw::load(path, 2).map_err(|e| format!("没有可读取的内嵌预览；{e}"))?;
            // dcraw_process already applied orientation. Never rotate this a second time.
            let image = DynamicImage::ImageRgb8(
                image::RgbImage::from_raw(d.width, d.height, d.bytes).ok_or("RAW 预览尺寸无效")?,
            );
            let bytes = formats::png(&image)?;
            self.cache(&file, &bytes);
            bytes
        };
        Ok(Media {
            bytes,
            mime: "image/png",
        })
    }
}

pub fn resource(engine: &Engine, path: &str) -> Result<Media, String> {
    let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
    match parts.as_slice() {
        ["image", id, kind] => engine.media(id, kind),
        ["metadata", id] => Ok(Media {
            bytes: serde_json::to_vec(&engine.meta(id)?).map_err(|e| e.to_string())?,
            mime: "application/json",
        }),
        _ => Err("资源不存在".into()),
    }
}
