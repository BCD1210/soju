use super::*;
use std::{
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::Command,
};

pub(crate) struct Sandbox(pub PathBuf);
impl Sandbox {
    pub fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/fs-tests");
        let path = root.join(format!(
            "{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn attr(id: u8) -> [u8; 64] {
    let mut a = [0; 64];
    a[0] = id;
    a[8] = 1;
    a[32] = 1;
    a
}
fn saves() -> Saves {
    let mut tree = Tree::empty();
    tree.dirs.insert("/slot".into());
    tree.files.insert("/slot/data".into(), vec![0, 1, 128, 255]);
    BTreeMap::from([(
        attr(1),
        Save {
            tree,
            size: 1024,
            id: 1,
        },
    )])
}
const HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

#[test]
fn python_bidirectional_snapshot_compatibility() {
    let sandbox = Sandbox::new();
    let home = sandbox.0.join("home");
    let store = Store::new(&home, HASH).unwrap();
    store.save(&saves()).unwrap();
    drop(store);
    let script = r#"
import sys
sys.path.insert(0, sys.argv[1])
from save_store import SaveStore
store = SaveStore(sys.argv[2], sys.argv[3])
saves = store.load()
assert len(saves) == 1
item = next(iter(saves.values()))
assert item['files'].read('/slot/data', 0, 4) == bytes([0, 1, 128, 255])
item['files'].write('/slot/data', 1, b'PY')
store.save(saves)
store.close()
print('python-read-rust-write-and-python-write: PASS')
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
        .arg(&home)
        .arg(HASH)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let store = Store::new(&home, HASH).unwrap();
    let restored = store.load_saves().unwrap();
    assert_eq!(
        restored[&attr(1)].tree.files["/slot/data"],
        [0, b'P', b'Y', 255]
    );
}
#[test]
fn snapshot_lock_nofollow_permissions_and_regular_files() {
    let sandbox = Sandbox::new();
    let home = sandbox.0.join("home");
    let store = Store::new(&home, HASH).unwrap();
    assert!(Store::new(&home, HASH).is_err());
    store.save(&saves()).unwrap();
    let directory = home.join("saves").join(HASH);
    assert_eq!(
        std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(directory.join("state.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    std::fs::remove_file(directory.join("state.json")).unwrap();
    symlink(sandbox.0.join("outside"), directory.join("state.json")).unwrap();
    assert!(store.load_saves().is_err());
    assert!(store.save(&saves()).is_err());
    assert!(!sandbox.0.join("outside").exists());
    drop(store);
    std::fs::remove_file(directory.join(".lock")).unwrap();
    symlink(sandbox.0.join("outside"), directory.join(".lock")).unwrap();
    assert!(Store::new(&home, HASH).is_err());
    let linked = sandbox.0.join("linked");
    symlink(&home, &linked).unwrap();
    assert!(Store::new(&linked, HASH).is_err());
    let home2 = sandbox.0.join("home2");
    std::fs::create_dir(&home2).unwrap();
    symlink(&directory, home2.join("saves")).unwrap();
    assert!(Store::new(&home2, HASH).is_err());
    let home3 = sandbox.0.join("home3");
    let store3 = Store::new(&home3, HASH).unwrap();
    let fifo = cstr(
        home3
            .join("saves")
            .join(HASH)
            .join("state.json")
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert!(store3.load_saves().is_err());
    assert!(store3.save(&saves()).is_err());
}
#[test]
fn atomic_failure_preserves_snapshot_and_cleans_temporary_file() {
    let sandbox = Sandbox::new();
    let store = Store::new(&sandbox.0, HASH).unwrap();
    store.save(&saves()).unwrap();
    let before = store.read("state.json").unwrap().unwrap();
    store.fail_before_rename.set(true);
    assert!(store.save(&Saves::new()).is_err());
    assert_eq!(store.read("state.json").unwrap().unwrap(), before);
    let names = std::fs::read_dir(sandbox.0.join("saves").join(HASH))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect::<Vec<_>>();
    assert!(
        !names
            .iter()
            .any(|n| n.to_string_lossy().starts_with(".soju-save-"))
    );
    store.fail_before_rename.set(false);
    store.save(&Saves::new()).unwrap();
    assert!(store.load_saves().unwrap().is_empty());
}
#[test]
fn duplicate_json_metadata_paths_base64_and_limits_are_rejected() {
    assert!(decode_saves(br#"{"version":1,"version":1,"saves":[]}"#).is_err());
    assert!(parse::<serde_json::Value>(br#"{"a":{"x":1,"x":2}}"#).is_err());
    let base =
        serde_json::from_slice::<serde_json::Value>(&encode_saves(&saves()).unwrap()).unwrap();
    for (key, value) in [
        ("attribute", "00".into()),
        ("space", 2.into()),
        ("size", (-1).into()),
        ("id", 0.into()),
        ("dirs", serde_json::json!(["/", "/a/b"])),
        ("files", serde_json::json!({"/../escape":"AA=="})),
        ("files", serde_json::json!({"/slot/data":"AA==garbage"})),
    ] {
        let mut bad = base.clone();
        bad["saves"][0][key] = value;
        assert!(
            decode_saves(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{key}"
        );
    }
    let mut too_many = Saves::new();
    for id in 1..=17 {
        let mut s = saves().remove(&attr(1)).unwrap();
        s.id = id as u64;
        too_many.insert(attr(id), s);
    }
    assert!(encode_saves(&too_many).is_err());
    let mut tree = Tree::empty();
    assert!(tree.create("/huge".into(), MAX_FILE + 1).is_err());
    assert!(tree.files.is_empty());
    for id in 0..4 {
        tree.create(format!("/{id}"), MAX_FILE).unwrap();
    }
    assert!(tree.create("/overflow".into(), 1).is_err());
    tree.resize("/0", 1).unwrap();
    assert!(tree.resize("/0", MAX_FILE + 1).is_err());
    let mut tree = Tree::empty();
    for id in 0..128 {
        tree.create(format!("/{id}"), 0).unwrap();
    }
    assert!(tree.create("/129".into(), 0).is_err());
    let sandbox = Sandbox::new();
    let store = Store::new(&sandbox.0, HASH).unwrap();
    let file = open_at(
        store.dir.as_raw_fd(),
        "state.json",
        libc::O_WRONLY | libc::O_CREAT,
        0o600,
    )
    .unwrap();
    file.set_len(MAX_SNAPSHOT + 1).unwrap();
    assert!(store.load_saves().is_err());
}

#[test]
fn snapshot_directory_file_counts_and_duplicate_identities_are_bounded() {
    let raw = encode_saves(&saves()).unwrap();
    let base: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let mut bad = base.clone();
    bad["version"] = 2.into();
    assert!(decode_saves(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut bad = base.clone();
    bad["saves"]
        .as_array_mut()
        .unwrap()
        .push(base["saves"][0].clone());
    assert!(decode_saves(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut bad = base.clone();
    bad["saves"][0]["dirs"] = serde_json::json!(["/", "/slot", "/slot"]);
    assert!(decode_saves(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut dirs = vec!["/".to_owned()];
    dirs.extend((0..256).map(|n| format!("/d{n}")));
    assert!(decode_tree(dirs, BTreeMap::new()).is_err());
    assert!(
        decode_tree(
            vec!["/".into(), format!("/{}", "x".repeat(768))],
            BTreeMap::new()
        )
        .is_err()
    );
    let files = (0..129)
        .map(|n| (format!("/f{n}"), String::new()))
        .collect();
    assert!(decode_tree(vec!["/".into()], files).is_err());
    let mut bad = saves();
    bad.get_mut(&attr(1)).unwrap().size = 3;
    assert!(encode_saves(&bad).is_err());
}
