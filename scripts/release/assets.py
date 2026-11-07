"""Published archive names for a Portable Pipelines GitHub Release."""

# Four platform archives. Each is uploaded twice: a versioned name and a
# forever-stable name with no version, plus a sidecar checksum for each file.
PLATFORMS = (
    ("pipe-windows-x64", ".zip"),
    ("pipe-linux-x86_64", ".tar.gz"),
    ("pipe-macos-arm64", ".tar.gz"),
    ("pipe-macos-x86_64", ".tar.gz"),
)

STABLE_DOWNLOAD_PREFIX = (
    "https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/"
)


def expected_asset_names(tag: str) -> list[str]:
    """Return the 16 file names a complete published release must have."""
    names: list[str] = []
    for prefix, ext in PLATFORMS:
        versioned = f"{prefix}-{tag}{ext}"
        stable = f"{prefix}{ext}"
        names.extend(
            [
                versioned,
                f"{versioned}.sha256",
                stable,
                f"{stable}.sha256",
            ]
        )
    return names


def versioned_asset_names(tag: str) -> list[str]:
    names: list[str] = []
    for prefix, ext in PLATFORMS:
        versioned = f"{prefix}-{tag}{ext}"
        names.extend([versioned, f"{versioned}.sha256"])
    return names


def stable_asset_names() -> list[str]:
    names: list[str] = []
    for prefix, ext in PLATFORMS:
        stable = f"{prefix}{ext}"
        names.extend([stable, f"{stable}.sha256"])
    return names
