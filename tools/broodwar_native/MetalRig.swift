import Cocoa
import SceneKit
import CryptoKit
import Metal
import MetalKit
import ImageIO
import simd

// glTF data stays on the scene owner. Published poses contain immutable GPU buffers.
final class MetalRigAsset {
    struct Part {
        let node:Int,skin:Int?,vertices:[MetalBatches.Vertex],indices:[UInt32]
        let vertexBuffer:MTLBuffer,indexBuffer:MTLBuffer,textures:[MTLTexture],samplers:[MTLSamplerState]
        let paint:Bool,replace:Bool,opaque:Bool
    }
    struct Track {let node:Int,path:String,times:[Float],values:[[Float]],step:Bool}
    struct Clip {
        let tracks:[Track],duration:Float
        var hasMotion:Bool {tracks.contains {track in
            guard let first=track.values.first else {return false}
            return track.values.dropFirst().contains {value in zip(first,value).contains {abs($0-$1)>0.000001}}
        }}
    }
    let nodes:[Obj],skins:[Obj],parts:[Part],clips:[String:Clip]
    let parents:[Int],inverseBinds:[[simd_float4x4]]
    struct PosedPart {let part:Part,bones:MTLBuffer,points:[SIMD3<Float>],lower:SIMD3<Float>,upper:SIMD3<Float>}
    let poses=ResidentCache<[PosedPart]>(limit:32*1024*1024)
    init(url:URL,definition:ModelDefinition,batches:MetalBatches)throws {
        let data=try Data(contentsOf:url)
        func word(_ offset:Int)->Int {data.withUnsafeBytes{Int($0.loadUnaligned(fromByteOffset:offset,as:UInt32.self))}}
        guard data.count>28,word(0)==0x46546c67,word(4)==2 else {throw SceneError.invalid("Invalid rig GLB")}
        let length=word(12),binaryStart=28+length
        guard binaryStart<=data.count,let doc=try JSONSerialization.jsonObject(with:data.subdata(in:20..<20+length)) as? Obj else {throw SceneError.invalid("Invalid rig JSON")}
        let accessors=doc["accessors"] as? [Obj] ?? [],views=doc["bufferViews"] as? [Obj] ?? []
        func values(_ index:Int)throws->[[Float]] {
            guard accessors.indices.contains(index) else {throw SceneError.invalid("Rig accessor out of range")}
            let a=accessors[index],viewIndex=Int(number(a,"bufferView"))
            guard views.indices.contains(viewIndex),a["sparse"]==nil else {throw SceneError.invalid("Unsupported rig accessor")}
            let view=views[viewIndex],component=Int(number(a,"componentType")),count=Int(number(a,"count"))
            let width=["SCALAR":1,"VEC2":2,"VEC3":3,"VEC4":4,"MAT4":16][a["type"] as? String ?? ""] ?? 0
            let bytes=component==5126 || component==5125 ? 4:component==5123 ? 2:1
            let stride=Int(number(view,"byteStride"))>0 ? Int(number(view,"byteStride")):width*bytes
            let start=binaryStart+Int(number(view,"byteOffset"))+Int(number(a,"byteOffset"))
            guard count>0,width>0,start>=0,start+(count-1)*stride+width*bytes<=data.count else {throw SceneError.invalid("Invalid rig accessor bounds")}
            return try data.withUnsafeBytes { raw in
                try (0..<count).map { i in try (0..<width).map { j in
                    let offset=start+i*stride+j*bytes;var value:Float
                    switch component {
                    case 5126:value=raw.loadUnaligned(fromByteOffset:offset,as:Float.self)
                    case 5125:value=Float(raw.loadUnaligned(fromByteOffset:offset,as:UInt32.self))
                    case 5123:value=Float(raw.loadUnaligned(fromByteOffset:offset,as:UInt16.self))
                    case 5121:value=Float(raw[offset])
                    default:throw SceneError.invalid("Unsupported rig component")
                    }
                    if a["normalized"] as? Bool == true {value /= component==5123 ? 65535:255}
                    guard value.isFinite else {throw SceneError.invalid("Nonfinite rig attribute")};return value
                }}
            }
        }
        let nodes=doc["nodes"] as? [Obj] ?? [],skins=doc["skins"] as? [Obj] ?? []
        self.nodes=nodes;self.skins=skins
        var parent=Array(repeating:-1,count:nodes.count)
        for (i,node) in nodes.enumerated() {for child in node["children"] as? [Int] ?? [] {guard nodes.indices.contains(child),parent[child]==(-1) else {throw SceneError.invalid("Invalid rig hierarchy")};parent[child]=i}}
        for i in nodes.indices {
            var seen=Set<Int>(),cursor=i
            while cursor>=0 {guard seen.insert(cursor).inserted else {throw SceneError.invalid("Cyclic rig hierarchy")};cursor=parent[cursor]}
        }
        parents=parent
        let inverseBinds=try skins.map { skin in try values(Int(number(skin,"inverseBindMatrices"))).map(Self.matrix) }
        self.inverseBinds=inverseBinds
        let images=doc["images"] as? [Obj] ?? [],textures=doc["textures"] as? [Obj] ?? []
        var imageCache=[String:MTLTexture]()
        func texture(_ info:Obj?,_ fallback:MTLTexture,_ color:Bool=true)throws->MTLTexture {
            guard let index=info?["index"] as? Int,textures.indices.contains(index),let source=textures[index]["source"] as? Int,images.indices.contains(source),let vi=images[source]["bufferView"] as? Int,views.indices.contains(vi) else {return fallback}
            let cacheKey="\(source)/\(color)"
            if let cached=imageCache[cacheKey] {return cached}
            let v=views[vi],start=binaryStart+Int(number(v,"byteOffset")),end=binaryStart+Int(number(v,"byteOffset"))+Int(number(v,"byteLength"))
            guard start>=binaryStart,end<=data.count,let sourceImage=CGImageSourceCreateWithData(data.subdata(in:start..<end) as CFData,nil),let image=CGImageSourceCreateImageAtIndex(sourceImage,0,nil) else {throw SceneError.invalid("Invalid rig texture")}
            let result=try TextureResidency.shared.load(batches.loader,image,options:[.SRGB:color,.generateMipmaps:true,.origin:MTKTextureLoader.Origin.topLeft],owner:"authored-rigs");imageCache[cacheKey]=result;return result
        }
        let sourceSamplers=doc["samplers"] as? [Obj] ?? []
        var samplerCache=[Int:MTLSamplerState]()
        func sampler(_ info:Obj?)throws->MTLSamplerState {
            let textureIndex=info?["index"] as? Int ?? -1
            let index=textures.indices.contains(textureIndex) ? textures[textureIndex]["sampler"] as? Int ?? -1:-1
            if let cached=samplerCache[index] {return cached}
            let source=sourceSamplers.indices.contains(index) ? sourceSamplers[index]:[:],descriptor=MTLSamplerDescriptor()
            func address(_ key:String)throws->MTLSamplerAddressMode {
                switch source[key] as? Int ?? 10497 {
                case 10497:return .repeat
                case 33071:return .clampToEdge
                case 33648:return .mirrorRepeat
                default:throw SceneError.invalid("Unsupported rig texture wrap")
                }
            }
            descriptor.sAddressMode=try address("wrapS");descriptor.tAddressMode=try address("wrapT")
            let minimum=source["minFilter"] as? Int ?? 9987
            descriptor.minFilter=[9728,9984,9986].contains(minimum) ? .nearest:.linear
            descriptor.magFilter=source["magFilter"] as? Int == 9728 ? .nearest:.linear
            descriptor.mipFilter=[9728,9729].contains(minimum) ? .notMipmapped:([9984,9985].contains(minimum) ? .nearest:.linear)
            guard let result=batches.device.makeSamplerState(descriptor:descriptor) else {throw SceneError.invalid("Rig texture sampler allocation")}
            samplerCache[index]=result;return result
        }
        let meshes=doc["meshes"] as? [Obj] ?? [],materials=doc["materials"] as? [Obj] ?? []
        let teamNames=Set(definition.raw["teamMaterials"] as? [String] ?? [])
        var output=[Part]()
        for (nodeIndex,node) in nodes.enumerated() {
            guard let meshIndex=node["mesh"] as? Int,meshes.indices.contains(meshIndex) else {continue}
            let skinIndex=node["skin"] as? Int
            for primitive in meshes[meshIndex]["primitives"] as? [Obj] ?? [] {
                guard let attrs=primitive["attributes"] as? [String:Int],let pi=attrs["POSITION"],let ii=primitive["indices"] as? Int else {throw SceneError.invalid("Rig primitive missing geometry")}
                let positions=try values(pi),normals=try attrs["NORMAL"].map(values),uvs=try attrs["TEXCOORD_0"].map(values)
                let weights=try attrs["WEIGHTS_0"].map(values),joints=try attrs["JOINTS_0"].map(values)
                let indices=try values(ii).map{UInt32($0[0])}
                guard indices.allSatisfy({Int($0)<positions.count}) else {throw SceneError.invalid("Rig index out of range")}
                let vertices=try positions.indices.map { i -> MetalBatches.Vertex in
                    let p=positions[i],n=normals?[i] ?? [0,1,0],uv=uvs?[i] ?? [0,0]
                    let w=weights?[i] ?? [1,0,0,0],j=joints?[i] ?? [0,0,0,0]
                    if let skinIndex=skinIndex {
                        guard skins.indices.contains(skinIndex),j.allSatisfy({$0>=0 && Int($0)<inverseBinds[skinIndex].count}) else {throw SceneError.invalid("Rig joint out of range")}
                    }
                    return MetalBatches.Vertex(position:SIMD4(p[0],p[1],p[2],1),uv:SIMD4(uv[0],uv[1],0,0),normal:SIMD4(n[0],n[1],n[2],0),joints:SIMD4(UInt32(j[0]),UInt32(j[1]),UInt32(j[2]),UInt32(j[3])),weights:SIMD4(w[0],w[1],w[2],w[3]))
                }
                let mi=primitive["material"] as? Int ?? 0,material=materials.indices.contains(mi) ? materials[mi]:[:],pbr=material["pbrMetallicRoughness"] as? Obj ?? [:]
                let maps=try [texture(pbr["baseColorTexture"] as? Obj,batches.white),texture(material["normalTexture"] as? Obj,batches.normal,false),texture(pbr["metallicRoughnessTexture"] as? Obj,batches.white,false),texture(material["emissiveTexture"] as? Obj,batches.black)]
                let samplers=try [sampler(pbr["baseColorTexture"] as? Obj),sampler(material["normalTexture"] as? Obj),sampler(pbr["metallicRoughnessTexture"] as? Obj),sampler(material["emissiveTexture"] as? Obj)]
                output.append(Part(node:nodeIndex,skin:skinIndex,vertices:vertices,indices:indices,vertexBuffer:batches.buffer(vertices),indexBuffer:batches.buffer(indices),textures:maps,samplers:samplers,paint:definition.teamMask=="alpha" && (teamNames.isEmpty || teamNames.contains(material["name"] as? String ?? "")),replace:definition.raw["teamColorBlend"] as? String == "replace",opaque:material["alphaMode"] as? String != "BLEND"))
            }
        }
        parts=output
        var animations=[String:Clip]()
        for animation in doc["animations"] as? [Obj] ?? [] {
            let samplers=animation["samplers"] as? [Obj] ?? [];var tracks=[Track]();var duration:Float=0
            for channel in animation["channels"] as? [Obj] ?? [] {
                guard let target=channel["target"] as? Obj,let node=target["node"] as? Int,let path=target["path"] as? String,let si=channel["sampler"] as? Int,samplers.indices.contains(si) else {throw SceneError.invalid("Invalid rig channel")}
                let sampler=samplers[si],interpolation=sampler["interpolation"] as? String ?? "LINEAR"
                guard ["LINEAR","STEP"].contains(interpolation),["translation","rotation","scale"].contains(path) else {throw SceneError.invalid("Unsupported rig interpolation or channel")}
                let times=try values(Int(number(sampler,"input"))).map{$0[0]},output=try values(Int(number(sampler,"output")))
                guard times.count==output.count,nodes.indices.contains(node) else {throw SceneError.invalid("Invalid rig keyframes")}
                duration=max(duration,times.last ?? 0);tracks.append(Track(node:node,path:path,times:times,values:output,step:interpolation=="STEP"))
            }
            animations[animation["name"] as? String ?? ""]=Clip(tracks:tracks,duration:max(duration,0.001))
        }
        clips=animations
    }
    func sample(clip:String,time:Float,loop:Bool,turrets:[String:Float]=[:],batches:MetalBatches)->[PosedPart] {
        let key="\(clip)/\(time)/\(loop)/"+turrets.keys.sorted().map{"\($0)=\(turrets[$0]!)"}.joined(separator:",")
        if let cached=poses.get(key) {return cached}
        let transforms=transforms(clip:clip,time:time,loop:loop,turrets:turrets)
        var posed=[PosedPart]()
        for part in parts {
            let matrices:[simd_float4x4]
            if let skin=part.skin {let joints=skins[skin]["joints"] as? [Int] ?? [];matrices=joints.enumerated().map{transforms[$0.element]*inverseBinds[skin][$0.offset]}}
            else {matrices=[transforms[part.node]]}
            var points=[SIMD3<Float>]();points.reserveCapacity(part.vertices.count)
            var lower=SIMD3<Float>(repeating:.greatestFiniteMagnitude),upper=SIMD3<Float>(repeating: -.greatestFiniteMagnitude)
            for v in part.vertices {
                var p=SIMD4<Float>.zero
                for j in 0..<4 where v.weights[j]>0 {p += (matrices[Int(v.joints[j])]*v.position)*v.weights[j]}
                let xyz=SIMD3(p.x,p.y,p.z);points.append(xyz);lower=simd_min(lower,xyz);upper=simd_max(upper,xyz)
            }
            if simd_length(upper-lower)<0.00001 {continue}
            posed.append(PosedPart(part:part,bones:batches.buffer(matrices),points:points,lower:lower,upper:upper))
        }
        poses.insert(posed,key:key,bytes:posed.reduce(0){$0+$1.points.count*MemoryLayout<SIMD3<Float>>.stride+$1.bones.length})
        return posed
    }

