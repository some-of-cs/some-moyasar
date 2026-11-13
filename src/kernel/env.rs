//! Environment and `.env` file resolution for `$ENV_VAR` API key references.

use std::fs::{self, DirEntry};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use zeroize::Zeroize;

use crate::kernel::error::MoyasarError;

/// Resolves an API key reference of the form `$VAR_NAME` or `${VAR_NAME}`.
///
/// Lookup order:
/// 1. Process environment via `std::env::var`.
/// 2. `.env` and `.env.*` files in the current working directory (`.env.local`,
///    `.env`, then remaining `.env.*` files in lexicographical order).
///
/// Returns `Err(MoyasarError::InvalidKey)` if the syntax is invalid, the variable
/// is not found, or its resolved value is empty.
pub fn resolve_env_secret(raw: &str) -> Result<String, MoyasarError> {
    let cwd = std::env::current_dir().map_err(|_| MoyasarError::InvalidKey)?;
    resolve_env_secret_in_dir(raw, &cwd)
}

pub(crate) fn resolve_env_secret_in_dir(raw: &str, dir: &Path) -> Result<String, MoyasarError> {
    let name = extract_var_name(raw).ok_or(MoyasarError::InvalidKey)?;
    if let Some(val) = read_process_env(name) {
        return Ok(val);
    }
    scan_dotenv_dir(dir, name)
}

fn read_process_env(name: &str) -> Option<String> {
    let mut val = std::env::var(name).ok()?;
    if val.is_empty() {
        val.zeroize();
        return None;
    }
    Some(val)
}

fn extract_var_name(raw: &str) -> Option<&str> {
    let rest = raw.strip_prefix('$')?;
    let name = match rest.strip_prefix('{') {
        Some(braced) => braced.strip_suffix('}')?,
        None => rest,
    };
    is_valid_var_name(name).then_some(name)
}

fn is_valid_var_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn scan_dotenv_dir(dir: &Path, var_name: &str) -> Result<String, MoyasarError> {
    let files = collect_dotenv_files(dir)?;
    for path in files {
        if let Some(val) = find_var_in_file(&path, var_name) {
            return Ok(val);
        }
    }
    Err(MoyasarError::InvalidKey)
}

fn collect_dotenv_files(dir: &Path) -> Result<Vec<PathBuf>, MoyasarError> {
    let entries = fs::read_dir(dir).map_err(|_| MoyasarError::InvalidKey)?;
    let mut files: Vec<(u8, String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(classify_dotenv_entry)
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(files.into_iter().map(|(_, _, path)| path).collect())
}

fn classify_dotenv_entry(entry: DirEntry) -> Option<(u8, String, PathBuf)> {
    let path = entry.path();
    if !path.is_file() {
        return None;
    }
    let name = entry.file_name().into_string().ok()?;
    let priority = dotenv_priority(&name)?;
    Some((priority, name, path))
}

fn dotenv_priority(name: &str) -> Option<u8> {
    if name == ".env.local" {
        Some(0)
    } else if name == ".env" {
        Some(1)
    } else if name.starts_with(".env.") {
        Some(2)
    } else {
        None
    }
}

fn find_var_in_file(path: &Path, var_name: &str) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        let parsed = parse_dotenv_line(&line, var_name);
        line.zeroize();
        line.clear();
        if let Some(val) = parsed {
            return Some(val);
        }
    }
    None
}

fn parse_dotenv_line(line: &str, var_name: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let entry = strip_export_prefix(trimmed);
    let (key, raw_val) = entry.split_once('=')?;
    if key.trim() != var_name {
        return None;
    }
    let val = strip_matching_quotes(raw_val.trim());
    (!val.is_empty()).then(|| val.to_owned())
}

fn strip_export_prefix(s: &str) -> &str {
    match s.strip_prefix("export") {
        Some(rest) if rest.starts_with(char::is_whitespace) => rest.trim_start(),
        _ => s,
    }
}

fn strip_matching_quotes(s: &str) -> &str {
    if let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return inner;
    }
    if let Some(inner) = s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        return inner;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "some_moyasar_env_test_{prefix}_{}_{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn write(&self, name: &str, content: &str) {
            fs::write(self.path.join(name), content).expect("write temp file");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn validates_var_name_syntax() {
        assert_eq!(extract_var_name("$MOYASAR_KEY"), Some("MOYASAR_KEY"));
        assert_eq!(extract_var_name("${_KEY_1}"), Some("_KEY_1"));
        assert_eq!(extract_var_name("MOYASAR_KEY"), None);
        assert_eq!(extract_var_name("$"), None);
        assert_eq!(extract_var_name("${}"), None);
        assert_eq!(extract_var_name("$1INVALID"), None);
        assert_eq!(extract_var_name("${BAD-NAME}"), None);
        assert_eq!(extract_var_name("${UNCLOSED"), None);
    }

    #[test]
    fn resolves_from_dotenv_with_quotes_export_and_comments() {
        let dir = TempDir::new("quotes");
        dir.write(
            ".env",
            "# comment line\n\nexport MOYASAR_SECRET_TEST=\"sk_test_quoted\"\nOTHER_KEY='pk_test_single'\n",
        );

        let sk = resolve_env_secret_in_dir("$MOYASAR_SECRET_TEST", &dir.path)
            .expect("resolve double-quoted export");
        assert_eq!(sk, "sk_test_quoted");

        let pk = resolve_env_secret_in_dir("${OTHER_KEY}", &dir.path)
            .expect("resolve single-quoted braced");
        assert_eq!(pk, "pk_test_single");
    }

    #[test]
    fn prioritizes_dotenv_local_before_dotenv_and_others() {
        let dir = TempDir::new("priority");
        dir.write(".env.staging", "MOYASAR_PRIO_KEY=sk_test_staging\n");
        dir.write(".env", "MOYASAR_PRIO_KEY=sk_test_default\n");
        dir.write(".env.local", "MOYASAR_PRIO_KEY=sk_test_local\n");

        let resolved =
            resolve_env_secret_in_dir("$MOYASAR_PRIO_KEY", &dir.path).expect("resolve .env.local");
        assert_eq!(resolved, "sk_test_local");

        let dir2 = TempDir::new("priority_fallback");
        dir2.write(".env.staging", "MOYASAR_ONLY_STAGING=sk_test_staging\n");
        let resolved2 = resolve_env_secret_in_dir("$MOYASAR_ONLY_STAGING", &dir2.path)
            .expect("resolve .env.staging");
        assert_eq!(resolved2, "sk_test_staging");
    }

    #[test]
    fn rejects_missing_or_empty_values() {
        let dir = TempDir::new("empty");
        dir.write(".env", "EMPTY_KEY=\"\"\n");

        assert!(matches!(
            resolve_env_secret_in_dir("$EMPTY_KEY", &dir.path),
            Err(MoyasarError::InvalidKey)
        ));
        assert!(matches!(
            resolve_env_secret_in_dir("$NONEXISTENT_MOYASAR_VAR_XYZ", &dir.path),
            Err(MoyasarError::InvalidKey)
        ));
        assert!(matches!(
            resolve_env_secret_in_dir("$123BAD", &dir.path),
            Err(MoyasarError::InvalidKey)
        ));
    }

    #[test]
    fn resolves_from_process_env() {
        let resolved = resolve_env_secret("$PATH").expect("PATH exists in process env");
        assert!(!resolved.is_empty());
    }
}
