// muguet - import.rs
// Constantin Clerc (c22dev) - 0.1

use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SKIPPED_DIRS: &[&str] = &["env_test", "verityradio"];
const MAX_ENTRIES: usize = 250_000;
const MAX_TOTAL: u64 = 128 << 30;
const MAX_FILE: u64 = 32 << 30;
const MAX_METADATA: u64 = 32 << 20;
static IMPORT_LOCK: Mutex<()> = Mutex::new(());

pub fn expected_build_id() -> Option<&'static str> {
    option_env!("MUGUET_GAME_BUILD_ID").filter(|id| !id.is_empty())
}

pub fn embedded_wasm_sha256() -> Option<&'static str> {
    option_env!("MUGUET_GAME_WASM_SHA256").filter(|hash| !hash.is_empty())
}

struct Entry {
    index: usize,
    name: String,
    size: u64,
    directory: bool,
}

enum Source {
    Zip(zip::ZipArchive<std::io::BufReader<File>>),
    Directory { root: PathBuf, paths: Vec<PathBuf> },
}

impl Source {
    fn reader(&mut self, index: usize) -> Result<Box<dyn Read + '_>> {
        match self {
            Self::Zip(zip) => Ok(Box::new(zip.by_index(index)?)),
            Self::Directory { root, paths } => {
                // Recheck each ancestor before reading a provider's file. The
                // native picker coordinates this whole read with its provider.
                let path = &paths[index];
                let mut parent = root.clone();
                for part in path.strip_prefix(&*root)?.components() {
                    parent.push(part);
                    let meta = std::fs::symlink_metadata(&parent)?;
                    if meta.file_type().is_symlink() { bail!("symbolic links are not allowed: {}", parent.display()); }
                }
                Ok(Box::new(File::open(path).with_context(|| format!("reading {}", path.display()))?))
            }
        }
    }
}

// Validate the final relative spelling, including Windows ZIP separators.
// Checking enclosed_name before stripping a wrapper does not protect the
// extraction root from wrapper/data/../../outside.
fn relative_name(name: &str) -> Result<String> {
    let name = name.replace('\\', "/");
    let name = name.strip_suffix('/').unwrap_or(&name);
    if name.is_empty() || name.len() > 1024 || name.split('/').any(|p| p.is_empty() || p.len() > 255 || p == "." || p == ".." || p.contains(':') || p.chars().any(|c| c.is_control())) {
        bail!("unsafe relative path: {name:?}");
    }
    Ok(name.to_string())
}

fn skipped(name: &str) -> bool {
    name.split('/').any(|part| SKIPPED_DIRS.contains(&part.to_ascii_lowercase().as_str()))
}

fn check_inventory(entries: &[Entry]) -> Result<()> {
    if entries.len() > MAX_ENTRIES { bail!("too many archive entries (limit {MAX_ENTRIES})"); }
    let mut paths: HashMap<String, (String, bool, bool)> = HashMap::new();
    let mut total = 0u64;
    for entry in entries {
        if entry.size > MAX_FILE { bail!("file exceeds 32 GiB: {}", entry.name); }
        if !entry.directory { total = total.checked_add(entry.size).context("archive size overflow")?; }
        if total > MAX_TOTAL { bail!("archive exceeds 128 GiB"); }
        let parts: Vec<_> = entry.name.split('/').collect();
        for n in 1..=parts.len() {
            let path = parts[..n].join("/");
            let key = path.to_lowercase();
            let directory = n != parts.len() || entry.directory;
            let explicit = n == parts.len();
            if let Some((previous, was_dir, was_explicit)) = paths.get_mut(&key) {
                if previous != &path || *was_dir != directory || (explicit && *was_explicit && !directory) {
                    bail!("duplicate or conflicting file path: {path} (previously {previous})");
                }
                *was_explicit |= explicit;
            } else {
                paths.insert(key, (path, directory, explicit));
            }
        }
    }
    Ok(())
}

