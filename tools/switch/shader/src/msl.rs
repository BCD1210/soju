//! Metal source backend. A decoded opcode is not a promise of semantic support.
use crate::decoder::{bits, Instruction, Opcode, Operand, Predicate, Space};
use crate::header::{Header, Interpolation, Stage};
use crate::ir::{Node, Program};
use crate::{Error, Result};
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Debug, Clone)]
pub struct Options {
    /// Metal argument buffer binding. Must not collide with a used cbuf slot.
    pub texture_buffer: u8,
    /// Host provides a normalized descriptor table; see README for the ABI.
    pub texture_count: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            texture_buffer: 30,
            texture_count: 128,
        }
    }
}
#[derive(Debug, Clone)]
pub struct MetalShader {
    pub source: String,
    pub constant_buffers: Vec<u8>,
    pub uses_texture_table: bool,
    pub entry_point: &'static str,
}
fn reg(r: u8) -> String {
    if r == 255 {
        "0u".into()
    } else {
        format!("r[{r}]")
    }
}
fn pred(p: Predicate) -> String {
    let v = if p.index == 7 {
        "true".into()
    } else {
        format!("p[{}]", p.index)
    };
    if p.inverted {
        format!("(!{v})")
    } else {
        v
    }
}
fn predicate_operand(x: &Instruction, i: usize) -> Result<String> {
    match x.operands.get(i) {
        Some(Operand::Predicate(p)) => Ok(pred(*p)),
        _ => Err(x.error("expected predicate operand")),
    }
}
fn register_operand(x: &Instruction, i: usize) -> Result<u8> {
    match x.operands.get(i) {
        Some(Operand::Register(r)) => Ok(*r),
        _ => Err(x.error("expected register operand")),
    }
}
fn source(x: &Instruction, i: usize, float: bool) -> Result<String> {
    let raw = match x.operands.get(i) {
        Some(Operand::Register(r)) => reg(*r),
        Some(Operand::Immediate(v) | Operand::FloatBits(v)) => format!("0x{v:08x}u"),
        Some(Operand::Constant { slot, base, offset }) => {
            if *base != 255 || *offset < 0 || offset % 4 != 0 {
                return Err(x.error("only aligned direct cbuf operands are supported"));
            }
            format!("cb{slot}[{}]", offset / 4)
        }
        _ => return Err(x.error("unsupported source operand")),
    };
    let mut value = if float {
        format!("as_type<float>({raw})")
    } else {
        raw
    };
    if x.modified("abs", Some(i)) {
        value = if float {
            format!("abs({value})")
        } else {
            format!("uint(abs(as_type<int>({value})))")
        };
    }
    if x.modified("neg", Some(i)) {
        value = if float {
            format!("(-{value})")
        } else {
            format!("(0u - {value})")
        };
    }
    if x.modified("inv", Some(i)) {
        value = format!("(~{value})");
    }
    Ok(value)
}
fn assign(x: &Instruction, operand: usize, value: &str, float: bool) -> Result<String> {
    let r = register_operand(x, operand)?;
    let mut value = value.to_string();
    if float {
        if x.modified("sat", None) {
            value = format!("soju_sat({value})");
        }
        value = format!("as_type<uint>({value})");
    }
    Ok(if r == 255 {
        format!("(void)({value});\n")
    } else {
        format!("r[{r}] = {value};\n")
    })
}
fn bool_op(x: &Instruction, a: &str, b: &str, mode: u32) -> Result<String> {
    let op = match mode {
        0 => "&&",
        1 => "||",
        2 => "!=",
        _ => return Err(x.error("reserved boolean operation")),
    };
    Ok(format!("({a} {op} {b})"))
}
fn compare(a: &str, b: &str, mode: u32, float: bool) -> String {
    let unordered = if float {
        format!("(isnan({a}) || isnan({b}))")
    } else {
        "false".into()
    };
    let base = match mode & 7 {
        0 => "false".into(),
        1 => format!("({a} < {b})"),
        2 => format!("({a} == {b})"),
        3 => format!("({a} <= {b})"),
        4 => format!("({a} > {b})"),
        5 => format!("({a} != {b})"),
        6 => format!("({a} >= {b})"),
        _ => "true".into(),
    };
    if !float {
        base
    } else if mode & 8 != 0 {
        format!("({unordered} || {base})")
    } else {
        format!("(!{unordered} && {base})")
    }
}

