use std::io;
use std::path::{Path, PathBuf};

pub fn scan_workspace(root: &Path) -> io::Result<Vec<PathBuf>> {
    fn walk(root: &Path, relative: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in std::fs::read_dir(root.join(relative))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = relative.join(entry.file_name());
            if kind.is_dir() {
                walk(root, &path, out)?;
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("md"))
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    walk(root, Path::new(""), &mut paths)?;
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_finds_nested_unicode_markdown_only() {
        let root = std::env::temp_dir().join(format!("munin-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("rašymas")).unwrap();
        std::fs::write(root.join("rašymas/žodis.md"), "labas").unwrap();
        std::fs::write(root.join("other.txt"), "ignored").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
        assert_eq!(
            scan_workspace(&root).unwrap(),
            vec![PathBuf::from("rašymas/žodis.md")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
