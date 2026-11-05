import Cocoa
import Metal
import SceneKit
import simd

struct ScenePickHit {
    enum Target:Equatable {
        case unit(String),terrain,card(Int)
        func accepts(command:CommandTarget?)->Bool {
            switch self {
            case .card:return true
            case .unit:return command != .ground
            case .terrain:return command != .unit
            }
        }
    }
    let target:Target,position:SIMD3<Float>,distance:Float
    var worldCoordinates:SCNVector3 {SCNVector3(position)}
    var unitID:String? {if case .unit(let id)=target {return id};return nil}
    var name:String {switch target {case .unit(let id):return id;case .terrain:return "terrain";case .card(let index):return "card-\(index)"}}
}
struct ScenePickMesh {
    let target:ScenePickHit.Target,transform:simd_float4x4
    let vertices:[SIMD3<Float>],indices:[UInt32]
    let lower:SIMD3<Float>,upper:SIMD3<Float>
    func hit(origin:SIMD3<Float>,direction:SIMD3<Float>,limit:Float)->ScenePickHit? {
        guard indices.count>=3 else {return nil}
        let inverse=simd_inverse(transform)
        let p=inverse*SIMD4<Float>(origin,1),d=inverse*SIMD4<Float>(direction,0)
        let from=SIMD3<Float>(p.x,p.y,p.z),ray=SIMD3<Float>(d.x,d.y,d.z)
        var near:Float=0,far=limit
        for axis in 0..<3 {
            if abs(ray[axis])<0.000001 {if from[axis]<lower[axis] || from[axis]>upper[axis] {return nil}}
            else {
                let a=(lower[axis]-from[axis])/ray[axis],b=(upper[axis]-from[axis])/ray[axis]
                near=max(near,min(a,b));far=min(far,max(a,b));if near>far {return nil}
            }
        }
        var closest=limit,found=false
        for i in stride(from:0,to:indices.count-2,by:3) {
            let a=vertices[Int(indices[i])],b=vertices[Int(indices[i+1])],c=vertices[Int(indices[i+2])]
            let e1=b-a,e2=c-a,h=simd_cross(ray,e2),det=simd_dot(e1,h)
            if abs(det)<0.000001 {continue}
            let inv=1/det,s=from-a,u=inv*simd_dot(s,h)
            if u<0 || u>1 {continue}
            let q=simd_cross(s,e1),v=inv*simd_dot(ray,q)
            if v<0 || u+v>1 {continue}
            let distance=inv*simd_dot(e2,q)
            if distance>=0 && distance<closest {closest=distance;found=true}
        }
        return found ? ScenePickHit(target:target,position:origin+direction*closest,distance:closest):nil
    }
}
struct SceneTiming {
    var native=0.0,bridgeStarted=0.0,bridgeRead=0.0,workerStarted=0.0,published=0.0,predictedDisplay=0.0
}
struct ScenePacket {
    let generation:Int,session:String,batches:[MetalBatches.PacketBatch],picks:[ScenePickMesh]
    let nativeFrame:Double,publishedAt:Double
    var timing=SceneTiming()
    let unitPositions:[String:SIMD3<Float>],diagnostics:Obj
    func hit(origin:SIMD3<Float>,direction:SIMD3<Float>,accept:(ScenePickHit.Target)->Bool)->ScenePickHit? {
        var nearest:ScenePickHit?
        for mesh in picks where accept(mesh.target) {
            if let hit=mesh.hit(origin:origin,direction:direction,limit:nearest?.distance ?? 1000) {nearest=hit}
        }
        return nearest
    }
}
// One pending publication, one consumer-held packet, and three in-flight GPU
// slots. Replacing a publication never mutates a packet retained by a consumer.
final class SceneMailbox {
    private let lock=NSLock()
    private var packet:ScenePacket?,session=""
    private var displayed:ScenePacket?,protected:ScenePacket?
    private var failure=""
    func invalidate(_ next:String) {lock.lock();if session != next {protected=displayed;session=next;packet=nil};lock.unlock()}
    func publish(_ next:ScenePacket) {
        lock.lock()
        if next.session==session {protected=displayed;packet=next;failure=""}
        lock.unlock()
    }
    // Release the consumer's previous handle while its mailbox owner is still
    // retained under the same lock. Only publication retires that owner.
    func acquire(into destination:inout ScenePacket?) {
        lock.lock();destination=packet;displayed=packet;lock.unlock()
    }
    func presented()->ScenePacket? {lock.lock();defer{lock.unlock()};return displayed?.session==session ? displayed:nil}
    func fail(_ message:String) {lock.lock();failure=message;lock.unlock()}
    func error()->String {lock.lock();defer{lock.unlock()};return failure}
    func latest()->ScenePacket? {lock.lock();defer{lock.unlock()};return packet}
}

