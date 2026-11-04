//! soju-switch-fs info <game.nsp|game.xci|file.nca> --keys <prod.keys> [--title-keys <title.keys>]
//!
//! Prints a JSON summary of the player's own game file: container type, files, and for
//! each NCA its content type, program id and sections. Key values are never printed.

use soju_switch_fs::keys::KeySet;
use soju_switch_fs::nca::Nca;
use soju_switch_fs::pfs0::{self, Entry, Storage, Sub};
use soju_switch_fs::xci::Xci;
use std::path::PathBuf;

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn describe_nca(s: &dyn Storage, keys: &KeySet) -> String {
    match Nca::open(s, keys) {
        Ok(n) => {
            let secs: Vec<String> = n
                .header
                .sections
                .iter()
                .map(|x| {
                    let files = if x.fs_type == soju_switch_fs::nca::FsType::PartitionFs {
                        match n.pfs0_files(x.index) {
                            Ok(f) => {
                                let mut extra = String::new();
                                if let (Some(meta), Ok(sec)) = (f.iter().find(|e| e.name == "main.npdm"), n.section(x.index)) {
                                    if let Ok(np) = sec.read_vec(meta.offset, meta.size.min(1 << 20) as usize).and_then(|b| soju_switch_fs::npdm::parse(&b)) {
                                        extra = format!(",\"npdm\":{{\"name\":{},\"program_id\":\"{:016x}\",\"is_64bit\":{}}}", json_str(&np.name), np.program_id, np.is_64bit);
                                    }
                                }
                                format!(",\"files\":[{}]{}", f.iter().map(|e| json_str(&e.name)).collect::<Vec<_>>().join(","), extra)
                            }
                            Err(e) => format!(",\"error\":{}", json_str(&e.to_string())),
                        }
                    } else {
                        match n.section(x.index).and_then(|s| soju_switch_fs::romfs::list(&s)) {
                            Ok(f) => format!(",\"romfs_files\":{}", f.len()),
                            Err(e) => format!(",\"error\":{}", json_str(&e.to_string())),
                        }
                    };
                    format!("{{\"index\":{},\"type\":\"{}\",\"size\":{}{}}}", x.index, if x.fs_type == soju_switch_fs::nca::FsType::PartitionFs { "pfs0" } else { "romfs" }, x.data_size, files)
                })
                .collect();
            format!(
                "{{\"content\":\"{}\",\"program_id\":\"{:016x}\",\"title_key\":{},\"sections\":[{}]}}",
                n.header.content_type.name(),
                n.header.program_id,
                n.header.rights_id != [0u8; 16],
                secs.join(",")
            )
        }
        Err(e) => format!("{{\"error\":{}}}", json_str(&e.to_string())),
    }
}

fn entries_json(s: &dyn Storage, entries: &[Entry], keys: &KeySet) -> String {
    entries
        .iter()
        .map(|e| {
            let nca = if e.name.ends_with(".nca") {
                let sub = Sub { inner: s, offset: e.offset, size: e.size };
                format!(",\"nca\":{}", describe_nca(&sub, keys))
            } else {
                String::new()
            };
            format!("{{\"name\":{},\"size\":{}{}}}", json_str(&e.name), e.size, nca)
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn run() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut file = None;
    let mut prod = None;
    let mut title = None;
    let mut it = args.iter();
    match it.next().map(String::as_str) {
        Some("info") => {}
        _ => return Err("usage: soju-switch-fs info <game.nsp|game.xci|file.nca> --keys <prod.keys> [--title-keys <title.keys>]".into()),
    }
    while let Some(a) = it.next() {
        match a.as_str() {
            "--keys" => prod = it.next().map(PathBuf::from),
            "--title-keys" => title = it.next().map(PathBuf::from),
            other if file.is_none() => file = Some(PathBuf::from(other)),
            _ => return Err("unexpected argument".into()),
        }
    }
    let file = file.ok_or("choose a game file")?;
    let prod = prod.ok_or("pass --keys with the prod.keys dumped from your own Switch")?;
    let keys = KeySet::load(&prod, title.as_deref()).map_err(|e| e.to_string())?;
    let f = std::fs::File::open(&file).map_err(|e| format!("cannot open the game file: {e}"))?;
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let body = match ext.as_str() {
        "nsp" => {
            let entries = pfs0::parse(&f).map_err(|e| e.to_string())?;
            format!("{{\"container\":\"nsp\",\"files\":[{}]}}", entries_json(&f, &entries, &keys))
        }
        "xci" => {
            let x = Xci::open(&f).map_err(|e| e.to_string())?;
            let secure: Vec<Entry> = x.partition("secure").map_err(|e| e.to_string())?.into_iter().map(|h| h.entry).collect();
            format!("{{\"container\":\"xci\",\"secure\":[{}]}}", entries_json(&f, &secure, &keys))
        }
        "nca" => format!("{{\"container\":\"nca\",\"nca\":{}}}", describe_nca(&f, &keys)),
        _ => return Err("choose an .nsp, .xci or .nca file".into()),
    };
    Ok(body)
}

fn main() {
    match run() {
        Ok(j) => println!("{j}"),
        Err(e) => {
            eprintln!("soju-switch-fs: {e}");
            std::process::exit(1);
        }
    }
}
