//! OpenGL 2.1 §3.4.2 line rectangles and exact pixel-square coverage.
use super::*;

pub(super) fn wrap(
    vs: &mut MetalShader,
    fs: &mut MetalShader,
    vh: &Header,
    fh: &Header,
    r: &Registers,
) -> Result<()> {
    require(
        vh.outputs
            .iter()
            .all(|a| (0x70..0x280).contains(&a.address)),
        "smooth line special outputs unsupported",
    )?;
    require(
        !vh.inputs.iter().any(|a| matches!(a.address, 0x2f8 | 0x2fc)),
        "smooth line vertex/instance ID fetch unsupported",
    )?;
    require(
        r.get(0x12ec) == 0 && !fs.source.contains("uint4 color"),
        "smooth line alpha test/integer output unsupported",
    )?;
    require(
        fh.inputs
            .iter()
            .all(|a| (0x80..0x280).contains(&a.address) || a.address == 0x7c)
            && fh.inputs.len() <= 16,
        "smooth line special/large fragment interface unsupported",
    )?;
    let slots: BTreeSet<_> = vh.inputs.iter().map(|a| a.address / 16 - 8).collect();
    for slot in slots {
        require(
            slot < 16,
            "smooth line requires at most sixteen attribute slots",
        )?;
        let field = vs
            .source
            .lines()
            .find(|line| line.contains(&format!(" a{slot} [[attribute({slot})]]")))
            .ok_or_else(|| Error("smooth line input declaration".into()))?
            .to_owned();
        let extra = field.replace(
            &format!(" a{slot} [[attribute({slot})]]"),
            &format!(" b{slot} [[attribute({})]]", slot + 16),
        );
        vs.source = vs.source.replace(&field, &format!("{field}\n{extra}"));
    }
    let start = vs
        .source
        .find("vertex StageOut soju_vertex(")
        .ok_or_else(|| Error("smooth line vertex entry".into()))?;
    let end = start
        + vs.source[start..]
            .find(") {\n")
            .ok_or_else(|| Error("smooth line vertex signature".into()))?
        + 1;
    let signature = vs.source[start..end].to_owned();
    let mut helper = signature.replace(
        "vertex StageOut soju_vertex",
        "inline StageOut soju_line_endpoint",
    );
    while let Some(a) = helper.find(" [[") {
        let b = a
            + helper[a..]
                .find("]]")
                .ok_or_else(|| Error("smooth line parameter annotation".into()))?
            + 2;
        helper.replace_range(a..b, "");
    }
    vs.source.replace_range(start..end, &helper);
    vs.source = vs.source.replace(
        "struct StageOut {",
        "struct StageOut {\nfloat4 line_endpoints [[user(soju_line)]];",
    );
    let mut entry = signature;
    entry.insert_str(entry.len() - 1, ", uint soju_line_id [[vertex_id]]");
    let args = |input: &str| {
        let mut args = input.to_owned();
        for slot in &vs.constant_buffers {
            write!(args, ", cb{slot}").unwrap();
        }
        if vs.uses_texture_table {
            args.push_str(", resources");
        }
        args
    };
    writeln!(vs.source, "\n{entry} {{\nStageIn other=in;").unwrap();
    for slot in vh
        .inputs
        .iter()
        .map(|a| a.address / 16 - 8)
        .collect::<BTreeSet<_>>()
    {
        writeln!(vs.source, "other.a{slot}=in.b{slot};").unwrap();
    }
    writeln!(
        vs.source,
        "StageOut original_a=soju_line_endpoint({}), original_b=soju_line_endpoint({});",
        args("in"),
        args("other")
    )
    .unwrap();
    let width = r.float(0x13b0);
    let sx = r.float(0xa00).abs();
    let sy = r.float(0xa04).abs();
    require(
        width.is_finite()
            && width > 0.
            && width <= 64.
            && sx.is_finite()
            && sy.is_finite()
            && sx > 0.
            && sy > 0.,
        "smooth line width/viewport",
    )?;
    let ox = r.float(0xa0c);
    let oy = r.float(0xa10);
    require(
        ox.is_finite() && oy.is_finite(),
        "smooth line viewport origin",
    )?;
    vs.source.push_str(
        r#"
StageOut out={}; out.position=float4(2,2,2,1);
float4 pa=original_a.position, pb=original_b.position;
float da[6]={pa.w+pa.x,pa.w-pa.x,pa.w+pa.y,pa.w-pa.y,pa.z,pa.w-pa.z};
float db[6]={pb.w+pb.x,pb.w-pb.x,pb.w+pb.y,pb.w-pb.y,pb.z,pb.w-pb.z};
float lo=0,hi=1;
for(uint i=0;i<6;i++) {
    if(da[i]<0 && db[i]<0) return out;
    if(da[i]<0) lo=max(lo,da[i]/(da[i]-db[i]));
    if(db[i]<0) hi=min(hi,da[i]/(da[i]-db[i]));
}
if(lo>=hi) return out;
StageOut a=original_a,b=original_b;
a.position=mix(pa,pb,lo);b.position=mix(pa,pb,hi);
if(a.position.w<=0 || b.position.w<=0) return out;
"#,
    );
    for attr in &vh.outputs {
        if attr.address >= 0x80 {
            let f = attr.address / 4;
            let linear = fh
                .inputs
                .iter()
                .find(|x| x.address == attr.address)
                .and_then(|x| x.interpolation)
                == Some(soju_switch_shader::header::Interpolation::Linear);
            let (ta, tb) = if linear {
                ("lo*pb.w/a.position.w", "hi*pb.w/b.position.w")
            } else {
                ("lo", "hi")
            };
            writeln!(vs.source,"a.v{f}=mix(original_a.v{f},original_b.v{f},{ta});b.v{f}=mix(original_a.v{f},original_b.v{f},{tb});").unwrap();
        }
    }
    writeln!(
        vs.source,
        "float2 scale=float2({sx:?}f,-{sy:?}f), origin=float2({ox:?}f,{oy:?}f);"
    )
    .unwrap();
    vs.source.push_str(
        r#"
float qa=1/a.position.w,qb=1/b.position.w;
float2 sa=a.position.xy*qa*scale+origin,sb=b.position.xy*qb*scale+origin;
float2 delta=sb-sa;float len=length(delta);
if(len<=0) return out;
float2 normal=float2(-delta.y,delta.x)/len;
uint corners[6]={0,1,2,0,2,3};uint corner=corners[soju_line_id%6];
uint section=(soju_line_id%18)/6;bool end=(corner==1 || corner==2);
float t=section==0 ? (end ? 0 : -1/len) : (section==1 ? (end ? 1 : 0) : (end ? 1+1/len : 1));
float u=clamp(t,0.0f,1.0f);
float rw=mix(qa,qb,u);
if(rw<=0) return out;
"#,
    );
    writeln!(
        vs.source,
        "float2 screen=mix(sa,sb,t)+normal*({:?}f+1)*(corner>=2 ? 1.0f : -1.0f);",
        width / 2.
    )
    .unwrap();
    vs.source.push_str("out.position=float4((screen-origin)/scale,mix(a.position.z*qa,b.position.z*qb,u),1)/rw;\nout.line_endpoints=float4(sa,sb);\n");
    for attr in &vh.outputs {
        if attr.address < 0x80 {
            continue;
        }
        let f = attr.address / 4;
        let interpolation = fh
            .inputs
            .iter()
            .find(|x| x.address == attr.address)
            .and_then(|x| x.interpolation);
        match interpolation {
            Some(soju_switch_shader::header::Interpolation::Flat) => writeln!(
                vs.source,
                "out.v{f}={}.v{f};",
                if r.get(0x1684) == 1 {
                    "original_b"
                } else {
                    "original_a"
                }
            )
            .unwrap(),
            Some(soju_switch_shader::header::Interpolation::Linear) => {
                writeln!(vs.source, "out.v{f}=mix(a.v{f},b.v{f},u);").unwrap()
            }
            _ => writeln!(vs.source, "out.v{f}=mix(a.v{f}*qa,b.v{f}*qb,u)/rw;").unwrap(),
        }
    }
    vs.source.push_str("return out;\n}\n");
    fs.source = fs.source.replace(
        "struct StageIn {",
        "struct StageIn {\nfloat4 line_endpoints [[user(soju_line),flat]];",
    );
    if !fs.source.contains("float4 position [[position]];") {
        fs.source = fs.source.replace(
            "struct StageIn {",
            "struct StageIn {\nfloat4 position [[position]];",
        );
    }
    let entry = fs
        .source
        .find("fragment StageOut soju_fragment")
        .ok_or_else(|| Error("smooth line fragment entry".into()))?;
    let coverage = format!(
        r#"
float soju_line_coverage(float4 ends,float2 center) {{
    float2 a=ends.xy-center,b=ends.zw-center,d=b-a;
    float2 n=normalize(float2(-d.y,d.x))*{half:?}f;
    float2 polygon[12]={{a-n,b-n,b+n,a+n}},next[12];uint count=4;
    for(uint plane=0;plane<4;plane++) {{
        uint axis=plane/2;float sign=(plane%2)==0 ? 1.0f : -1.0f;uint used=0;
        for(uint j=0;j<count;j++) {{
            float2 p=polygon[j],q=polygon[(j+1)%count];
            float dp=sign*p[axis]-0.5f,dq=sign*q[axis]-0.5f;
            if(dp<=0) next[used++]=p;
            if((dp<0 && dq>0) || (dp>0 && dq<0)) next[used++]=mix(p,q,dp/(dp-dq));
        }}
        count=used;if(count<3) return 0;
        for(uint j=0;j<count;j++) polygon[j]=next[j];
    }}
    float area=0;for(uint j=0;j<count;j++) {{float2 p=polygon[j],q=polygon[(j+1)%count];area+=p.x*q.y-p.y*q.x;}}
    return clamp(abs(area)*0.5f,0.0f,1.0f);
}}
"#,
        half = width / 2.
    );
    let mut finish="float coverage=soju_line_coverage(in.line_endpoints,in.position.xy);if(coverage<=0) discard_fragment();\n".to_owned();
    for (slot, mask) in fh.color_masks.iter().enumerate() {
        if *mask != 0 {
            require(mask & 8 != 0, "smooth line output lacks alpha")?;
            writeln!(finish, "out.color{slot}.a*=coverage;").unwrap();
        }
    }
    finish.push_str("return out;");
    fs.source = format!(
        "{}{}{}",
        &fs.source[..entry],
        coverage,
        fs.source[entry..].replace("return out;", &finish)
    );
    Ok(())
}

