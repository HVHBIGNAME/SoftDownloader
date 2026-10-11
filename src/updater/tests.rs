use super::*;

fn release(tag: &str) -> serde_json::Value {
    serde_json::json!({
        "tag_name": tag, "draft": false, "prerelease": false,
        "assets": [
            {"name": "SoftDownloader.exe", "size": 8192, "browser_download_url": format!("{ASSET_PREFIX}{tag}/SoftDownloader.exe")},
            {"name": "SoftDownloader.exe.sha256", "size": 85, "browser_download_url": format!("{ASSET_PREFIX}{tag}/SoftDownloader.exe.sha256")}
        ]
    })
}

#[test]
fn only_newer_stable_versions_are_selected() {
    let parse = |value: serde_json::Value| {
        newer_release(&serde_json::to_vec(&value).unwrap(), "0.5.9")
            .unwrap()
            .is_some()
    };
    assert!(parse(release("v0.5.10")));
    assert!(!parse(release("v0.5.9")));
    assert!(!parse(release("v0.5.8")));
    assert!(!parse(release("v0.6.0-beta.1")));
    for field in ["draft", "prerelease"] {
        let mut candidate = release("v1.0.0");
        candidate[field] = true.into();
        assert!(!parse(candidate));
    }
}

#[test]
fn release_assets_must_be_complete_unique_bounded_and_owned_by_the_repository() {
    let mut bad = Vec::new();
    let mut value = release("v1.0.0");
    value["assets"][0]["browser_download_url"] =
        "https://github.com/other/project/releases/download/v1.0.0/SoftDownloader.exe".into();
    bad.push(value);
    let mut value = release("v1.0.0");
    value["assets"][0]["size"] = (MAX_EXE_BYTES + 1).into();
    bad.push(value);
    let mut value = release("v1.0.0");
    value["assets"].as_array_mut().unwrap().pop();
    bad.push(value);
    let mut value = release("v1.0.0");
    let duplicate = value["assets"][0].clone();
    value["assets"].as_array_mut().unwrap().push(duplicate);
    bad.push(value);
    for candidate in bad {
        assert!(newer_release(&serde_json::to_vec(&candidate).unwrap(), "0.5.2").is_err());
    }
}

#[test]
fn checksum_binds_to_the_expected_filename() {
    let hash = "a".repeat(64);
    assert_eq!(
        checksum(format!("{hash}  SoftDownloader.exe\r\n").as_bytes()).unwrap(),
        hash
    );
    assert_eq!(
        checksum(format!("{hash} *SoftDownloader.exe\n").as_bytes()).unwrap(),
        hash
    );
    for value in [
        format!("{hash} other.exe"),
        format!("{hash} ../../SoftDownloader.exe"),
        format!("{hash} SoftDownloader.exe\n{hash} other.exe"),
        "bad SoftDownloader.exe".into(),
    ] {
        assert!(checksum(value.as_bytes()).is_err());
    }
}

#[test]
fn executable_check_rejects_html_and_wrong_architecture() {
    let temporary = tempfile::tempdir().unwrap();
    let file = temporary.path().join("candidate.exe");
    std::fs::write(&file, b"<html>not an executable</html>").unwrap();
    assert!(validate_executable(&file).is_err());
    let mut bytes = vec![0; 1024];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64_u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&[0x64, 0x86]);
    bytes[88..90].copy_from_slice(&[0x0b, 0x02]);
    std::fs::write(&file, &bytes).unwrap();
    validate_executable(&file).unwrap();
    bytes[68..70].copy_from_slice(&[0x4c, 0x01]);
    std::fs::write(&file, bytes).unwrap();
    assert!(validate_executable(&file).is_err());
}
