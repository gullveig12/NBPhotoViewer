use std::{fs, path::Path};
fn sources(path: &Path, build: &mut cc::Build) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, build);
        } else if path.extension().is_some_and(|e| e == "cpp")
            && !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("_ph.cpp")
        {
            build.file(path);
        }
    }
}
fn main() {
    let webp = Path::new("vendor/libwebp-1.6.0");
    let mut decoder = cc::Build::new();
    decoder
        .include(webp)
        .opt_level(3)
        .warnings(false)
        .define("WEBP_USE_THREAD", None)
        .define("_CRT_SECURE_NO_WARNINGS", None);
    for entry in fs::read_dir(webp.join("src/dec")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "c") {
            decoder.file(path);
        }
    }
    for name in [
        "alpha_processing",
        "cpu",
        "dec",
        "dec_clip_tables",
        "filters",
        "lossless",
        "rescaler",
        "upsampling",
        "yuv",
    ] {
        for suffix in ["", "_sse2", "_sse41", "_avx2", "_neon"] {
            let path = webp.join(format!("src/dsp/{name}{suffix}.c"));
            if path.exists() {
                decoder.file(path);
            }
        }
    }
    for name in [
        "bit_reader_utils",
        "color_cache_utils",
        "filters_utils",
        "huffman_utils",
        "palette",
        "quant_levels_dec_utils",
        "rescaler_utils",
        "random_utils",
        "thread_utils",
        "utils",
    ] {
        decoder.file(webp.join(format!("src/utils/{name}.c")));
    }
    decoder.compile("webpdecoder");
    let root = Path::new("vendor/LibRaw-0.22.2");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(root)
        .include(webp)
        .define("LIBRAW_NODLL", None)
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .warnings(false)
        .opt_level(3)
        .flag_if_supported("/EHsc")
        .file("native/bridge.cpp");
    build.file("native/jpeg.cpp");
    build.file("native/webp.cpp");
    sources(&root.join("src"), &mut build);
    build.compile("nefraw");
    println!("cargo:rerun-if-changed=native/bridge.cpp");
    println!("cargo:rerun-if-changed=native/jpeg.cpp");
    println!("cargo:rerun-if-changed=native/webp.cpp");
    println!("cargo:rerun-if-changed=vendor/libwebp-1.6.0");
    println!("cargo:rustc-link-lib=windowscodecs");
    println!("cargo:rustc-link-lib=ole32");
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rerun-if-changed=vendor/LibRaw-0.22.2");
    tauri_build::build();
}
