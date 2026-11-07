//! UTF-8 pipeline documents with explicit newline and BOM preservation.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Newline {
    Lf,
    Crlf,
    Cr,
}

impl Newline {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Cr => "CR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipeDocument {
    pub path: Option<PathBuf>,
    pub text: String,
    loaded_text: String,
    pub newline: Newline,
    pub utf8_bom: bool,
}

#[derive(Debug)]
pub enum DocumentError {
    Io(String),
    Unsupported(&'static str),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(s) => write!(f, "{s}"),
            Self::Unsupported(s) => write!(f, "{s}"),
        }
    }
}

impl PipeDocument {
    pub fn untitled() -> Self {
        Self {
            path: None,
            text: String::new(),
            loaded_text: String::new(),
            newline: Newline::Lf,
            utf8_bom: false,
        }
    }

    pub fn from_bytes(path: Option<PathBuf>, bytes: &[u8]) -> Result<Self, DocumentError> {
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            return Err(DocumentError::Unsupported(
                "this file looks like UTF-16. Open only UTF-8 pipeline specs (optional UTF-8 BOM).",
            ));
        }
        if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00])
            || bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
        {
            return Err(DocumentError::Unsupported(
                "this file looks like UTF-32. Open only UTF-8 pipeline specs.",
            ));
        }
        let utf8_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
        let body = if utf8_bom { &bytes[3..] } else { bytes };
        if body.contains(&0) {
            return Err(DocumentError::Unsupported(
                "this file contains NUL bytes and is not a UTF-8 pipeline spec.",
            ));
        }
        let raw = std::str::from_utf8(body).map_err(|_| {
            DocumentError::Unsupported(
                "this file is not valid UTF-8. The editor will not open it lossy.",
            )
        })?;
        let newline = detect_newline(raw);
        let text = normalize_to_lf(raw);
        Ok(Self {
            path,
            loaded_text: text.clone(),
            text,
            newline,
            utf8_bom,
        })
    }

    pub fn open(path: &Path) -> Result<Self, DocumentError> {
        let bytes = std::fs::read(path)
            .map_err(|e| DocumentError::Io(format!("could not read {}: {e}", path.display())))?;
        Self::from_bytes(Some(path.to_path_buf()), &bytes)
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.loaded_text
    }

    pub fn mark_clean(&mut self) {
        self.loaded_text = self.text.clone();
    }

    pub fn encoded_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.utf8_bom {
            out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        }
        let nl = self.newline.as_str();
        let mut i = 0;
        let b = self.text.as_bytes();
        while i < b.len() {
            if b[i] == b'\n' {
                out.extend_from_slice(nl.as_bytes());
                i += 1;
            } else {
                out.push(b[i]);
                i += 1;
            }
        }
        out
    }

    pub fn save_to(&mut self, path: &Path) -> Result<(), DocumentError> {
        std::fs::write(path, self.encoded_bytes())
            .map_err(|e| DocumentError::Io(format!("could not write {}: {e}", path.display())))?;
        self.path = Some(path.to_path_buf());
        self.mark_clean();
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), DocumentError> {
        let Some(path) = self.path.clone() else {
            return Err(DocumentError::Io(
                "this buffer has no path. Use Save as.".into(),
            ));
        };
        self.save_to(&path)
    }
}

fn detect_newline(s: &str) -> Newline {
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' && i + 1 < b.len() && b[i + 1] == b'\n' {
            crlf += 1;
            i += 2;
            continue;
        }
        if b[i] == b'\n' {
            lf += 1;
        } else if b[i] == b'\r' {
            cr += 1;
        }
        i += 1;
    }
    if crlf >= lf && crlf >= cr && crlf > 0 {
        Newline::Crlf
    } else if cr > lf && cr > crlf {
        Newline::Cr
    } else {
        Newline::Lf
    }
}

fn normalize_to_lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_crlf_bom_and_non_ascii() {
        let raw = b"\xEF\xBB\xBF# caf\xc3\xa9\r\nliteral hola| hole\r\n";
        let mut doc = PipeDocument::from_bytes(None, raw).unwrap();
        assert!(doc.utf8_bom);
        assert_eq!(doc.newline, Newline::Crlf);
        assert!(doc.text.contains("café"));
        assert!(doc.text.contains('\n'));
        assert!(!doc.text.contains('\r'));
        assert_eq!(doc.encoded_bytes(), raw);
        doc.text.push_str("literal x| hole\n");
        assert!(doc.is_dirty());
        let saved = doc.encoded_bytes();
        assert!(saved.starts_with(&[0xEF, 0xBB, 0xBF]));
        assert!(saved.windows(2).any(|w| w == b"\r\n"));
    }

    #[test]
    fn rejects_utf16_and_invalid_utf8() {
        assert!(PipeDocument::from_bytes(None, &[0xFF, 0xFE, 0x61, 0x00]).is_err());
        assert!(PipeDocument::from_bytes(None, &[0x80, 0xFF]).is_err());
        assert!(PipeDocument::from_bytes(None, b"ok\0no").is_err());
    }

    #[test]
    fn lf_file_stays_lf() {
        let raw = b"literal a| hole\n";
        let doc = PipeDocument::from_bytes(None, raw).unwrap();
        assert_eq!(doc.newline, Newline::Lf);
        assert_eq!(doc.encoded_bytes(), raw);
    }
}
