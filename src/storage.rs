use rusqlite::{Connection, DatabaseName, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn data_dir() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("NBPHOTOVIEWER_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| ".".into()));
    let current = base.join("NBPhotoViewer");
    // The old name is intentionally kept only to import existing user marks.
    migrate_marks(&base.join("NEFViewer"), &current)?;
    Ok(current)
}

fn migrate_marks(legacy: &Path, current: &Path) -> Result<(), String> {
    let source = legacy.join("library.sqlite3");
    let destination = current.join("library.sqlite3");
    if destination.exists() || !source.is_file() {
        return Ok(());
    }
    fs::create_dir_all(current).map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let staging = current.join(format!(
        "marks-migration-{}-{nonce}.sqlite3",
        std::process::id()
    ));
    let result = (|| {
        // SQLite backup includes committed WAL records, unlike a plain file copy.
        let db = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        db.backup(DatabaseName::Main, &staging, None)
            .map_err(|e| e.to_string())?;
        // Publish without overwriting a library created by another app instance.
        match fs::hard_link(&staging, &destination) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    })();
    let _ = fs::remove_file(&staging);
    result.map_err(|e| format!("迁移旧版照片标记失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "nbphotoviewer-migration-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn paths(&self) -> (PathBuf, PathBuf) {
            (self.0.join("legacy"), self.0.join("current"))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn library(path: &Path, value: i32) -> Connection {
        fs::create_dir_all(path).unwrap();
        let db = Connection::open(path.join("library.sqlite3")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE marks(id TEXT PRIMARY KEY, path TEXT, marked INTEGER);").unwrap();
        db.execute(
            "INSERT INTO marks VALUES ('photo', 'sample.jpg', ?1)",
            [value],
        )
        .unwrap();
        db
    }
    fn value(path: &Path) -> i32 {
        Connection::open(path.join("library.sqlite3"))
            .unwrap()
            .query_row("SELECT marked FROM marks WHERE id='photo'", [], |r| {
                r.get(0)
            })
            .unwrap()
    }
    #[test]
    fn migration_preserves_committed_wal_and_original_library() {
        let fixture = Fixture::new();
        let (old, new) = fixture.paths();
        let db = library(&old, 1);
        assert!(old.join("library.sqlite3-wal").exists());
        migrate_marks(&old, &new).unwrap();
        assert_eq!(value(&new), 1);
        assert_eq!(value(&old), 1);
        assert_eq!(fs::read_dir(&new).unwrap().count(), 1);
        drop(db);
    }
    #[test]
    fn existing_library_is_never_overwritten() {
        let fixture = Fixture::new();
        let (old, new) = fixture.paths();
        let _old_db = library(&old, 1);
        let _new_db = library(&new, 0);
        migrate_marks(&old, &new).unwrap();
        assert_eq!(value(&new), 0);
    }
    #[test]
    fn no_legacy_library_needs_no_migration() {
        let fixture = Fixture::new();
        let (old, new) = fixture.paths();
        migrate_marks(&old, &new).unwrap();
        assert!(!new.exists());
    }
    #[test]
    fn failed_migration_does_not_publish_an_empty_library() {
        let fixture = Fixture::new();
        let (old, new) = fixture.paths();
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("library.sqlite3"), b"not a database").unwrap();
        assert!(migrate_marks(&old, &new).is_err());
        assert!(!new.join("library.sqlite3").exists());
        assert_eq!(fs::read_dir(&new).unwrap().count(), 0);
        let _db = library(&fixture.0.join("valid"), 1);
        migrate_marks(&fixture.0.join("valid"), &new).unwrap();
        assert_eq!(value(&new), 1);
    }
}
