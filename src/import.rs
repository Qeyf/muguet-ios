// muguet - import.rs
// Constantin Clerc (c22dev) - 0.1

use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SKIPPED_DIRS: &[&str] = &["env_test", "verityradio"];
const MAX_ARCHIVES: usize = 256;
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

type Archive = zip::ZipArchive<std::io::BufReader<File>>;

struct ZipMember {
    archive: usize,
    index: usize,
    name: String,
    size: u64,
    directory: bool,
    crc: u32,
}

enum Source {
    Zip(Archive),
    Directory { root: PathBuf, paths: Vec<PathBuf> },
    ZipFolder { root: PathBuf, paths: Vec<PathBuf>, members: Vec<ZipMember>, archive: Option<(usize, Archive)> },
}

impl Source {
    fn reader(&mut self, index: usize) -> Result<Box<dyn Read + '_>> {
        match self {
            Self::Zip(zip) => Ok(Box::new(zip.by_index(index)?)),
            Self::ZipFolder { root, paths, members, archive } => {
                let member = &members[index];
                let path = &paths[member.archive];
                check_source_path(root, path)?;
                if !archive.as_ref().is_some_and(|(index, _)| *index == member.archive) {
                    // Drop the previous reader before opening another archive;
                    // a folder of parts never keeps N large buffers or copies.
                    *archive = None;
                    let zip = open_zip(path)?;
                    *archive = Some((member.archive, zip));
                }
                let zip = &mut archive.as_mut().unwrap().1;
                let entry = zip.by_index(member.index)?;
                check_zip_member(&entry)?;
                if relative_name(entry.name())? != member.name || entry.size() != member.size || entry.is_dir() != member.directory || entry.crc32() != member.crc {
                    bail!("ZIP member changed during import: {} in {}", member.name, path.display());
                }
                Ok(Box::new(entry))
            }
            Self::Directory { root, paths } => {
                // Recheck each ancestor before reading a provider's file. The
                // native picker coordinates this whole read with its provider.
                let path = &paths[index];
                check_source_path(root, path)?;
                Ok(Box::new(File::open(path).with_context(|| format!("reading {}", path.display()))?))
            }
        }
    }
}

fn check_source_path(root: &Path, path: &Path) -> Result<()> {
    // canonicalize fixed the initial spelling, but a provider may later replace
    // an ancestor with a link. Check the whole source path again on each read.
    let mut ancestor = PathBuf::new();
    for part in root.components() {
        ancestor.push(part);
        let meta = std::fs::symlink_metadata(&ancestor)?;
        if meta.file_type().is_symlink() { bail!("symbolic links are not allowed: {}", ancestor.display()); }
        if !meta.is_dir() { bail!("source folder changed: {}", ancestor.display()); }
    }
    let mut parent = root.to_path_buf();
    for part in path.strip_prefix(root)?.components() {
        parent.push(part);
        let meta = std::fs::symlink_metadata(&parent)?;
        if meta.file_type().is_symlink() { bail!("symbolic links are not allowed: {}", parent.display()); }
        if !(meta.is_dir() || meta.is_file()) { bail!("unsupported file type: {}", parent.display()); }
    }
    Ok(())
}

fn check_zip_member(entry: &zip::read::ZipFile<'_>) -> Result<()> {
    if entry.is_symlink() { bail!("symbolic links are not allowed: {}", entry.name()); }
    if entry.unix_mode().is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o100000 | 0o040000)) {
        bail!("unsupported file type: {}", entry.name());
    }
    Ok(())
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

fn open_zip(zip_path: &Path) -> Result<Archive> {
    let meta = std::fs::symlink_metadata(zip_path)?;
    if meta.file_type().is_symlink() || !meta.is_file() { bail!("unsupported file or symbolic link: {}", zip_path.display()); }
    let file = File::open(zip_path).with_context(|| format!("{}", zip_path.display()))?;
    let zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 20, file)).context("not a zip file")?;
    if zip.len() > MAX_ENTRIES { bail!("too many archive entries (limit {MAX_ENTRIES})"); }
    Ok(zip)
}