pub fn generate(header: &Header, program: &Program, options: &Options) -> Result<MetalShader> {
    if !matches!(header.stage, Stage::Vertex | Stage::Fragment) {
        return Err(Error(
            "only standalone Vertex B and Fragment stages have a Metal backend".into(),
        ));
    }
    if header.depth_output {
        return Err(Error(
            "fragment depth register ABI is not implemented".into(),
        ));
    }
    if header.stage == Stage::Fragment && (header.raw[14] != 0 || header.raw[17] != 0) {
        return Err(Error("special fragment inputs are not implemented".into()));
    }
    if options.texture_count == 0 || options.texture_count > 4096 || options.texture_buffer > 30 {
        return Err(Error("invalid Metal argument table configuration".into()));
    }
    let mut cb = BTreeSet::new();
    let mut textures = false;
    for x in &program.instructions {
        for o in &x.operands {
            if let Operand::Constant { slot, .. } = o {
                cb.insert(*slot);
            }
        }
        textures |= matches!(
            x.opcode,
            Opcode::TEX | Opcode::TEXS | Opcode::TLD | Opcode::TLDS | Opcode::TLD4 | Opcode::TXQ
        );
    }
    if cb
        .iter()
        .any(|s| *s > 30 || (textures && *s == options.texture_buffer))
    {
        return Err(Error(
            "cbuf slot exceeds Metal binding range or collides with texture table".into(),
        ));
    }
    let vertex = header.stage == Stage::Vertex;
    let mut s = String::from(
        "#include <metal_stdlib>\nusing namespace metal;\n#pragma clang fp contract(off)\n",
    );
    s.push_str(HELPERS);
    s.push_str("struct StageIn {\n");
    if vertex {
        let slots: BTreeSet<_> = header.inputs.iter().map(|a| a.address / 16).collect();
        for slot in slots {
            if !(8..39).contains(&slot) {
                return Err(Error(format!(
                    "unsupported vertex input address 0x{:x}",
                    slot * 16
                )));
            }
            writeln!(s, "float4 a{} [[attribute({})]];", slot - 8, slot - 8).unwrap();
        }
    } else {
        if header
            .inputs
            .iter()
            .any(|a| a.address >= 0x70 && a.address < 0x80)
        {
            s.push_str("float4 position [[position]];\n");
        }
        for a in &header.inputs {
            if (0x70..0x80).contains(&a.address) {
                continue;
            }
            if !(0x80..0x280).contains(&a.address) {
                return Err(Error(format!(
                    "unsupported fragment input 0x{:x}",
                    a.address
                )));
            }
            let mode = match a.interpolation {
                Some(Interpolation::Flat) => "flat",
                Some(Interpolation::Linear) => "center_no_perspective",
                _ => "center_perspective",
            };
            writeln!(
                s,
                "float v{} [[user(locn{}), {mode}]];",
                a.address / 4,
                a.address / 4
            )
            .unwrap();
        }
    }
    s.push_str("};\nstruct StageOut {\n");
    if vertex {
        s.push_str("float4 position [[position]];\n");
        for a in &header.outputs {
            if (0x70..0x80).contains(&a.address) {
                continue;
            }
            if !(0x80..0x280).contains(&a.address) {
                return Err(Error(format!(
                    "unsupported vertex output 0x{:x}",
                    a.address
                )));
            }
            writeln!(
                s,
                "float v{} [[user(locn{})]];",
                a.address / 4,
                a.address / 4
            )
            .unwrap();
        }
    } else {
        for (i, m) in header.color_masks.iter().enumerate() {
            if *m != 0 {
                writeln!(s, "float4 color{i} [[color({i})]];").unwrap();
            }
        }
    }
    s.push_str("};\n");
    if textures {
        writeln!(s,"struct TextureTable {{\narray<texture2d<float>, {}> images [[id(0)]];\narray<sampler, {}> samplers [[id({})]];\n}};",options.texture_count,options.texture_count,options.texture_count).unwrap();
    }
    let entry = if vertex {
        "soju_vertex"
    } else {
        "soju_fragment"
    };
    write!(
        s,
        "{} StageOut {entry}(StageIn in [[stage_in]]",
        if vertex { "vertex" } else { "fragment" }
    )
    .unwrap();
    for slot in &cb {
        write!(s, ", constant uint* cb{slot} [[buffer({slot})]]").unwrap();
    }
    if textures {
        write!(
            s,
            ", constant TextureTable& resources [[buffer({})]]",
            options.texture_buffer
        )
        .unwrap();
    }
    s.push_str(") {\nuint r[255] = {};\nbool p[7] = {};\nStageOut out = {};\n");
    let local = header
        .local_positive
        .checked_add(header.local_negative)
        .ok_or_else(|| Error("local allocation overflow".into()))?;
    if local > 64 * 1024 {
        return Err(Error(
            "local memory allocation exceeds backend limit 64 KiB".into(),
        ));
    }
    if local > 0 {
        writeln!(s, "uint local_mem[{}] = {{}};", local.div_ceil(4)).unwrap();
    }
    let mut emitter = Emitter {
        header,
        program,
        options,
        local_bytes: local,
    };
    emitter.nodes(&program.body, &mut s)?;
    s.push_str(&emitter.finish());
    s.push_str("}\n");
    Ok(MetalShader {
        source: s,
        constant_buffers: cb.into_iter().collect(),
        uses_texture_table: textures,
        entry_point: entry,
    })
}
struct Emitter<'a> {
    header: &'a Header,
    program: &'a Program,
    options: &'a Options,
    local_bytes: u32,
}
impl Emitter<'_> {
    fn finish(&self) -> String {
        let mut s = String::new();
        if self.header.stage == Stage::Fragment {
            let mut r = 0;
            for (i, mask) in self.header.color_masks.iter().enumerate() {
                for c in 0..4 {
                    if mask & (1 << c) != 0 {
                        writeln!(s, "out.color{i}[{c}] = as_type<float>(r[{r}]);").unwrap();
                        r += 1;
                    }
                }
            }
        }
        s.push_str("return out;\n");
        s
    }
    fn nodes(&mut self, nodes: &[Node], s: &mut String) -> Result<()> {
        for n in nodes {
            match n {
                Node::If { condition, body } => {
                    writeln!(s, "if ({}) {{", pred(*condition)).unwrap();
                    self.nodes(body, s)?;
                    s.push_str("}\n");
                }
                Node::Loop { body } => {
                    s.push_str("while (true) {\n");
                    self.nodes(body, s)?;
                    s.push_str("}\n");
                }
                Node::Break(p) => writeln!(s, "if ({}) {{ break; }}", pred(*p)).unwrap(),
                Node::Continue(p) => writeln!(s, "if ({}) {{ continue; }}", pred(*p)).unwrap(),
                Node::Instruction(i) => {
                    let x = &self.program.instructions[*i];
                    if x.predicate.index == 7 && x.predicate.inverted {
                        continue;
                    }
                    let line = self.instruction(x)?;
                    writeln!(
                        s,
                        "// 0x{:x} {:?}\nif ({}) {{\n{line}}}",
                        x.address,
                        x.opcode,
                        pred(x.predicate)
                    )
                    .unwrap();
                }
            }
        }
        Ok(())
    }
    fn instruction(&self, x: &Instruction) -> Result<String> {
        use Opcode::*;
        self.validate(x)?;
        let a = |i| source(x, i, false);
        let f = |i| source(x, i, true);
        let dst = |v: String| assign(x, 0, &v, false);
        let fdst = |v: String| assign(x, 0, &v, true);
        match x.opcode {
            NOP | DEPBAR => Ok(String::new()),
            EXIT => Ok(self.finish()),
            KIL => {
                if self.header.stage == Stage::Fragment {
                    Ok(format!("discard_fragment();\n{}", self.finish()))
                } else {
                    Err(x.error("KIL requires fragment stage"))
                }
            }
            MOV => dst(a(1)?),
            FADD | FMUL | FFMA => {
                let ftz = x.modified("ftz", None)
                    || x.options.iter().any(|o| {
                        (o.name == "5c68_0" && o.bits == 1 << 44)
                            || (o.name == "5980_0" && o.bits == 1 << 53)
                    });
                let value = |i| -> Result<String> {
                    let v = f(i)?;
                    Ok(if ftz { format!("soju_ftz({v})") } else { v })
                };
                let v = match x.opcode {
                    FADD => format!("({} + {})", value(1)?, value(2)?),
                    FMUL => format!("({} * {})", value(1)?, value(2)?),
                    _ => format!("fma({}, {}, {})", value(1)?, value(2)?, value(3)?),
                };
                fdst(if ftz { format!("soju_ftz({v})") } else { v })
            }
            FMNMX => fdst(format!(
                "({} ? fmin({}, {}) : fmax({}, {}))",
                predicate_operand(x, 3)?,
                f(1)?,
                f(2)?,
                f(1)?,
                f(2)?
            )),
            IADD => dst(format!("({} + {})", a(1)?, a(2)?)),
            IADD3 => dst(format!("({} + {} + {})", a(1)?, a(2)?, a(3)?)),
            ISCADD => dst(format!("(({} << {}) + {})", a(1)?, a(3)?, a(2)?)),
            IMAD => dst(format!("({} * {} + {})", a(1)?, a(2)?, a(3)?)),
            XMAD => dst(format!(
                "(({} & 0xffffu) * ({} & 0xffffu) + {})",
                a(1)?,
                a(2)?,
                a(3)?
            )),
            SEL => dst(format!(
                "({} ? {} : {})",
                predicate_operand(x, 3)?,
                a(1)?,
                a(2)?
            )),
            IMNMX => {
                let signed = bits(x.raw, 48, 1) != 0;
                let cast = if signed { "as_type<int>" } else { "uint" };
                dst(format!(
                    "uint({} ? min({cast}({}), {cast}({})) : max({cast}({}), {cast}({})))",
                    predicate_operand(x, 3)?,
                    a(1)?,
                    a(2)?,
                    a(1)?,
                    a(2)?
                ))
            }
            FSET | FSETP | ISET | ISETP => {
                let float = matches!(x.opcode, FSET | FSETP);
                let setp = matches!(x.opcode, FSETP | ISETP);
                let first = if setp { 2 } else { 1 };
                let mut left = source(x, first, float)?;
                let mut right = source(x, first + 1, float)?;
                if !float && bits(x.raw, 48, 1) != 0 {
                    left = format!("as_type<int>({left})");
                    right = format!("as_type<int>({right})");
                }
                let c = compare(
                    &left,
                    &right,
                    bits(
                        x.raw,
                        if float { 48 } else { 49 },
                        if float { 4 } else { 3 },
                    ),
                    float,
                );
                let p = predicate_operand(x, first + 2)?;
                let op = bits(x.raw, 45, 2);
                let yes = bool_op(x, "test", &p, op)?;
                if setp {
                    let no = bool_op(x, "(!test)", &p, op)?;
                    let mut s = format!("bool test = {c};\nbool yes = {yes};\nbool no = {no};\n");
                    for (i, v) in [(0, "yes"), (1, "no")] {
                        if let Operand::Predicate(p) = x.operands[i] {
                            if p.index != 7 {
                                writeln!(s, "p[{}] = {v};", p.index).unwrap();
                            }
                        }
                    }
                    Ok(s)
                } else {
                    let bf = bits(x.raw, if float { 52 } else { 44 }, 1) != 0;
                    Ok(format!(
                        "bool test = {c};\n{}",
                        dst(format!(
                            "({yes} ? {} : 0u)",
                            if bf { "0x3f800000u" } else { "0xffffffffu" }
                        ))?
                    ))
                }
            }
            PSETP => {
                let first = bool_op(
                    x,
                    &predicate_operand(x, 2)?,
                    &predicate_operand(x, 3)?,
                    bits(x.raw, 24, 2),
                )?;
                let c = predicate_operand(x, 4)?;
                let op = bits(x.raw, 45, 2);
                let mut s = format!(
                    "bool test = {first};\nbool yes = {};\nbool no = {};\n",
                    bool_op(x, "test", &c, op)?,
                    bool_op(x, "(!test)", &c, op)?
                );
                for (i, v) in [(0, "yes"), (1, "no")] {
                    if let Operand::Predicate(p) = x.operands[i] {
                        if p.index != 7 {
                            writeln!(s, "p[{}] = {v};", p.index).unwrap();
                        }
                    }
                }
                Ok(s)
            }
            LOP => {
                let op = bits(x.raw, 41, 2);
                let b = a(3)?;
                let v = match op {
                    0 => format!("({} & {b})", a(2)?),
                    1 => format!("({} | {b})", a(2)?),
                    2 => format!("({} ^ {b})", a(2)?),
                    _ => b,
                };
                assign(x, 1, &v, false)
            }
            LOP3 => assign(
                x,
                1,
                &format!("soju_lop3({}, {}, {}, {})", a(2)?, a(3)?, a(4)?, a(5)?),
                false,
            ),
            SHL | SHR => {
                let right = x.opcode == SHR;
                let signed = right && bits(x.raw, 48, 1) != 0;
                let value = if signed {
                    format!("as_type<int>({})", a(1)?)
                } else {
                    a(1)?
                };
                let amount = if x.modified("w", None) {
                    format!("({} & 31u)", a(2)?)
                } else {
                    a(2)?
                };
                dst(format!(
                    "({amount} >= 32u ? {} : uint({value} {} {amount}))",
                    if signed {
                        format!("uint({value} >> 31u)")
                    } else {
                        "0u".into()
                    },
                    if right { ">>" } else { "<<" }
                ))
            }
            POPC => dst(format!("popcount({})", a(1)?)),
            FLO => {
                let v = a(1)?;
                let v = if bits(x.raw, 48, 1) != 0 {
                    format!("(as_type<int>({v}) < 0 ? ~{v} : {v})")
                } else {
                    v
                };
                dst(format!("({v} == 0u ? 0xffffffffu : 31u - clz({v}))"))
            }
            BFE => dst(format!(
                "soju_bfe({}, {}, {})",
                a(1)?,
                a(2)?,
                bits(x.raw, 48, 1) != 0
            )),
            BFI => dst(format!("soju_bfi({}, {}, {})", a(1)?, a(2)?, a(3)?)),
            MUFU => {
                let v = f(1)?;
                let expr = match bits(x.raw, 20, 4) {
                    2 => format!("exp2({v})"),
                    3 => format!("log2({v})"),
                    4 => format!("(1.0f / {v})"),
                    5 => format!("rsqrt({v})"),
                    8 => format!("sqrt({v})"),
                    _ => {
                        return Err(
                            x.error("MUFU function requires range reduction or FP64 emulation")
                        )
                    }
                };
                fdst(expr)
            }
            I2F => fdst(format!(
                "float({}({}))",
                if bits(x.raw, 13, 1) != 0 {
                    "as_type<int>"
                } else {
                    "uint"
                },
                a(1)?
            )),
            F2I => dst(format!(
                "soju_f2i({}, {}, {})",
                f(1)?,
                bits(x.raw, 12, 1) != 0,
                bits(x.raw, 39, 2)
            )),
            F2F => {
                let v = f(1)?;
                let mode = bits(x.raw, 39, 4) & 11;
                fdst(match mode {
                    0 | 3 => v,
                    8 => format!("rint({v})"),
                    9 => format!("floor({v})"),
                    10 => format!("ceil({v})"),
                    11 => format!("trunc({v})"),
                    _ => return Err(x.error("F2F rounding mode unsupported")),
                })
            }
            I2I => dst(a(1)?),
            LDC => {
                let Some(Operand::Constant { slot, base, offset }) = x.operands.get(1) else {
                    return Err(x.error("invalid LDC operand"));
                };
                if *base != 255 || offset % 4 != 0 || *offset < 0 {
                    return Err(x.error("indexed/unaligned LDC not supported"));
                }
                dst(format!("cb{slot}[{}]", offset / 4))
            }
            ALD | AST | IPA => self.attribute(x),
            LDL | STL => self.local(x),
            TEX => self.texture(x),
            _ => Err(x.error("opcode decoded but Metal lowering is not implemented")),
        }
    }
    fn validate(&self, x: &Instruction) -> Result<()> {
        use Opcode::*;
        // Every decoded modifier must be explicitly consumed or safely scheduling-only.
        for m in &x.modifiers {
            let allowed = match m.name {
                "neg" => matches!(
                    x.opcode,
                    FADD | FMUL
                        | FFMA
                        | FMNMX
                        | FSET
                        | FSETP
                        | MUFU
                        | F2F
                        | F2I
                        | IADD
                        | IADD3
                        | ISCADD
                ),
                "abs" => matches!(x.opcode, FADD | FMNMX | FSET | FSETP | MUFU | F2F | F2I),
                "inv" => matches!(x.opcode, LOP | POPC | FLO),
                "sat" => matches!(x.opcode, FADD | FMUL | FFMA | MUFU | F2F | IPA),
                "ftz" => matches!(x.opcode, FADD),
                "bf" => matches!(x.opcode, FSET | ISET),
                "w" => matches!(x.opcode, SHL | SHR),
                "nodep" => x.opcode == TEX,
                _ => false,
            };
            if !allowed {
                return Err(x.error(format!("unsupported modifier {}", m.name)));
            }
        }
        for o in &x.options {
            let allowed = match o.name {
                "5cb8_2" | "5980_1" | "5c68_1" | "5cc0_0" | "5cc0_1" | "5cc0_2" | "5cc0_3"
                | "5b00_0" | "5b00_1" | "5100_0" | "5c20_0" | "5c40_1" | "5cf0_1" | "5be0_0"
                | "ef90_0" | "ef40_0" | "ef50_0" => o.bits == 0,
                "5c68_0" => o.bits == 0 || o.bits == 1 << 44,
                "5980_0" => o.bits == 0 || o.bits == 1 << 53,
                "5bb0_0" | "5b60_0" | "5c30_0" | "5a00_0" | "5a00_1" | "5c40_0" | "5cb0_1"
                | "5080_0" | "5ca8_0" => true,
                "5bb0_1" => bits(o.bits, 45, 2) < 3,
                "5090_0" => bits(o.bits, 24, 2) < 3,
                "5cb8_0" => o.bits == 0x200,
                "5cb8_1" | "5ce0_1" => o.bits == 0x800 || o.bits == 0x2800,
                "5cb0_0" => o.bits == 0x800,
                "5cb0_2" | "5ce0_0" => o.bits == 0x200 || o.bits == 0x1200,
                "ef90sz" => o.bits == 4 << 48,
                "ef58sz" => o.bits == 4 << 48,
                "eff0_0" => true,
                "e000_0" => o.bits == 0 || o.bits == 2 << 54,
                "e000_1" => o.bits == 0,
                "df60_0" => o.bits == 1 << 29,
                "c038_0" => o.bits == 0 || o.bits == 1 << 55,
                "deb8_0" => o.bits == 0 || o.bits == 1 << 37,
                "f0f8_0" => o.bits == 15,
                "50b0_0" => o.bits == 0xf00,
                "f0f0_0" => true,
                "b" | "0x0" | "lut" => true,
                _ => false,
            };
            if !allowed {
                return Err(x.error(format!("unsupported mode {} = 0x{:x}", o.name, o.bits)));
            }
        }
        if x.opcode == MOV {
            if let Some(Operand::Immediate(mask)) = x.operands.get(2) {
                if *mask != 15 {
                    return Err(x.error("MOV component mask must be 0xf"));
                }
            }
        }
        if matches!(x.opcode, LOP | LOP3) {
            if !matches!(
                x.operands.first(),
                Some(Operand::Predicate(Predicate { index: 7, .. }))
            ) {
                return Err(x.error("logic predicate destination is not supported"));
            }
            if x.opcode == LOP3 && x.encoding != 0x5be0000000000000 {
                return Err(x.error("only register LOP3 is lowered"));
            }
        }
        Ok(())
    }
    fn attribute(&self, x: &Instruction) -> Result<String> {
        let store = x.opcode == Opcode::AST;
        let index = if store { 0 } else { 1 };
        let Some(Operand::Memory {
            space: Space::Attribute,
            base: 255,
            offset,
        }) = x.operands.get(index)
        else {
            return Err(x.error("indexed attribute access is not supported"));
        };
        if offset % 4 != 0 || *offset < 0 {
            return Err(x.error("unaligned attribute access"));
        }
        if x.opcode != Opcode::IPA && register_operand(x, 2)? != 255 {
            return Err(x.error("per-vertex attribute indexing is not supported"));
        }
        if x.opcode == Opcode::IPA {
            if self.header.stage != Stage::Fragment {
                return Err(x.error("IPA requires fragment stage"));
            }
            if register_operand(x, 2)? != 255
                || register_operand(x, 3)? != 255
                || predicate_operand(x, 4)? != "true"
            {
                return Err(x.error("IPA arithmetic or sample predicate is not supported"));
            }
        } else if self.header.stage != Stage::Vertex {
            return Err(x.error("ALD/AST backend requires vertex stage"));
        }
        let count = if x.opcode == Opcode::IPA {
            1
        } else {
            bits(x.raw, 47, 2) + 1
        };
        let first = register_operand(x, if store { 1 } else { 0 })?;
        if u32::from(first) + count > 256 {
            return Err(x.error("attribute register range overflow"));
        }
        let mut s = String::new();
        for c in 0..count {
            let address = (*offset as u32 + c * 4) as u16;
            let map = if store {
                &self.header.outputs
            } else {
                &self.header.inputs
            };
            let attr = map
                .iter()
                .find(|a| a.address == address)
                .ok_or_else(|| x.error(format!("attribute 0x{address:x} absent from SPH")))?;
            let r = (u32::from(first) + c) as u8;
            if store {
                let field = if (0x70..0x80).contains(&address) {
                    format!("position[{}]", (address - 0x70) / 4)
                } else {
                    format!("v{}", address / 4)
                };
                writeln!(s, "out.{field} = as_type<float>({});", reg(r)).unwrap();
            } else {
                let field = if self.header.stage == Stage::Vertex {
                    format!("a{}[{}]", (address - 0x80) / 16, (address % 16) / 4)
                } else if (0x70..0x80).contains(&address) {
                    format!("position[{}]", (address - 0x70) / 4)
                } else {
                    format!("v{}", address / 4)
                };
                if x.opcode == Opcode::IPA
                    && bits(x.raw, 54, 2) == 2
                    && attr.interpolation != Some(Interpolation::Flat)
                {
                    return Err(x.error("IPA.CONSTANT requires flat SPH interpolation"));
                }
                if r != 255 {
                    let value = if x.modified("sat", None) {
                        format!("soju_sat(in.{field})")
                    } else {
                        format!("in.{field}")
                    };
                    writeln!(s, "r[{r}] = as_type<uint>({value});").unwrap();
                }
            }
        }
        Ok(s)
    }
    fn local(&self, x: &Instruction) -> Result<String> {
        let store = x.opcode == Opcode::STL;
        let index = if store { 0 } else { 1 };
        let Some(Operand::Memory {
            space: Space::Local,
            base: 255,
            offset,
        }) = x.operands.get(index)
        else {
            return Err(x.error("only static local addresses are supported"));
        };
        let adjusted = i64::from(*offset) + i64::from(self.header.local_negative);
        if adjusted < 0 || adjusted % 4 != 0 || adjusted + 4 > i64::from(self.local_bytes) {
            return Err(x.error("local address outside SPH allocation"));
        }
        if store {
            Ok(format!(
                "local_mem[{}] = {};\n",
                adjusted / 4,
                source(x, 1, false)?
            ))
        } else {
            assign(x, 0, &format!("local_mem[{}]", adjusted / 4), false)
        }
    }
    fn texture(&self, x: &Instruction) -> Result<String> {
        let bindless = x.encoding == 0xdeb8000000000000;
        if !bindless && x.encoding != 0xc038000000000000 {
            return Err(x.error("unsupported TEX encoding"));
        }
        if !bindless && register_operand(x, 2)? != 255 {
            return Err(x.error("TEX auxiliary register must be RZ for supported modes"));
        }
        let coord = register_operand(x, 1)?;
        if coord > 253 {
            return Err(x.error("TEX requires two consecutive coordinate registers"));
        }
        let handle = if bindless {
            source(x, 2, false)?
        } else {
            match x.operands.get(3) {
                Some(Operand::Immediate(i)) if *i < self.options.texture_count => format!("{i}u"),
                _ => return Err(x.error("bound TEX table index exceeds configured resource table")),
            }
        };
        let lod_zero = if bindless {
            bits(x.raw, 37, 3) == 1
        } else {
            bits(x.raw, 55, 3) == 1
        };
        if !lod_zero && self.header.stage != Stage::Fragment {
            return Err(x.error("implicit texture LOD requires fragment stage"));
        }
        let mut s=format!("uint handle = {handle};\nfloat4 texel = resources.images[handle].sample(resources.samplers[handle], float2(as_type<float>({}), as_type<float>({})){});\n",reg(coord),reg(coord+1),if lod_zero {", level(0.0f)"} else {""});
        let mut dest = u32::from(register_operand(x, 0)?);
        let mask = bits(x.raw, 31, 4);
        if dest + mask.count_ones() > 256 {
            return Err(x.error("texture destination register range overflow"));
        }
        for c in 0..4 {
            if mask & (1 << c) != 0 {
                if dest != 255 {
                    writeln!(s, "r[{dest}] = as_type<uint>(texel[{c}]);").unwrap();
                }
                dest += 1;
            }
        }
        Ok(s)
    }
}