    static func floats(_ value:Any?)->[Float]? {(value as? [NSNumber])?.map{$0.floatValue}}
    static func matrix(_ a:[Float])->simd_float4x4 {simd_float4x4(SIMD4(a[0],a[1],a[2],a[3]),SIMD4(a[4],a[5],a[6],a[7]),SIMD4(a[8],a[9],a[10],a[11]),SIMD4(a[12],a[13],a[14],a[15]))}
    func transforms(clip:String,time:Float,loop:Bool,turrets:[String:Float]=[:])->[simd_float4x4] {
        var translation=nodes.map{Self.floats($0["translation"]) ?? [0,0,0]},rotation=nodes.map{Self.floats($0["rotation"]) ?? [0,0,0,1]},scale=nodes.map{Self.floats($0["scale"]) ?? [1,1,1]}
        if let clip=clips[clip] {
            let t=loop ? max(0,time).truncatingRemainder(dividingBy:clip.duration):min(max(0,time),clip.duration)
            for track in clip.tracks {
                var low=0,high=track.times.count-1
                while low<high {let mid=(low+high+1)/2;if track.times[mid]<=t {low=mid}else {high=mid-1}}
                let next=min(low+1,track.times.count-1),a=track.values[low],b=track.values[next]
                let fraction=track.step || next==low ? 0:max(0,min(1,(t-track.times[low])/max(0.000001,track.times[next]-track.times[low])))
                var value=zip(a,b).map{$0+($1-$0)*fraction}
                if track.path=="rotation" {let q=simd_slerp(simd_quatf(vector:SIMD4(a[0],a[1],a[2],a[3])),simd_quatf(vector:SIMD4(b[0],b[1],b[2],b[3])),fraction).vector;value=[q.x,q.y,q.z,q.w]}
                switch track.path {case "translation":translation[track.node]=value;case "rotation":rotation[track.node]=value;case "scale":scale[track.node]=value;default:break}
            }
        }
        var result=Array(repeating:matrix_identity_float4x4,count:nodes.count),done=Set<Int>()
        func visit(_ i:Int)->simd_float4x4 {
            if done.contains(i) {return result[i]}
            var local:simd_float4x4
            if let matrix=Self.floats(nodes[i]["matrix"]) {local=Self.matrix(matrix)}
            else {let q=rotation[i];local=simd_float4x4(simd_quatf(vector:SIMD4(q[0],q[1],q[2],q[3])));local.columns.0 *= scale[i][0];local.columns.1 *= scale[i][1];local.columns.2 *= scale[i][2];local.columns.3=SIMD4(translation[i][0],translation[i][1],translation[i][2],1)}
            result[i]=parents[i]<0 ? local:visit(parents[i])*local
            if let name=nodes[i]["name"] as? String,let angle=turrets[name] {
                let turn=simd_float4x4(simd_quatf(angle:angle,axis:SIMD3(0,1,0)))
                for column in 0..<3 {result[i][column]=turn*result[i][column]}
            }
            done.insert(i);return result[i]
        }
        for i in nodes.indices {_=visit(i)};return result
    }
}