pub fn import(zip_path: &Path, dest: &Path, keep_code: bool, progress: impl FnMut(u64, u64)) -> Result<()> {
    let _lock = IMPORT_LOCK.lock().map_err(|_| anyhow::anyhow!("import lock poisoned"))?;
    let file = File::open(zip_path).with_context(|| format!("{}", zip_path.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 20, file)).context("not a zip file")?;
    if zip.len() > MAX_ENTRIES { bail!("too many archive entries (limit {MAX_ENTRIES})"); }
    // zip 2.4.2 stores filenames in an IndexMap and silently collapses exact
    // duplicates. Count the original directory records as well as its view.
    let mut directory = std::io::BufReader::new(File::open(zip_path)?);
    directory.seek(SeekFrom::Start(zip.central_directory_start()))?;
    let mut count = 0;
    loop {
        let mut signature = [0u8; 4];
        directory.read_exact(&mut signature)?;
        if signature != *b"PK\x01\x02" { break; }
        let mut header = [0u8; 42];
        directory.read_exact(&mut header)?;
        let skip: u64 = [24, 26, 28].into_iter().map(|p| u16::from_le_bytes([header[p], header[p + 1]]) as u64).sum();
        directory.seek(SeekFrom::Current(skip as i64))?;
        count += 1;
        if count > MAX_ENTRIES { bail!("too many archive directory entries"); }
    }
    if count != zip.len() { bail!("duplicate or inconsistent ZIP directory entries"); }
    let mut entries = Vec::with_capacity(zip.len());
    for k in 0..zip.len() {
        let entry = zip.by_index_raw(k)?;
        if entry.is_symlink() { bail!("symbolic links are not allowed: {}", entry.name()); }
        if entry.unix_mode().is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o100000 | 0o040000)) {
            bail!("unsupported file type: {}", entry.name());
        }
        entries.push(Entry { index: k, name: relative_name(entry.name())?, size: entry.size(), directory: entry.is_dir() });
    }
    let mut source = Source::Zip(zip);
    run_import(&mut source, entries, dest, keep_code, true, progress)
}

pub fn import_directory(root: &Path, dest: &Path, keep_code: bool, progress: impl FnMut(u64, u64)) -> Result<()> {
    let _lock = IMPORT_LOCK.lock().map_err(|_| anyhow::anyhow!("import lock poisoned"))?;
    let meta = std::fs::symlink_metadata(root)?;
    if !meta.is_dir() || meta.file_type().is_symlink() { bail!("select a folder containing data/ and b/, not a symbolic link"); }
    let root = root.canonicalize()?;
    if dest.canonicalize().ok().is_some_and(|d| root.starts_with(&d)) {
        bail!("select source data outside the app's current game folder");
    }
    let mut paths = Vec::new();
    let mut entries = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        for child in std::fs::read_dir(&dir).with_context(|| format!("reading folder {}", dir.display()))? {
            let path = child?.path();
            let meta = std::fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() || !(meta.is_dir() || meta.is_file()) { bail!("unsupported file or symbolic link: {}", path.display()); }
            let name = path.strip_prefix(&root)?.to_str().context("file path is not UTF-8")?;
            entries.push(Entry { index: paths.len(), name: relative_name(name)?, size: if meta.is_file() { meta.len() } else { 0 }, directory: meta.is_dir() });
            if entries.len() > MAX_ENTRIES { bail!("too many folder entries (limit {MAX_ENTRIES})"); }
            if meta.is_dir() { pending.push(path.clone()); }
            paths.push(path);
        }
    }
    let mut source = Source::Directory { root, paths };
    run_import(&mut source, entries, dest, keep_code, false, progress)
}

struct Plan {
    jobs: Vec<(usize, String, u64)>,
    total: u64,
    manifest: serde_json::Value,
    manifest_bytes: Vec<u8>,
    shader_path: String,
    shader_bytes: Vec<u8>,
}

fn metadata(source: &mut Source, entry: &Entry) -> Result<Vec<u8>> {
    if entry.size > MAX_METADATA { bail!("metadata exceeds 32 MiB: {}", entry.name); }
    let mut bytes = Vec::new();
    source.reader(entry.index)?.take(MAX_METADATA + 1).read_to_end(&mut bytes).with_context(|| format!("reading {}", entry.name))?;
    if bytes.len() as u64 != entry.size || bytes.len() as u64 > MAX_METADATA { bail!("metadata size mismatch: {}", entry.name); }
    Ok(bytes)
}