const HELPERS: &str = r#"
inline float soju_ftz(float v) {
    uint b = as_type<uint>(v);
    return as_type<float>((b & 0x7f800000u) == 0u ? b & 0x80000000u : b);
}
inline float soju_sat(float v) { return isnan(v) ? 0.0f : clamp(v, 0.0f, 1.0f); }
inline uint soju_lop3(uint a, uint b, uint c, uint lut) {
    uint out = 0u;
    for (uint i = 0; i < 8; ++i) {
        if ((lut >> i) & 1u) out |= ((i & 4u) ? a : ~a) & ((i & 2u) ? b : ~b) & ((i & 1u) ? c : ~c);
    }
    return out;
}
inline uint soju_bfe(uint a, uint control, bool sign) {
    uint start = control & 255u;
    uint width = (control >> 8u) & 255u;
    if (width == 0u) return 0u;
    if (start >= 32u) return sign && (a >> 31u) ? 0xffffffffu : 0u;
    width = min(width, 32u - start);
    uint v = a >> start;
    if (width == 32u) return v;
    uint mask = (1u << width) - 1u;
    v &= mask;
    if (sign && (v & (1u << (width - 1u)))) v |= ~mask;
    return v;
}
inline uint soju_bfi(uint insert, uint control, uint base) {
    uint start = control & 255u;
    uint width = (control >> 8u) & 255u;
    if (start >= 32u || width == 0u) return base;
    width = min(width, 32u - start);
    uint mask = width == 32u ? 0xffffffffu : ((1u << width) - 1u) << start;
    return (base & ~mask) | ((insert << start) & mask);
}
inline uint soju_f2i(float v, bool sign, uint mode) {
    v = mode == 0u ? rint(v) : mode == 1u ? floor(v) : mode == 2u ? ceil(v) : trunc(v);
    if (isnan(v)) return 0u;
    if (sign) {
        if (v >= 2147483648.0f) return 0x7fffffffu;
        if (v <= -2147483648.0f) return 0x80000000u;
        return as_type<uint>(int(v));
    }
    if (v >= 4294967296.0f) return 0xffffffffu;
    return v <= 0.0f ? 0u : uint(v);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    fn binary(words: &[u64]) -> Vec<u8> {
        let mut h = [0u32; 20];
        h[0] = 2 | (5 << 10);
        h[18] = 15;
        h.into_iter()
            .flat_map(u32::to_le_bytes)
            .chain(words.iter().flat_map(|w| w.to_le_bytes()))
            .collect()
    }
    #[test]
    fn cbuf_binding_rz_and_predicated_exit() {
        let b = binary(&[
            0,
            0x4c980000000700ff | 2 << 34 | 15 << 39,
            0xe30000000000000f,
            0xe30000000007000f,
        ]);
        let m = crate::translate(&b, &Options::default()).unwrap();
        assert_eq!(m.constant_buffers, [2]);
        assert!(m.source.contains("cb2 [[buffer(2)]]"));
        assert!(m.source.contains("(void)(cb2[0])"));
        assert!(m.source.contains("if (p[0])"));
        assert!(!m.source.contains("\nr[255] ="));
    }
    #[test]
    fn rejects_unimplemented_modes() {
        let b = binary(&[0, 0x5c58000000070000 | 1 << 47, 0xe30000000007000f]);
        assert!(crate::translate(&b, &Options::default())
            .unwrap_err()
            .0
            .contains("modifier cc"));
        let b = binary(&[0, 0xef48000000070000, 0xe30000000007000f]);
        assert!(crate::translate(&b, &Options::default()).is_err());
    }
}