fn inventory_zip(zip_path: &Path) -> Result<(Archive, Vec<ZipMember>)> {
    let mut zip = open_zip(zip_path)?;
    // zip 2.4.2 stores filenames in an IndexMap and silently collapses exact
    // duplicates. Count the original directory records as well as its view.
    let mut directory = File::open(zip_path)?;
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
        check_zip_member(&entry)?;
        entries.push(ZipMember { archive: 0, index: k, name: relative_name(entry.name())?, size: entry.size(), directory: entry.is_dir(), crc: entry.crc32() });
    }
    Ok((zip, entries))
}

pub fn import(zip_path: &Path, dest: &Path, keep_code: bool, progress: impl FnMut(u64, u64)) -> Result<()> {
    let _lock = IMPORT_LOCK.lock().map_err(|_| anyhow::anyhow!("import lock poisoned"))?;
    let (zip, members) = inventory_zip(zip_path)?;
    let entries = members.into_iter().map(|m| Entry { index: m.index, name: m.name, size: m.size, directory: m.directory }).collect();
    let mut source = Source::Zip(zip);
    run_import(&mut source, entries, dest, keep_code, true, progress)
}

fn split_zip_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some((base, suffix)) = lower.rsplit_once('.') else { return false };
    (base.ends_with(".zip") && !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())) ||
        (suffix.len() == 3 && suffix.starts_with('z') && suffix[1..].bytes().all(|b| b.is_ascii_digit()))
}

fn numbered_zip_name(path: &Path) -> Result<Option<(String, usize, usize)>> {
    let name = path.file_name().and_then(|n| n.to_str()).context("ZIP filename is not UTF-8")?.to_ascii_lowercase();
    let stem = name.strip_suffix(".zip").unwrap();
    let Some((prefix, numbers)) = stem.rsplit_once("-part-") else { return Ok(None) };
    let Some((number, total)) = numbers.split_once("-of-") else { return Ok(None) };
    if prefix.is_empty() || number.is_empty() || total.is_empty() || !number.bytes().chain(total.bytes()).all(|b| b.is_ascii_digit()) {
        bail!("invalid numbered ZIP set filename: {name}");
    }
    let number: usize = number.parse().context("invalid numbered ZIP set part number")?;
    let total: usize = total.parse().context("invalid numbered ZIP set total")?;
    if total == 0 || total > MAX_ARCHIVES || number == 0 || number > total { bail!("invalid numbered ZIP set: {name}"); }
    Ok(Some((prefix.to_string(), number, total)))
}

fn check_zip_set(paths: &[PathBuf]) -> Result<()> {
    if paths.len() > MAX_ARCHIVES { bail!("too many ZIP archives (limit {MAX_ARCHIVES})"); }
    let mut expected = None;
    let mut numbers = HashSet::new();
    let mut generic = 0;
    for path in paths {
        match numbered_zip_name(path)? {
            Some((prefix, number, total)) => {
                let set = (prefix, total);
                if expected.as_ref().is_some_and(|previous| previous != &set) || !numbers.insert(number) {
                    bail!("mixed or duplicate numbered ZIP set");
                }
                expected = Some(set);
            }
            None => generic += 1,
        }
    }
    if let Some((_, total)) = expected {
        if generic != 0 { bail!("numbered ZIP set cannot contain unrelated ZIP archives"); }
        if numbers.len() != total { bail!("numbered ZIP set is incomplete: found {} of {total} parts", numbers.len()); }
    }
    Ok(())
}