struct MetalRigDraw {
    let part:MetalRigAsset.Part,bones:MTLBuffer,transform:simd_float4x4,color:SIMD4<Float>,opacity:Float
    let center:SIMD3<Float>,radius:Float
}
final class MetalRigInstance {
    let model:ModelInstance,asset:MetalRigAsset
    var clip="",clipFrame=0.0,turretYaw:Float=0
    init(model:ModelInstance,asset:MetalRigAsset) {self.model=model;self.asset=asset}
    func pose(frame:Double,batches:MetalBatches,hidden:Bool)->([MetalRigDraw],[ScenePickMesh]) {
        if clip != model.playing {clip=model.playing;clipFrame=frame}
        let turrets=Dictionary(uniqueKeysWithValues:(model.definition.raw["turretNodes"] as? [String] ?? []).map{($0,turretYaw)})
        let sampled=asset.sample(clip:clip,time:Float(max(0,frame-clipFrame)/24),loop:!model.dying,turrets:turrets,batches:batches)
        if model.root.isHidden {return ([],[])}
        let world=model.root.simdWorldTransform*model.placement.simdTransform*model.rig.simdTransform
        let rgb=model.appliedTeamColor.count>=3 ? model.appliedTeamColor:[244,4,4]
        func linear(_ v:Int)->Float {let x=Float(v)/255;return x<=0.04045 ? x/12.92:pow((x+0.055)/1.055,2.4)}
        var draws=[MetalRigDraw](),picks=[ScenePickMesh]()
        for posed in sampled {
            let part=posed.part,lower=posed.lower,upper=posed.upper,radius=simd_length(upper-lower)/2
            let color=SIMD4(linear(rgb[0]),linear(rgb[1]),linear(rgb[2]),part.paint ? (part.replace ? 2:1):0)
            draws.append(MetalRigDraw(part:part,bones:posed.bones,transform:world,color:color,opacity:Float(model.root.opacity),center:(lower+upper)/2,radius:radius))
            picks.append(ScenePickMesh(target:.unit(model.root.name ?? ""),transform:world,vertices:posed.points,indices:part.indices,lower:lower,upper:upper))
        }
        return (hidden ? []:draws,picks)
    }
}

