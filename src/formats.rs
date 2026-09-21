use crate::raw::Metadata;
use exif::{In, Tag, Value};
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader, metadata::Orientation};
use std::{
    fs::File,
    io::{BufReader, Cursor, Read},
    path::Path,
};

pub const IMAGES: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff", "webp", "bmp", "gif"];
pub const RAWS: &[&str] = &[
    "nef", "nrw", "cr2", "cr3", "crw", "arw", "sr2", "srf", "raf", "dng", "orf", "rw2", "pef",
    "srw", "rwl", "3fr", "fff", "iiq", "kdc", "dcr", "mos", "mrw", "x3f",
];
pub fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
}
pub fn is_raw(path: &Path) -> bool {
    RAWS.contains(&extension(path).as_str())
}
pub fn supported(path: &Path) -> bool {
    is_raw(path) || IMAGES.contains(&extension(path).as_str())
}
pub fn direct_mime(path: &Path) -> Option<&'static str> {
    match extension(path).as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}
pub fn read_exif(path: &Path) -> Option<exif::Exif> {
    let mut file = BufReader::new(File::open(path).ok()?);
    // Partially damaged metadata must not prevent a usable photograph opening.
    exif::Reader::new()
        .continue_on_error(true)
        .read_from_container(&mut file)
        .or_else(|e| e.distill_partial_result(|_| {}))
        .ok()
}
fn number(exif: &exif::Exif, tag: Tag) -> f32 {
    let Some(field) = exif.get_field(tag, In::PRIMARY) else {
        return 0.0;
    };
    let v = match &field.value {
        Value::Rational(v) => v
            .first()
            .filter(|r| r.denom != 0)
            .map(|r| r.to_f64())
            .unwrap_or(0.0),
        Value::SRational(v) => v
            .first()
            .filter(|r| r.denom != 0)
            .map(|r| r.to_f64())
            .unwrap_or(0.0),
        v => v.get_uint(0).unwrap_or(0) as f64,
    };
    if v.is_finite() && v > 0.0 {
        v as f32
    } else {
        0.0
    }
}
fn ascii(exif: &exif::Exif, tag: Tag) -> String {
    match exif.get_field(tag, In::PRIMARY).map(|f| &f.value) {
        Some(Value::Ascii(v)) => v
            .first()
            .map(|v| {
                String::from_utf8_lossy(v)
                    .trim_matches(['\0', ' '])
                    .to_string()
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}
pub fn orientation(exif: Option<&exif::Exif>) -> Orientation {
    let value = exif
        .and_then(|e| e.get_field(Tag::Orientation, In::PRIMARY))
        .and_then(|f| f.value.get_uint(0))
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1);
    Orientation::from_exif(value as u8).unwrap_or(Orientation::NoTransforms)
}
pub fn metadata(path: &Path) -> Result<Metadata, String> {
    let (mut width, mut height) = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    let exif = read_exif(path);
    if orientation(exif.as_ref()).to_exif() >= 5 {
        std::mem::swap(&mut width, &mut height);
    }
    let mut meta = Metadata {
        width,
        height,
        ..Metadata::default()
    };
    if let Some(e) = exif {
        meta.aperture = number(&e, Tag::FNumber);
        meta.shutter = number(&e, Tag::ExposureTime);
        meta.iso = number(&e, Tag::PhotographicSensitivity);
        if meta.iso == 0.0 || meta.iso == 65535.0 {
            meta.iso = number(&e, Tag::ISOSpeed);
        }
        meta.focal = number(&e, Tag::FocalLength);
        meta.model = ascii(&e, Tag::Model);
        meta.lens = ascii(&e, Tag::LensModel);
        // EXIF usually has no timezone. Preserve the camera's wall-clock time;
        // file modification time is never a substitute for capture time.
        let value = ascii(&e, Tag::DateTimeOriginal);
        if let Ok(date) = chrono::NaiveDateTime::parse_from_str(&value, "%Y:%m:%d %H:%M:%S") {
            meta.captured_at = date.format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }
    Ok(meta)
}
pub fn decode(path: &Path) -> Result<DynamicImage, String> {
    let exif = read_exif(path);
    let mut reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(768 * 1024 * 1024);
    reader.limits(limits);
    let mut image = reader.decode().map_err(|e| format!("无法读取图片：{e}"))?;
    image.apply_orientation(orientation(exif.as_ref()));
    Ok(image)
}
/// Decode at JPEG's reduced IDCT resolution before resizing. A 24 MP image
/// becomes about 0.38 MP, so rotation/resampling never touches the full bitmap.
pub fn jpeg_thumbnail(path: &Path) -> Result<Vec<u8>, String> {
    match windows_jpeg_thumbnail(path) {
        Ok(bytes) => Ok(bytes),
        Err(_) => portable_jpeg_thumbnail(path),
    }
}
#[repr(C)]
struct NativeJpeg {
    data: *mut u8,
    length: usize,
    width: u32,
    height: u32,
}
unsafe extern "C" {
    fn nv_jpeg_scaled(path: *const u16, edge: u32, out: *mut NativeJpeg) -> i32;
    fn nv_jpeg_memory_scaled(
        bytes: *const u8,
        length: usize,
        edge: u32,
        out: *mut NativeJpeg,
    ) -> i32;
    fn nv_raster_scaled(path: *const u16, edge: u32, out: *mut NativeJpeg) -> i32;
    fn nv_webp_scaled(bytes: *const u8, length: usize, edge: u32, out: *mut NativeJpeg) -> i32;
    fn nv_free(data: *mut u8);
}
fn windows_jpeg_thumbnail(path: &Path) -> Result<Vec<u8>, String> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut out: NativeJpeg = unsafe { std::mem::zeroed() };
    if unsafe { nv_jpeg_scaled(wide.as_ptr(), 360, &mut out) } < 0 {
        return Err("使用兼容解码器".into());
    }
    let mut image = take_native(out)?;
    image.apply_orientation(orientation(read_exif(path).as_ref()));
    thumbnail(&image)
}
fn take_native(out: NativeJpeg) -> Result<DynamicImage, String> {
    if out.data.is_null() {
        return Err("JPEG 数据为空".into());
    }
    let valid = (out.width as usize)
        .checked_mul(out.height as usize)
        .and_then(|n| n.checked_mul(3))
        == Some(out.length)
        && out.length > 0
        && out.length <= 64 * 1024 * 1024;
    let pixels = if valid {
        unsafe { std::slice::from_raw_parts(out.data, out.length) }.to_vec()
    } else {
        Vec::new()
    };
    unsafe { nv_free(out.data) };
    if !valid {
        return Err("JPEG 尺寸无效".into());
    }
    Ok(DynamicImage::ImageRgb8(
        image::RgbImage::from_raw(out.width, out.height, pixels).ok_or("JPEG 尺寸无效")?,
    ))
}
/// Native thumbnail paths never replace full-resolution photo decoding.
pub fn raster_thumbnail(path: &Path) -> Result<Vec<u8>, String> {
    use std::os::windows::ffi::OsStrExt;
    // Rust's PNG decoder benchmarks faster than WIC on photographic PNGs;
    // keep GIF's logical-canvas/first-frame behavior too. Both run in the pool.
    if matches!(extension(path).as_str(), "png" | "gif") {
        return thumbnail(&decode(path)?);
    }
    let mut out: NativeJpeg = unsafe { std::mem::zeroed() };
    let result = if extension(path) == "webp" {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|e| e.to_string())?
            .take(128 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 128 * 1024 * 1024 {
            return Err("使用兼容解码器".into());
        }
        unsafe { nv_webp_scaled(bytes.as_ptr(), bytes.len(), 360, &mut out) }
    } else {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe { nv_raster_scaled(wide.as_ptr(), 360, &mut out) }
    };
    if result < 0 {
        return Err("使用兼容解码器".into());
    }
    let mut image = take_native(out)?;
    image.apply_orientation(orientation(read_exif(path).as_ref()));
    thumbnail(&image)
}
pub fn embedded_jpeg_thumbnail(bytes: &[u8], flip: i32) -> Result<Vec<u8>, String> {
    let (w, h) = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    if u64::from(w) * u64::from(h) <= 1_000_000 {
        // Tiny camera previews cost less to decode directly than to initialize WIC.
        let mut image = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
        image.apply_orientation(raw_orientation(Some(bytes), flip));
        return thumbnail(&image);
    }
    let mut out: NativeJpeg = unsafe { std::mem::zeroed() };
    let mut image =
        if unsafe { nv_jpeg_memory_scaled(bytes.as_ptr(), bytes.len(), 360, &mut out) } >= 0 {
            take_native(out)?
        } else {
            scaled_jpeg(Cursor::new(bytes))?.0
        };
    image.apply_orientation(raw_orientation(Some(bytes), flip));
    thumbnail(&image)
}
fn portable_jpeg_thumbnail(path: &Path) -> Result<Vec<u8>, String> {
    let file = BufReader::with_capacity(128 * 1024, File::open(path).map_err(|e| e.to_string())?);
    let (mut image, exif) = scaled_jpeg(file)?;
    image.apply_orientation(orientation(exif.as_ref()));
    thumbnail(&image)
}
fn scaled_jpeg(reader: impl Read) -> Result<(DynamicImage, Option<exif::Exif>), String> {
    let mut decoder = jpeg_decoder::Decoder::new(reader);
    decoder.set_max_decoding_buffer_size(128 * 1024 * 1024);
    decoder.scale(360, 360).map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("JPEG 尺寸无效")?;
    // Keep unusual/lossless/CMYK JPEGs on the existing, separately bounded path.
    if !matches!(
        info.pixel_format,
        jpeg_decoder::PixelFormat::RGB24 | jpeg_decoder::PixelFormat::L8
    ) {
        return Err("使用兼容解码器".into());
    }
    let pixels = decoder.decode().map_err(|e| e.to_string())?;
    let image = match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => DynamicImage::ImageLuma8(
            image::GrayImage::from_raw(info.width.into(), info.height.into(), pixels)
                .ok_or("JPEG 尺寸无效")?,
        ),
        _ => DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(info.width.into(), info.height.into(), pixels)
                .ok_or("JPEG 尺寸无效")?,
        ),
    };
    let exif = decoder.exif_data().and_then(|bytes| {
        exif::Reader::new()
            .continue_on_error(true)
            .read_raw(bytes.to_vec())
            .or_else(|e| e.distill_partial_result(|_| {}))
            .ok()
    });
    Ok((image, exif))
}
pub fn png(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}
pub fn png_with_profile(image: &DynamicImage, path: &Path) -> Result<Vec<u8>, String> {
    let profile = ImageReader::open(path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.into_decoder().ok())
        .and_then(|mut d| d.icc_profile().ok().flatten());
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    if let Some(profile) = profile {
        encoder
            .set_icc_profile(profile)
            .map_err(|e| e.to_string())?;
    }
    image
        .write_with_encoder(encoder)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
pub fn raw_orientation(bytes: Option<&[u8]>, flip: i32) -> Orientation {
    if let Some(exif) = bytes.and_then(|b| {
        exif::Reader::new()
            .read_from_container(&mut Cursor::new(b))
            .ok()
    }) {
        if exif.get_field(Tag::Orientation, In::PRIMARY).is_some() {
            return orientation(Some(&exif));
        }
    }
    let exif_value = [1, 2, 4, 3, 5, 8, 6, 7]
        .get(flip as usize)
        .copied()
        .unwrap_or(1);
    Orientation::from_exif(exif_value).unwrap()
}
pub fn orient_preview_jpeg(bytes: Vec<u8>, flip: i32) -> Vec<u8> {
    // Some cameras omit EXIF from embedded JPEGs. Supply orientation without
    // re-encoding pixels; retain existing JPEG EXIF/ICC when present.
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return bytes;
    }
    let orientation = raw_orientation(Some(&bytes), flip).to_exif();
    let has_orientation = exif::Reader::new()
        .read_from_container(&mut Cursor::new(&bytes))
        .ok()
        .is_some_and(|e| e.get_field(Tag::Orientation, In::PRIMARY).is_some());
    if has_orientation || orientation == 1 {
        return bytes;
    }
    let exif = [
        b'E',
        b'x',
        b'i',
        b'f',
        0,
        0,
        b'I',
        b'I',
        42,
        0,
        8,
        0,
        0,
        0,
        1,
        0,
        0x12,
        1,
        3,
        0,
        1,
        0,
        0,
        0,
        orientation,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    let mut out = Vec::with_capacity(bytes.len() + 36);
    out.extend_from_slice(&[0xff, 0xd8, 0xff, 0xe1, 0, 34]);
    out.extend_from_slice(&exif);
    out.extend_from_slice(&bytes[2..]);
    out
}
pub fn thumbnail(image: &DynamicImage) -> Result<Vec<u8>, String> {
    // Composite transparency onto the same neutral tone as the photo area.
    let mut view = image
        .thumbnail(360.min(image.width()), 360.min(image.height()))
        .to_rgba8();
    for p in view.pixels_mut() {
        let a = p[3] as u16;
        for c in &mut p.0[..3] {
            *c = ((*c as u16 * a + 29 * (255 - a) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
        .encode_image(&DynamicImage::ImageRgba8(view).to_rgb8())
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
