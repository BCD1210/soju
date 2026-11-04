//! Read-only audit of embedded homebrew DKSH modules; never writes shader bytes.
use soju_switch_shader::{compute, decoder, header, msl, translate};
use std::{collections::BTreeMap, error::Error, fs, path::Path};

fn word(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn audit(path: &Path, totals: &mut [usize; 4]) -> Result<(), Box<dyn Error>> {
    if fs::metadata(path)?.len() > 128 * 1024 * 1024 {
        return Err("input exceeds 128 MiB audit limit".into());
    }
    let bytes = fs::read(path)?;
    let name = path
        .file_name()
        .ok_or("missing filename")?
        .to_string_lossy();
    for (offset, magic) in bytes.windows(4).enumerate() {
        if magic != b"DKSH" {
            continue;
        }
        let data = &bytes[offset..];
        if word(data, 4) != Some(24) {
            continue;
        }
        let control = usize::try_from(word(data, 8).ok_or("truncated DKSH")?)?;
        let code_size = usize::try_from(word(data, 12).ok_or("truncated DKSH")?)?;
        let programs = usize::try_from(word(data, 16).ok_or("truncated DKSH")?)?;
        let count = usize::try_from(word(data, 20).ok_or("truncated DKSH")?)?;
        if control < 24
            || !control.is_multiple_of(256)
            || !code_size.is_multiple_of(256)
            || count == 0
            || count > 256
            || programs < 24
            || programs + count * 64 > control
        {
            return Err(format!("invalid DKSH table in {name} at {offset}").into());
        }
        let code = data
            .get(control..control.checked_add(code_size).ok_or("DKSH size overflow")?)
            .ok_or("DKSH code exceeds input")?;
        for program in 0..count {
            totals[0] += 1;
            let meta = &data[programs + program * 64..programs + (program + 1) * 64];
            let kind = word(meta, 0).ok_or("program type")?;
            let entry = usize::try_from(word(meta, 4).ok_or("program entry")?)?;
            let constant_start = usize::try_from(word(meta, 12).ok_or("constant offset")?)?;
            let end = if constant_start > entry {
                constant_start
            } else {
                code.len()
            };
            let shader = code.get(entry..end).ok_or("program exceeds code")?;
            let header_bytes = if kind == 5 { 0 } else { header::HEADER_SIZE };
            let instructions = shader.get(header_bytes..).ok_or("truncated SPH")?;
            // Audit complete executable spans through the last unconditional EXIT;
            // alignment padding after EXIT is not instruction coverage.
            let extent = instructions
                .chunks_exact(8)
                .enumerate()
                .filter_map(|(slot, chunk)| {
                    let raw = u64::from_le_bytes(chunk.try_into().ok()?);
                    (slot % 4 != 0 && raw & 0xfff0_0000_000f_001f == 0xe300_0000_0007_000f)
                        .then_some((slot + 1) * 8)
                })
                .next_back();
            let Some(extent) = extent else {
                println!("{name}@{offset}:{program}|type={kind}|no-unconditional-exit");
                continue;
            };
            let instructions = &instructions[..extent];
            let mut opcodes = BTreeMap::new();
            let mut unknown = BTreeMap::new();
            for (slot, chunk) in instructions.chunks_exact(8).enumerate() {
                if slot % 4 == 0 {
                    continue;
                }
                totals[1] += 1;
                let raw = u64::from_le_bytes(chunk.try_into()?);
                match decoder::decode_instruction(u32::try_from(slot * 8)?, raw) {
                    Ok(decoded) => {
                        totals[2] += 1;
                        *opcodes.entry(decoded.opcode).or_insert(0usize) += 1;
                    }
                    Err(_) => {
                        *unknown.entry(raw >> 48).or_insert(0usize) += 1;
                    }
                }
            }
            let options = msl::Options {
                constant_buffer_bytes: [65536; 32],
                global_memory: Some(msl::GlobalMemory {
                    buffer: 29,
                    base_address: 0,
                    bytes: 1024 * 1024,
                }),
                ..msl::Options::default()
            };
            let result = if kind == 5 {
                let mut raw = [0u32; 64];
                raw[18] = 0x17 | word(meta, 24).ok_or("local x")? << 16;
                raw[19] = word(meta, 28).ok_or("local y")? | word(meta, 32).ok_or("local z")? << 16;
                raw[17] = word(meta, 36).ok_or("shared bytes")?;
                raw[45] = word(meta, 40).ok_or("local positive")?;
                raw[46] = word(meta, 44).ok_or("local negative")?;
                let qmd: Vec<_> = raw.into_iter().flat_map(u32::to_le_bytes).collect();
                compute::Launch::parse(&qmd)
                    .and_then(|launch| compute::translate(instructions, &launch, &options))
            } else {
                translate(&shader[..header_bytes + extent], &options)
            };
            let status = match result {
                Ok(_) => {
                    totals[3] += 1;
                    "translated".to_owned()
                }
                Err(error) => error.to_string(),
            };
            println!("{name}@{offset}:{program}|type={kind}|{status}|opcodes={opcodes:?}|unknown-prefixes={unknown:x?}");
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args_os()
        .nth(1)
        .ok_or("usage: corpus_audit <homebrew-directory>")?;
    let mut paths = fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    let mut totals = [0; 4];
    for path in paths {
        if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("nro" | "dksh")
        ) {
            audit(&path, &mut totals)?;
        }
    }
    println!(
        "total|programs={}|instructions={}|decoded={}|translated={}",
        totals[0], totals[1], totals[2], totals[3]
    );
    Ok(())
}