func metalRigProof()throws {
    guard let device=MTLCreateSystemDefaultDevice() else {throw SceneError.invalid("Metal device unavailable")}
    let folder=argument("--model-dir").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("models-3d")
    let library=try Models3D(folder:folder,catalogURL:argument("--catalog").map{URL(fileURLWithPath:$0)})
    let output=URL(fileURLWithPath:argument("--proof-dir") ?? runtime.appendingPathComponent("3d-proof/metal-rigs").path)
    try FileManager.default.createDirectory(at:output,withIntermediateDirectories:true)
    let kinds=argument("--model-types")?.split(separator:",").compactMap{Int($0)} ?? library.catalog.definitions.keys.sorted()
    let batches=try MetalBatches(device:device)
    let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgra8Unorm_srgb,width:512,height:512,mipmapped:false)
    descriptor.storageMode = .shared;descriptor.usage=[.renderTarget,.shaderRead]
    guard let texture=device.makeTexture(descriptor:descriptor) else {throw SceneError.invalid("Rig proof target allocation")}
    var revision=0,results=[Obj](),errors=[String]()
    for kind in kinds {
        do {
            let model=try library.make(kind);model.root.name="proof-\(kind)"
            let asset=try MetalRigAsset(url:library.catalog.url(model.definition),definition:model.definition,batches:batches),rig=MetalRigInstance(model:model,asset:asset)
            model.root.position.y=CGFloat(model.definition.elevation)
            model.teamColor(Models3D.playerColors[0])
            let camera=SCNNode();camera.camera=SCNCamera();camera.camera!.fieldOfView=45;camera.camera!.zNear=0.01;camera.camera!.zFar=100
            let height=Float(model.definition.targetHeight ?? 1)
            let framing=rig.pose(frame:0,batches:batches,hidden:false).1
            var frameLower=SIMD3<Float>(repeating:.greatestFiniteMagnitude),frameUpper=SIMD3<Float>(repeating: -.greatestFiniteMagnitude)
            for pick in framing {for p in pick.vertices {let v=pick.transform*SIMD4(p,1);frameLower=simd_min(frameLower,SIMD3(v.x,v.y,v.z));frameUpper=simd_max(frameUpper,SIMD3(v.x,v.y,v.z))}}
            let center=(frameLower+frameUpper)/2,extent=frameUpper-frameLower,span=max(extent.x,max(extent.y,extent.z))
            let heading=Float(argument("--proof-heading") ?? "0") ?? 0
            camera.simdPosition=center+SIMD3<Float>(sin(heading)*span*2.2,span*0.6,cos(heading)*span*2.2);camera.look(at:SCNVector3(center))
            var latestPicks=[ScenePickMesh]()
            func draw(_ name:String,_ frame:Double,possession:Bool=false)throws->[UInt8] {
                let pose=rig.pose(frame:frame,batches:batches,hidden:false);latestPicks=pose.1;revision+=1
                try batches.prepare(MetalBatches.Input(sceneRevision:revision,terrainRevision:0,terrainFogRevision:0,mapWidth:1,terrain:[:],billboards:[],overlays:[],objectCount:1,rigs:pose.0))
                batches.consume(ScenePacket(generation:revision,session:"rig-proof",batches:batches.batches,picks:pose.1,nativeFrame:frame,publishedAt:0,unitPositions:[:],diagnostics:["possessionActive":possession]))
                guard let command=batches.queue.makeCommandBuffer() else {throw SceneError.invalid("Rig proof command")}
                var right=camera.simdWorldTransform;right.columns.3.x+=0.064
                try batches.render(command:command,output:texture,camera:camera,eye:camera.simdWorldPosition,rightTransform:right,rightProjection:simd_float4x4(camera.camera!.projectionTransform))
                batches.copyEye(command:command,to:texture,eye:0);command.commit();command.waitUntilCompleted()
                guard command.status == .completed else {throw SceneError.invalid("Rig proof GPU failed: \(String(describing:command.error))")}
                var bytes=[UInt8](repeating:0,count:512*512*4);texture.getBytes(&bytes,bytesPerRow:2048,from:MTLRegionMake2D(0,0,512,512),mipmapLevel:0)
                let bitmap=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:512,pixelsHigh:512,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:2048,bitsPerPixel:32)!
                for i in stride(from:0,to:bytes.count,by:4) {bitmap.bitmapData![i]=bytes[i+2];bitmap.bitmapData![i+1]=bytes[i+1];bitmap.bitmapData![i+2]=bytes[i];bitmap.bitmapData![i+3]=255}
                guard let png=bitmap.representation(using:.png,properties:[:]) else {throw SceneError.invalid("Rig proof PNG")}
                try png.write(to:output.appendingPathComponent("\(kind)-\(name).png"));return bytes
            }
            let initial=try draw("idle",0)
            let outsideTransform=camera.simdTransform
            camera.simdPosition=center
            _=try draw("possession-inside",0,possession:true)
            let nearReport=batches.possessionReport
            guard batches.possessionNearCulled>0,batches.possessionNearSubmitted==0,!latestPicks.isEmpty else {throw SceneError.invalid("Possession eye intersects submitted rig geometry or loses picking")}
            camera.simdTransform=outsideTransform
            var colorImages=[[UInt8]]()
            for (i,color) in Models3D.playerColors.enumerated() {model.teamColor(color);colorImages.append(try draw("player-\(i+1)",0))}
            var minimumColorPixels=Int.max
            for i in 0..<8 {for j in (i+1)..<8 {
                let changed=stride(from:0,to:initial.count,by:4).filter {p in (0..<3).contains{abs(Int(colorImages[i][p+$0])-Int(colorImages[j][p+$0]))>8}}.count
                minimumColorPixels=min(minimumColorPixels,changed)
            }}
            let hasPaint=asset.parts.contains{$0.paint}
            guard !hasPaint || minimumColorPixels>100 else {throw SceneError.invalid("Metal team colors indistinguishable")}
            model.teamColor(Models3D.playerColors[0]);var turretPixels=0
            if !(model.definition.raw["turretNodes"] as? [String] ?? []).isEmpty {
                rig.turretYaw=0;let straight=try draw("turret-straight",0)
                rig.turretYaw=Float.pi/2;let turned=try draw("turret-turned",0)
                turretPixels=stride(from:0,to:straight.count,by:4).filter{p in (0..<3).contains{abs(Int(straight[p+$0])-Int(turned[p+$0]))>4}}.count
                guard turretPixels>100 else {throw SceneError.invalid("Metal turret did not turn")};rig.turretYaw=0
            }
            var clips=[Obj]()
            for action in model.definition.animations.keys.sorted() where model.clip(for:action) != nil {
                model.playAction(action,force:true,blendDuration:0,loop:action != "death");model.dying=action=="death";rig.clip=""
                let start=try draw("\(action)-0",0),duration=asset.clips[model.playing]?.duration ?? 1
                let end=try draw("\(action)-1",Double(duration)*24*0.61)
                let changed=stride(from:0,to:start.count,by:4).filter{p in (0..<3).contains{abs(Int(start[p+$0])-Int(end[p+$0]))>4}}.count
                if ["walk","attack","death","work"].contains(action),asset.clips[model.playing]?.hasMotion == true,changed==0 {throw SceneError.invalid("Metal animation did not change pixels: \(action)")}
                clips.append(["action":action,"clip":model.playing,"changedPixels":changed,"staticPose":asset.clips[model.playing]?.hasMotion == false,"duration":duration])
            }
            model.dying=false;model.playAction("idle",force:true);rig.clip="";_=try draw("idle-final",0)
            var pickCount=0
            let inverse=simd_inverse(simd_float4x4(camera.camera!.projectionTransform)*simd_inverse(camera.simdWorldTransform))
            for y in stride(from:64,through:448,by:48) {for x in stride(from:64,through:448,by:48) {
                let clip=SIMD4<Float>(Float(x)/256-1,1-Float(y)/256,1,1),far=inverse*clip
                let direction=simd_normalize(SIMD3(far.x,far.y,far.z)/far.w-camera.simdWorldPosition)
                if latestPicks.contains(where:{$0.hit(origin:camera.simdWorldPosition,direction:direction,limit:100) != nil}) {pickCount+=1}
            }}
            guard pickCount>0 else {throw SceneError.invalid("Metal rig cannot be picked")}
            var lower=SIMD3<Float>(repeating:.greatestFiniteMagnitude),upper=SIMD3<Float>(repeating: -.greatestFiniteMagnitude)
            for pick in latestPicks {for p in pick.vertices {let v=pick.transform*SIMD4(p,1);lower=simd_min(lower,SIMD3(v.x,v.y,v.z));upper=simd_max(upper,SIMD3(v.x,v.y,v.z))}}
            results.append(["type":kind,"passed":true,"modelRevision":model.revision,"modelSha256":SHA256.hash(data:try Data(contentsOf:library.catalog.url(model.definition))).map{String(format:"%02x",$0)}.joined(),"clips":clips,"pickSamples":pickCount,"possessionNearPlane":nearReport,"turretChangedPixels":turretPixels,"minimumTeamColorPixels":minimumColorPixels,"hasTeamMask":hasPaint,"worldBounds":[[lower.x,lower.y,lower.z],[upper.x,upper.y,upper.z]],"targetHeight":height])
        } catch {errors.append("\(kind): \(error)");results.append(["type":kind,"passed":false,"error":String(describing:error)])}
    }
    let report:Obj=["passed":errors.isEmpty,"models":results,"errors":errors,"textureResidency":TextureResidency.shared.report(),"renderer":"Metal GPU skinning; amplified stereo, left-eye readback"]
    let data=try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]);try data.write(to:output.appendingPathComponent("metal-rig-checks.json"));print(String(data:data,encoding:.utf8)!)
    if !errors.isEmpty {throw SceneError.invalid(errors.joined(separator:"; "))}
}
