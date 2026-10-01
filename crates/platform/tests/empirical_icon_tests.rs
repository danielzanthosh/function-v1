use std::process::Command;

/// Child process entry point for testing find_icon_path across varying CWDs and ENV vars.
#[test]
fn icon_test_harness_entry() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--run-case") {
        let case_name = &args[pos + 1];
        match case_name.as_str() {
            "case_resolve_icon" => {
                let icon_opt = function_platform::find_icon_path();
                assert!(
                    icon_opt.is_some(),
                    "Expected find_icon_path() to find icon in CWD: {:?}",
                    std::env::current_dir()
                );
                let icon_path = icon_opt.unwrap();
                assert!(
                    icon_path.exists(),
                    "Resolved icon does not exist: {:?}",
                    icon_path
                );
                assert!(icon_path.to_string_lossy().ends_with("icon.ico"));
                println!("OK case_resolve_icon: {}", icon_path.display());
            }
            "case_custom_override" => {
                let expected =
                    std::env::var("EXPECTED_ICON_PATH").expect("EXPECTED_ICON_PATH missing");
                let icon_opt = function_platform::find_icon_path();
                assert!(icon_opt.is_some(), "Expected icon override to be found");
                let icon_path = icon_opt.unwrap();
                assert_eq!(icon_path, std::path::PathBuf::from(&expected));
                println!("OK case_custom_override: {}", icon_path.display());
            }
            "case_invalid_override_fallback" => {
                let icon_opt = function_platform::find_icon_path();
                assert!(icon_opt.is_some(), "Expected fallback to find default icon");
                let icon_path = icon_opt.unwrap();
                assert!(icon_path.exists());
                assert!(icon_path.to_string_lossy().ends_with("icon.ico"));
                println!("OK case_invalid_override_fallback: {}", icon_path.display());
            }
            other => panic!("Unknown test case: {}", other),
        }
        std::process::exit(0);
    }
}

#[test]
fn stress_test_icon_from_workspace_root() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

    let output = Command::new(&current_exe)
        .arg("icon_test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_resolve_icon")
        .current_dir(workspace_root)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Failed from workspace root: status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(stdout.contains("OK case_resolve_icon"));
}

#[test]
fn stress_test_icon_from_crates_app() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let app_dir = manifest_dir.parent().unwrap().join("app");

    let output = Command::new(&current_exe)
        .arg("icon_test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_resolve_icon")
        .current_dir(&app_dir)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Failed from crates/app: status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(stdout.contains("OK case_resolve_icon"));
}

#[test]
fn stress_test_icon_from_crates_app_src() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let app_src_dir = manifest_dir.parent().unwrap().join("app").join("src");

    let output = Command::new(&current_exe)
        .arg("icon_test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_resolve_icon")
        .current_dir(&app_src_dir)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Failed from crates/app/src: status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(stdout.contains("OK case_resolve_icon"));
}

#[test]
fn stress_test_icon_from_temp_directory() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let temp_dir = std::env::temp_dir();

    let output = Command::new(&current_exe)
        .arg("icon_test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_resolve_icon")
        .current_dir(&temp_dir)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Failed from temp directory: status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(stdout.contains("OK case_resolve_icon"));
}

#[test]
fn stress_test_icon_custom_env_override() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let temp_root = std::env::temp_dir().join("function_icon_override_test");
    let _ = std::fs::create_dir_all(&temp_root);
    let dummy_icon = temp_root.join("custom_icon.ico");
    std::fs::write(&dummy_icon, b"dummy-icon-bytes").expect("failed to write dummy icon");

    let output = Command::new(&current_exe)
        .arg("icon_test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_custom_override")
        .env("FUNCTION_ICON_PATH", &dummy_icon)
        .env("EXPECTED_ICON_PATH", &dummy_icon)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_dir_all(&temp_root);

    assert!(
        output.status.success(),
        "Failed custom override: status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(stdout.contains("OK case_custom_override"));
}

#[test]
fn stress_test_icon_nonexistent_override_fallback() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let non_existent = std::env::temp_dir().join("does_not_exist_icon_12345.ico");

    let output = Command::new(&current_exe)
        .arg("icon_test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_invalid_override_fallback")
        .env("FUNCTION_ICON_PATH", &non_existent)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Failed nonexistent override fallback: status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(stdout.contains("OK case_invalid_override_fallback"));
}