pub(super) fn expand(
    source: &[u8],
    stride: u32,
    first: u32,
    count: u32,
    topology: u32,
) -> Result<Vec<u8>> {
    require(
        stride <= 1024,
        "smooth line expanded stride exceeds Metal limit",
    )?;
    let segments = if topology == 1 { count / 2 } else { count - 1 };
    let bytes = u64::from(segments) * 18 * u64::from(stride) * 2;
    require(
        bytes <= 64 * 1024 * 1024,
        "smooth line expanded stream exceeds 64 MiB",
    )?;
    let stride = stride as usize;
    let mut output = vec![0; bytes as usize];
    for segment in 0..segments as usize {
        let a = first as usize + segment * if topology == 1 { 2 } else { 1 };
        for corner in 0..18 {
            for endpoint in 0..2 {
                let from = (a + endpoint) * stride;
                require(from < source.len(), "smooth line source vertex range")?;
                let n = stride.min(source.len() - from);
                let to = ((segment * 18 + corner) * 2 + endpoint) * stride;
                output[to..to + n].copy_from_slice(&source[from..from + n]);
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_streams_preserve_strip_adjacency_and_enforce_limits() {
        let source = [1, 2, 3, 4];
        let strip = expand(&source, 1, 1, 3, 3).unwrap();
        assert_eq!(strip.len(), 72);
        assert!(strip[..36].chunks_exact(2).all(|v| v == [2, 3]));
        assert!(strip[36..].chunks_exact(2).all(|v| v == [3, 4]));
        let lines = expand(&source, 1, 0, 4, 1).unwrap();
        assert!(lines[..36].chunks_exact(2).all(|v| v == [1, 2]));
        assert!(lines[36..].chunks_exact(2).all(|v| v == [3, 4]));
        assert!(expand(&source, 1025, 0, 2, 1).is_err());
        assert!(expand(&source, 1024, 0, 100_000, 1).is_err());
        assert!(expand(&source, 1, 3, 2, 1).is_err());
    }
}
