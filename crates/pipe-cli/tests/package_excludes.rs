//! Packaging scripts must drop Python bytecode from the zip / tarball.

#[test]
fn windows_script_strips_pycache() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ps1 = std::fs::read_to_string(root.join("scripts/package-windows.ps1")).unwrap();
    assert!(
        ps1.contains("Remove-PythonBytecode"),
        "package-windows.ps1 should call Remove-PythonBytecode"
    );
    assert!(
        ps1.contains("__pycache__") && ps1.contains("*.pyc"),
        "package-windows.ps1 should drop __pycache__ and *.pyc"
    );
}

#[test]
fn unix_release_workflow_strips_pycache() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let yml = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(
        yml.contains("__pycache__") && yml.contains("*.pyc"),
        "Unix packaging should drop __pycache__ and *.pyc"
    );
}
