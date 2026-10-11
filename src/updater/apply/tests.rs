use super::*;

fn fixture(root: &Path) -> Plan {
    let target = root.join("application.exe");
    let incoming = root.join("new.exe");
    std::fs::write(&target, b"old version fixture").unwrap();
    std::fs::write(&incoming, b"new version fixture").unwrap();
    Plan {
        old_sha256: digest(&target).unwrap(),
        sha256: digest(&incoming).unwrap(),
        size: std::fs::metadata(&incoming).unwrap().len(),
        target,
        backup: incoming.with_extension("backup"),
        incoming,
        working_directory: root.into(),
        arguments: Vec::new(),
        parent_pid: 0,
        version: "1.0.0".into(),
    }
}

#[test]
#[cfg(windows)]
fn replaces_verified_bytes_and_rolls_back_a_failed_start() {
    let folder = tempfile::tempdir().unwrap();
    let plan = fixture(folder.path());
    replace_and_start(&plan, || Ok(())).unwrap();
    assert_eq!(std::fs::read(&plan.target).unwrap(), b"new version fixture");
    assert!(!plan.backup.exists());
    let plan = fixture(folder.path());
    assert!(replace_and_start(&plan, || anyhow::bail!("startup failed")).is_err());
    assert_eq!(std::fs::read(&plan.target).unwrap(), b"old version fixture");
    assert!(!plan.backup.exists());
}

#[test]
fn tampering_or_a_concurrent_update_never_overwrites_the_installed_file() {
    let folder = tempfile::tempdir().unwrap();
    let plan = fixture(folder.path());
    std::fs::write(&plan.incoming, b"tampered").unwrap();
    assert!(replace_and_start(&plan, || panic!("must not launch")).is_err());
    assert_eq!(std::fs::read(&plan.target).unwrap(), b"old version fixture");
    let plan = fixture(folder.path());
    std::fs::write(&plan.target, b"installed by another updater").unwrap();
    assert!(replace_and_start(&plan, || panic!("must not launch")).is_err());
    assert_eq!(
        std::fs::read(&plan.target).unwrap(),
        b"installed by another updater"
    );
}
