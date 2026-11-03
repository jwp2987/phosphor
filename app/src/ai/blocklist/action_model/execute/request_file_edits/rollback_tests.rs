use async_io::block_on;

use super::*;

fn pre(path: &str, original: &str) -> PreEdit {
    PreEdit {
        path: path.to_owned(),
        rename_to: None,
        created: false,
        original: original.to_owned(),
    }
}

/// Batch of two: file 1 landed, file 2's write failed. Rollback restores file 1 only.
#[test]
fn landed_file_is_restored_and_failed_file_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt").to_string_lossy().into_owned();
    let b = dir.path().join("b.txt").to_string_lossy().into_owned();
    std::fs::write(&a, "EDITED a").unwrap();
    std::fs::write(&b, "original b").unwrap();

    let pres = vec![pre(&a, "original a"), pre(&b, "original b")];
    let ops = plan_rollback(&pres, std::slice::from_ref(&a), &[]);
    assert_eq!(
        ops,
        vec![RollbackOp::Restore {
            path: a.clone(),
            content: "original a".into()
        }]
    );

    let report = block_on(run_rollback(ops));
    assert!(report.failed.is_empty());
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "original a");
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "original b");
}

#[test]
fn created_file_is_removed_and_rename_is_undone() {
    let dir = tempfile::tempdir().unwrap();
    let new = dir.path().join("new.txt").to_string_lossy().into_owned();
    let src = dir.path().join("src.txt").to_string_lossy().into_owned();
    let dst = dir.path().join("dst.txt").to_string_lossy().into_owned();
    std::fs::write(&new, "x").unwrap();
    std::fs::write(&dst, "moved").unwrap();

    let pres = vec![
        PreEdit {
            path: new.clone(),
            rename_to: None,
            created: true,
            original: String::new(),
        },
        PreEdit {
            path: src.clone(),
            rename_to: Some(dst.clone()),
            created: false,
            original: "src body".into(),
        },
    ];
    let ops = plan_rollback(
        &pres,
        &[new.clone(), dst.clone()],
        std::slice::from_ref(&src),
    );
    let report = block_on(run_rollback(ops));
    assert!(report.failed.is_empty(), "{report:?}");
    assert!(!std::path::Path::new(&new).exists());
    assert!(!std::path::Path::new(&dst).exists());
    assert_eq!(std::fs::read_to_string(&src).unwrap(), "src body");
}

#[test]
fn nothing_landed_means_nothing_to_roll_back() {
    assert!(plan_rollback(&[pre("/x", "o")], &[], &[]).is_empty());
}

#[test]
fn message_distinguishes_full_and_partial_rollback() {
    let full = rollback_message(
        "disk full",
        &["/a".into()],
        &RollbackReport {
            restored: vec!["/a".into()],
            failed: vec![],
        },
    );
    assert!(full.starts_with("No changes were applied"), "{full}");
    let partial = rollback_message(
        "disk full",
        &["/a".into()],
        &RollbackReport {
            restored: vec![],
            failed: vec![("/a".into(), "denied".into())],
        },
    );
    assert!(partial.contains("do NOT apply it again"), "{partial}");
    assert!(partial.contains("/a (denied)"), "{partial}");
}
