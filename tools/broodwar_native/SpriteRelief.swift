import Cocoa
import SceneKit
import Metal
import simd

// Source depth is an artistic relief, not recovered hidden-side geometry.
final class SpriteRelief {
    let device=MTLCreateSystemDefaultDevice()!
    let dataTextures=ResidentCache<MTLTexture>(limit:256*1024*1024)
    let materials=ResidentCache<SCNMaterial>(limit:2048)
    var meshes=[String:SCNGeometry](),depthAtlases=[String:NSBitmapImageRep]()
    var contactHeights=[String:Float]()
    let alphaAtlases=ResidentCache<NSBitmapImageRep>(limit:128*1024*1024)
    static let sun=SIMD3<Float>(-0.22033581,0.89032002,0.39847509)
    static let reference=SIMD3<Float>(-0.22033581,0.54387457,0.80972377)
    func geometry(_ id:Int,frame:Int,art:WorldSprites)->SCNGeometry? {
        let key="\(id)/\(frame)"
        if let mesh=meshes[key] {return mesh.copy() as? SCNGeometry}
        guard let entry=art.images[String(id)],let relief=entry["relief"] as? Obj,
            let files=relief["files"] as? [String:String],let file=files["ao_depth"],
            let records=art.frameRecords(id),records.indices.contains(frame) else {return nil}
        let depth:NSBitmapImageRep
        if let cached=depthAtlases[file] {depth=cached}
        else {
            guard let data=try? Data(contentsOf:art.folder.appendingPathComponent(file)),let loaded=NSBitmapImageRep(data:data) else {return nil}
            depth=loaded
            if depthAtlases.count>40 {depthAtlases.removeAll()};depthAtlases[file]=loaded
        }
        var alpha:NSBitmapImageRep?
        if let file=entry["file"] as? String {
            alpha=alphaAtlases.get(file)
            if alpha==nil,let data=try? Data(contentsOf:art.folder.appendingPathComponent(file)),let bitmap=NSBitmapImageRep(data:data) {
                alpha=bitmap;alphaAtlases.insert(bitmap,key:file,bytes:bitmap.bytesPerRow*bitmap.pixelsHigh)
            }
        }
        var minimum=Double.infinity
        let f=records[frame],w=Int(number(f,"w")),h=Int(number(f,"h")),scale=number(entry,"scale"),unit=scale*32
        guard w>0,h>0,unit>0 else {return nil}
        let cw=number(entry,"grpWidth")*scale,ch=number(entry,"grpHeight")*scale
        // Four HD pixels per cell; the silhouette still uses full-resolution alpha.
        let nx=max(1,(w+3)/4),ny=max(1,(h+3)/4)
        var vertices=[SCNVector3](),uv=[CGPoint](),indices=[Int32]()
        for y in 0...ny {for x in 0...nx {
            let px=Double(x)*Double(w)/Double(nx),py=Double(y)*Double(h)/Double(ny)
            let ax=Int(number(f,"x"))+min(w-1,Int(px)),ay=Int(number(f,"y"))+min(h-1,Int(py))
            var sample=[Int](repeating:0,count:4);depth.getPixel(&sample,atX:ax,y:ay)
            let base=max(0,(number(f,"oy")+Double(h)-ch/2)/unit)/tan(Terrain3D.canonicalElevation)
            let z=base+Double(sample[0])*0.5/unit
            let u=(number(f,"ox")+px)/cw,v=(number(f,"oy")+py)/ch
            let localY=(0.5-v)*ch/unit
            vertices.append(SCNVector3((u-0.5)*cw/unit,localY,z))
            if let alpha=alpha {
                var coverage=[Int](repeating:0,count:4);alpha.getPixel(&coverage,atX:ax,y:ay)
                if coverage[3]>=16 {minimum=min(minimum,localY*cos(Terrain3D.canonicalElevation)+z*sin(Terrain3D.canonicalElevation))}
            }
            uv.append(CGPoint(x:u,y:v))
        }}
        for y in 0..<ny {for x in 0..<nx {
            let a=Int32(y*(nx+1)+x),b=a+1,c=a+Int32(nx+1),d=c+1
            indices += [a,c,b,b,c,d]
        }}
        let mesh=SCNGeometry(sources:[SCNGeometrySource(vertices:vertices),SCNGeometrySource(textureCoordinates:uv)],
                             elements:[SCNGeometryElement(indices:indices,primitiveType:.triangles)])
        contactHeights[key]=minimum.isFinite ? Float(minimum):0
        if meshes.count>512 {meshes.removeAll()};meshes[key]=mesh
        return mesh.copy() as? SCNGeometry
    }
    func dataTexture(_ id:Int,frame:Int,layer:String,art:WorldSprites)->MTLTexture? {
        let key="\(id)/\(frame)/\(layer)"
        if let cached=dataTextures.get(key) {return cached}
        guard let image=art.texture(id,frame:frame,layer:layer),let data=image.tiffRepresentation,
            let bitmap=NSBitmapImageRep(data:data) else {return nil}
        let w=bitmap.pixelsWide,h=bitmap.pixelsHigh
        var bytes=[UInt8](repeating:255,count:w*h*4),pixel=[Int](repeating:0,count:4)
        for y in 0..<h {for x in 0..<w {
            bitmap.getPixel(&pixel,atX:x,y:y)
            for c in 0..<3 {bytes[(y*w+x)*4+c]=UInt8(clamping:pixel[c])}
        }}
        let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm,width:w,height:h,mipmapped:false)
        descriptor.storageMode = .shared;descriptor.usage = .shaderRead
        guard let texture=TextureResidency.shared.make(device,descriptor) else {return nil}
        bytes.withUnsafeBytes{texture.replace(region:MTLRegionMake2D(0,0,w,h),mipmapLevel:0,withBytes:$0.baseAddress!,bytesPerRow:w*4)}
        dataTextures.insert(texture,key:key,bytes:w*h*4)
        return texture
    }
    func material(_ id:Int,frame:Int,color:[Int],art:WorldSprites)->SCNMaterial {
        let key="\(id)/\(frame)/\(color)"
        if let cached=materials.get(key) {return cached.copy() as! SCNMaterial}
        let m=SCNMaterial();m.lightingModel = .constant;m.isDoubleSided=true
        m.diffuse.contents=art.texture(id,frame:frame,color:color)
        m.readsFromDepthBuffer=true;m.writesToDepthBuffer=true
        m.transparencyMode = .aOne
        let names=["normal","bright","specular","emissive","ao_depth"]
        for name in names {
            let fallback:NSColor = name=="normal" ? NSColor(red:0.5,green:0.5,blue:1,alpha:1):(name=="ao_depth" ? .white:.black)
            let contents:Any
            if name=="normal" || name=="ao_depth" {contents=dataTexture(id,frame:frame,layer:name,art:art) ?? fallback as Any}
            else {contents=art.texture(id,frame:frame,color:color,layer:name) ?? fallback as Any}
            let property=SCNMaterialProperty(contents:contents)
            property.wrapS = .clamp;property.wrapT = .clamp
            m.setValue(property,forKey:"relief_"+name)
        }
        m.shaderModifiers=[.surface:"""
        uniform sampler2D relief_normal;
        uniform sampler2D relief_bright;
        uniform sampler2D relief_specular;
        uniform sampler2D relief_emissive;
        uniform sampler2D relief_ao_depth;
        uniform vec3 reliefLight;
        uniform vec3 reliefEye;
        #pragma body
        if (_surface.diffuse.a < 0.01) { discard_fragment(); }
        vec2 uv = _surface.diffuseTexcoord;
        vec3 n = normalize(texture2D(relief_normal, uv).rgb * 2.0 - 1.0);
        vec3 l0 = vec3(-0.22033581, 0.54387457, 0.80972377);
        vec3 l = normalize(reliefLight);
        float ao = texture2D(relief_ao_depth, uv).g;
        vec3 bright = texture2D(relief_bright, uv).rgb;
        vec3 spec = texture2D(relief_specular, uv).rgb;
        vec3 emission = texture2D(relief_emissive, uv).rgb;
        float lambert0 = max(dot(n,l0),0.0);
        float lambert = max(dot(n,l),0.0);
        float highlight0 = pow(max(dot(n,normalize(l0+vec3(0.0,0.0,1.0))),0.0),24.0);
        float highlight = pow(max(dot(n,normalize(l+normalize(reliefEye))),0.0),24.0);
        vec3 sourceLighting = bright * (0.19 + ao * lambert0) + spec * highlight0 + emission;
        vec3 residual = _surface.diffuse.rgb - sourceLighting;
        _surface.diffuse.rgb = max(_surface.diffuse.rgb * 0.35, residual + bright * (0.19 + ao * lambert) + spec * highlight + emission);
        """]
        m.setValue(NSValue(scnVector3:SCNVector3(Self.reference)),forKey:"reliefLight")
        m.setValue(NSValue(scnVector3:SCNVector3(0,0,1)),forKey:"reliefEye")
        materials.insert(m,key:key,bytes:1)
        return m.copy() as! SCNMaterial
    }
}
