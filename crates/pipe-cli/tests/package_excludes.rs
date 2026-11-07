//! Packaging scripts must drop Python bytecode from the zip / tarball
//! and must stage the public docs/ tree.

fn read_text(path: &std::path::Path) -> String {
    // Windows checkouts may use CRLF. Compare against LF so these string
    // checks match Linux and macOS CI.
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
        .replace("\r\n", "\n")
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn windows_script_strips_pycache() {
    let root = repo_root();
    let ps1 = read_text(&root.join("scripts/package-windows.ps1"));
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
    let root = repo_root();
    let yml = read_text(&root.join(".github/workflows/release.yml"));
    assert!(
        yml.contains("__pycache__") && yml.contains("*.pyc"),
        "Unix packaging should drop __pycache__ and *.pyc"
    );
}

#[test]
fn windows_script_stages_docs_tree() {
    let root = repo_root();
    let ps1 = read_text(&root.join("scripts/package-windows.ps1"));
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
    let root = repo_root();
    let yml = read_text(&root.join(".github/workflows/release.yml"));
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
    let root = repo_root();
    let ps1 = read_text(&root.join("scripts/package-windows.ps1"));
    let yml = read_text(&root.join(".github/workflows/release.yml"));
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
    let root = repo_root();
    let ps1 = read_text(&root.join("scripts/package-windows.ps1"));
    assert!(
        ps1.contains("[switch]$NoZip") && ps1.contains("[switch]$ZipOnly"),
        "package-windows.ps1 should accept -NoZip and -ZipOnly so CI can sign before zipping"
    );
}

#[test]
fn windows_release_workflow_signs_with_artifact_signing() {
    let root = repo_root();
    let yml = read_text(&root.join(".github/workflows/release.yml"));
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
        yml.contains("python3 scripts/release/prepare_release.py workflow-config"),
        "Packaging mode must be resolved by the config job, including pull_request -> sign=false"
    );
    assert_eq!(
        yml.matches("if: needs.config.outputs.sign == 'true'").count(),
        3,
        "Azure login, Artifact Signing, and Authenticode verify must all use the config sign gate"
    );
    assert!(
        yml.contains("name: Azure login (OIDC)\n        if: needs.config.outputs.sign == 'true'\n        uses: azure/login@v3"),
        "Azure OIDC login must be gated by needs.config.outputs.sign"
    );
    assert!(
        yml.contains("name: Sign first-party binaries with Microsoft Artifact Signing\n        if: needs.config.outputs.sign == 'true'\n        uses: azure/artifact-signing-action@v2"),
        "Artifact Signing must be gated by needs.config.outputs.sign"
    );
    assert!(
        yml.contains("name: Verify Authenticode signature\n        if: needs.config.outputs.sign == 'true'"),
        "Authenticode verify must be gated by needs.config.outputs.sign"
    );
    assert_eq!(
        yml.matches("if: needs.config.outputs.upload_release == 'true'")
            .count(),
        2,
        "Unix and Windows Release uploads must both use the config upload gate"
    );
    assert!(
        !yml.contains("if: github.event_name != 'pull_request'")
            && !yml.contains("if: github.event_name == 'release'"),
        "Signing and upload must not use the old event-name if conditions"
    );
}

#[test]
fn signing_gate_checks_survive_crlf() {
    let raw = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml")).unwrap();
    let crlf = raw.replace("\n", "\r\n").replace("\r\r\n", "\r\n");
    let yml = crlf.replace("\r\n", "\n");
    assert!(
        yml.contains("name: Azure login (OIDC)\n        if: needs.config.outputs.sign == 'true'\n        uses: azure/login@v3"),
        "CRLF checkouts must still see the Azure OIDC sign gate"
    );
}
