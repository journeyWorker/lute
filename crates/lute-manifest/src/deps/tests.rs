use super::*;
use lute_scratch::ScratchDir;
use std::time::{Duration, SystemTime};

/// Set `path`'s mtime to `at` (what `touch -d` does).
fn set_mtime(path: &Path, at: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(at))
        .unwrap();
}

fn mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

/// A file record read through a recording sink.
fn recorded(path: &Path) -> Dependency {
    let sink = DependencySink::recording();
    sink.read_to_string(path).unwrap();
    let mut records = sink.into_records();
    assert_eq!(records.len(), 1);
    records.remove(0)
}

/// Mark `record` as old enough not to be racy, as a record read more than
/// [`RACY_NS`] after its file's last write is.
fn settled(mut record: Dependency) -> Dependency {
    if let Dependency::File {
        content: Content::Read { racy, .. },
        ..
    } = &mut record
    {
        *racy = false;
    }
    record
}

#[test]
fn an_unchanged_file_keeps_its_fingerprint() {
    let dir = ScratchDir::named("fp-same");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a: 1\n").unwrap();
    assert_eq!(
        Fingerprint::of(&file).unwrap(),
        Fingerprint::of(&file).unwrap()
    );
    assert!(settled(recorded(&file)).revalidate(false));
}

#[test]
fn a_touched_unchanged_file_changes_its_fingerprint_and_keeps_its_record() {
    let dir = ScratchDir::named("fp-touch");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a: 1\n").unwrap();
    let before = Fingerprint::of(&file).unwrap();
    let mut record = settled(recorded(&file));
    set_mtime(&file, mtime(&file) + Duration::from_secs(5));
    assert_ne!(Fingerprint::of(&file).unwrap(), before);
    assert!(record.revalidate(false), "same bytes keep the record");
    let Dependency::File {
        content: Content::Read { fingerprint, .. },
        ..
    } = &record
    else {
        panic!("a read record")
    };
    assert_eq!(
        *fingerprint,
        Some(Fingerprint::of(&file).unwrap()),
        "the recorded tuple is updated"
    );
}

#[test]
fn an_edited_file_fails_its_record() {
    let dir = ScratchDir::named("fp-edit");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a: 1\n").unwrap();
    let mut record = settled(recorded(&file));
    std::fs::write(&file, "a: 12\n").unwrap();
    assert!(!record.revalidate(false));
}

#[cfg(unix)]
#[test]
fn an_equal_length_edit_with_its_mtime_restored_changes_the_ctime() {
    let dir = ScratchDir::named("fp-ctime");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a: 1\n").unwrap();
    let old = mtime(&file);
    let before = Fingerprint::of(&file).unwrap();
    let mut record = settled(recorded(&file));
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(&file, "a: 2\n").unwrap();
    set_mtime(&file, old);
    let after = Fingerprint::of(&file).unwrap();
    assert_eq!(
        (after.len, after.modified_ns),
        (before.len, before.modified_ns)
    );
    assert_ne!(after.changed_ns, before.changed_ns, "ctime moved");
    assert!(!record.revalidate(false), "detected through ctime");
}

#[test]
fn a_replace_by_rename_changes_the_file_identity() {
    let dir = ScratchDir::named("fp-rename");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a: 1\n").unwrap();
    let old = mtime(&file);
    let before = Fingerprint::of(&file).unwrap();
    let mut record = settled(recorded(&file));
    // Another file of equal length, given the old mtime, renamed over it.
    let next = dir.join("a.yaml.new");
    std::fs::write(&next, "a: 2\n").unwrap();
    set_mtime(&next, old);
    std::fs::rename(&next, &file).unwrap();
    let after = Fingerprint::of(&file).unwrap();
    assert_eq!(
        (after.len, after.modified_ns),
        (before.len, before.modified_ns)
    );
    assert_ne!(after.id, before.id, "a new inode (file index)");
    assert!(!record.revalidate(false));
}

#[test]
fn a_dirty_record_compares_content_whatever_its_fingerprint() {
    let dir = ScratchDir::named("fp-dirty");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a: 1\n").unwrap();
    let mut record = settled(recorded(&file));
    let Dependency::File { content, .. } = &mut record else {
        panic!("a read record")
    };
    // Tampering that preserves every fingerprint field: the recorded hash
    // no longer matches the bytes, which only a content compare sees.
    let Content::Read { hash, .. } = content else {
        panic!("read")
    };
    hash[0] ^= 1;
    assert!(record.revalidate(false), "a matching tuple is trusted");
    assert!(!record.revalidate(true), "a dirty mark compares content");
}

#[test]
fn a_freshly_written_file_is_racy_and_always_compared() {
    let dir = ScratchDir::named("fp-racy");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a: 1\n").unwrap();
    let mut record = recorded(&file);
    assert!(matches!(
        &record,
        Dependency::File {
            content: Content::Read { racy: true, .. },
            ..
        }
    ));
    let Dependency::File {
        content: Content::Read { hash, .. },
        ..
    } = &mut record
    else {
        panic!("read")
    };
    hash[0] ^= 1;
    assert!(!record.revalidate(false));
}

#[test]
fn lookups_record_missing_created_and_retargeted_paths() {
    let dir = ScratchDir::named("deps-lookup");
    let path = dir.join("b.yaml");
    let sink = DependencySink::recording();
    assert!(!sink.is_file(&path));
    let mut records = sink.into_records();
    assert!(matches!(
        &records[0],
        Dependency::Lookup {
            outcome: Outcome::Missing,
            ..
        }
    ));
    assert!(records[0].revalidate(false));
    std::fs::write(&path, "x").unwrap();
    assert!(!records[0].revalidate(false), "a created file");
}

#[test]
fn a_listing_fails_on_a_matching_file_only() {
    let dir = ScratchDir::named("deps-listing");
    std::fs::write(dir.join("a.yaml"), "x").unwrap();
    let sink = DependencySink::recording();
    let listing = sink.list(&dir, ListFilter::Yaml).unwrap();
    assert_eq!(listing.entries.len(), 1);
    let mut records = sink.into_records();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    assert!(records[0].revalidate(false), "a non-matching file");
    std::fs::write(dir.join("b.yml"), "x").unwrap();
    assert!(!records[0].revalidate(false), "a matching file");
}

#[test]
fn the_none_sink_records_nothing() {
    let dir = ScratchDir::named("deps-none");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a").unwrap();
    let sink = DependencySink::none();
    assert_eq!(sink.read_to_string(&file).unwrap(), "a");
    assert!(sink.is_file(&file));
    assert!(!sink.is_recording());
}
