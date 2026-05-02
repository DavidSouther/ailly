use std::io::Read;

use vfs::{FileSystem, SeekAndRead, SeekAndWrite, VfsMetadata, VfsPath, VfsResult};

use crate::content::gitignore_fs_constants::{
    BINARY_EXTENSIONS, IGNORED_NAMES, SIGNATURES, TEXT_EXTENSIONS,
};

#[derive(Debug, Clone)]
pub struct GitignoreFs {
    inner: VfsPath,
}

impl GitignoreFs {
    pub fn new(inner: VfsPath) -> Self {
        Self { inner }
    }

    fn join_inner(&self, path: &str) -> VfsResult<VfsPath> {
        if path.is_empty() {
            Ok(self.inner.clone())
        } else {
            self.inner.join(path.trim_start_matches('/'))
        }
    }

    fn collect_gitignores(&self, path: &str) -> Vec<GitignoreParser> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut out = Vec::new();
        for i in 0..=parts.len() {
            let rel = if i == 0 {
                ".gitignore".to_string()
            } else {
                format!("{}/.gitignore", parts[..i].join("/"))
            };
            let Ok(gi_path) = self.inner.join(&rel) else {
                continue;
            };
            let content = gi_path.read_to_string().unwrap_or_default();
            out.push(GitignoreParser::parse(&content));
        }
        out
    }

    fn keep_entry(&self, dir_path: &str, entry: &VfsPath, parsers: &[GitignoreParser]) -> bool {
        let name = entry.filename();
        if IGNORED_NAMES.contains(&name.as_str()) {
            return false;
        }
        let is_dir = entry.is_dir().unwrap_or(false);
        let gi_input = if is_dir {
            format!("{name}/")
        } else {
            name.clone()
        };
        if !parsers.iter().all(|p| p.accepts(&gi_input)) {
            return false;
        }
        is_dir || self.is_text_file(dir_path, &name)
    }

    fn is_text_file(&self, dir_path: &str, name: &str) -> bool {
        if classify_by_extension(name) == ExtensionClassification::Text {
            return true;
        }

        let Ok(file_path) = self.join_inner(dir_path).and_then(|p| p.join(name)) else {
            return false;
        };

        sniff_is_text(&file_path)
    }
}

impl FileSystem for GitignoreFs {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        let dir = self.join_inner(path)?;
        let parsers = self.collect_gitignores(path);

        let mut filtered: Vec<String> = dir
            .read_dir()?
            .filter(|entry| self.keep_entry(path, entry, &parsers))
            .map(|entry| entry.filename())
            .collect();
        filtered.sort();
        Ok(Box::new(filtered.into_iter()))
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.join_inner(path)?.create_dir()
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        self.join_inner(path)?.open_file()
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        self.join_inner(path)?.create_file()
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        self.join_inner(path)?.append_file()
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        self.join_inner(path)?.metadata()
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        self.join_inner(path)?.exists()
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        self.join_inner(path)?.remove_file()
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        self.join_inner(path)?.remove_dir()
    }
}

#[derive(Debug, PartialEq)]
enum ExtensionClassification {
    Text,
    Binary,
    Unknown,
}

fn classify_by_extension(name: &str) -> ExtensionClassification {
    let lower = name.to_lowercase();
    let ext_index = match lower.rfind('.') {
        Some(idx) => idx,
        None => {
            return ExtensionClassification::Unknown;
        }
    };
    let ext = &lower[ext_index..];
    if TEXT_EXTENSIONS.contains(ext) {
        ExtensionClassification::Text
    } else if BINARY_EXTENSIONS.contains(ext) {
        ExtensionClassification::Binary
    } else {
        ExtensionClassification::Unknown
    }
}

fn sniff_is_text(file_path: &VfsPath) -> bool {
    let Ok(mut file) = file_path.open_file() else {
        return false;
    };
    let mut buf = [0u8; 512];
    let n = file.read(&mut buf).unwrap_or(0);
    let sample = &buf[..n];
    if has_magic_signature(sample) {
        return false;
    }
    !is_bin_by_control_chars(sample, ControlCharThresholds::default())
}

fn has_magic_signature(content: &[u8]) -> bool {
    SIGNATURES.iter().any(|sig| content.starts_with(sig))
}

struct ControlCharThresholds {
    sample_size: usize,
    control_pct: f64,
    extended_base_pct: f64,
    extended_control_pct: f64,
}

impl Default for ControlCharThresholds {
    fn default() -> Self {
        Self {
            sample_size: 512,
            control_pct: 10.0,
            extended_base_pct: 30.0,
            extended_control_pct: 5.0,
        }
    }
}

