import os.signpost
import Cocoa
import SceneKit
import MetalKit

// The worker owns its decoders and SceneKit construction objects. Only completed,
// immutable resources cross to the render thread; a cache miss never decodes there.
final class FrameStream {
    struct Prepared {
        let geometry:SCNGeometry?,material:SCNMaterial,image:MTLTexture,contact:Float
    }
    let queue=DispatchQueue(label:"soju.bw.hd-stream",qos:.userInitiated)
    private let lock=NSLock()
    private let resident=ResidentCache<Prepared>(limit:384*1024*1024)
    private var pending=Set<String>(),failed=Set<String>()
    private lazy var source=WorldSprites(streaming:false)
    private lazy var loader=MTKTextureLoader(device:source.relief.device)
    private func key(_ id:Int,_ frame:Int,_ color:[Int])->String {"\(id)/\(frame)/\(color)"}
    func get(_ id:Int,frame:Int,color:[Int])->Prepared? {
        os_signpost(.begin,log:XRFrameMetrics.log,name:"Stream lookup")
        defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Stream lookup")}
        let key=key(id,frame,color)
        lock.lock();defer{lock.unlock()}
        return resident.get(key)
    }
    func request(_ id:Int,frame:Int,color:[Int]) {
        let key=key(id,frame,color)
        lock.lock()
        guard resident.get(key)==nil,!pending.contains(key),!failed.contains(key),pending.count<64 else {lock.unlock();return}
        pending.insert(key);lock.unlock()
        queue.async { [self] in
            autoreleasepool {
                var result:Prepared?,cost=0
                do {
                    guard let image=source.texture(id,frame:frame,color:color),
                        let cg=image.cgImage(forProposedRect:nil,context:nil,hints:nil) else {throw SceneError.invalid("Missing sprite frame")}
                    let diffuse=MetalBatches.colorView(try loader.newTexture(cgImage:MetalBatches.colorImage(cg),options:[.SRGB:true,.generateMipmaps:true,.origin:MTKTextureLoader.Origin.topLeft]))
                    let mesh=source.relief.geometry(id,frame:frame,art:source)
                    let material=source.relief.material(id,frame:frame,color:color,art:source)
                    material.diffuse.contents=diffuse;cost+=MetalBatches.textureBytes(diffuse)
                    for name in ["normal","bright","specular","emissive","ao_depth"] {
                        guard let property=material.value(forKey:"relief_"+name) as? SCNMaterialProperty else {continue}
                        if let image=property.contents as? NSImage,let cg=image.cgImage(forProposedRect:nil,context:nil,hints:nil) {
                            property.contents=MetalBatches.colorView(try loader.newTexture(cgImage:MetalBatches.colorImage(cg),options:[.SRGB:true,.generateMipmaps:true,.origin:MTKTextureLoader.Origin.topLeft]))
                        }
                        if let texture=property.contents as? MTLTexture {cost+=MetalBatches.textureBytes(texture)}
                    }
                    if let mesh=mesh {
                        cost+=mesh.sources.reduce(0){$0+$1.data.count}+mesh.elements.reduce(0){$0+$1.data.count}
                    }
                    result=Prepared(geometry:mesh,material:material,image:diffuse,contact:source.relief.contactHeights["\(id)/\(frame)"] ?? 0)
                } catch {fputs("HD stream \(id)/\(frame): \(error)\n",stderr)}
                os_signpost(.begin,log:XRFrameMetrics.log,name:"Stream publish")
                defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Stream publish")}
                lock.lock()
                pending.remove(key)
                if let result=result,cost<=resident.limit {resident.insert(result,key:key,bytes:cost)}else {failed.insert(key)}
                lock.unlock()
            }
        }
    }
}
