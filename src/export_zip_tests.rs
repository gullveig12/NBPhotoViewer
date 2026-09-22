use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("nbphoto-zip-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn engine(&self) -> (Arc<Engine>, Vec<String>) {
        let sources: Vec<_> = ["a.png", "b.png", "unselected.png"]
            .iter()
            .map(|name| {
                let path = self.0.join(name);
                image::RgbImage::from_pixel(80, 48, image::Rgb([30, 115, 205]))
                    .save(&path)
                    .unwrap();
                path.to_string_lossy().into_owned()
            })
            .collect();
        let engine = Engine::new(self.0.join("cache")).unwrap();
        let photos = engine.select(sources).unwrap().photos;
        let ids = photos
            .iter()
            .filter(|p| p.name != "unselected.png")
            .map(|p| p.id.clone())
            .collect();
        (engine, ids)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path())
            && self
                .0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nbphoto-zip-test-")
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn filenames(report: &ExportReport) -> Vec<String> {
    assert!(report.fatal.is_none(), "{:?}", report.fatal);
    report
        .files
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}
fn assert_zip(path: &str, expected: usize, limit: u64) {
    let bytes = fs::read(path).unwrap();
    assert!((bytes.len() as u64) < limit);
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
    assert_eq!(
        u16::from_le_bytes(
            bytes[bytes.len() - 12..bytes.len() - 10]
                .try_into()
                .unwrap()
        ) as usize,
        expected
    );
}

#[test]
fn named_exports_continue_across_splits_batches_folders_and_prefix_changes() {
    let f = Fixture::new();
    let (engine, ids) = f.engine();
    let before = fs::read(f.0.join("a.png")).unwrap();
    let session = ZipSession::default();
    let path = f.0.join("北京旅游.zip");
    let cancel = AtomicBool::new(false);
    // Enough for exactly one picture per archive, including all ZIP headers.
    let limit = jpeg(&f.0.join("a.png"), false).unwrap().bytes.len() as u64 + 130;
    let first = batch_named(
        &engine,
        vec![ids[0].clone(), ids[1].clone(), ids[0].clone()],
        &path,
        &session,
        &cancel,
        |_| {},
        limit,
    );
    assert_eq!((first.total, first.exported), (2, 2));
    assert_eq!(filenames(&first), ["北京旅游-001.zip", "北京旅游-002.zip"]);
    for path in &first.files {
        assert_zip(path, 1, limit);
    }
    assert_eq!(session.default_path().unwrap(), path.to_string_lossy());
    let next = batch_named(
        &engine,
        ids.clone(),
        &path,
        &session,
        &cancel,
        |_| {},
        ZIP_LIMIT,
    );
    assert_eq!(filenames(&next), ["北京旅游-003.zip"]);
    assert_zip(&next.files[0], 2, ZIP_LIMIT);
    let other_name = batch_named(
        &engine,
        ids.clone(),
        &f.0.join("上海旅游"),
        &session,
        &cancel,
        |_| {},
        ZIP_LIMIT,
    );
    assert_eq!(filenames(&other_name), ["上海旅游-001.zip"]);
    let other_dir = f.0.join("another");
    fs::create_dir(&other_dir).unwrap();
    let back = batch_named(
        &engine,
        ids,
        &other_dir.join("北京旅游.ZIP"),
        &session,
        &cancel,
        |_| {},
        ZIP_LIMIT,
    );
    assert_eq!(filenames(&back), ["北京旅游-004.zip"]);
    assert_eq!(
        session.default_path().unwrap(),
        other_dir.join("北京旅游.zip").to_string_lossy()
    );
    assert_eq!(fs::read(f.0.join("a.png")).unwrap(), before);
    assert_eq!(engine.collection.read().unwrap().photos.len(), 3);
}

#[test]
fn existing_files_and_reserved_parts_are_skipped_even_after_session_reset() {
    let f = Fixture::new();
    let (engine, ids) = f.engine();
    fs::write(f.0.join("北京旅游-001.zip"), b"old zip").unwrap();
    fs::write(f.0.join("北京旅游-002.zip.part"), b"another process").unwrap();
    let path = f.0.join("北京旅游.zip");
    let session = ZipSession::default();
    let first = batch_named(
        &engine,
        ids.clone(),
        &path,
        &session,
        &AtomicBool::new(false),
        |_| {},
        ZIP_LIMIT,
    );
    assert_eq!(filenames(&first), ["北京旅游-003.zip"]);
    let fresh = ZipSession::default();
    assert_eq!(fresh.default_path().unwrap(), "照片导出.zip");
    let second = batch_named(
        &engine,
        ids,
        &path,
        &fresh,
        &AtomicBool::new(false),
        |_| {},
        ZIP_LIMIT,
    );
    assert_eq!(filenames(&second), ["北京旅游-004.zip"]);
    assert_eq!(fs::read(f.0.join("北京旅游-001.zip")).unwrap(), b"old zip");
    assert_eq!(
        fs::read(f.0.join("北京旅游-002.zip.part")).unwrap(),
        b"another process"
    );
}

