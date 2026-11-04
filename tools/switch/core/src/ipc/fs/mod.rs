//! Bounded virtual SD/account filesystems. Guest paths never address the host filesystem.
mod store;
use super::types::*;
use crate::kernel::{Handle, ObjectId};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use store::{Save, Saves, Store, Tree};
const MAX_FILE: usize = 8 * 1024 * 1024;
const MAX_TOTAL: usize = 32 * 1024 * 1024;
const SELF_PATH: &str = "/switch/soju-current.nro";
const DIRECTORY_ENTRY: usize = 0x310;

#[derive(Clone, Copy)]
enum Volume {
    Sd,
    Save([u8; 64]),
}
enum Object {
    Proxy,
    Volume(Volume),
    File {
        volume: Volume,
        path: String,
        mode: u32,
    },
    SelfFile,
    Directory {
        entries: Vec<u8>,
        position: usize,
    },
    Reader {
        entries: Vec<u8>,
        position: usize,
    },
}
pub struct Filesystem {
    hash: String,
    program: u64,
    nro: Vec<u8>,
    store: Store,
    sd: Tree,
    sd_dirty: bool,
    saves: Saves,
    committed: Saves,
    objects: BTreeMap<ObjectId, Object>,
}
macro_rules! command {
    ($id:expr, $size:expr, $handler:ident) => {
        CommandSpec {
            id: $id,
            schema: Schema::at_least($size),
            handler: Self::$handler,
        }
    };
}
impl Filesystem {
    pub fn new(nro_path: &Path, home: &Path) -> Result<Self, Error> {
        let nro = std::fs::read(nro_path)?;
        let digest = Sha256::digest(&nro);
        let hash = digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let program =
            0x01ff_0000_0000_0000 | u64::from_str_radix(&hash[..12], 16).expect("digest hex");
        let store = Store::new(home, &hash)?;
        let saves = store.load_saves()?;
        let sd = store.load_sd()?;
        Ok(Self {
            hash,
            program,
            nro,
            store,
            sd,
            sd_dirty: false,
            committed: saves.clone(),
            saves,
            objects: BTreeMap::new(),
        })
    }
    pub fn game_hash(&self) -> &str {
        &self.hash
    }
    pub fn program_id(&self) -> u64 {
        self.program
    }
    pub fn open(&mut self, name: &str, ctx: &mut Context<'_>) -> Result<Handle, Error> {
        if name != "fsp-srv" {
            return Err(Error::Malformed("unknown filesystem service"));
        }
        self.register(Object::Proxy, ctx)
    }
    fn register(&mut self, object: Object, ctx: &mut Context<'_>) -> Result<Handle, Error> {
        let (handle, id) = ctx.new_session(Family::Filesystem)?;
        self.objects.insert(id, object);
        Ok(handle)
    }
    pub fn close_object(&mut self, id: ObjectId) {
        self.objects.remove(&id);
    }
    /// Explicit graceful shutdown boundary; account saves still require their own commit.
    pub fn finish(&mut self) -> Result<(), Error> {
        self.flush_sd()
    }
    fn flush_sd(&mut self) -> Result<(), Error> {
        if self.sd_dirty {
            self.store.save_sd(&self.sd)?;
            self.sd_dirty = false;
        }
        Ok(())
    }
    pub fn dispatch(&mut self, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        let table: &[CommandSpec<Self>] = match self
            .objects
            .get(&ctx.object)
            .ok_or(Error::Malformed("filesystem object"))?
        {
            Object::Proxy => &[
                command!(1, 8, proxy),
                command!(18, 0, proxy),
                command!(22, 0x65, proxy),
                command!(51, 0x48, proxy),
                command!(61, 1, proxy),
            ],
            Object::Volume(_) => &[
                command!(0, 16, volume),
                command!(2, 0, volume),
                command!(7, 0, volume),
                command!(8, 4, volume),
                command!(9, 4, volume),
                command!(10, 0, volume),
                command!(14, 0, volume),
            ],
            Object::File { .. } => &[
                command!(0, 24, file),
                command!(1, 24, file),
                command!(2, 0, file),
                command!(3, 8, file),
                command!(4, 0, file),
            ],
            Object::SelfFile => &[command!(0, 24, file), command!(4, 0, file)],
            Object::Directory { .. } => &[command!(0, 0, cursor), command!(1, 0, cursor)],
            Object::Reader { .. } => &[command!(0, 0, cursor)],
        };
        dispatch_table(self, table, req, ctx)
    }
    fn tree(&self, volume: Volume) -> Result<&Tree, Error> {
        match volume {
            Volume::Sd => Ok(&self.sd),
            Volume::Save(attr) => self
                .saves
                .get(&attr)
                .map(|s| &s.tree)
                .ok_or(Error::Malformed("save object")),
        }
    }
    fn tree_mut(&mut self, volume: Volume) -> Result<&mut Tree, Error> {
        match volume {
            Volume::Sd => {
                self.sd_dirty = true;
                Ok(&mut self.sd)
            }
            Volume::Save(attr) => self
                .saves
                .get_mut(&attr)
                .map(|s| &mut s.tree)
                .ok_or(Error::Malformed("save object")),
        }
    }
    fn commit(&mut self, volume: Volume) -> Result<(), Error> {
        match volume {
            Volume::Sd => self.flush_sd(),
            Volume::Save(attr) => {
                let mut snapshot = self.committed.clone();
                snapshot.insert(
                    attr,
                    self.saves
                        .get(&attr)
                        .ok_or(Error::Malformed("missing save"))?
                        .clone(),
                );
                self.store.save(&snapshot)?;
                self.committed = snapshot;
                Ok(())
            }
        }
    }
    fn proxy(&mut self, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        match req.command_id {
            1 => Ok(Reply::success()),
            18 => Ok(Reply::object(
                self.register(Object::Volume(Volume::Sd), ctx)?,
            )),
            22 | 51 => {
                let offset = if req.command_id == 22 { 0 } else { 8 };
                let attr: [u8; 64] = req.data[offset..offset + 64]
                    .try_into()
                    .map_err(|_| Error::Malformed("save identity"))?;
                if !store::valid_attr(&attr) {
                    return Err(Error::Malformed("unsupported save identity"));
                }
                if req.command_id == 22 {
                    let size = req.u64(64)?;
                    let journal = req.u64(72)?;
                    if size > 128 * 1024 * 1024 || journal > i64::MAX as u64 || req.data[100] != 1 {
                        return Err(Error::Malformed("save allocation"));
                    }
                    if !self.saves.contains_key(&attr) {
                        if self.saves.len() >= 16 {
                            return Err(Error::Malformed("save count"));
                        }
                        let id = self
                            .saves
                            .values()
                            .map(|s| s.id)
                            .max()
                            .unwrap_or(0)
                            .checked_add(1)
                            .ok_or(Error::Malformed("save id overflow"))?;
                        let save = Save {
                            tree: Tree::empty(),
                            size,
                            id,
                        };
                        let mut snapshot = self.committed.clone();
                        snapshot.insert(attr, save.clone());
                        self.store.save(&snapshot)?;
                        self.committed = snapshot;
                        self.saves.insert(attr, save);
                    }
                    Ok(Reply::success())
                } else if req.data[0] != 1 || !self.saves.contains_key(&attr) {
                    Ok(Reply::result(0x202))
                } else {
                    Ok(Reply::object(
                        self.register(Object::Volume(Volume::Save(attr)), ctx)?,
                    ))
                }
            }
            61 => {
                let mut entries = Vec::new();
                if req.data[0] == 1 {
                    for (attr, save) in &self.saves {
                        let mut entry = [0; 0x60];
                        entry[..8].copy_from_slice(&save.id.to_le_bytes());
                        entry[8] = 1;
                        entry[9] = attr[32];
                        entry[16..32].copy_from_slice(&attr[8..24]);
                        entry[32..40].copy_from_slice(&attr[24..32]);
                        entry[40..48].copy_from_slice(&attr[..8]);
                        entry[48..56].copy_from_slice(&save.size.to_le_bytes());
                        entry[56..58].copy_from_slice(&attr[34..36]);
                        entry[58] = attr[33];
                        entries.extend(entry);
                    }
                }
                Ok(Reply::object(self.register(
                    Object::Reader {
                        entries,
                        position: 0,
                    },
                    ctx,
                )?))
            }
            _ => unreachable!(),
        }
    }
    fn volume(&mut self, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        let Some(Object::Volume(volume)) = self.objects.get(&ctx.object) else {
            return Err(Error::Malformed("volume object"));
        };
        let volume = *volume;
        if req.command_id == 10 {
            self.commit(volume)?;
            return Ok(Reply::success());
        }
        let path = guest_path(req, ctx)?;
        let tree = self.tree(volume)?;
        let self_file = matches!(volume, Volume::Sd) && path == SELF_PATH;
        let is_file = tree.files.contains_key(&path) || self_file;
        let is_dir = tree.dirs.contains(&path);
        match req.command_id {
            0 | 2 => {
                if is_file || is_dir {
                    return Ok(Reply::result(0x402));
                }
                if !tree.dirs.contains(parent(&path)) {
                    return Ok(Reply::result(0x202));
                }
                if req.command_id == 0 {
                    if req.u32(0)? != 0 {
                        return Err(Error::Malformed("create option"));
                    }
                    let size =
                        usize::try_from(req.u64(8)?).map_err(|_| Error::Malformed("file size"))?;
                    self.tree_mut(volume)?.create(path, size)?;
                } else {
                    if tree.dirs.len() >= 256 {
                        return Err(Error::Malformed("directory count"));
                    }
                    self.tree_mut(volume)?.dirs.insert(path);
                }
                Ok(Reply::success())
            }
            7 | 14 => {
                if !is_file && !is_dir {
                    return Ok(Reply::result(0x202));
                }
                // Command 14 intentionally retains the Python existence-only approximation.
                Ok(if req.command_id == 7 {
                    Reply::data(u32::from(is_file).to_le_bytes().to_vec())
                } else {
                    Reply::success()
                })
            }
            8 => {
                let mode = req.u32(0)?;
                let object = if self_file && mode == 1 {
                    Object::SelfFile
                } else if !self_file && is_file && mode & 3 != 0 && mode & !7 == 0 {
                    Object::File { volume, path, mode }
                } else {
                    return Ok(Reply::result(0x202));
                };
                Ok(Reply::object(self.register(object, ctx)?))
            }
            9 => {
                if !is_dir {
                    return Ok(Reply::result(0x202));
                }
                let mode = req.u32(0)?;
                if mode & !0x8000_0003 != 0 || mode & 3 == 0 {
                    return Err(Error::Malformed("directory mode"));
                }
                let mut listing = BTreeMap::new();
                if mode & 1 != 0 {
                    for p in &tree.dirs {
                        if p != "/" && parent(p) == path {
                            listing.insert(basename(p), (0u8, 0usize));
                        }
                    }
                }
                if mode & 2 != 0 {
                    for (p, data) in &tree.files {
                        if parent(p) == path {
                            listing.insert(basename(p), (1u8, data.len()));
                        }
                    }
                    if matches!(volume, Volume::Sd) && path == "/switch" {
                        listing.insert("soju-current.nro", (1, self.nro.len()));
                    }
                }
                let mut entries = Vec::new();
                for (name, (kind, size)) in listing {
                    let mut entry = [0; DIRECTORY_ENTRY];
                    entry[..name.len()].copy_from_slice(name.as_bytes());
                    entry[0x304] = kind;
                    if mode & 0x8000_0000 == 0 {
                        entry[0x308..].copy_from_slice(&(size as u64).to_le_bytes());
                    }
                    entries.extend(entry);
                }
                Ok(Reply::object(self.register(
                    Object::Directory {
                        entries,
                        position: 0,
                    },
                    ctx,
                )?))
            }
            _ => unreachable!(),
        }
    }
    fn file(&mut self, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        let (volume, path, mode) = match self.objects.get(&ctx.object) {
            Some(Object::SelfFile) => (None, String::new(), 1),
            Some(Object::File { volume, path, mode }) => (Some(*volume), path.clone(), *mode),
            _ => return Err(Error::Malformed("file object")),
        };
        let data = match volume {
            None => &self.nro,
            Some(v) => self
                .tree(v)?
                .files
                .get(&path)
                .ok_or(Error::Malformed("missing open file"))?,
        };
        match req.command_id {
            4 => Ok(Reply::data((data.len() as u64).to_le_bytes().to_vec())),
            2 => {
                if matches!(volume, Some(Volume::Sd)) {
                    self.flush_sd()?;
                }
                Ok(Reply::success())
            }
            3 => {
                if mode & 2 == 0 {
                    return Err(Error::Malformed("file not writable"));
                }
                let size =
                    usize::try_from(req.u64(0)?).map_err(|_| Error::Malformed("file size"))?;
                self.tree_mut(volume.ok_or(Error::Malformed("readonly file"))?)?
                    .resize(&path, size)?;
                Ok(Reply::success())
            }
            0 | 1 => {
                let read = req.command_id == 0;
                let option = req.u32(0)?;
                let offset = req.u64(8)?;
                let length = req.u64(16)?;
                if mode & if read { 1 } else { 2 } == 0
                    || option > u32::from(!read)
                    || offset > i64::MAX as u64
                    || length > MAX_FILE as u64
                {
                    return Err(Error::Malformed("file mode, option or range"));
                }
                let mut buffer = if read { req.output()? } else { req.input()? };
                if length > buffer.len {
                    return Err(Error::Malformed("file buffer length"));
                }
                let offset =
                    usize::try_from(offset).map_err(|_| Error::Malformed("file offset"))?;
                let length = length as usize;
                if read {
                    let start = offset.min(data.len());
                    let end = start + length.min(data.len() - start);
                    let bytes = data[start..end].to_vec();
                    Reply::data((bytes.len() as u64).to_le_bytes().to_vec()).write(buffer, bytes)
                } else {
                    buffer.len = length as u64;
                    let bytes = ctx.read_buffer(buffer, MAX_FILE)?;
                    let end = offset
                        .checked_add(length)
                        .ok_or(Error::Malformed("write range"))?;
                    if end > data.len() && mode & 4 == 0 {
                        return Err(Error::Malformed("write requires append mode"));
                    }
                    let volume = volume.ok_or(Error::Malformed("readonly file"))?;
                    let tree = self.tree_mut(volume)?;
                    if end > tree.files[&path].len() {
                        tree.resize(&path, end)?;
                    }
                    tree.files.get_mut(&path).expect("open file")[offset..end]
                        .copy_from_slice(&bytes);
                    if option == 1 && matches!(volume, Volume::Sd) {
                        self.flush_sd()?;
                    }
                    Ok(Reply::success())
                }
            }
            _ => unreachable!(),
        }
    }
    fn cursor(&mut self, req: &Request, ctx: &mut Context<'_>) -> Result<Reply, Error> {
        let (entries, position, size) = match self.objects.get_mut(&ctx.object) {
            Some(Object::Directory { entries, position }) => (entries, position, DIRECTORY_ENTRY),
            Some(Object::Reader { entries, position }) => (entries, position, 0x60),
            _ => return Err(Error::Malformed("cursor object")),
        };
        if req.command_id == 1 {
            return Ok(Reply::data(
                ((entries.len() / size) as u64).to_le_bytes().to_vec(),
            ));
        }
        let buffer = req.output()?;
        if buffer.len > MAX_FILE as u64 {
            return Err(Error::Malformed("directory buffer limit"));
        }
        let count = ((buffer.len as usize) / size).min((entries.len() - *position) / size);
        let end = *position + count * size;
        let bytes = entries[*position..end].to_vec();
        let reply = Reply::data((count as u64).to_le_bytes().to_vec()).write(buffer, bytes)?;
        *position = end;
        Ok(reply)
    }
}
fn parent(path: &str) -> &str {
    let p = path.rsplit_once('/').map_or("", |(p, _)| p);
    if p.is_empty() { "/" } else { p }
}
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
fn valid_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() < 0x301
        && !path.contains('\0')
        && (path == "/"
            || path[1..]
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".."))
}
fn guest_path(req: &Request, ctx: &Context<'_>) -> Result<String, Error> {
    let mut buffer = req.input()?;
    buffer.len = buffer.len.min(0x301);
    let raw = ctx.read_buffer(buffer, 0x301)?;
    let end = raw
        .iter()
        .position(|b| *b == 0)
        .ok_or(Error::Malformed("unterminated guest path"))?;
    let path =
        std::str::from_utf8(&raw[..end]).map_err(|_| Error::Malformed("guest path encoding"))?;
    if !path.starts_with('/') || path.split('/').any(|p| p == "." || p == "..") {
        return Err(Error::Malformed("guest path traversal"));
    }
    let path = format!(
        "/{}",
        path.split('/')
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    );
    if !valid_path(&path) {
        return Err(Error::Malformed("guest path"));
    }
    Ok(path)
}
#[cfg(test)]
mod tests;