fn is_bin_by_control_chars(sample: &[u8], t: ControlCharThresholds) -> bool {
    let n = sample.len().min(t.sample_size);
    if n == 0 {
        return false;
    }

    let mut control = 0usize;
    let mut extended = 0usize;

    for &b in &sample[..n] {
        if b == 0 {
            return true;
        }
        let is_ctrl = (b < 32 && b != 9 && b != 10 && b != 13) || b == 127;
        if is_ctrl {
            control += 1;
        }
        if b > 127 {
            extended += 1;
        }
    }

    let control_pct = (control as f64 / n as f64) * 100.0;
    let extended_pct = (extended as f64 / n as f64) * 100.0;

    if control_pct > t.control_pct {
        return true;
    }
    if extended_pct > t.extended_base_pct && control_pct > t.extended_control_pct {
        return true;
    }
    false
}

/// Minimal `.gitignore` matcher mirroring the call site of the TypeScript
/// `gitignore-parser` library: each input is the basename of an entry, with a
/// trailing `/` for directories. The last matching rule wins; `!` negates.
struct GitignoreParser {
    rules: Vec<Rule>,
}

struct Rule {
    pattern: String,
    negate: bool,
    only_dir: bool,
}

impl GitignoreParser {
    fn parse(content: &str) -> Self {
        let rules = content
            .lines()
            .filter_map(|raw| {
                let line = raw.trim_end();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let mut s = line;
                let negate = s.starts_with('!');
                if negate {
                    s = &s[1..];
                }
                let only_dir = s.ends_with('/');
                if only_dir {
                    s = &s[..s.len() - 1];
                }
                let s = s.trim_start_matches('/');
                if s.is_empty() {
                    return None;
                }
                Some(Rule {
                    pattern: s.to_string(),
                    negate,
                    only_dir,
                })
            })
            .collect();
        Self { rules }
    }

    fn accepts(&self, input: &str) -> bool {
        let is_dir_input = input.ends_with('/');
        let bare = input.trim_end_matches('/');
        let mut accepted = true;
        for rule in &self.rules {
            if rule.matches(bare, is_dir_input) {
                accepted = rule.negate;
            }
        }
        accepted
    }
}

impl Rule {
    fn matches(&self, name: &str, is_dir: bool) -> bool {
        if self.only_dir && !is_dir {
            return false;
        }
        glob_match(self.pattern.as_bytes(), name.as_bytes())
    }
}

fn glob_match(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.first(), name.first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(b'*'), _) => {
            if glob_match(&pattern[1..], name) {
                return true;
            }
            !name.is_empty() && glob_match(pattern, &name[1..])
        }
        (Some(b'?'), Some(_)) => glob_match(&pattern[1..], &name[1..]),
        (Some(&p), Some(&n)) if p == n => glob_match(&pattern[1..], &name[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_util::mem_fs;
    use std::io::Write;
    use vfs::VfsPath;

    fn wrap(inner: VfsPath) -> VfsPath {
        VfsPath::new(GitignoreFs::new(inner))
    }

    fn read_dir_sorted(p: &VfsPath) -> Vec<String> {
        let mut v: Vec<String> = p.read_dir().unwrap().map(|e| e.filename()).collect();
        v.sort();
        v
    }

    fn write_binary(parent: &VfsPath, name: &str, prefix: &[u8]) {
        let mut bytes = vec![0x00, 0x01, 0x02, 0x03, 0x04];
        bytes.extend_from_slice(prefix);
        parent
            .join(name)
            .unwrap()
            .create_file()
            .unwrap()
            .write_all(&bytes)
            .unwrap();
    }

    #[test]
    fn skips_files_that_look_like_binary() {
        let inner = mem_fs! {
            "text1.txt": "This is a text file",
            "text2.md": "# Markdown heading\n\nSome content",
            "config.json": r#"{"key": "value"}"#,
        };
        write_binary(&inner, "image.png", b"\x89PNG\r\n\x1A\n");
        write_binary(&inner, "archive.zip", b"PK\x03\x04");
        write_binary(&inner, "executable", b"\x7FELF");

        let fs = wrap(inner);
        assert_eq!(
            read_dir_sorted(&fs),
            vec!["config.json", "text1.txt", "text2.md"]
        );
    }

    #[test]
    fn reads_while_obeying_gitignores() {
        let inner = mem_fs! {
            "file.txt": "abc",
            "skip": "def",
            ".gitignore": "skip\nskipdir",
            ".git": {
                "objects": {
                    "aa": { "012345": "code" },
                },
            },
            "dir": {
                "file.txt": "ghi",
                "skip": "def2",
                "deep": {
                    ".gitignore": "other",
                    "file.txt": "jkl",
                    "skip": "still skipped",
                    "other": "skipped",
                },
            },
            "skipdir": { "file.txt": "abc" },
        };
        let fs = wrap(inner);

        assert_eq!(read_dir_sorted(&fs), vec!["dir", "file.txt"]);
        assert_eq!(
            read_dir_sorted(&fs.join("dir").unwrap()),
            vec!["deep", "file.txt"]
        );
        assert_eq!(
            read_dir_sorted(&fs.join("dir/deep").unwrap()),
            vec!["file.txt"]
        );
    }

    #[test]
    fn does_not_filter_golang_files() {
        let inner = mem_fs! { "test.go": "gogo" };
        let fs = wrap(inner);

        assert_eq!(read_dir_sorted(&fs), vec!["test.go"]);
    }
}