fn plan(source: &mut Source, entries: &[Entry], keep_code: bool, wrapped: bool, expected: Option<&str>) -> Result<Plan> {
    check_inventory(entries)?;
    let manifests: Vec<_> = entries.iter().filter(|e| !e.directory && (e.name == "data/manifest.json" || e.name.ends_with("/data/manifest.json"))).collect();
    if manifests.len() != 1 { bail!("select exactly one game root with data/manifest.json"); }
    let manifest_entry = manifests[0];
    let prefix = &manifest_entry.name[..manifest_entry.name.len() - "data/manifest.json".len()];
    if !wrapped && !prefix.is_empty() { bail!("select the folder directly containing data/ and b/"); }
    let mut selected = HashMap::new();
    let mut builds = HashSet::new();
    for entry in entries {
        let Some(rel) = entry.name.strip_prefix(prefix) else { continue };
        // Deliberately validate again after removing the outer game folder.
        let rel = relative_name(rel)?;
        if let Some(rest) = rel.strip_prefix("b/") {
            if entry.directory || rest.contains('/') { builds.insert(rest.split('/').next().unwrap().to_string()); }
        }
        if !entry.directory { selected.insert(rel, entry); }
    }
    if builds.len() != 1 { bail!("select exactly one build under b/ (found {})", builds.len()); }
    let build = builds.into_iter().next().unwrap();
    if let Some(expected) = expected {
        if build != expected { bail!("game build {build} does not match this app's embedded build {expected}"); }
    }
    let shader_prefix = format!("b/{build}/shaders/");
    let shader_path = format!("{shader_prefix}index.json");
    let shader_entry = selected.get(&shader_path).context("this game has no shaders (b/<build>/shaders/index.json)")?;
    if keep_code && !selected.contains_key(&format!("b/{build}/game.wasm")) { bail!("this game has no matching code (b/<build>/game.wasm)"); }
    let manifest_bytes = metadata(source, manifest_entry)?;
    let mut manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).context("manifest.json")?;
    let files = manifest.get_mut("files").and_then(|v| v.as_array_mut()).context("manifest files must be an array")?;
    if files.is_empty() { bail!("manifest files must not be empty"); }
    let mut names = HashSet::new();
    for file in files.iter_mut() {
        let row = file.as_array_mut().filter(|r| r.len() >= 2).context("manifest file must be [relative path, size, ...]")?;
        let name = relative_name(row[0].as_str().context("manifest file path must be a string")?)?;
        if !names.insert(name.to_lowercase()) { bail!("duplicate manifest file: {name}"); }
        let size = row[1].as_u64().context("manifest file size must be a nonnegative integer")?;
        if size > MAX_FILE { bail!("manifest file exceeds 32 GiB: {name}"); }
        if !skipped(&name) {
            let entry = selected.get(&format!("data/{name}")).with_context(|| format!("missing mandatory asset: {name}"))?;
            if entry.size != size { bail!("asset size mismatch: {name} (manifest {size}, file {})", entry.size); }
        }
        row[0] = name.into();
    }
    files.retain(|row| !skipped(row[0].as_str().unwrap()));
    if files.is_empty() { bail!("manifest has no required assets after intentional skips"); }
    let shader_bytes = metadata(source, shader_entry)?;
    let shaders: serde_json::Value = serde_json::from_slice(&shader_bytes).context("shader index.json")?;
    let shaders = shaders.as_object().context("shader index must be an object")?;
    let mut compiled = 0;
    for (hash, record) in shaders {
        if hash.starts_with('_') { continue; }
        if hash.len() != 16 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) { bail!("invalid shader key: {hash}"); }
        let record = record.as_object().context("shader record must be an object")?;
        let ok = record.get("ok") == Some(&serde_json::Value::Bool(true));
        if ok {
            compiled += 1;
            let path = format!("{shader_prefix}{hash}.wgsl");
            if !selected.get(&path).is_some_and(|e| e.size > 0) { bail!("missing or empty shader source: {path}"); }
            if record.get("consts").is_some_and(|v| !v.is_null() && v != &serde_json::Value::Bool(false)) {
                let path = format!("{shader_prefix}{hash}.consts.json");
                let entry = selected.get(&path).with_context(|| format!("missing shader constants: {path}"))?;
                let constants: serde_json::Value = serde_json::from_slice(&metadata(source, entry)?).context("shader constants json")?;
                if !constants.is_array() { bail!("shader constants must be an array: {path}"); }
            }
        }
    }
    if compiled == 0 { bail!("shader index contains no compiled shader sources"); }
    let mut total = 0u64;
    let mut jobs = Vec::new();
    // Original entry order is stable for progress; manifest order is untouched.
    for entry in entries {
        let Some(rel) = entry.name.strip_prefix(prefix) else { continue };
        if entry.directory || skipped(rel) { continue; }
        if rel.starts_with("data/") || rel.starts_with(&shader_prefix) || (keep_code && rel == format!("b/{build}/game.wasm")) {
            total = total.checked_add(entry.size).context("import size overflow")?;
            jobs.push((entry.index, relative_name(rel)?, entry.size));
        }
    }
    Ok(Plan { jobs, total, manifest, manifest_bytes, shader_path, shader_bytes })
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

