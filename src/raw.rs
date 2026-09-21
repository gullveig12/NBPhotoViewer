use serde::Serialize;
use std::{
    ffi::{CStr, c_char},
    os::windows::ffi::OsStrExt,
    path::Path,
};

#[repr(C)]
struct NativeImage {
    data: *mut u8,
    length: usize,
    width: u32,
    height: u32,
    raw_width: u32,
    raw_height: u32,
    format: i32,
    flip: i32,
    raw_supported: i32,
    aperture: f32,
    shutter: f32,
    iso: f32,
    focal: f32,
    timestamp: i64,
    model: [c_char; 80],
    lens: [c_char; 128],
    error: [c_char; 256],
}
unsafe extern "C" {
    fn nv_load(path: *const u16, mode: i32, image: *mut NativeImage) -> i32;
    fn nv_free(data: *mut u8);
}
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub width: u32,
    pub height: u32,
    pub flip: i32,
    pub aperture: f32,
    pub shutter: f32,
    pub iso: f32,
    pub focal: f32,
    pub timestamp: i64,
    pub captured_at: String,
    pub model: String,
    pub lens: String,
    pub raw_supported: bool,
}
pub struct Decoded {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub jpeg: bool,
    pub meta: Metadata,
}
pub fn load(path: &Path, mode: i32) -> Result<Decoded, String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut n: NativeImage = unsafe { std::mem::zeroed() };
    let result = unsafe { nv_load(wide.as_ptr(), mode, &mut n) };
    if result != 0 {
        let error = unsafe { CStr::from_ptr(n.error.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        return Err(format!(
            "无法解码此 RAW（{error}）。该机型或压缩方式可能暂不受支持。"
        ));
    }
    let bytes = if n.data.is_null() {
        Vec::new()
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(n.data, n.length) }.to_vec();
        unsafe { nv_free(n.data) };
        bytes
    };
    let (w, h) = if matches!(n.flip, 5 | 6) {
        (n.raw_height, n.raw_width)
    } else {
        (n.raw_width, n.raw_height)
    };
    Ok(Decoded {
        bytes,
        width: n.width,
        height: n.height,
        jpeg: n.format == 1,
        meta: Metadata {
            width: w,
            height: h,
            flip: n.flip,
            aperture: n.aperture,
            shutter: n.shutter,
            iso: n.iso,
            focal: n.focal,
            timestamp: n.timestamp,
            captured_at: String::new(),
            raw_supported: n.raw_supported != 0,
            model: unsafe { CStr::from_ptr(n.model.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
            lens: unsafe { CStr::from_ptr(n.lens.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        },
    })
}