fn zip_folder_source(root: PathBuf, paths: Vec<PathBuf>) -> Result<(Source, Vec<Entry>)> {
    check_zip_set(&paths)?;
    let mut entries = Vec::new();
    let mut members = Vec::new();
    for (archive, path) in paths.iter().enumerate() {
        check_source_path(&root, path)?;
        let (zip, inventory) = inventory_zip(path)?;
        drop(zip);
        if entries.len().checked_add(inventory.len()).context("archive entry count overflow")? > MAX_ENTRIES {
            bail!("too many archive entries (limit {MAX_ENTRIES})");
        }
        for mut member in inventory {
            member.archive = archive;
            entries.push(Entry { index: members.len(), name: member.name.clone(), size: member.size, directory: member.directory });
            members.push(member);
        }
    }
    // Limits and duplicate paths apply to the entire extraction layout, not
    // separately to each ZIP. Repeated explicit directory records are harmless.
    check_inventory(&entries)?;
    Ok((Source::ZipFolder { root, paths, members, archive: None }, entries))
}

pub fn import_directory(root: &Path, dest: &Path, keep_code: bool, progress: impl FnMut(u64, u64)) -> Result<()> {
    let _lock = IMPORT_LOCK.lock().map_err(|_| anyhow::anyhow!("import lock poisoned"))?;
    let meta = std::fs::symlink_metadata(root)?;
    if !meta.is_dir() || meta.file_type().is_symlink() { bail!("select a folder containing data/ and b/ or independent ZIP archives, not a symbolic link"); }
    let root = root.canonicalize()?;
    if dest.canonicalize().ok().is_some_and(|d| root.starts_with(&d)) {
        bail!("select source data outside the app's current game folder");
    }
    let mut archives = Vec::new();
    for child in std::fs::read_dir(&root)? {
        let child = child?;
        let path = child.path();
        let name = child.file_name();
        let name = name.to_str().context("file path is not UTF-8")?;
        if split_zip_name(name) { bail!("split ZIP streams (.z01 or .zip.001) are not supported; select independent complete ZIP archives"); }
        if name.to_ascii_lowercase().ends_with(".zip") {
            let meta = std::fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() { bail!("symbolic links are not allowed: {}", path.display()); }
            if meta.is_file() { archives.push(path); }
        }
    }
    if !archives.is_empty() {
        if root.join("data/manifest.json").exists() {
            bail!("folder contains both extracted game data and ZIP archives; select one source");
        }
        archives.sort();
        let (mut source, entries) = zip_folder_source(root, archives)?;
        return run_import(&mut source, entries, dest, keep_code, true, progress);
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
    if matches!(source, Source::ZipFolder { .. }) {
        for entry in entries {
            // A second build root may contain only assets, with no manifest or
            // shader index. Do not silently ignore it outside the chosen wrapper.
            if entry.name.starts_with(&format!("{prefix}data/")) || entry.name.starts_with(&format!("{prefix}b/")) ||
                (!prefix.is_empty() && entry.name.starts_with(prefix)) { continue; }
            let parts: Vec<_> = entry.name.split('/').collect();
            for n in 0..parts.len().saturating_sub(1) {
                if parts[n] == "b" && (entry.directory || n + 2 < parts.len()) {
                    let root = if n == 0 { String::new() } else { format!("{}/", parts[..n].join("/")) };
                    if root != prefix { bail!("foreign b/ build root outside the selected game: {}", entry.name); }
                }
            }
        }
    }
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
    let mut constants_entries = Vec::new();
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
                constants_entries.push(*entry);
            }
        }
    }
    if compiled == 0 { bail!("shader index contains no compiled shader sources"); }
    // Union indices group members by archive. Reading constants in this order
    // avoids reopening/reparsing ZIPs in the shader index's hash-key order.
    // This does not reorder the game manifest or the extraction jobs.
    constants_entries.sort_by_key(|entry| entry.index);
    for entry in constants_entries {
        let constants: serde_json::Value = serde_json::from_slice(&metadata(source, entry)?).context("shader constants json")?;
        if !constants.is_array() { bail!("shader constants must be an array: {}", entry.name); }
    }
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
        write_zip(&path, entries);
        path
    }

    fn write_zip(path: &Path, entries: Vec<(String, Vec<u8>)>) {
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in entries {
            writer.start_file(name, SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
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

    fn zip_folder_at(temp: &Temp, archives: Vec<(&str, Vec<(String, Vec<u8>)>)>) -> PathBuf {
        let root = temp.0.join("parts");
        std::fs::create_dir_all(&root).unwrap();
        for (name, entries) in archives { write_zip(&root.join(name), entries); }
        root
    }

    fn generic_zip_folder(temp: &Temp, entries: Vec<(String, Vec<u8>)>) -> PathBuf {
        let mut groups = [Vec::new(), Vec::new(), Vec::new()];
        for (name, bytes) in entries {
            let group = if name.ends_with("/manifest.json") { 0 } else if name.contains("/shaders/") { 2 } else { 1 };
            groups[group].push((name, bytes));
        }
        zip_folder_at(temp, vec![("a-metadata.zip", groups[0].clone()), ("b-assets.zip", groups[1].clone()), ("c-shaders.zip", groups[2].clone())])
    }

    fn assert_folder_rejected(temp: &Temp, root: &Path, reason: &str) {
        std::fs::create_dir_all(temp.dest()).unwrap();
        std::fs::write(temp.dest().join("previous"), b"keep previous import").unwrap();
        let error = import_directory(root, &temp.dest(), false, |_, _| {}).unwrap_err();
        assert!(format!("{error:#}").contains(reason), "unexpected rejection: {error:#}");
        assert_eq!(std::fs::read(temp.dest().join("previous")).unwrap(), b"keep previous import");
        assert!(!temp.dest().with_extension("importing").exists());
    }

    #[test]
    fn fourteen_numbered_zip_parts_import_the_wrapped_producer_layout() {
        let temp = Temp::new();
        let mut entries: Vec<_> = (0..14).map(|n| (format!("data/asset-{n}.bin"), vec![n as u8])).collect();
        let files: Vec<_> = (0..14).map(|n| serde_json::json!([format!("asset-{n}.bin"), 1])).collect();
        entries.push(("data/manifest.json".into(), serde_json::to_vec(&serde_json::json!({ "files": files })).unwrap()));
        entries.extend(fixture().into_iter().filter(|(name, _)| name.starts_with("b/")));
        entries.push(("b/test-build/game.wasm".into(), b"unselected code marker".to_vec()));
        let mut groups = vec![Vec::new(); 14];
        for (n, (name, bytes)) in entries.into_iter().enumerate() {
            groups[n % 14].push((format!("mirror/playgta5.com/{}", name.replace("test-build", "8b0b5899ed")), bytes));
        }
        let root = temp.0.join("parts");
        std::fs::create_dir_all(&root).unwrap();
        for (n, entries) in groups.into_iter().enumerate() { write_zip(&root.join(format!("snapshot-part-{:02}-of-14.zip", n + 1)), entries); }
        std::fs::create_dir_all(temp.dest()).unwrap();
        std::fs::write(temp.dest().join("previous"), b"previous").unwrap();
        let mut progress = Vec::new();
        import_directory(&root, &temp.dest(), false, |d, t| progress.push((d, t))).unwrap();
        for n in 0..14 { assert_eq!(std::fs::read(temp.dest().join(format!("data/asset-{n}.bin"))).unwrap(), vec![n]); }
        assert!(temp.dest().join("b/8b0b5899ed/shaders/index.json").exists());
        assert!(!temp.dest().join("b/8b0b5899ed/game.wasm").exists());
        assert!(!temp.dest().join("mirror").exists());
        assert!(!temp.dest().join("previous").exists());
        assert!(!temp.dest().with_extension("previous").exists());
        assert_eq!(progress.first().unwrap().0, 0);
        let &(done, total) = progress.last().unwrap();
        assert_eq!(done, total);
        assert!(progress.windows(2).all(|p| p[0].0 <= p[1].0 && p[0].1 == p[1].1));
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 14, "import created a combined ZIP or extracted copy");
    }

    #[test]
    fn generic_zip_folder_requires_all_manifest_assets() {
        let temp = Temp::new();
        let entries = fixture().into_iter().filter(|(name, _)| name != "data/asset.bin").collect();
        let root = generic_zip_folder(&temp, entries);
        assert_folder_rejected(&temp, &root, "missing mandatory asset");
    }

    #[test]
    fn generic_zip_folder_requires_shader_sources_and_constants() {
        for constants in [false, true] {
            let temp = Temp::new();
            let mut entries = fixture();
            if constants {
                entries[2].1 = br#"{"0123456789abcdef":{"ok":true,"consts":true}}"#.to_vec();
            } else { entries.retain(|(name, _)| !name.ends_with(".wgsl")); }
            let root = generic_zip_folder(&temp, entries);
            assert_folder_rejected(&temp, &root, if constants { "missing shader constants" } else { "missing or empty shader source" });
        }
    }

    #[test]
    fn numbered_zip_folder_requires_all_declared_parts_even_with_complete_metadata() {
        let temp = Temp::new();
        let root = zip_folder_at(&temp, vec![("snapshot-part-01-of-14.zip", fixture())]);
        assert_folder_rejected(&temp, &root, "numbered ZIP set is incomplete");
    }

    #[test]
    fn numbered_zip_folder_rejects_mixed_sets_and_generic_extras() {
        for second in ["extra.zip", "different-part-02-of-02.zip", "snapshot-part-02-of-03.zip"] {
            let temp = Temp::new();
            let root = zip_folder_at(&temp, vec![("snapshot-part-01-of-02.zip", fixture()), (second, Vec::new())]);
            assert_folder_rejected(&temp, &root, "numbered ZIP set");
        }
    }

    #[test]
    fn numbered_zip_folder_rejects_invalid_and_duplicate_part_numbers() {
        for names in [
            ["snapshot-part-00-of-01.zip", "empty.zip"],
            ["snapshot-part-02-of-01.zip", "empty.zip"],
            ["snapshot-part-01-of-257.zip", "empty.zip"],
            ["snapshot-part-1-of-2.zip", "snapshot-part-01-of-02.zip"],
        ] {
            let temp = Temp::new();
            let root = zip_folder_at(&temp, vec![(names[0], fixture()), (names[1], Vec::new())]);
            assert_folder_rejected(&temp, &root, "numbered ZIP set");
        }
    }

    #[test]
    fn split_zip_sets_are_rejected_instead_of_treated_as_independent_archives() {
        for name in ["snapshot.z01", "snapshot.zip.001", "snapshot.Z99"] {
            let temp = Temp::new();
            let root = zip_folder_at(&temp, vec![("snapshot.zip", fixture())]);
            std::fs::write(root.join(name), b"split stream marker").unwrap();
            assert_folder_rejected(&temp, &root, "split ZIP");
        }
    }

    #[test]
    fn zip_folder_uses_only_top_level_archives() {
        let temp = Temp::new();
        let root = generic_zip_folder(&temp, fixture().into_iter().filter(|(name, _)| name != "data/asset.bin").collect());
        std::fs::create_dir(root.join("nested")).unwrap();
        write_zip(&root.join("nested/assets.zip"), vec![("data/asset.bin".into(), b"SYNTHETIC_ASSET_CONTENT!".to_vec())]);
        assert_folder_rejected(&temp, &root, "missing mandatory asset");
    }

    #[test]
    fn zip_folder_rejects_duplicate_and_case_colliding_union_paths() {
        for name in ["data/asset.bin", "data/ASSET.bin", r"data\asset.bin"] {
            let temp = Temp::new();
            let root = zip_folder_at(&temp, vec![("complete.zip", fixture()), ("collision.zip", vec![(name.into(), b"collision".to_vec())])]);
            assert_folder_rejected(&temp, &root, "duplicate or conflicting file path");
        }
    }

    #[test]
    fn zip_folder_rejects_raw_duplicate_central_directory_names() {
        let temp = Temp::new();
        let root = zip_folder_at(&temp, vec![("complete.zip", fixture()), ("duplicates.zip", vec![
            ("other/name.bin".into(), b"a".to_vec()), ("other/file.bin".into(), b"b".to_vec()),
        ])]);
        let path = root.join("duplicates.zip");
        let mut bytes = std::fs::read(&path).unwrap();
        let positions: Vec<_> = bytes.windows(14).enumerate().filter_map(|(p, b)| (b == b"other/file.bin").then_some(p)).collect();
        assert_eq!(positions.len(), 2);
        for p in positions { bytes[p..p + 14].copy_from_slice(b"other/name.bin"); }
        std::fs::write(path, bytes).unwrap();
        assert_folder_rejected(&temp, &root, "duplicate or inconsistent ZIP directory entries");
    }

    #[test]
    fn zip_folder_rejects_unsafe_paths_in_any_part() {
        let temp = Temp::new();
        let root = zip_folder_at(&temp, vec![("complete.zip", fixture()), ("unsafe.zip", vec![("wrapper/data/../../outside".into(), b"bad".to_vec())])]);
        assert_folder_rejected(&temp, &root, "unsafe relative path");
        assert!(!temp.0.join("outside").exists());
    }

    #[test]
    fn zip_folder_rejects_assets_only_foreign_builds_inside_the_selected_root() {
        let temp = Temp::new();
        let root = zip_folder_at(&temp, vec![("complete.zip", fixture()), ("foreign.zip", vec![("b/other-build/texture.bin".into(), b"foreign".to_vec())])]);
        assert_folder_rejected(&temp, &root, "select exactly one build under b/");
    }

    #[test]
    fn zip_folder_rejects_foreign_build_roots_outside_the_wrapped_prefix() {
        for foreign in ["b/test-build/texture.bin", "another-root/b/other-build/texture.bin"] {
            let temp = Temp::new();
            let wrapped = fixture().into_iter().map(|(name, bytes)| (format!("mirror/playgta5.com/{name}"), bytes)).collect();
            let root = zip_folder_at(&temp, vec![("complete.zip", wrapped), ("foreign.zip", vec![(foreign.into(), b"foreign".to_vec())])]);
            assert_folder_rejected(&temp, &root, "foreign b/ build root");
        }
    }

    #[test]
    fn zip_folder_rejects_more_than_256_archives() {
        let temp = Temp::new();
        let root = zip_folder_at(&temp, vec![("complete.zip", fixture())]);
        for n in 0..256 { write_zip(&root.join(format!("empty-{n}.zip")), Vec::new()); }
        assert_folder_rejected(&temp, &root, "too many ZIP archives");
    }

    #[test]
    fn zip_folder_crc_failure_cleans_staging_and_preserves_previous_import() {
        let temp = Temp::new();
        let root = generic_zip_folder(&temp, fixture());
        let path = root.join("b-assets.zip");
        let mut bytes = std::fs::read(&path).unwrap();
        let pos = bytes.windows(24).position(|w| w == b"SYNTHETIC_ASSET_CONTENT!").unwrap();
        bytes[pos] ^= 1;
        std::fs::write(path, bytes).unwrap();
        assert_folder_rejected(&temp, &root, "Invalid checksum");
    }

    #[test]
    fn zip_folder_reopened_member_is_bound_to_the_inventoried_crc() {
        let temp = Temp::new();
        let root = generic_zip_folder(&temp, fixture());
        std::fs::create_dir_all(temp.dest()).unwrap();
        std::fs::write(temp.dest().join("previous"), b"previous").unwrap();
        let error = import_directory(&root, &temp.dest(), false, |done, _| {
            if done == 0 { write_zip(&root.join("b-assets.zip"), vec![("data/asset.bin".into(), vec![b'X'; 24])]); }
        }).unwrap_err();
        assert!(format!("{error:#}").contains("ZIP member changed"), "{error:#}");
        assert_eq!(std::fs::read(temp.dest().join("previous")).unwrap(), b"previous");
        assert!(!temp.dest().with_extension("importing").exists());
    }

    #[test]
    fn zip_folder_reopened_member_rechecks_archive_symlinks() {
        let temp = Temp::new();
        let root = generic_zip_folder(&temp, fixture());
        std::fs::create_dir_all(temp.dest()).unwrap();
        std::fs::write(temp.dest().join("previous"), b"previous").unwrap();
        let error = import_directory(&root, &temp.dest(), false, |done, _| {
            if done == 0 {
                let mut writer = zip::ZipWriter::new(File::create(root.join("b-assets.zip")).unwrap());
                writer.add_symlink("data/asset.bin", "a-target-with-24-letters!", SimpleFileOptions::default()).unwrap();
                writer.finish().unwrap();
            }
        }).unwrap_err();
        assert!(format!("{error:#}").contains("symbolic links are not allowed"), "{error:#}");
        assert_eq!(std::fs::read(temp.dest().join("previous")).unwrap(), b"previous");
        assert!(!temp.dest().with_extension("importing").exists());
    }

    #[cfg(unix)]
    #[test]
    fn zip_folder_rechecks_the_source_root_and_archive_file_before_reading() {
        for replace_root in [false, true] {
            let temp = Temp::new();
            let root = generic_zip_folder(&temp, fixture());
            std::fs::create_dir_all(temp.dest()).unwrap();
            std::fs::write(temp.dest().join("previous"), b"previous").unwrap();
            let error = import_directory(&root, &temp.dest(), false, |done, _| {
                if done == 0 {
                    if replace_root {
                        let moved = temp.0.join("moved");
                        std::fs::rename(&root, &moved).unwrap();
                        std::os::unix::fs::symlink(&moved, &root).unwrap();
                    } else {
                        let path = root.join("b-assets.zip");
                        let moved = temp.0.join("moved.zip");
                        std::fs::rename(&path, &moved).unwrap();
                        std::os::unix::fs::symlink(&moved, &path).unwrap();
                    }
                }
            }).unwrap_err();
            assert!(format!("{error:#}").contains("symbolic links are not allowed"), "{error:#}");
            assert_eq!(std::fs::read(temp.dest().join("previous")).unwrap(), b"previous");
            assert!(!temp.dest().with_extension("importing").exists());
        }
    }

    #[test]
    fn zip_folder_source_inside_the_current_game_is_rejected() {
        let temp = Temp::new();
        let root = temp.dest().join("parts");
        std::fs::create_dir_all(&root).unwrap();
        write_zip(&root.join("complete.zip"), fixture());
        assert_folder_rejected(&temp, &root, "outside the app's current game folder");
    }

    #[test]
    fn zip_folder_rejects_foreign_wrapped_build_with_a_direct_game_root() {
        let temp = Temp::new();
        let root = zip_folder_at(&temp, vec![("complete.zip", fixture()),
            ("foreign.zip", vec![("other/b/other-build/texture.bin".into(), b"foreign".to_vec())])]);
        assert_folder_rejected(&temp, &root, "foreign b/ build root");
    }

    #[test]
    fn zip_folder_rejects_an_ambiguous_extracted_source_beside_parts() {
        let temp = Temp::new();
        let root = generic_zip_folder(&temp, fixture());
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("data/manifest.json"), br#"{"files":[["old.bin",1]]}"#).unwrap();
        assert_folder_rejected(&temp, &root, "both extracted game data and ZIP archives");
    }

    #[test]
    fn zip_folder_allows_asset_subdirectories_named_b_inside_the_selected_wrapper() {
        let temp = Temp::new();
        let mut entries = fixture();
        entries[0].1 = br#"{"files":[["textures/b/group/asset.bin",24]]}"#.to_vec();
        entries[1].0 = "data/textures/b/group/asset.bin".into();
        let wrapped = entries.into_iter().map(|(name, bytes)| (format!("mirror/playgta5.com/{name}"), bytes)).collect();
        let root = generic_zip_folder(&temp, wrapped);
        import_directory(&root, &temp.dest(), false, |_, _| {}).unwrap();
        assert_eq!(std::fs::read(temp.dest().join("data/textures/b/group/asset.bin")).unwrap(), b"SYNTHETIC_ASSET_CONTENT!");
    }

    fn set_central_size(path: &Path, size: u32) {
        let mut bytes = std::fs::read(path).unwrap();
        let positions: Vec<_> = bytes.windows(4).enumerate().filter_map(|(n, signature)| (signature == b"PK\x01\x02").then_some(n)).collect();
        assert!(!positions.is_empty());
        for n in positions { bytes[n + 24..n + 28].copy_from_slice(&size.to_le_bytes()); }
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn zip_folder_total_size_limit_applies_to_the_union() {
        let temp = Temp::new();
        let root = zip_folder_at(&temp, vec![("complete.zip", fixture())]);
        // Advertise large uncompressed sizes without allocating their contents.
        // Each part is below 128 GiB, and each entry is below 32 GiB.
        for archive in 0..2 {
            let path = root.join(format!("large-{archive}.zip"));
            write_zip(&path, (0..17).map(|n| (format!("data/large-{archive}-{n}.bin"), vec![b'x'])).collect());
            set_central_size(&path, u32::MAX - 1);
        }
        assert_folder_rejected(&temp, &root, "archive exceeds 128 GiB");
    }

    #[test]
    fn zip_folder_json_metadata_limit_is_checked_before_decompression() {
        let temp = Temp::new();
        let root = generic_zip_folder(&temp, fixture());
        set_central_size(&root.join("a-metadata.zip"), (33 << 20) as u32);
        assert_folder_rejected(&temp, &root, "metadata exceeds 32 MiB");
    }

    #[test]
    fn zip_folder_plan_enforces_the_embedded_build_id() {
        let temp = Temp::new();
        let root = generic_zip_folder(&temp, fixture());
        let mut paths: Vec<_> = std::fs::read_dir(&root).unwrap().map(|entry| entry.unwrap().path()).collect();
        paths.sort();
        let (mut source, entries) = zip_folder_source(root, paths).unwrap();
        let error = plan(&mut source, &entries, false, true, Some("8b0b5899ed")).err().unwrap();
        assert!(format!("{error:#}").contains("does not match this app's embedded build"));
        assert!(plan(&mut source, &entries, false, true, Some("test-build")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn zip_folder_rechecks_source_ancestors_after_inventory() {
        let temp = Temp::new();
        let initial = generic_zip_folder(&temp, fixture());
        let parent = temp.0.join("provider");
        std::fs::create_dir(&parent).unwrap();
        let root = parent.join("parts");
        std::fs::rename(initial, &root).unwrap();
        std::fs::create_dir_all(temp.dest()).unwrap();
        std::fs::write(temp.dest().join("previous"), b"previous").unwrap();
        let result = import_directory(&root, &temp.dest(), false, |done, _| {
            if done == 0 {
                let moved = temp.0.join("moved-provider");
                std::fs::rename(&parent, &moved).unwrap();
                std::os::unix::fs::symlink(&moved, &parent).unwrap();
            }
        });
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("symbolic links are not allowed"), "{error:#}");
        assert_eq!(std::fs::read(temp.dest().join("previous")).unwrap(), b"previous");
        assert!(!temp.dest().with_extension("importing").exists());
    }
}
