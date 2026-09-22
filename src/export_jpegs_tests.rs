use super::*;
use std::{
    cell::RefCell,
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("nbphoto-jpegs-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn picture(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        image::RgbImage::from_pixel(80, 48, image::Rgb([30, 115, 205]))
            .save(&path)
            .unwrap();
        path
    }
    fn engine(&self) -> Arc<Engine> {
        Engine::new(self.0.join("cache")).unwrap()
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
                .starts_with("nbphoto-jpegs-test-")
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn selected_only_deduplicates_and_preserves_sources_and_existing_outputs() {
    let f = Fixture::new();
    let sources = [
        f.picture("same.jpg"),
        f.picture("same.png"),
        f.picture("unused.bmp"),
    ];
    let original: Vec<_> = sources.iter().map(|p| fs::read(p).unwrap()).collect();
    let existing = f.0.join("same_2.jpg");
    fs::write(&existing, b"existing export").unwrap();
    let engine = f.engine();
    let collection = engine
        .select(
            sources
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        )
        .unwrap();
    let mut ids: Vec<_> = collection
        .photos
        .iter()
        .filter(|p| p.name.starts_with("same"))
        .map(|p| p.id.clone())
        .collect();
    ids.push(ids[0].clone());
    let events = RefCell::new(vec![]);
    let report = batch_jpegs(&engine, ids, &f.0, &AtomicBool::new(false), |p| {
        events.borrow_mut().push(p.completed)
    });
    assert_eq!(
        (report.total, report.exported, report.files.len()),
        (2, 2, 2)
    );
    assert!(report.failed.is_empty() && report.fatal.is_none() && !report.cancelled);
    for file in &report.files {
        assert!(file.ends_with(".jpg"));
        let bytes = fs::read(file).unwrap();
        assert_eq!(&bytes[..2], b"\xff\xd8");
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (80, 48));
    }
    for (source, before) in sources.iter().zip(original) {
        assert_eq!(fs::read(source).unwrap(), before);
    }
    assert_eq!(fs::read(existing).unwrap(), b"existing export");
    assert_eq!(engine.collection.read().unwrap().photos.len(), 3);
    assert_eq!(events.borrow().last(), Some(&2));
    assert!(!fs::read_dir(&f.0).unwrap().any(|p| {
        p.unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "zip")
    }));
}

#[test]
fn cancellation_keeps_completed_jpegs_and_stops_remaining_photos() {
    let f = Fixture::new();
    let sources = [f.picture("a.png"), f.picture("b.png")];
    let engine = f.engine();
    let collection = engine
        .select(
            sources
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        )
        .unwrap();
    let ids: Vec<_> = collection.photos.iter().map(|p| p.id.clone()).collect();
    let cancel = AtomicBool::new(false);
    let report = batch_jpegs(&engine, ids.clone(), &f.0, &cancel, |p| {
        if p.completed == 1 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(report.cancelled);
    assert_eq!(report.exported, 1);
    assert_eq!(report.files.len(), 1);
    image::load_from_memory(&fs::read(&report.files[0]).unwrap()).unwrap();
    assert!(!f.0.join("b.jpg").exists());
    let stopped = batch_jpegs(&engine, ids, &f.0, &cancel, |_| {
        panic!("cancelled job must not start decoding")
    });
    assert!(stopped.cancelled && stopped.files.is_empty());
}

#[test]
fn corrupt_or_changed_sources_do_not_block_valid_photos() {
    let f = Fixture::new();
    let good = f.picture("good.png");
    let changed = f.picture("changed.bmp");
    let broken = f.0.join("broken.jpg");
    fs::write(&broken, b"invalid photo").unwrap();
    let engine = f.engine();
    let collection = engine
        .select(
            vec![good, changed.clone(), broken]
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        )
        .unwrap();
    fs::write(changed, b"replacement").unwrap();
    let events = RefCell::new(vec![]);
    let report = batch_jpegs(
        &engine,
        collection.photos.iter().map(|p| p.id.clone()).collect(),
        &f.0,
        &AtomicBool::new(false),
        |p| events.borrow_mut().push(p.completed),
    );
    assert_eq!(
        (report.total, report.exported, report.failed.len()),
        (3, 1, 2)
    );
    assert!(report.fatal.is_none());
    assert_eq!(events.borrow().last(), Some(&3));
    assert!(report.files[0].ends_with("good.jpg"));
}

#[test]
fn invalid_directory_and_cancellation_during_conversion_write_nothing() {
    let f = Fixture::new();
    let source = f.picture("one.png");
    let engine = f.engine();
    let collection = engine
        .select(vec![source.to_string_lossy().into_owned()])
        .unwrap();
    let ids = vec![collection.photos[0].id.clone()];
    let cancel = AtomicBool::new(false);
    let report = batch_jpegs(&engine, ids.clone(), &f.0.join("missing"), &cancel, |_| {});
    assert!(report.fatal.is_some() && report.files.is_empty());
    let report = batch_jpegs(&engine, ids, &f.0, &cancel, |_| {
        cancel.store(true, Ordering::Relaxed)
    });
    assert!(report.cancelled && report.files.is_empty());
    assert!(!f.0.join("one.jpg").exists());
}