struct SceneView {
    let session:String
    let viewport:SIMD2<Float>
    let camera:MetalBatches.CameraPacket,center:SIMD2<Int>,radius:Int,selected:Set<String>
    let overlays:[SceneOverlay]
    var hiddenUnit:String?=nil
    func equivalent(to other:SceneView)->Bool {
        hiddenUnit==other.hiddenUnit && session==other.session && viewport==other.viewport && center==other.center && radius==other.radius && selected==other.selected &&
        camera.orientation.vector==other.camera.orientation.vector && camera.orthographic==other.camera.orthographic && simd_equal(camera.transform,other.camera.transform) && simd_equal(camera.projection,other.camera.projection) &&
        overlays.count==other.overlays.count && zip(overlays,other.overlays).allSatisfy{$0.equivalent(to:$1)}
    }
}
// UI construction nodes are read only by the main owner. Their immutable scalar
// geometry and image snapshots are rebuilt into private nodes by the worker.
struct SceneOverlay {
    static func tint(_ contents:Any?)->SIMD4<Float> {
        guard let color=(contents as? NSColor)?.usingColorSpace(.sRGB) else {return SIMD4<Float>(repeating:1)}
        return SIMD4<Float>(Float(color.redComponent),Float(color.greenComponent),Float(color.blueComponent),Float(color.alphaComponent))
    }
    let transform:simd_float4x4,vertices:[SCNVector3],uv:[CGPoint],indices:[Int32]
    let image:CGImage?,color:SIMD4<Float>,opacity:Float,depth:Bool,order:Int,line:Bool,name:String?
    func sameContents(as other:SceneOverlay)->Bool {
        image === other.image && color==other.color && opacity==other.opacity && depth==other.depth && order==other.order && line==other.line && name==other.name &&
        uv==other.uv && indices==other.indices && vertices.count==other.vertices.count &&
        zip(vertices,other.vertices).allSatisfy{$0.x==$1.x && $0.y==$1.y && $0.z==$1.z}
    }
    func equivalent(to other:SceneOverlay)->Bool {
        sameContents(as:other) && simd_equal(transform,other.transform)
    }
    func node()->SCNNode {
        var sources=[SCNGeometrySource(vertices:vertices)]
        if !uv.isEmpty {sources.append(SCNGeometrySource(textureCoordinates:uv))}
        let geometry=SCNGeometry(sources:sources,elements:[SCNGeometryElement(indices:indices,primitiveType:line ? .line:.triangles)])
        let material=SCNMaterial();material.lightingModel = .constant;material.isDoubleSided=true
        material.diffuse.contents=image.map{NSImage(cgImage:$0,size:.zero)} ?? NSColor(srgbRed:CGFloat(color.x),green:CGFloat(color.y),blue:CGFloat(color.z),alpha:CGFloat(color.w)) as Any
        material.readsFromDepthBuffer=depth;material.writesToDepthBuffer=false;geometry.materials=[material]
        let node=SCNNode(geometry:geometry);node.simdTransform=transform;node.opacity=CGFloat(opacity);node.renderingOrder=order;node.name=name
        return node
    }
}

// Worker-owned nodes retain image identity while tracked poses change. Packets
// still capture immutable geometry and transforms before the next mutation.
final class SceneOverlayCache {
    private var entries=[(SceneOverlay,SCNNode)]()
    func nodes(_ overlays:[SceneOverlay])->[SCNNode] {
        let next=overlays.enumerated().map {index,overlay -> (SceneOverlay,SCNNode) in
            let node=entries.indices.contains(index) && entries[index].0.sameContents(as:overlay) ? entries[index].1:overlay.node()
            node.simdTransform=overlay.transform
            return (overlay,node)
        }
        entries=next
        return next.map{$0.1}
    }
}
