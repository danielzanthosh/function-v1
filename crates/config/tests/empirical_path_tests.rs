use std::process::Command;

/// Child process entry point for isolated environment testing.
/// This guarantees complete process-level isolation of environment variables.
#[test]
fn test_harness_entry() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--run-case") {
        let case_name = &args[pos + 1];
        match case_name.as_str() {
            "case_userprofile" => {
                let dir = function_config::function_dir();
                let cfg = function_config::config_path();
                let mem = function_config::memory_path();
                assert!(dir.ends_with(".function"));
                assert!(dir.to_string_lossy().contains("mock_userprofile"));
                assert_eq!(cfg, dir.join("config.json"));
                assert_eq!(mem, dir.join("memory.json"));
                println!("OK case_userprofile: {}", dir.display());
            }
            "case_home_only" => {
                let dir = function_config::function_dir();
                let cfg = function_config::config_path();
                let mem = function_config::memory_path();
                assert!(dir.ends_with(".function"));
                assert!(dir.to_string_lossy().contains("mock_home"));
                assert_eq!(cfg, dir.join("config.json"));
                assert_eq!(mem, dir.join("memory.json"));
                println!("OK case_home_only: {}", dir.display());
            }
            "case_temp_fallback" => {
                let dir = function_config::function_dir();
                let cfg = function_config::config_path();
                let mem = function_config::memory_path();
                let expected_base = std::env::temp_dir();
                assert!(dir.starts_with(&expected_base));
                assert!(dir.ends_with(".function"));
                assert_eq!(cfg, dir.join("config.json"));
                assert_eq!(mem, dir.join("memory.json"));
                println!("OK case_temp_fallback: {}", dir.display());
            }
            "case_userprofile_precedence" => {
                let dir = function_config::function_dir();
                assert!(dir.to_string_lossy().contains("mock_userprofile"));
                assert!(!dir.to_string_lossy().contains("mock_home"));
                println!("OK case_userprofile_precedence: {}", dir.display());
            }
            other => panic!("Unknown test case: {}", other),
        }
        std::process::exit(0);
    }
}

#[test]
fn stress_test_userprofile_set() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let temp_root = std::env::temp_dir().join("function_test_userprofile");
    let mock_profile = temp_root.join("mock_userprofile");
    let _ = std::fs::create_dir_all(&mock_profile);

    let output = Command::new(&current_exe)
        .arg("test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_userprofile")
        .env("USERPROFILE", &mock_profile)
        .env_remove("HOME")
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_dir_all(&temp_root);

    assert!(
        output.status.success(),
        "Process failed with status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(
        stdout.contains("OK case_userprofile"),
        "Expected confirmation in stdout: {}",
        stdout
    );
}

#[test]
fn stress_test_home_set_userprofile_unset() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let temp_root = std::env::temp_dir().join("function_test_home");
    let mock_home = temp_root.join("mock_home");
    let _ = std::fs::create_dir_all(&mock_home);

    let output = Command::new(&current_exe)
        .arg("test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_home_only")
        .env_remove("USERPROFILE")
        .env("HOME", &mock_home)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_dir_all(&temp_root);

    assert!(
        output.status.success(),
        "Process failed with status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(
        stdout.contains("OK case_home_only"),
        "Expected confirmation in stdout: {}",
        stdout
    );
}

#[test]
fn stress_test_both_unset_temp_fallback() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");

    let output = Command::new(&current_exe)
        .arg("test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_temp_fallback")
        .env_remove("USERPROFILE")
        .env_remove("HOME")
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Process failed with status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(
        stdout.contains("OK case_temp_fallback"),
        "Expected confirmation in stdout: {}",
        stdout
    );
}

#[test]
fn stress_test_userprofile_precedence_over_home() {
    let current_exe = std::env::current_exe().expect("failed to get current_exe");
    let temp_root = std::env::temp_dir().join("function_test_precedence");
    let mock_profile = temp_root.join("mock_userprofile");
    let mock_home = temp_root.join("mock_home");
    let _ = std::fs::create_dir_all(&mock_profile);
    let _ = std::fs::create_dir_all(&mock_home);

    let output = Command::new(&current_exe)
        .arg("test_harness_entry")
        .arg("--exact")
        .arg("--nocapture")
        .arg("--")
        .arg("--run-case")
        .arg("case_userprofile_precedence")
        .env("USERPROFILE", &mock_profile)
        .env("HOME", &mock_home)
        .output()
        .expect("failed to execute child process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_dir_all(&temp_root);

    assert!(
        output.status.success(),
        "Process failed with status {:?}\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert!(
        stdout.contains("OK case_userprofile_precedence"),
        "Expected confirmation in stdout: {}",
        stdout
    );
}
