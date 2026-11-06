//! Packaging scripts must drop Python bytecode from the zip / tarball
//! and must stage the public docs/ tree.

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

#[test]
fn windows_script_stages_docs_tree() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ps1 = std::fs::read_to_string(root.join("scripts/package-windows.ps1")).unwrap();
    assert!(
        ps1.contains("Join-Path $RepoRoot 'docs'")
            && ps1.contains("(Join-Path $stage 'docs')"),
        "package-windows.ps1 should copy docs/ into the zip staging directory"
    );
    assert!(
        ps1.contains("Remove-PythonBytecode -Root (Join-Path $stage 'docs')"),
        "package-windows.ps1 should drop __pycache__ and *.pyc from staged docs/"
    );
}

#[test]
fn unix_release_workflow_stages_docs_tree() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let yml = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(
        yml.contains("cp -R docs"),
        "Unix packaging should copy docs/ into the tarball"
    );
    assert!(
        yml.contains("docs/STATUS.md"),
        "Unix packaging should fail if staged docs/STATUS.md is missing"
    );
}

#[test]
fn packaging_omits_local_docs_pdfs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ps1 = std::fs::read_to_string(root.join("scripts/package-windows.ps1")).unwrap();
    let yml = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(
        ps1.contains("Remove-GitignoredDocsJunk") && ps1.contains("*.pdf"),
        "package-windows.ps1 should omit local IBM PDFs under docs/"
    );
    assert!(
        yml.contains("*.pdf") && yml.contains("*-extract.tmp.txt"),
        "Unix packaging should omit local IBM PDFs and extract scratch under docs/"
    );
}

#[test]
fn windows_script_can_stage_then_zip() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ps1 = std::fs::read_to_string(root.join("scripts/package-windows.ps1")).unwrap();
    assert!(
        ps1.contains("[switch]$NoZip") && ps1.contains("[switch]$ZipOnly"),
        "package-windows.ps1 should accept -NoZip and -ZipOnly so CI can sign before zipping"
    );
}

#[test]
fn windows_release_workflow_signs_with_artifact_signing() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let yml = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(
        yml.contains("environment: signing"),
        "Windows signing job must use GitHub environment `signing` for OIDC"
    );
    assert!(
        yml.contains("azure/login@v3"),
        "Windows job should log in with azure/login@v3 over OIDC"
    );
    assert!(
        yml.contains("azure/artifact-signing-action@v2"),
        "Windows job should sign with azure/artifact-signing-action@v2"
    );
    assert!(
        yml.contains("Get-AuthenticodeSignature"),
        "Windows job should verify Authenticode after signing"
    );
    assert!(
        yml.contains("http://timestamp.acs.microsoft.com"),
        "Windows job should use the Microsoft Artifact Signing timestamp server"
    );
    assert!(
        yml.contains("Not signing third-party Regina DLL"),
        "Windows job should refuse to sign official Regina"
    );
    assert!(
        yml.contains("if: github.event_name != 'pull_request'"),
        "Windows job should skip Azure login and signing on pull_request"
    );
    assert!(
        yml.contains("if: github.event_name == 'release'"),
        "Windows job should publish zip assets only on a published Release"
    );
}