fn run_import(source: &mut Source, entries: Vec<Entry>, dest: &Path, keep_code: bool, wrapped: bool, mut progress: impl FnMut(u64, u64)) -> Result<()> {
    let mut plan = plan(source, &entries, keep_code, wrapped, expected_build_id())?;
    let staging = dest.with_extension("importing");
    let backup = dest.with_extension("previous");
    for path in [dest, &staging, &backup] {
        if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) { bail!("import destination is a symbolic link: {}", path.display()); }
    }
    if !dest.exists() && backup.exists() { std::fs::rename(&backup, dest).context("restoring previous import")?; }
    if staging.exists() { std::fs::remove_dir_all(&staging).context("removing incomplete staging data")?; }
    std::fs::create_dir_all(&staging)?;
    let staging = Staging(staging);
    let mut done = 0u64;
    let mut reported = 0u64;
    progress(0, plan.total);
    let mut buf = vec![0u8; 1 << 20];
    for (index, rel, expected_size) in &plan.jobs {
        let mut entry = source.reader(*index)?;
        let out_path = staging.0.join(rel);
        if let Some(parent) = out_path.parent() { std::fs::create_dir_all(parent)?; }
        // create_new also catches filesystem-specific name equivalences.
        let mut out = File::options().write(true).create_new(true).open(&out_path).with_context(|| format!("creating {}", out_path.display()))?;
        let mut size = 0u64;
        loop {
            let n = entry.read(&mut buf).with_context(|| format!("reading {rel}"))?;
            if n == 0 { break; }
            size = size.checked_add(n as u64).context("file size overflow")?;
            if size > *expected_size { bail!("file grew or has an invalid size: {rel}"); }
            out.write_all(&buf[..n])?;
            done += n as u64;
            if done - reported >= 64 << 20 { reported = done; progress(done, plan.total); }
        }
        if size != *expected_size { bail!("file size mismatch while reading: {rel}"); }
    }
    if std::fs::read(staging.0.join("data/manifest.json"))? != plan.manifest_bytes || std::fs::read(staging.0.join(&plan.shader_path))? != plan.shader_bytes {
        bail!("game metadata changed during import; retry with a stable source");
    }
    tidy_lists(&staging.0.join("data"), &mut plan.manifest)?;
    if backup.exists() { std::fs::remove_dir_all(&backup).context("removing previous backup")?; }
    let previous = dest.exists();
    if previous { std::fs::rename(dest, &backup).context("preserving previous import")?; }
    if let Err(error) = std::fs::rename(&staging.0, dest) {
        if previous {
            std::fs::rename(&backup, dest).with_context(|| format!("replacement failed ({error}); previous data remains at {}", backup.display()))?;
        }
        return Err(error).context("replacing imported game");
    }
    if previous {
        if let Err(error) = std::fs::remove_dir_all(&backup) { eprintln!("[import] old data retained at {}: {error}", backup.display()); }
    }
    progress(plan.total, plan.total);
    Ok(())
}