#[test]
fn cancellation_keeps_valid_package_and_next_export_continues() {
    let f = Fixture::new();
    let (engine, ids) = f.engine();
    let session = ZipSession::default();
    let path = f.0.join("取消测试.zip");
    let cancel = AtomicBool::new(true);
    let stopped = batch_named(
        &engine,
        ids.clone(),
        &path,
        &session,
        &cancel,
        |_| panic!("no decoding after cancellation"),
        ZIP_LIMIT,
    );
    assert!(stopped.cancelled && stopped.files.is_empty());
    cancel.store(false, Ordering::Relaxed);
    let partial = batch_named(
        &engine,
        ids.clone(),
        &path,
        &session,
        &cancel,
        |p| {
            if p.completed == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        ZIP_LIMIT,
    );
    assert!(partial.cancelled);
    assert_eq!(partial.exported, 1);
    assert_eq!(filenames(&partial), ["取消测试-001.zip"]);
    assert_zip(&partial.files[0], 1, ZIP_LIMIT);
    let next = batch_named(
        &engine,
        ids,
        &path,
        &session,
        &AtomicBool::new(false),
        |_| {},
        ZIP_LIMIT,
    );
    assert_eq!(filenames(&next), ["取消测试-002.zip"]);
    assert!(
        !fs::read_dir(&f.0).unwrap().any(|p| p
            .unwrap()
            .path()
            .extension()
            .is_some_and(|e| e == "part"))
    );
}

#[test]
fn failures_without_output_do_not_consume_numbers_or_replace_valid_defaults() {
    let f = Fixture::new();
    let (engine, ids) = f.engine();
    let session = ZipSession::default();
    let path = f.0.join("旅行.zip");
    let cancel = AtomicBool::new(false);
    let rejected = batch_named(&engine, ids.clone(), &path, &session, &cancel, |_| {}, 128);
    assert_eq!(rejected.failed.len(), 2);
    assert!(rejected.files.is_empty());
    let failed = batch_named(
        &engine,
        vec!["stale-id".into()],
        &path,
        &session,
        &cancel,
        |_| {},
        ZIP_LIMIT,
    );
    assert_eq!(failed.failed.len(), 1);
    let invalid = batch_named(
        &engine,
        ids.clone(),
        &f.0.join("missing/other.zip"),
        &session,
        &cancel,
        |_| {},
        ZIP_LIMIT,
    );
    assert!(invalid.fatal.is_some());
    assert_eq!(session.default_path().unwrap(), path.to_string_lossy());
    let next = batch_named(&engine, ids, &path, &session, &cancel, |_| {}, ZIP_LIMIT);
    assert_eq!(filenames(&next), ["旅行-001.zip"]);
}

#[test]
fn unicode_dots_extensions_reserved_names_and_case_folding() {
    let f = Fixture::new();
    let session = ZipSession::default();
    for (name, prefix) in [
        ("北京.2026.ZIP", "北京.2026"),
        ("北京.2026", "北京.2026"),
        ("CON.zip", "_CON"),
        ("a?b.zip", "a_b"),
    ] {
        assert_eq!(session.prepare(&f.0.join(name)).unwrap().prefix, prefix);
    }
    let previous = session.default_path().unwrap();
    assert!(session.prepare(&f.0.join(".zip")).is_err());
    assert_eq!(session.default_path().unwrap(), previous);
    let a = session.prepare(&f.0.join("Trip.zip")).unwrap();
    let part = session.next_part(&a).unwrap();
    assert_eq!(part.number, 1);
    drop(part);
    let b = session.prepare(&f.0.join("trip.zip")).unwrap();
    assert_eq!(session.next_part(&b).unwrap().number, 2);
}

#[test]
fn file_appearing_during_export_is_never_overwritten() {
    let f = Fixture::new();
    let mut part = ZipPart::new(&f.0, "collision", 1).unwrap();
    let temp = part.temp.clone();
    let output = part.output.clone();
    part.add("a.jpg".into(), b"test data").unwrap();
    fs::write(&output, b"created by someone else").unwrap();
    assert!(part.finish().is_err());
    assert_eq!(fs::read(output).unwrap(), b"created by someone else");
    assert!(!temp.exists());
}
