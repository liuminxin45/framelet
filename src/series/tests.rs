use super::{args::SeriesArgs, model::*, store};
use std::path::{Path, PathBuf};
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "framelet-series-test-{}",
            crate::identity::new_id().unwrap()
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        assert!(
            self.0.starts_with(std::env::temp_dir())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("framelet-series-test-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn args(root: &Path) -> SeriesArgs {
    SeriesArgs {
        output_dir: root.into(),
        duration: Some(650.0),
        region: Some(crate::capture::Region {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        }),
        ..Default::default()
    }
}
fn stats(path: PathBuf) -> crate::encode::Stats {
    crate::encode::Stats {
        playback: None,
        capture_metrics: None,
        output: path,
        width: 8,
        height: 8,
        frames: 1,
        samples: 1,
        duration_ms: 1000,
        bytes: 100,
        timeline: None,
        manifest: None,
        warnings: vec![],
    }
}

#[test]
fn independent_total_and_segment_limits_and_request_identity() {
    let root = Root::new();
    let mut a = args(&root.0);
    assert!(a.validate().is_ok());
    a.segment_seconds = 301;
    assert!(a.validate().is_err());
    a.segment_seconds = 300;
    a.until_stop = true;
    assert!(a.validate().is_err());
    a.until_stop = false;
    a.name = "CON".into();
    assert!(a.validate().is_err());
    a.name = "demo".into();
    a.request_id = Some("test-001".into());
    let p = store::create(&a).unwrap();
    assert_eq!(store::create(&a).unwrap().directory, p.directory);
    a.fps = 5;
    assert!(store::create(&a).is_err());
}

#[test]
fn index_paths_ranges_and_atomic_replacement() {
    let root = Root::new();
    let p = store::create(&args(&root.0)).unwrap();
    let mut i = p.index;
    let part = Part {
        number: 1,
        start_ms: 0,
        reason: "segment_time".into(),
        recording: stats(store::filename(&i.prefix, 1).into()),
        observation_range: Some([0, 10]),
        utc_range: None,
    };
    i.parts.push(part);
    store::save_index(&p.directory, &i).unwrap();
    assert_eq!(store::read_index(&p.directory).unwrap().parts.len(), 1);
    let previous: Index =
        store::read_json(&p.directory.join("recording.previous.json"), INDEX_LIMIT).unwrap();
    assert!(previous.parts.is_empty());
    let mut bad = i.clone();
    bad.parts[0].recording.output = "..\\outside.gif".into();
    assert!(store::validate_index(&bad).is_err());
    bad = i.clone();
    bad.parts[0].recording.duration_ms = 300010;
    assert!(store::validate_index(&bad).is_err());
    bad = i.clone();
    let mut next = bad.parts[0].clone();
    next.number = 2;
    next.recording.output = store::filename(&i.prefix, 2).into();
    next.start_ms = 990;
    bad.parts.push(next);
    assert!(store::validate_index(&bad).is_err());
    bad = i;
    bad.parts[0].observation_range = Some([10, 0]);
    assert!(store::validate_index(&bad).is_err());
}

#[test]
fn checkpoint_prefix_integrity_recovery_and_idempotency() {
    use std::io::Read;
    let root = Root::new();
    let plan = store::create(&args(&root.0)).unwrap();
    let group = &plan.directory;
    let mut i = plan.index;
    i.owner_pid = 0;
    i.owner_created = 0;
    i.state = "failed".into();
    store::save_index(group, &i).unwrap();
    let target = group.join(store::filename(&i.prefix, 1));
    let mut w =
        crate::encode::GifWriter::new_in(&target, 8, 8, 10, Some(&group.join(".work"))).unwrap();
    w.push(vec![42; 8 * 8 * 3], 100).unwrap();
    let bytes = w.checkpoint().unwrap();
    let source = w.partial_path().to_path_buf();
    let sha =
        crate::identity::sha256_reader(std::fs::File::open(&source).unwrap().take(bytes)).unwrap();
    let stat = w.snapshot_stats(bytes + 1);
    let timeline = crate::timeline::Data {
        schema: 2,
        session_id: i.session_id.clone(),
        asset_id: Some(crate::identity::new_id().unwrap()),
        gif_sha256: None,
        region: crate::capture::Region {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        },
        width: 8,
        height: 8,
        gif_bytes: bytes + 1,
        duration_ms: 1000,
        frames: 1,
        samples: vec![],
        events: vec![],
        dropped_events: 0,
        warnings: vec![],
    };
    let mut c = Checkpoint {
        schema: 1,
        session_id: i.session_id.clone(),
        number: 1,
        start_ms: 0,
        source: source.file_name().unwrap().to_string_lossy().into_owned(),
        target: target.file_name().unwrap().to_string_lossy().into_owned(),
        safe_bytes: bytes,
        gif_prefix_sha256: sha,
        recording: stat,
        timeline,
    };
    drop(w);
    let original = std::fs::read(&source).unwrap();
    let cp = group
        .join(".work")
        .join(target.file_name().unwrap())
        .with_extension("checkpoint.json");
    let mut altered = original.clone();
    altered[0] ^= 1;
    std::fs::write(&source, altered).unwrap();
    crate::storage::replace_json(&cp, &c).unwrap();
    let result = super::recover(group).unwrap();
    assert_eq!(result["recovered_parts"], 0);
    assert_eq!(result["state"], "failed");
    assert!(!target.exists());
    std::fs::write(&source, &original).unwrap();
    let good_source = c.source.clone();
    c.source = "..\\outside.partial".into();
    crate::storage::replace_json(&cp, &c).unwrap();
    assert_eq!(super::recover(group).unwrap()["recovered_parts"], 0);
    assert!(!target.exists());
    c.source = good_source;
    crate::storage::replace_json(&cp, &c).unwrap();
    let good = super::recover(group).unwrap();
    assert_eq!(good["recovered_parts"], 1);
    assert_eq!(good["state"], "partial");
    assert!(crate::timeline::load(&target).unwrap().is_some());
    assert_eq!(super::recover(group).unwrap()["recovered_parts"], 0);
    assert!(source.exists());
}

#[test]
fn empty_series_writer_leaves_no_unfinished_artifact() {
    let root = Root::new();
    let plan = store::create(&args(&root.0)).unwrap();
    let path = plan.directory.join(store::filename(&plan.index.prefix, 1));
    let w = crate::encode::GifWriter::new_in(&path, 8, 8, 10, Some(&plan.directory.join(".work")))
        .unwrap();
    let partial = w.partial_path().to_path_buf();
    assert!(partial.exists());
    drop(w);
    assert!(!partial.exists());
    assert!(!path.exists());
    assert_eq!(
        std::fs::read_dir(plan.directory.join(".work"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn disk_accounting_tolerates_async_staging_publication() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let root = Root::new();
    std::fs::create_dir(root.0.join(".work")).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let path = root.0.join(".work").join("moving.partial");
    let writer = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            std::fs::write(&path, b"frame").unwrap();
            std::fs::remove_file(&path).unwrap();
        }
    });
    let mut failure = None;
    for _ in 0..500 {
        if let Err(e) = store::disk_usage(&root.0) {
            failure = Some(e);
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
    assert!(failure.is_none(), "{failure:?}");
}

fn recovery_fixture() -> (Root, PathBuf, Checkpoint, PathBuf) {
    use std::io::Read;
    let root = Root::new();
    let plan = store::create(&args(&root.0)).unwrap();
    let group = &plan.directory;
    let mut i = plan.index;
    i.owner_pid = 0;
    i.owner_created = 0;
    i.state = "failed".into();
    store::save_index(group, &i).unwrap();
    let target = group.join(store::filename(&i.prefix, 1));
    let mut w =
        crate::encode::GifWriter::new_in(&target, 8, 8, 10, Some(&group.join(".work"))).unwrap();
    w.push(vec![42; 8 * 8 * 3], 100).unwrap();
    let bytes = w.checkpoint().unwrap();
    let source = w.partial_path().to_path_buf();
    let sha =
        crate::identity::sha256_reader(std::fs::File::open(&source).unwrap().take(bytes)).unwrap();
    let stat = w.snapshot_stats(bytes + 1);
    let timeline = crate::timeline::Data {
        schema: 2,
        session_id: i.session_id.clone(),
        asset_id: Some(crate::identity::new_id().unwrap()),
        gif_sha256: None,
        region: crate::capture::Region {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        },
        width: 8,
        height: 8,
        gif_bytes: bytes + 1,
        duration_ms: 1000,
        frames: 1,
        samples: vec![],
        events: vec![],
        dropped_events: 0,
        warnings: vec![],
    };
    let c = Checkpoint {
        schema: 1,
        session_id: i.session_id.clone(),
        number: 1,
        start_ms: 0,
        source: source.file_name().unwrap().to_string_lossy().into_owned(),
        target: target.file_name().unwrap().to_string_lossy().into_owned(),
        safe_bytes: bytes,
        gif_prefix_sha256: sha,
        recording: stat,
        timeline,
    };
    drop(w);
    let group = group.clone();
    (root, group, c, source)
}

#[test]
fn recovery_resumes_after_each_publication_boundary() {
    for boundary in 0..=4 {
        let (_root, group, c, source) = recovery_fixture();
        let target = group.join(&c.target);
        let cp = group
            .join(".work")
            .join(&c.target)
            .with_extension("checkpoint.json");
        crate::storage::replace_json(&cp, &c).unwrap();
        let mut bytes = std::fs::read(&source).unwrap();
        bytes.truncate(c.safe_bytes as usize);
        bytes.push(0x3b);
        if boundary >= 1 {
            std::fs::write(&target, &bytes).unwrap();
        }
        if boundary >= 2 {
            crate::timeline::publish(&target, &c.timeline).unwrap();
        }
        if boundary >= 3 {
            crate::transaction::complete(&target).unwrap();
        }
        if boundary >= 4 {
            let mut stat = c.recording.clone();
            stat.output = target.clone();
            stat.bytes = bytes.len() as u64;
            stat.timeline = Some(crate::timeline::sidecar_path(&target));
            stat.manifest = Some(crate::transaction::manifest(&target));
            store::relative_stats(&mut stat).unwrap();
            let part = Part::new(1, 0, "recovered_checkpoint".into(), stat, &c.timeline);
            store::commit_part(&group, &part).unwrap();
        }
        let value = super::recover(&group).unwrap();
        assert!(
            value["errors"].as_array().unwrap().is_empty(),
            "boundary {boundary}: {value}"
        );
        assert_eq!(store::read_index(&group).unwrap().parts.len(), 1);
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        assert_eq!(super::recover(&group).unwrap()["recovered_parts"], 0);
        assert!(
            !group
                .join(c.target.replace(".gif", ".recovered.gif"))
                .exists()
        );
        assert!(!std::fs::read_dir(group.join(".work")).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("recovery-")
        }));
    }
}

#[test]
fn recovery_preserves_foreign_outputs_and_refuses_conflicting_sidecars() {
    let (_root, group, c, _source) = recovery_fixture();
    let target = group.join(&c.target);
    let cp = group
        .join(".work")
        .join(&c.target)
        .with_extension("checkpoint.json");
    crate::storage::replace_json(&cp, &c).unwrap();
    std::fs::write(&target, b"foreign output").unwrap();
    let recovered = group.join(c.target.replace(".gif", ".recovered.gif"));
    let sidecar = crate::timeline::sidecar_path(&recovered);
    std::fs::write(&sidecar, b"foreign sidecar").unwrap();
    let value = super::recover(&group).unwrap();
    assert_eq!(value["recovered_parts"], 0);
    assert!(!value["errors"].as_array().unwrap().is_empty());
    assert_eq!(std::fs::read(&target).unwrap(), b"foreign output");
    assert_eq!(std::fs::read(&sidecar).unwrap(), b"foreign sidecar");
    // The retry can reuse only matching media once the conflict is explicitly removed.
    std::fs::remove_file(sidecar).unwrap();
    assert_eq!(super::recover(&group).unwrap()["recovered_parts"], 1);
    assert_eq!(std::fs::read(&target).unwrap(), b"foreign output");
}
