use super::*;
use crate::texture::{Binding, Handle, TextureType};
fn is_texture(x: &Instruction) -> bool {
    matches!(
        x.opcode,
        Opcode::TEX | Opcode::TEXS | Opcode::TLD | Opcode::TLDS | Opcode::TLD4 | Opcode::TXQ
    )
}
fn kind(x: &Instruction) -> Result<TextureType> {
    use TextureType::*;
    if matches!(x.opcode, Opcode::TEXS | Opcode::TLDS | Opcode::TXQ) {
        return Ok(D2);
    }
    let array = x.modified("array", None);
    let shadow = x.modified("dc", None);
    match (bits(x.raw, 29, 2), array, shadow) {
        (0, false, false) => Ok(D1),
        (0, true, false) => Ok(D1Array),
        (1, false, false) => Ok(D2),
        (1, true, false) => Ok(D2Array),
        (2, false, false) => Ok(D3),
        (3, false, false) => Ok(Cube),
        (3, true, false) => Ok(CubeArray),
        (1, false, true) => Ok(Shadow2D),
        (1, true, true) => Ok(Shadow2DArray),
        _ => Err(x.error("unsupported texture type")),
    }
}
fn bound(x: &Instruction) -> Option<u32> {
    if x.options.iter().any(|o| o.name == "b") {
        return None;
    }
    let index = match x.opcode {
        Opcode::TEXS | Opcode::TLDS => 4,
        Opcode::TXQ => 2,
        _ => 3,
    };
    match x.operands.get(index) {
        Some(Operand::Immediate(v)) => Some(*v),
        _ => None,
    }
}
pub(super) fn bindings(program: &Program, o: &Options) -> Result<Vec<Binding>> {
    if !o.texture_bindings.is_empty() {
        return crate::texture::normalize(&o.texture_bindings, o.texture_count);
    }
    let mut bindings = Vec::new();
    for x in reachability::instructions(program)
        .into_iter()
        .filter(|x| is_texture(x))
    {
        if let Some(handle) = bound(x) {
            bindings.push(Binding {
                handle: Handle::Bound(handle),
                texture_index: handle,
                sampler_index: handle,
                texture_type: kind(x)?,
            });
        }
    }
    crate::texture::normalize(&bindings, o.texture_count)
}
pub(super) fn declarations(program: &Program, o: &Options) -> Result<String> {
    let table = bindings(program, o)?;
    let kinds: BTreeSet<_> = reachability::instructions(program)
        .into_iter()
        .filter(|x| is_texture(x))
        .map(kind)
        .collect::<Result<_>>()?;
    let mut s = String::from("struct TextureTable {\n");
    for kind in kinds {
        let (ty, name, group) = kind.metal();
        writeln!(
            s,
            "array<{ty}, {}> {name} [[id({})]];",
            o.texture_count,
            group * o.texture_count
        )
        .unwrap();
    }
    writeln!(
        s,
        "array<sampler, {}> samplers [[id({})]];\n}};",
        o.texture_count,
        9 * o.texture_count
    )
    .unwrap();
    for bindless in [false, true] {
        writeln!(
            s,
            "inline uint2 soju_{}(uint handle) {{",
            if bindless { "bindless" } else { "bound" }
        )
        .unwrap();
        if o.texture_bindings.is_empty() {
            s.push_str("return uint2(handle, handle);\n");
        } else {
            s.push_str("switch (handle) {\n");
            for b in &table {
                let raw = match (bindless, b.handle) {
                    (false, Handle::Bound(raw)) | (true, Handle::Bindless(raw)) => raw,
                    _ => continue,
                };
                writeln!(
                    s,
                    "case {raw}u: return uint2({}u, {}u);",
                    b.texture_index, b.sampler_index
                )
                .unwrap();
            }
            s.push_str("default: return uint2(0xffffffffu);\n}\n");
        }
        s.push_str("}\n");
    }
    Ok(s)
}
impl Emitter<'_> {
    pub(super) fn texture(&self, x: &Instruction) -> Result<String> {
        use Opcode::*;
        use TextureType::*;
        if self.compute.is_some() {
            return Err(x.error("compute textures not implemented"));
        }
        let ty = kind(x)?;
        let (_, images, _) = ty.metal();
        let is_bound = bound(x);
        let scalar = matches!(x.opcode, TEXS | TLDS);
        let coord = register_operand(x, if scalar { 2 } else { 1 })?;
        let handle = if let Some(v) = is_bound {
            format!("{v}u")
        } else if x.opcode == TXQ {
            return Err(x.error("bindless TXQ operand ABI not documented"));
        } else {
            source(x, 2, false)?
        };
        if !self.options.texture_bindings.is_empty() {
            let table = bindings(self.program, self.options)?;
            let matching: Vec<_> = table
                .iter()
                .filter(|b| match (is_bound, b.handle) {
                    (Some(v), Handle::Bound(h)) => v == h,
                    (None, Handle::Bindless(_)) => true,
                    _ => false,
                })
                .collect();
            if matching.is_empty() || matching.iter().any(|b| b.texture_type != ty) {
                return Err(x.error("missing handle mapping or texture type mismatch"));
            }
        }
        let (coords, array, shadow) = match ty {
            D1 => (1, false, false),
            D2 => (2, false, false),
            D3 | Cube => (3, false, false),
            D1Array => (1, true, false),
            D2Array => (2, true, false),
            CubeArray => (3, true, false),
            Shadow2D => (2, false, true),
            Shadow2DArray => (2, true, true),
        };
        let total = coords + u8::from(array) + u8::from(shadow);
        if coord.checked_add(total - 1).is_none_or(|last| last == 255) {
            return Err(x.error("texture coordinate register overflow"));
        }
        if !scalar && x.opcode != TXQ && is_bound.is_some() && register_operand(x, 2)? != 255 {
            return Err(x.error("auxiliary texture operands unsupported"));
        }
        if scalar && !matches!(x.operands.first(), Some(Operand::Register(255))) {
            return Err(x.error("TEXS/TLDS currently support one/two-component destinations only"));
        }
        let lod_zero = match x.opcode {
            TLD | TLDS => true,
            TEXS => bits(x.raw, 53, 4) == 2,
            TEX => {
                if is_bound.is_some() {
                    bits(x.raw, 55, 3) == 1
                } else {
                    bits(x.raw, 37, 3) == 1
                }
            }
            _ => false,
        };
        if matches!(x.opcode, TEX | TEXS | TLD4)
            && !lod_zero
            && self.header.stage != Stage::Fragment
        {
            return Err(x.error("implicit texture LOD requires fragment stage"));
        }
        let mut c = Vec::new();
        for i in 0..coords {
            let r = if scalar && i == 1 {
                register_operand(x, 3)?
            } else {
                coord + u8::from(array) + i
            };
            c.push(reg(r));
        }
        let coordexpr = if matches!(x.opcode, TLD | TLDS) {
            if coords == 1 {
                c[0].clone()
            } else {
                format!("uint{coords}({})", c.join(", "))
            }
        } else {
            let c: Vec<_> = c.iter().map(|v| format!("as_type<float>({v})")).collect();
            if coords == 1 {
                c[0].clone()
            } else {
                format!("float{coords}({})", c.join(", "))
            }
        };
        let layer = if array {
            format!(", {}", reg(coord))
        } else {
            String::new()
        };
        let texture = format!("resources.{images}[handle]");
        let sample = format!("resources.samplers[pair.y], {coordexpr}{layer}");
        let expr=match x.opcode {
            TEX|TEXS=>if shadow {format!("float4({texture}.sample_compare({sample}, as_type<float>({}){}))",reg(coord+total-1),if lod_zero && !matches!(ty,D1|D1Array) {", level(0.0f)"} else {""})} else {format!("{texture}.sample({sample}{})",if lod_zero && !matches!(ty,D1|D1Array) {", level(0.0f)"} else {""})},
            TLD|TLDS=>{if matches!(ty,Cube|CubeArray|Shadow2D|Shadow2DArray) {return Err(x.error("texture read type unsupported"));}format!("{texture}.read({coordexpr}{layer}{})",if matches!(ty,D1|D1Array) {""} else {", 0u"})},
            TLD4=>{if ty!=D2 {return Err(x.error("gather currently requires float 2D"));}let component=["x","y","z","w"][bits(x.raw,if is_bound.is_some() {56} else {38},2) as usize];format!("{texture}.gather({sample}, int2(0), component::{component})")},
            TXQ=>format!("uint4({texture}.get_width({}), {texture}.get_height({}), 1u, {texture}.get_num_mip_levels())",reg(coord),reg(coord)),
            _=>return Err(x.error("unexpected texture opcode")),
        };
        let lookup = if is_bound.is_some() {
            "bound"
        } else {
            "bindless"
        };
        let mut s=format!("uint handle = {handle};\nuint2 pair = soju_{lookup}(handle);\nhandle = pair.x;\n{}4 texel = {{}};\nif (pair.x < {}u && pair.y < {}u) texel = {expr};\n",if x.opcode==TXQ {"uint"} else {"float"},self.options.texture_count,self.options.texture_count);
        let mask = if scalar {
            [1, 2, 4, 8, 3, 9, 10, 12][bits(x.raw, 50, 3) as usize]
        } else {
            bits(x.raw, 31, 4)
        };
        let mut dest = u32::from(register_operand(x, usize::from(scalar))?);
        if dest + mask.count_ones() > 256 {
            return Err(x.error("texture destination range overflow"));
        }
        for c in 0..4 {
            if mask & (1 << c) != 0 {
                if dest != 255 {
                    writeln!(
                        s,
                        "r[{dest}] = {}texel[{c}]{};",
                        if x.opcode == TXQ {
                            ""
                        } else {
                            "as_type<uint>("
                        },
                        if x.opcode == TXQ { "" } else { ")" }
                    )
                    .unwrap();
                }
                dest += 1;
            }
        }
        Ok(s)
    }
}
