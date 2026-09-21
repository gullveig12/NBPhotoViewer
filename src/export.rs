use crate::{engine::Engine, formats, raw};
use exif::{Field, In, Tag, Value};
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub const ZIP_LIMIT: u64 = 1_000_000_000;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub completed: usize,
    pub total: usize,
    pub name: String,
    pub phase: String,
}
#[derive(Debug, Serialize)]
pub struct ExportIssue {
    pub name: String,
    pub message: String,
}
#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub exported: usize,
    pub total: usize,
    pub files: Vec<String>,
    pub failed: Vec<ExportIssue>,
    pub warnings: Vec<ExportIssue>,
    pub cancelled: bool,
    pub fatal: Option<String>,
}
#[derive(Default)]
pub struct ExportJobs {
    active: Mutex<Option<(String, Arc<AtomicBool>)>>,
}
pub struct Job<'a> {
    owner: &'a ExportJobs,
    pub cancel: Arc<AtomicBool>,
}
impl ExportJobs {
    pub fn start(&self, id: String) -> Result<Job<'_>, String> {
        let mut active = self.active.lock().map_err(|e| e.to_string())?;
        if active.is_some() {
            return Err("已有导出任务正在进行。".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *active = Some((id, cancel.clone()));
        Ok(Job {
            owner: self,
            cancel,
        })
    }
    pub fn cancel(&self, id: &str) {
        if let Ok(active) = self.active.lock() {
            if let Some((key, cancel)) = &*active {
                if key == id {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        }
    }
}
impl Drop for Job<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.owner.active.lock() {
            *active = None;
        }
    }
}

pub struct JpegExport {
    pub bytes: Vec<u8>,
    pub warning: Option<String>,
    pub width: u32,
    pub height: u32,
}
fn decode_bytes(bytes: &[u8]) -> Result<(DynamicImage, Option<Vec<u8>>), String> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let profile = decoder.icc_profile().ok().flatten();
    let image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    Ok((image, profile))
}
fn developed(path: &Path) -> Result<DynamicImage, String> {
    let d = raw::load(path, 2)?;
    Ok(DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(d.width, d.height, d.bytes).ok_or("RAW 尺寸无效")?,
    ))
}
/// Always encode once at quality 100, 4:4:4, full available resolution. This is
/// never the viewport canvas and never a thumbnail/half-size cache entry.
pub fn jpeg(path: &Path, prefer_raw: bool) -> Result<JpegExport, String> {
    let (image, profile, warning) = if formats::is_raw(path) {
        let preview = raw::load(path, 1).and_then(|d| {
            let orientation =
                formats::raw_orientation(if d.jpeg { Some(&d.bytes) } else { None }, d.meta.flip);
            let (mut image, profile) = if d.jpeg {
                decode_bytes(&d.bytes)?
            } else {
                (
                    DynamicImage::ImageRgb8(
                        image::RgbImage::from_raw(d.width, d.height, d.bytes)
                            .ok_or("RAW 预览尺寸无效")?,
                    ),
                    None,
                )
            };
            image.apply_orientation(orientation);
            let small = u64::from(image.width()) * u64::from(image.height()) * 100
                < u64::from(d.meta.width) * u64::from(d.meta.height) * 95;
            Ok((image, profile, small))
        });
        match preview {
            Ok((image, profile, small)) if !prefer_raw && !small => (image, profile, None),
            Ok((image, profile, small)) => match developed(path) {
                Ok(full) => (full, None, None),
                Err(_) => (
                    image,
                    profile,
                    Some(
                        if small {
                            "此 RAW 暂不能完整显影，已使用相机内嵌预览，分辨率可能低于原始 RAW。"
                        } else {
                            "此 RAW 暂不能显影，已使用相机内嵌预览。"
                        }
                        .into(),
                    ),
                ),
            },
            Err(_) => (developed(path)?, None, None),
        }
    } else {
        let profile = ImageReader::open(path)
            .ok()
            .and_then(|r| r.with_guessed_format().ok())
            .and_then(|r| r.into_decoder().ok())
            .and_then(|mut d| d.icc_profile().ok().flatten());
        (formats::decode(path)?, profile, None)
    };
    let (width, height) = (image.width(), image.height());
    if width > 65535 || height > 65535 {
        return Err("JPEG 不支持超过 65535 像素的边长，未缩小原图。".into());
    }
    // JPEG has no alpha channel. Composite onto white without changing size.
    let rgb = if image.color().has_alpha() {
        let mut rgba = image.into_rgba8();
        for p in rgba.pixels_mut() {
            let a = u32::from(p[3]);
            for c in &mut p.0[..3] {
                *c = ((u32::from(*c) * a + 255 * (255 - a) + 127) / 255) as u8;
            }
            p[3] = 255;
        }
        DynamicImage::ImageRgba8(rgba).into_rgb8()
    } else {
        image.into_rgb8()
    };
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 100);
    if let Some(profile) = profile {
        encoder
            .set_icc_profile(profile)
            .map_err(|e| e.to_string())?;
    }
    encoder
        .set_exif_metadata(export_exif(path, width, height)?)
        .map_err(|e| e.to_string())?;
    encoder.encode_image(&rgb).map_err(|e| e.to_string())?;
    Ok(JpegExport {
        bytes,
        warning,
        width,
        height,
    })
}
fn export_exif(path: &Path, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let source = formats::read_exif(path);
    let keep = [
        Tag::Make,
        Tag::Model,
        Tag::LensModel,
        Tag::FNumber,
        Tag::ExposureTime,
        Tag::PhotographicSensitivity,
        Tag::ISOSpeed,
        Tag::FocalLength,
        Tag::DateTimeOriginal,
        Tag::OffsetTimeOriginal,
        Tag::SubSecTimeOriginal,
        Tag::Artist,
        Tag::Copyright,
    ];
    let mut fields = vec![
        Field {
            tag: Tag::Orientation,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![1]),
        },
        Field {
            tag: Tag::PixelXDimension,
            ifd_num: In::PRIMARY,
            value: Value::Long(vec![width]),
        },
        Field {
            tag: Tag::PixelYDimension,
            ifd_num: In::PRIMARY,
            value: Value::Long(vec![height]),
        },
    ];
    if let Some(source) = source {
        for tag in keep {
            if let Some(field) = source.get_field(tag, In::PRIMARY) {
                fields.push(field.clone());
            }
        }
    }
    let mut writer = exif::experimental::Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut bytes = Cursor::new(Vec::new());
    writer.write(&mut bytes, false).map_err(|e| e.to_string())?;
    Ok(bytes.into_inner())
}
fn stem(name: &str) -> String {
    let original = Path::new(name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let mut name: String = original
        .chars()
        .take(160)
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    name = name.trim_matches([' ', '.']).to_string();
    if name.is_empty() {
        name = "photo".into();
    }
    let upper = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit())
    {
        name.insert(0, '_');
    }
    name
}
pub fn jpeg_name(name: &str, used: &mut HashSet<String>) -> String {
    let base = stem(name);
    let mut number = 1;
    loop {
        let name = if number == 1 {
            format!("{base}.jpg")
        } else {
            format!("{base}_{number}.jpg")
        };
        if used.insert(name.to_lowercase()) {
            return name;
        }
        number += 1;
    }
}
/// create_new prevents overwriting original photos or other exports, including
/// races with another running viewer. The returned path is the actual filename.
pub fn save_jpeg(path: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let parent = path.parent().ok_or("保存路径无效")?;
    let base = stem(&path.file_name().ok_or("保存路径无效")?.to_string_lossy());
    for n in 1..10000 {
        let output = parent.join(if n == 1 {
            format!("{base}.jpg")
        } else {
            format!("{base}_{n}.jpg")
        });
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
        {
            Ok(mut f) => {
                if let Err(e) = f.write_all(bytes).and_then(|_| f.sync_all()) {
                    drop(f);
                    let _ = fs::remove_file(&output);
                    return Err(e.to_string());
                }
                return Ok(output);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("同名文件过多，请选择其他文件名。".into())
}

// Small, streaming ZIP/Stored writer. JPEG is already compressed. Keeping it
// byte-for-byte avoids a second compression pass and makes the size cap exact.
// PKWARE APPNOTE 6.3.10, local header + central directory + EOCD; UTF-8 names.
struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}
struct ZipPart {
    file: Option<File>,
    temp: PathBuf,
    output: PathBuf,
    entries: Vec<Entry>,
    position: u64,
    central: u64,
    committed: bool,
}
fn u16le(w: &mut impl Write, n: u16) -> std::io::Result<()> {
    w.write_all(&n.to_le_bytes())
}
fn u32le(w: &mut impl Write, n: u32) -> std::io::Result<()> {
    w.write_all(&n.to_le_bytes())
}
impl ZipPart {
    fn new(directory: &Path, prefix: &str, part: usize) -> Result<Self, String> {
        for retry in 0..10000 {
            let extra = if retry == 0 {
                String::new()
            } else {
                format!("_{retry}")
            };
            let output = directory.join(format!("{prefix}{extra}-{part:03}.zip"));
            let temp = output.with_extension("zip.part");
            if output.exists() {
                continue;
            }
            match OpenOptions::new().write(true).create_new(true).open(&temp) {
                Ok(file) => {
                    return Ok(Self {
                        file: Some(file),
                        temp,
                        output,
                        entries: vec![],
                        position: 0,
                        central: 0,
                        committed: false,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("导出文件重名过多。".into())
    }
    fn fits(&self, name: &str, bytes: usize, limit: u64) -> bool {
        self.entries.len() < 65534
            && name.len() <= 65535
            && self.position + self.central + bytes as u64 + 76 + 2 * name.len() as u64 + 22 < limit
    }
    fn add(&mut self, name: String, bytes: &[u8]) -> std::io::Result<()> {
        let entry = Entry {
            name,
            crc: crc32fast::hash(bytes),
            size: bytes.len() as u32,
            offset: self.position as u32,
        };
        let f = self.file.as_mut().unwrap();
        u32le(f, 0x04034b50)?;
        u16le(f, 20)?;
        u16le(f, 0x800)?;
        u16le(f, 0)?;
        u16le(f, 0)?;
        u16le(f, 33)?;
        u32le(f, entry.crc)?;
        u32le(f, entry.size)?;
        u32le(f, entry.size)?;
        u16le(f, entry.name.len() as u16)?;
        u16le(f, 0)?;
        f.write_all(entry.name.as_bytes())?;
        f.write_all(bytes)?;
        self.position += 30 + entry.name.len() as u64 + bytes.len() as u64;
        self.central += 46 + entry.name.len() as u64;
        self.entries.push(entry);
        Ok(())
    }
    fn finish(mut self) -> Result<(String, usize), String> {
        let f = self.file.as_mut().unwrap();
        let result = (|| -> std::io::Result<()> {
            for e in &self.entries {
                u32le(f, 0x02014b50)?;
                u16le(f, 20)?;
                u16le(f, 20)?;
                u16le(f, 0x800)?;
                u16le(f, 0)?;
                u16le(f, 0)?;
                u16le(f, 33)?;
                u32le(f, e.crc)?;
                u32le(f, e.size)?;
                u32le(f, e.size)?;
                u16le(f, e.name.len() as u16)?;
                for _ in 0..4 {
                    u16le(f, 0)?;
                }
                u32le(f, 0)?;
                u32le(f, e.offset)?;
                f.write_all(e.name.as_bytes())?;
            }
            u32le(f, 0x06054b50)?;
            u16le(f, 0)?;
            u16le(f, 0)?;
            u16le(f, self.entries.len() as u16)?;
            u16le(f, self.entries.len() as u16)?;
            u32le(f, self.central as u32)?;
            u32le(f, self.position as u32)?;
            u16le(f, 0)?;
            f.sync_all()
        })();
        result.map_err(|e| e.to_string())?;
        drop(self.file.take());
        fs::rename(&self.temp, &self.output).map_err(|e| e.to_string())?;
        self.committed = true;
        Ok((
            self.output.to_string_lossy().into_owned(),
            self.entries.len(),
        ))
    }
}
impl Drop for ZipPart {
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.committed {
            let _ = fs::remove_file(&self.temp);
        }
    }
}

pub fn batch(
    engine: &Engine,
    ids: Vec<String>,
    directory: &Path,
    cancel: &AtomicBool,
    progress: impl Fn(ExportProgress),
    limit: u64,
) -> ExportReport {
    let mut seen = HashSet::new();
    let ids: Vec<_> = ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect();
    let mut report = ExportReport {
        total: ids.len(),
        ..Default::default()
    };
    if !directory.is_dir() || !(128..=ZIP_LIMIT).contains(&limit) {
        report.fatal = Some("导出目录或大小限制无效。".into());
        return report;
    }
    let prefix = format!("照片导出-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let names = engine
        .collection
        .read()
        .unwrap()
        .photos
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut used = HashSet::new();
    let mut part: Option<ZipPart> = None;
    let commit = |part: ZipPart, report: &mut ExportReport| -> Result<(), String> {
        let (file, count) = part.finish()?;
        report.files.push(file);
        report.exported += count;
        Ok(())
    };
    for (index, id) in ids.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let name = names.get(id).cloned().unwrap_or_else(|| id.clone());
        progress(ExportProgress {
            completed: index,
            total: ids.len(),
            name: name.clone(),
            phase: "正在转换 JPEG".into(),
        });
        let encoded = engine.export_source(id).and_then(|path| jpeg(&path, false));
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let encoded = match encoded {
            Ok(v) => v,
            Err(message) => {
                report.failed.push(ExportIssue { name, message });
                continue;
            }
        };
        let filename = jpeg_name(&name, &mut used);
        if encoded.bytes.len() as u64 + 76 + 2 * filename.len() as u64 + 22 >= limit {
            report.failed.push(ExportIssue {
                name,
                message: "单张 JPEG 超出 ZIP 大小上限；未降低质量或缩小尺寸。".into(),
            });
            continue;
        }
        if part
            .as_ref()
            .is_some_and(|p| !p.fits(&filename, encoded.bytes.len(), limit))
        {
            if let Err(error) = commit(part.take().unwrap(), &mut report) {
                report.fatal = Some(error);
                break;
            }
        }
        if part.is_none() {
            match ZipPart::new(directory, &prefix, report.files.len() + 1) {
                Ok(p) => part = Some(p),
                Err(e) => {
                    report.fatal = Some(e);
                    break;
                }
            }
        }
        progress(ExportProgress {
            completed: index,
            total: ids.len(),
            name: name.clone(),
            phase: "正在写入 ZIP".into(),
        });
        if let Err(e) = part.as_mut().unwrap().add(filename, &encoded.bytes) {
            report.fatal = Some(e.to_string());
            drop(part.take());
            break;
        }
        if let Some(message) = encoded.warning {
            report.warnings.push(ExportIssue { name, message });
        }
        progress(ExportProgress {
            completed: index + 1,
            total: ids.len(),
            name: String::new(),
            phase: "正在导出".into(),
        });
    }
    if let Some(part) = part {
        if let Err(e) = commit(part, &mut report) {
            report.fatal = Some(e);
        }
    }
    report
}

unsafe extern "C" {
    fn nv_copy_jpeg(owner: usize, bytes: *const u8, length: usize) -> i32;
}
pub fn copy_jpeg(owner: usize, bytes: &[u8]) -> Result<(), String> {
    let result = unsafe { nv_copy_jpeg(owner, bytes.as_ptr(), bytes.len()) };
    if result < 0 {
        Err(format!(
            "无法复制图片，剪贴板可能被其他程序占用（{result:#x}），请重试。"
        ))
    } else {
        Ok(())
    }
}