fn tidy_lists(data: &Path, manifest: &mut serde_json::Value) -> Result<()> {
    let dlc_rel = "common/data/dlcList.xml";
    let dlc = data.join(dlc_rel);
    if dlc.exists() {
        if std::fs::metadata(&dlc)?.len() > MAX_METADATA { bail!("dlcList.xml exceeds 32 MiB"); }
        let text = std::fs::read_to_string(&dlc).context("dlcList.xml")?;
        let kept: Vec<&str> = text.lines().filter(|l| !SKIPPED_DIRS.iter().any(|d| l.to_ascii_lowercase().contains(d))).collect();
        std::fs::write(&dlc, kept.join("\n") + "\n")?;
        let size = std::fs::metadata(&dlc)?.len();
        for row in manifest["files"].as_array_mut().unwrap() {
            if row[0].as_str() == Some(dlc_rel) { row[1] = size.into(); }
        }
    }
    std::fs::write(data.join("manifest.json"), serde_json::to_vec(manifest)?)?;
    Ok(())
}

#[cfg(any(target_os = "ios", test))]
mod ffi {
    use std::ffi::{c_char, CStr, CString};
    use std::sync::{Mutex, OnceLock};

    static ERROR: Mutex<Option<CString>> = Mutex::new(None);

    fn run(path: *const c_char, directory: bool, progress: extern "C" fn(u64, u64)) -> bool {
        let result = if path.is_null() {
            Err(anyhow::anyhow!("no source selected"))
        } else {
            let path = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
            let dest = crate::config::game_dir();
            if directory {
                super::import_directory(std::path::Path::new(&path), &dest, false, |d, t| progress(d, t))
            } else {
                super::import(std::path::Path::new(&path), &dest, false, |d, t| progress(d, t))
            }
        };
        match result {
            Ok(()) => { *ERROR.lock().unwrap() = None; true },
            Err(e) => {
                *ERROR.lock().unwrap() = CString::new(format!("{e:#}").replace('\0', "�")).ok();
                false
            }
        }
    }

    #[no_mangle]
    pub extern "C" fn muguet_import(zip: *const c_char, progress: extern "C" fn(u64, u64)) -> bool {
        run(zip, false, progress)
    }

    #[no_mangle]
    pub extern "C" fn muguet_import_directory(folder: *const c_char, progress: extern "C" fn(u64, u64)) -> bool {
        run(folder, true, progress)
    }

    #[no_mangle]
    pub extern "C" fn muguet_import_error() -> *const c_char {
        ERROR.lock().unwrap().as_ref().map(|s| s.as_ptr()).unwrap_or(std::ptr::null())
    }

    #[no_mangle]
    pub extern "C" fn muguet_game_ready() -> bool {
        crate::config::game_ready()
    }

    #[no_mangle]
    pub extern "C" fn muguet_has_game_code() -> bool {
        crate::has_game_code()
    }

    #[no_mangle]
    pub extern "C" fn muguet_game_build_id() -> *const c_char {
        static ID: OnceLock<Option<CString>> = OnceLock::new();
        ID.get_or_init(|| super::expected_build_id().and_then(|id| CString::new(id).ok())).as_ref().map_or(std::ptr::null(), |id| id.as_ptr())
    }

