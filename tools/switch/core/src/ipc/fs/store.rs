use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, SeqAccess, Visitor},
};
use std::{
    ffi::CString,
    fs::File,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_SNAPSHOT: u64 = 48 * 1024 * 1024;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Default)]
pub(super) struct Tree {
    pub dirs: BTreeSet<String>,
    pub files: BTreeMap<String, Vec<u8>>,
}
impl Tree {
    pub fn empty() -> Self {
        Self {
            dirs: BTreeSet::from(["/".into()]),
            files: BTreeMap::new(),
        }
    }
    pub fn total(&self) -> usize {
        self.files.values().map(Vec::len).sum()
    }
    pub fn resize(&mut self, path: &str, size: usize) -> Result<(), Error> {
        let old = self
            .files
            .get(path)
            .ok_or(Error::Malformed("missing file"))?
            .len();
        if size > MAX_FILE || self.total() - old + size > MAX_TOTAL {
            return Err(Error::Malformed("file size limit"));
        }
        self.files
            .get_mut(path)
            .expect("checked file")
            .resize(size, 0);
        Ok(())
    }
    pub fn create(&mut self, path: String, size: usize) -> Result<(), Error> {
        if self.files.len() >= 128
            || self.files.contains_key(&path)
            || size > MAX_FILE
            || self.total() + size > MAX_TOTAL
        {
            return Err(Error::Malformed("file creation limit"));
        }
        self.files.insert(path, vec![0; size]);
        Ok(())
    }
}
#[derive(Clone)]
pub(super) struct Save {
    pub tree: Tree,
    pub size: u64,
    pub id: u64,
}
pub(super) type Saves = BTreeMap<[u8; 64], Save>;
#[derive(Serialize, Deserialize)]
struct Record {
    attribute: String,
    dirs: Vec<String>,
    files: BTreeMap<String, String>,
    space: u8,
    size: u64,
    id: u64,
}
#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    saves: Vec<Record>,
}
#[derive(Serialize, Deserialize)]
struct SdSnapshot {
    version: u32,
    dirs: Vec<String>,
    files: BTreeMap<String, String>,
}

