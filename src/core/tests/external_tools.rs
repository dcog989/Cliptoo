//! External-tool integrations: Send-To and compare launch real processes with
//! temp-file payloads that are cleaned up shortly after launch.

/// Poll until `path` no longer exists, failing if the deadline passes.
/// `launch_with_temp_files` removes its files ~5 s after a successful spawn.
async fn wait_until_gone(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "temp file {:?} was not cleaned up",
            path
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Poll until `path` exists (the async-spawned helper has run), failing on
/// timeout.
async fn wait_until_exists(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "helper {:?} never ran",
            path
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn write_executable(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

/// A Send-To launch writes the clip to a temp file, passes its path to the app
/// as the first argument, and removes the temp file a few seconds later.
#[tokio::test]
async fn send_to_delivers_content_and_cleans_up_temp_file() {
    let dir = std::env::temp_dir().join(format!("cliptoo_sendto_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let marker = dir.join("marker.txt");
    let script = write_executable(
        &dir,
        "capture.sh",
        &format!(
            "#!/bin/sh\n{{ printf '%s\\n' \"$1\"; cat \"$1\"; }} > {}\n",
            marker.display()
        ),
    );

    cliptoo_core::send_to::send_to(script.to_str().unwrap(), "hello world")
        .await
        .unwrap();

    wait_until_exists(&marker).await;

    let output = std::fs::read_to_string(&marker).unwrap();
    let mut lines = output.lines();
    let temp_path = std::path::PathBuf::from(lines.next().unwrap());
    assert_eq!(
        lines.next(),
        Some("hello world"),
        "clip content reached the app"
    );

    wait_until_gone(&temp_path).await;

    let _ = std::fs::remove_dir_all(&dir);
}

/// A compare launch passes each clip's temp file to the tool as its two
/// arguments; both files carry the correct content and are cleaned up after.
#[tokio::test]
async fn compare_launches_tool_with_both_clips() {
    let dir = std::env::temp_dir().join(format!("cliptoo_compare_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let args_path = dir.join("args.txt");
    let left_copy = dir.join("left_copy.txt");
    let right_copy = dir.join("right_copy.txt");
    let script = write_executable(
        &dir,
        "capture2.sh",
        &format!(
            "#!/bin/sh\nprintf '%s\\n%s\\n' \"$1\" \"$2\" > {}\ncp \"$1\" {}\ncp \"$2\" {}\n",
            args_path.display(),
            left_copy.display(),
            right_copy.display(),
        ),
    );

    cliptoo_core::compare::compare_clips("left-content", "right-content", script.to_str().unwrap())
        .await
        .unwrap();

    wait_until_exists(&args_path).await;

    let args = std::fs::read_to_string(&args_path).unwrap();
    let mut paths = args.lines();
    let left = std::path::PathBuf::from(paths.next().unwrap());
    let right = std::path::PathBuf::from(paths.next().unwrap());
    assert_ne!(left, right, "each clip gets its own temp file");

    assert_eq!(std::fs::read_to_string(&left_copy).unwrap(), "left-content");
    assert_eq!(
        std::fs::read_to_string(&right_copy).unwrap(),
        "right-content"
    );

    wait_until_gone(&left).await;
    wait_until_gone(&right).await;

    let _ = std::fs::remove_dir_all(&dir);
}

/// An explicit `compare_tool_path` that is not a file is refused before any
/// temp files are created.
#[tokio::test]
async fn compare_rejects_invalid_explicit_path() {
    let err = cliptoo_core::compare::compare_clips("a", "b", "/nonexistent/cliptoo-diff-tool")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not a file"));
}