    #[no_mangle]
    pub extern "C" fn muguet_game_wasm_sha256() -> *const c_char {
        static HASH: OnceLock<Option<CString>> = OnceLock::new();
        HASH.get_or_init(|| super::embedded_wasm_sha256().and_then(|hash| CString::new(hash).ok())).as_ref().map_or(std::ptr::null(), |hash| hash.as_ptr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use zip::write::SimpleFileOptions;

    // These fixtures are synthetic importer tests, not playable game archives.
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!("muguet-import-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn dest(&self) -> PathBuf { self.0.join("game") }
    }
    impl Drop for Temp {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    fn fixture() -> Vec<(String, Vec<u8>)> {
        vec![
            ("data/manifest.json".into(), br#"{"files":[["asset.bin",24]]}"#.to_vec()),
            ("data/asset.bin".into(), b"SYNTHETIC_ASSET_CONTENT!".to_vec()),
            ("b/test-build/shaders/index.json".into(), br#"{"0123456789abcdef":{"ok":true}}"#.to_vec()),
            ("b/test-build/shaders/0123456789abcdef.wgsl".into(), b"// synthetic test source, not a playable game\n".to_vec()),
        ]
    }

    fn zip_at(temp: &Temp, entries: Vec<(String, Vec<u8>)>) -> PathBuf {
        let path = temp.0.join("synthetic.zip");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in entries {
            writer.start_file(name, SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    fn assert_rejected(temp: &Temp, path: &Path) {
        std::fs::create_dir_all(temp.dest()).unwrap();
        std::fs::write(temp.dest().join("previous"), b"keep previous import").unwrap();
        assert!(import(path, &temp.dest(), false, |_, _| {}).is_err(), "malicious or inconsistent fixture was accepted");
        assert_eq!(std::fs::read(temp.dest().join("previous")).unwrap(), b"keep previous import");
        assert!(!temp.dest().with_extension("importing").exists(), "failed import left staging data");
    }

    #[test]
    fn wrapped_parent_traversal_is_rejected_before_writing() {
        let temp = Temp::new();
        let mut entries: Vec<_> = fixture().into_iter().map(|(n, b)| (format!("wrapper/{n}"), b)).collect();
        entries.push(("wrapper/data/../../outside".into(), b"bad".to_vec()));
        let path = zip_at(&temp, entries);
        assert_rejected(&temp, &path);
        assert!(!temp.0.join("outside").exists());
    }

    #[test]
    fn backslash_parent_traversal_is_rejected() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries.push((r"data/..\../outside".into(), b"bad".to_vec()));
        assert_rejected(&temp, &zip_at(&temp, entries));
        assert!(!temp.0.join("outside").exists());
    }

    #[test]
    fn case_colliding_files_are_rejected() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries.push(("data/ASSET.bin".into(), b"collision".to_vec()));
        assert_rejected(&temp, &zip_at(&temp, entries));
    }

    #[test]
    fn normalized_duplicate_files_are_rejected() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries.push((r"data\asset.bin".into(), b"collision".to_vec()));
        assert_rejected(&temp, &zip_at(&temp, entries));
    }

    #[test]
    fn archive_symlinks_are_rejected() {
        let temp = Temp::new();
        let path = zip_at(&temp, fixture());
        let mut writer = zip::ZipWriter::new_append(std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap()).unwrap();
        writer.add_symlink("data/link", "../../outside", SimpleFileOptions::default()).unwrap();
        writer.finish().unwrap();
        assert_rejected(&temp, &path);
    }

    #[test]
    fn multiple_build_ids_are_rejected() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries.push(("b/other-build/shaders/index.json".into(), b"{}".to_vec()));
        assert_rejected(&temp, &zip_at(&temp, entries));
    }

    #[test]
    fn multiple_manifest_roots_are_rejected() {
        let temp = Temp::new();
        let entries: Vec<_> = fixture().into_iter().flat_map(|(n, b)| [(n.clone(), b.clone()), (format!("wrapper/{n}"), b)]).collect();
        assert_rejected(&temp, &zip_at(&temp, entries));
    }

    #[test]
    fn malformed_manifest_files_are_rejected() {
        for manifest in [r#"{}"#, r#"{"files":{}}"#, r#"{"files":["asset.bin"]}"#, r#"{"files":[["asset.bin","24"]]}"#] {
            let temp = Temp::new();
            let mut entries = fixture();
            entries[0].1 = manifest.as_bytes().to_vec();
            assert_rejected(&temp, &zip_at(&temp, entries));
        }
    }

    #[test]
    fn missing_mandatory_asset_is_rejected() {
        let temp = Temp::new();
        let entries = fixture().into_iter().filter(|(n, _)| n != "data/asset.bin").collect();
        assert_rejected(&temp, &zip_at(&temp, entries));
    }

    #[test]
    fn incorrect_declared_asset_size_is_rejected() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries[0].1 = br#"{"files":[["asset.bin",25]]}"#.to_vec();
        assert_rejected(&temp, &zip_at(&temp, entries));
    }

    #[test]
    fn manifest_path_traversal_is_rejected() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries[0].1 = br#"{"files":[["../config.json",3]]}"#.to_vec();
        assert_rejected(&temp, &zip_at(&temp, entries));
    }

    #[test]
    fn corrupt_crc_cleans_staging_and_preserves_previous_import() {
        let temp = Temp::new();
        let path = zip_at(&temp, fixture());
        let mut bytes = std::fs::read(&path).unwrap();
        let pos = bytes.windows(24).position(|w| w == b"SYNTHETIC_ASSET_CONTENT!").unwrap();
        bytes[pos] ^= 1;
        std::fs::write(&path, bytes).unwrap();
        assert_rejected(&temp, &path);
    }

    #[test]
    fn manifest_order_and_rewritten_dlc_size_are_preserved() {
        let temp = Temp::new();
        let dlc = b"<paths>\nkeep\nverityradio\n</paths>\n";
        let mut entries = vec![
            ("data/manifest.json".into(), format!(r#"{{"files":[["z.bin",1],["common/data/dlcList.xml",{}],["env_test/optional.bin",10],["a.bin",1]]}}"#, dlc.len()).into_bytes()),
            ("data/z.bin".into(), b"z".to_vec()),
            ("data/a.bin".into(), b"a".to_vec()),
            ("data/common/data/dlcList.xml".into(), dlc.to_vec()),
        ];
        entries.extend(fixture().into_iter().filter(|(name, _)| name.starts_with("b/")));
        import(&zip_at(&temp, entries), &temp.dest(), false, |_, _| {}).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(temp.dest().join("data/manifest.json")).unwrap()).unwrap();
        let files = manifest["files"].as_array().unwrap();
        assert_eq!(files.iter().map(|f| f[0].as_str().unwrap()).collect::<Vec<_>>(), ["z.bin", "common/data/dlcList.xml", "a.bin"]);
        assert_eq!(files[1][1].as_u64().unwrap(), std::fs::metadata(temp.dest().join("data/common/data/dlcList.xml")).unwrap().len());
    }

    fn directory_at(temp: &Temp, entries: Vec<(String, Vec<u8>)>) -> PathBuf {
        let root = temp.0.join("source");
        for (name, bytes) in entries {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        root
    }

    #[test]
    fn raw_duplicate_zip_entries_are_rejected() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries.push(("data/other.bin".into(), b"SYNTHETIC_ASSET_CONTENT!".to_vec()));
        let path = zip_at(&temp, entries);
        let mut bytes = std::fs::read(&path).unwrap();
        let positions: Vec<_> = bytes.windows(14).enumerate().filter_map(|(p, b)| (b == b"data/other.bin").then_some(p)).collect();
        assert_eq!(positions.len(), 2);
        for p in positions { bytes[p..p + 14].copy_from_slice(b"data/asset.bin"); }
        std::fs::write(&path, bytes).unwrap();
        assert_rejected(&temp, &path);
    }

    #[test]
    fn folder_import_uses_the_same_manifest_and_build_selection() {
        let temp = Temp::new();
        let source = directory_at(&temp, fixture());
        import_directory(&source, &temp.dest(), false, |_, _| {}).unwrap();
        assert_eq!(std::fs::read(temp.dest().join("data/asset.bin")).unwrap(), b"SYNTHETIC_ASSET_CONTENT!");
        assert!(temp.dest().join("b/test-build/shaders/index.json").exists());
        assert!(!temp.dest().with_extension("importing").exists());
    }

    #[test]
    fn folder_case_collision_preserves_previous_import() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries.push(("data/ASSET.bin".into(), b"collision".to_vec()));
        let source = directory_at(&temp, entries);
        std::fs::create_dir_all(temp.dest()).unwrap();
        std::fs::write(temp.dest().join("previous"), b"previous").unwrap();
        assert!(import_directory(&source, &temp.dest(), false, |_, _| {}).is_err());
        assert_eq!(std::fs::read(temp.dest().join("previous")).unwrap(), b"previous");
    }

    #[cfg(unix)]
    #[test]
    fn folder_symlink_is_rejected() {
        let temp = Temp::new();
        let source = directory_at(&temp, fixture());
        std::os::unix::fs::symlink(temp.0.join("outside"), source.join("data/link")).unwrap();
        assert!(import_directory(&source, &temp.dest(), false, |_, _| {}).is_err());
        assert!(!temp.dest().exists());
    }

    #[test]
    fn changing_folder_file_cleans_staging_and_preserves_previous_import() {
        let temp = Temp::new();
        let source = directory_at(&temp, fixture());
        std::fs::create_dir_all(temp.dest()).unwrap();
        std::fs::write(temp.dest().join("previous"), b"previous").unwrap();
        assert!(import_directory(&source, &temp.dest(), false, |done, _| {
            if done == 0 { std::fs::write(source.join("data/asset.bin"), b"changed").unwrap(); }
        }).is_err());
        assert_eq!(std::fs::read(temp.dest().join("previous")).unwrap(), b"previous");
        assert!(!temp.dest().with_extension("importing").exists());
    }

    #[test]
    fn missing_shader_source_and_invalid_shader_metadata_are_rejected() {
        for index in [r#"{}"#, r#"[]"#, r#"{"0123456789abcdef":{"ok":false}}"#, r#"{"0123456789abcdef":{"ok":true,"consts":true}}"#] {
            let temp = Temp::new();
            let mut entries = fixture();
            entries[2].1 = index.as_bytes().to_vec();
            assert_rejected(&temp, &zip_at(&temp, entries));
        }
        let temp = Temp::new();
        let entries = fixture().into_iter().filter(|(name, _)| !name.ends_with(".wgsl")).collect();
        assert_rejected(&temp, &zip_at(&temp, entries));
    }

    #[test]
    fn embedded_build_id_mismatch_is_rejected() {
        let temp = Temp::new();
        let path = zip_at(&temp, fixture());
        let mut zip = zip::ZipArchive::new(std::io::BufReader::new(File::open(path).unwrap())).unwrap();
        let entries: Vec<_> = (0..zip.len()).map(|index| {
            let entry = zip.by_index_raw(index).unwrap();
            Entry { index, name: entry.name().to_string(), size: entry.size(), directory: entry.is_dir() }
        }).collect();
        let mut source = Source::Zip(zip);
        assert!(plan(&mut source, &entries, false, true, Some("different-embedded-build")).is_err());
        assert!(plan(&mut source, &entries, false, true, Some("test-build")).is_ok());
    }

    #[test]
    fn wrapped_and_windows_separator_sources_stage_only_selected_data() {
        for windows in [false, true] {
            let temp = Temp::new();
            let mut entries: Vec<_> = fixture().into_iter().map(|(name, bytes)| {
                let name = format!("wrapper/{name}");
                (if windows { name.replace('/', "\\") } else { name }, bytes)
            }).collect();
            entries.push(("wrapper/b/.DS_Store".into(), b"unselected metadata".to_vec()));
            import(&zip_at(&temp, entries), &temp.dest(), false, |_, _| {}).unwrap();
            assert_eq!(std::fs::read(temp.dest().join("data/asset.bin")).unwrap(), b"SYNTHETIC_ASSET_CONTENT!");
            assert!(!temp.dest().join("b/.DS_Store").exists());
        }
    }

    #[test]
    fn keep_code_requires_and_copies_the_selected_build_only() {
        let temp = Temp::new();
        let path = zip_at(&temp, fixture());
        assert!(import(&path, &temp.dest(), true, |_, _| {}).is_err());
        let mut entries = fixture();
        // A marker tests copy selection; it is not a valid WASM module.
        entries.push(("b/test-build/game.wasm".into(), b"synthetic copy selection marker".to_vec()));
        import(&zip_at(&temp, entries), &temp.dest(), true, |_, _| {}).unwrap();
        assert_eq!(std::fs::read(temp.dest().join("b/test-build/game.wasm")).unwrap(), b"synthetic copy selection marker");
        import(&path, &temp.dest(), false, |_, _| {}).unwrap();
        assert!(!temp.dest().join("b/test-build/game.wasm").exists());
        assert!(!temp.dest().with_extension("previous").exists());
    }
}
