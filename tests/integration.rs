use std::path::Path;
use std::process::Command;

use nexus::{ProposedAction, NexusSettings, RunEvent};

#[test]
fn test_deserialize_test_fixtures() {
    let _settings = NexusSettings::default();
    let _ = std::mem::size_of::<RunEvent>();

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_path = Path::new(manifest_dir)
        .join(".nexus")
        .join("test-fixtures")
        .join("actions")
        .join("valid-patch.json");

    if !fixture_path.exists() {
        eprintln!(
            "fixture missing at {}, skipping",
            fixture_path.display()
        );
        return;
    }

    let contents = std::fs::read_to_string(&fixture_path)
        .expect("failed to read fixture file");
    let action: ProposedAction = serde_json::from_str(&contents)
        .expect("failed to deserialize fixture");
    assert!(
        !action.id.is_empty(),
        "expected non-empty action id"
    );
}

#[test]
fn test_cli_help() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("cargo")
        .current_dir(manifest_dir)
        .args(["run", "--", "--help"])
        .output()
        .expect("failed to run cargo for --help");

    assert!(
        output.status.success(),
        "expected help exit code 0, got: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Safe multi-file refactoring CLI"),
        "help output missing description. stdout:\n{stdout}"
    );
}