// Parsing through this visitor rejects duplicate keys at every nesting level.
struct Unique(serde_json::Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Unique, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some((key, value)) = map.next_entry::<String, Unique>()? {
                    if out.insert(key, value.0).is_some() {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(Unique(out.into()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Unique, A::Error> {
                let mut out = Vec::new();
                while let Some(value) = seq.next_element::<Unique>()? {
                    out.push(value.0);
                }
                Ok(Unique(out.into()))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
                Ok(Unique(serde_json::Value::from(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(serde_json::Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}
fn json_error(e: impl std::fmt::Display) -> Error {
    Error::Message(format!("invalid filesystem snapshot: {e}"))
}
fn parse<T: serde::de::DeserializeOwned>(raw: &[u8]) -> Result<T, Error> {
    let value: Unique = serde_json::from_slice(raw).map_err(json_error)?;
    serde_json::from_value(value.0).map_err(json_error)
}
pub(super) fn valid_attr(attr: &[u8; 64]) -> bool {
    attr[32] == 1
        && attr[8] == 1
        && attr[9..24].iter().all(|b| *b == 0)
        && attr[36..].iter().all(|b| *b == 0)
}
fn decode_tree(dirs: Vec<String>, files: BTreeMap<String, String>) -> Result<Tree, Error> {
    let count = dirs.len();
    let dirs: BTreeSet<_> = dirs.into_iter().collect();
    if count > 256
        || count != dirs.len()
        || !dirs.contains("/")
        || dirs
            .iter()
            .any(|p| !valid_path(p) || (p != "/" && !dirs.contains(parent(p))))
        || files.len() > 128
    {
        return Err(Error::Malformed("snapshot directories"));
    }
    let mut tree = Tree {
        dirs,
        files: BTreeMap::new(),
    };
    for (path, encoded) in files {
        if !valid_path(&path)
            || tree.dirs.contains(&path)
            || !tree.dirs.contains(parent(&path))
            || encoded.len() > 4 * MAX_FILE.div_ceil(3)
        {
            return Err(Error::Malformed("snapshot file path or size"));
        }
        let data = STANDARD.decode(encoded).map_err(json_error)?;
        tree.create(path.clone(), data.len())?;
        tree.files.insert(path, data);
    }
    Ok(tree)
}
fn encode_files(tree: &Tree) -> BTreeMap<String, String> {
    tree.files
        .iter()
        .map(|(p, v)| (p.clone(), STANDARD.encode(v)))
        .collect()
}
fn decode_saves(raw: &[u8]) -> Result<Saves, Error> {
    let snapshot: Snapshot = parse(raw)?;
    if snapshot.version != 1 || snapshot.saves.len() > 16 {
        return Err(Error::Malformed("snapshot version or save count"));
    }
    let mut saves = Saves::new();
    let mut ids = BTreeSet::new();
    let mut total = 0;
    for record in snapshot.saves {
        if record.attribute.len() != 128 || !record.attribute.is_ascii() {
            return Err(Error::Malformed("save attribute encoding"));
        }
        let mut attr = [0; 64];
        for (i, byte) in attr.iter_mut().enumerate() {
            *byte =
                u8::from_str_radix(&record.attribute[i * 2..i * 2 + 2], 16).map_err(json_error)?;
        }
        if !valid_attr(&attr)
            || saves.contains_key(&attr)
            || record.space != 1
            || record.size > 128 * 1024 * 1024
            || record.id == 0
            || !ids.insert(record.id)
        {
            return Err(Error::Malformed("save metadata"));
        }
        let tree = decode_tree(record.dirs, record.files)?;
        total += tree.total();
        if tree.total() as u64 > record.size || total > MAX_TOTAL {
            return Err(Error::Malformed("save aggregate size"));
        }
        saves.insert(
            attr,
            Save {
                tree,
                size: record.size,
                id: record.id,
            },
        );
    }
    Ok(saves)
}
fn encode_saves(saves: &Saves) -> Result<Vec<u8>, Error> {
    let saves = saves
        .iter()
        .map(|(attr, s)| Record {
            attribute: attr.iter().map(|b| format!("{b:02x}")).collect(),
            dirs: s.tree.dirs.iter().cloned().collect(),
            files: encode_files(&s.tree),
            space: 1,
            size: s.size,
            id: s.id,
        })
        .collect();
    let raw = serde_json::to_vec(&Snapshot { version: 1, saves }).map_err(json_error)?;
    decode_saves(&raw)?;
    Ok(raw)
}

pub(super) struct Store {
    dir: File,
    _lock: File,
    #[cfg(test)]
    fail_before_rename: std::cell::Cell<bool>,
}
fn cstr(s: &str) -> Result<CString, Error> {
    CString::new(s).map_err(|_| Error::Malformed("host filename"))
}
fn open_at(dir: i32, name: &str, flags: i32, mode: libc::mode_t) -> Result<File, Error> {
    let name = cstr(name)?;
    // The descriptor remains owned by File and every path component is opened without following links.
    let fd = unsafe {
        libc::openat(
            dir,
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn directory_at(dir: i32, name: &str) -> Result<File, Error> {
    let name_c = cstr(name)?;
    if unsafe { libc::mkdirat(dir, name_c.as_ptr(), 0o700) } != 0
        && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(std::io::Error::last_os_error().into());
    }
    open_at(dir, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
}
impl Store {
    pub fn new(home: &Path, hash: &str) -> Result<Self, Error> {
        // Walk all existing home components using directory descriptors, rejecting ancestor symlinks too.
        let mut dir = File::open(if home.is_absolute() { "/" } else { "." })?;
        for component in home.components() {
            match component {
                std::path::Component::RootDir | std::path::Component::CurDir => (),
                std::path::Component::Normal(name) => {
                    dir = directory_at(
                        dir.as_raw_fd(),
                        name.to_str().ok_or(Error::Malformed("home encoding"))?,
                    )?;
                }
                _ => return Err(Error::Malformed("home path")),
            }
        }
        let saves = directory_at(dir.as_raw_fd(), "saves")?;
        let dir = directory_at(saves.as_raw_fd(), hash)?;
        let lock = open_at(
            dir.as_raw_fd(),
            ".lock",
            libc::O_CREAT | libc::O_RDWR | libc::O_NONBLOCK,
            0o600,
        )?;
        if !lock.metadata()?.is_file() {
            return Err(Error::Malformed("nonregular lock"));
        }
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self {
            dir,
            _lock: lock,
            #[cfg(test)]
            fail_before_rename: std::cell::Cell::new(false),
        })
    }
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, Error> {
        let file = match open_at(
            self.dir.as_raw_fd(),
            name,
            libc::O_RDONLY | libc::O_NONBLOCK,
            0,
        ) {
            Ok(f) => f,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let meta = file.metadata()?;
        if !meta.is_file() || meta.len() > MAX_SNAPSHOT {
            return Err(Error::Malformed("snapshot file type or size"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_SNAPSHOT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_SNAPSHOT {
            return Err(Error::Malformed("snapshot length"));
        }
        Ok(Some(bytes))
    }
    fn write(&self, name: &str, raw: &[u8]) -> Result<(), Error> {
        if raw.len() as u64 > MAX_SNAPSHOT {
            return Err(Error::Malformed("snapshot length"));
        }
        // Validate any old entry without opening it, so FIFOs cannot block commits.
        let target = cstr(name)?;
        let mut info = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                self.dir.as_raw_fd(),
                target.as_ptr(),
                info.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0
        {
            if unsafe { info.assume_init() }.st_mode & libc::S_IFMT != libc::S_IFREG {
                return Err(Error::Malformed("nonregular snapshot"));
            }
        } else if std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound {
            return Err(std::io::Error::last_os_error().into());
        }
        let temp = format!(
            ".soju-save-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        );
        let mut file = open_at(
            self.dir.as_raw_fd(),
            &temp,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        let temp_c = cstr(&temp)?;
        let result = (|| {
            file.write_all(raw)?;
            file.sync_all()?;
            #[cfg(test)]
            if self.fail_before_rename.get() {
                return Err(Error::Io(std::io::Error::other(
                    "injected pre-rename failure",
                )));
            }
            if unsafe {
                libc::renameat(
                    self.dir.as_raw_fd(),
                    temp_c.as_ptr(),
                    self.dir.as_raw_fd(),
                    target.as_ptr(),
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            self.dir.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            unsafe {
                libc::unlinkat(self.dir.as_raw_fd(), temp_c.as_ptr(), 0);
            }
        }
        result
    }
    pub fn load_saves(&self) -> Result<Saves, Error> {
        self.read("state.json")?
            .map_or_else(|| Ok(Saves::new()), |raw| decode_saves(&raw))
    }
    pub fn save(&self, saves: &Saves) -> Result<(), Error> {
        self.write("state.json", &encode_saves(saves)?)
    }
    pub fn load_sd(&self) -> Result<Tree, Error> {
        let Some(raw) = self.read("sd-state.json")? else {
            let mut tree = Tree::empty();
            tree.dirs.insert("/switch".into());
            return Ok(tree);
        };
        let snapshot: SdSnapshot = parse(&raw)?;
        if snapshot.version != 1 {
            return Err(Error::Malformed("SD snapshot version"));
        }
        let tree = decode_tree(snapshot.dirs, snapshot.files)?;
        if !tree.dirs.contains("/switch")
            || tree.files.contains_key(SELF_PATH)
            || tree.dirs.contains(SELF_PATH)
        {
            return Err(Error::Malformed("reserved SD path"));
        }
        Ok(tree)
    }
    pub fn save_sd(&self, tree: &Tree) -> Result<(), Error> {
        let raw = serde_json::to_vec(&SdSnapshot {
            version: 1,
            dirs: tree.dirs.iter().cloned().collect(),
            files: encode_files(tree),
        })
        .map_err(json_error)?;
        self.write("sd-state.json", &raw)
    }
}
#[cfg(test)]
mod tests;
