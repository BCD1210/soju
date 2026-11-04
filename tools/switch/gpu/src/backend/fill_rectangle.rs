//! NV_fill_rectangle coverage with extrapolated triangle interpolation.
use super::*;
pub(super) fn wrap(
    vs: &mut MetalShader,
    vh: &Header,
    fh: &Header,
    provoking_last: bool,
) -> Result<()> {
    use soju_switch_shader::header::Interpolation;
    require(
        vh.outputs
            .iter()
            .all(|a| (0x70..0x280).contains(&a.address)),
        "rectangle special vertex outputs unsupported",
    )?;
    let start = vs
        .source
        .find("vertex StageOut soju_vertex(")
        .ok_or_else(|| Error("rectangle vertex entry".into()))?;
    let end = start
        + vs.source[start..]
            .find(") {\n")
            .ok_or_else(|| Error("rectangle vertex signature".into()))?
        + 1;
    let signature = vs.source[start..end].to_owned();
    let mut helper = signature.replace(
        "vertex StageOut soju_vertex",
        "inline StageOut soju_triangle_vertex",
    );
    while let Some(a) = helper.find(" [[") {
        let b = a
            + helper[a..]
                .find("]]")
                .ok_or_else(|| Error("rectangle parameter annotation".into()))?
            + 2;
        helper.replace_range(a..b, "");
    }
    vs.source.replace_range(start..end, &helper);
    let has_id = signature.contains("soju_vertex_id");
    let mut entry = signature.clone();
    if !has_id {
        entry.insert_str(entry.len() - 1, ", uint soju_vertex_id [[vertex_id]]");
    }
    let args = |vertex: &str| {
        let mut args = String::from("in");
        if has_id {
            write!(args, ", {vertex}").unwrap();
        }
        if signature.contains("soju_instance_id") {
            args.push_str(", soju_instance_id");
        }
        for slot in &vs.constant_buffers {
            write!(args, ", cb{slot}").unwrap();
        }
        if vs.uses_texture_table {
            args.push_str(", resources");
        }
        args
    };
    writeln!(vs.source, "\n{entry} {{\nuint base = (soju_vertex_id / 6u) * 3u;\nStageOut a=soju_triangle_vertex({});\nStageOut b=soju_triangle_vertex({});\nStageOut c=soju_triangle_vertex({});", args("base"), args("base+1u"), args("base+2u")).unwrap();
    vs.source.push_str(r#"
StageOut out = {};
out.position=float4(2,2,2,1);
if (a.position.w<=0 || b.position.w<=0 || c.position.w<=0) return out;
float3 q=1.0f/float3(a.position.w,b.position.w,c.position.w);
float3 pa=a.position.xyz*q.x, pb=b.position.xyz*q.y, pc=c.position.xyz*q.z;
if (any(abs(pa.xy)>1.0f) || any(abs(pb.xy)>1.0f) || any(abs(pc.xy)>1.0f) ||
    pa.z<0 || pa.z>1 || pb.z<0 || pb.z>1 || pc.z<0 || pc.z>1) return out;
float2 ab=pb.xy-pa.xy, ac=pc.xy-pa.xy;
float det=ab.x*ac.y-ab.y*ac.x;
if (det==0.0f) return out;
uint corner=soju_vertex_id%6u;
if(det<0.0f) corner=5u-corner;
float2 lo=min(pa.xy,min(pb.xy,pc.xy)), hi=max(pa.xy,max(pb.xy,pc.xy));
float2 p=(corner==0u || corner==3u) ? lo : (corner==1u ? float2(hi.x,lo.y) : (corner==5u ? float2(lo.x,hi.y) : hi));
float2 delta=p-pa.xy;
float by=(delta.x*ac.y-delta.y*ac.x)/det;
float bz=(ab.x*delta.y-ab.y*delta.x)/det;
float3 bary=float3(1.0f-by-bz,by,bz);
float rw=dot(bary,q);
if(rw<=0.0f) return out;
out.position=float4(p,dot(bary,float3(pa.z,pb.z,pc.z)),1.0f)/rw;
"#);
    for a in &vh.outputs {
        if !(0x80..0x280).contains(&a.address) {
            continue;
        }
        let field = a.address / 4;
        if fh
            .inputs
            .iter()
            .any(|f| f.address == a.address && f.interpolation == Some(Interpolation::Flat))
        {
            writeln!(
                vs.source,
                "out.v{field}={}.v{field};",
                if provoking_last { "c" } else { "a" }
            )
            .unwrap();
            continue;
        }
        let linear = fh
            .inputs
            .iter()
            .any(|f| f.address == a.address && f.interpolation == Some(Interpolation::Linear));
        writeln!(
            vs.source,
            "out.v{field}=dot({},float3(a.v{field},b.v{field},c.v{field})){};",
            if linear { "bary" } else { "bary*q" },
            if linear { "" } else { "/rw" }
        )
        .unwrap();
    }
    vs.source.push_str("return out;\n}\n");
    Ok(())
}
