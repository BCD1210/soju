import os.signpost
import Cocoa
import SceneKit
import MetalKit

// The worker owns its decoders and SceneKit construction objects. Only completed,
// immutable resources cross to the render thread; a cache miss never decodes there.
final class FrameStream {
    final class Material {
        enum Map {
            case texture(MTLTexture)
            case color(SIMD4<Float>)
            init(_ contents:Any?) {
                if let texture=contents as? MTLTexture {self = .texture(texture)}
                else {
                    let color=(contents as? NSColor)?.usingColorSpace(.sRGB) ?? NSColor(srgbRed:0,green:0,blue:0,alpha:1)
                    self = .color(SIMD4<Float>(Float(color.redComponent),Float(color.greenComponent),Float(color.blueComponent),Float(color.alphaComponent)))
                }
            }
            var contents:Any {
                switch self {
                case .texture(let texture):return texture
                case .color(let color):return NSColor(srgbRed:CGFloat(color.x),green:CGFloat(color.y),blue:CGFloat(color.z),alpha:CGFloat(color.w))
                }
            }
        }
        let diffuse:MTLTexture,maps:[String:Map],shaders:[SCNShaderModifierEntryPoint:String]?
        private var sharedMetal:SCNMaterial?
        init(_ material:SCNMaterial,diffuse:MTLTexture) {
            self.diffuse=diffuse;shaders=material.shaderModifiers
            maps=Dictionary(uniqueKeysWithValues:["normal","bright","specular","emissive","ao_depth"].map {
                ($0,Map((material.value(forKey:"relief_"+$0) as? SCNMaterialProperty)?.contents))
            })
        }
        // Only the stream's construction owner requests this shared value.
        // Metal reads it without mutation; fallback shaders keep private copies.
        func material(sharedForMetal:Bool=false)->SCNMaterial {
            let share=sharedForMetal && useMetalBatches
            if share,let material=sharedMetal {return material}
            let material=SCNMaterial();material.lightingModel = .constant;material.isDoubleSided=true
            material.diffuse.contents=diffuse;material.readsFromDepthBuffer=true;material.writesToDepthBuffer=true
            material.transparencyMode = .aOne
            if !useMetalBatches {material.shaderModifiers=shaders}
            for (name,map) in maps {
                let property=SCNMaterialProperty(contents:map.contents);property.wrapS = .clamp;property.wrapT = .clamp
                material.setValue(property,forKey:"relief_"+name)
            }
            material.setValue(NSValue(scnVector3:SCNVector3(SpriteRelief.reference)),forKey:"reliefLight")
            material.setValue(NSValue(scnVector3:SCNVector3(0,0,1)),forKey:"reliefEye")
            if share {sharedMetal=material}
            return material
        }
    }
    struct Prepared {
        let geometry:SpriteRelief.Mesh?,material:Material,image:MTLTexture,contact:Float
    }
    let queue=DispatchQueue(label:"soju.bw.hd-stream",qos:.userInitiated)
    private let lock=NSLock()
    private let resident=ResidentCache<Prepared>(limit:192*1024*1024)
    private var pending=Set<String>(),failed=Set<String>()
    var onReady:(()->Void)?
    private lazy var source=WorldSprites(streaming:false)
    private lazy var loader=MTKTextureLoader(device:source.relief.device)
    private func key(_ id:Int,_ frame:Int,_ color:[Int])->String {"\(id)/\(frame)/\(WorldSprites.colorKey(color))"}
    func get(_ id:Int,frame:Int,color:[Int])->Prepared? {
        os_signpost(.begin,log:XRFrameMetrics.log,name:"Stream lookup")
        defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Stream lookup")}
        let key=key(id,frame,color)
        lock.lock();defer{lock.unlock()}
        return resident.get(key)
    }
    func trim(to bytes:Int) {
        lock.lock();resident.trim(to:bytes);lock.unlock()
    }
    func request(_ id:Int,frame:Int,color:[Int]) {
        let key=key(id,frame,color)
        lock.lock()
        guard resident.get(key)==nil,!pending.contains(key),!failed.contains(key),pending.count<64 else {lock.unlock();return}
        pending.insert(key);lock.unlock()
        queue.async { [self] in
            autoreleasepool {
                var result:Prepared?,cost=0
                if number(TextureResidency.shared.report(),"residentBytes")>Double(TextureResidency.shared.limit)*0.70 {
                    trim(to:96*1024*1024)
                    source.relief.materials.trim(to:0)
                    source.relief.dataTextures.trim(to:16*1024*1024)
                }
                do {
                    guard let image=source.texture(id,frame:frame,color:color),
                        let cg=image.cgImage(forProposedRect:nil,context:nil,hints:nil) else {throw SceneError.invalid("Missing sprite frame")}
                    let diffuse=MetalBatches.colorView(try TextureResidency.shared.load(loader,MetalBatches.colorImage(cg),options:[.SRGB:true,.generateMipmaps:true,.origin:MTKTextureLoader.Origin.topLeft],owner:"unit-model"))
                    let mesh=source.relief.meshData(id,frame:frame,art:source)
                    let material=source.relief.material(id,frame:frame,color:color,art:source)
                    material.diffuse.contents=diffuse;cost+=MetalBatches.textureBytes(diffuse)
                    for name in ["normal","bright","specular","emissive","ao_depth"] {
                        guard let property=material.value(forKey:"relief_"+name) as? SCNMaterialProperty else {continue}
                        if let image=property.contents as? NSImage,let cg=image.cgImage(forProposedRect:nil,context:nil,hints:nil) {
                            property.contents=MetalBatches.colorView(try TextureResidency.shared.load(loader,MetalBatches.colorImage(cg),options:[.SRGB:true,.generateMipmaps:true,.origin:MTKTextureLoader.Origin.topLeft],owner:"unit-model"))
                        }
                        if let texture=property.contents as? MTLTexture {cost+=MetalBatches.textureBytes(texture)}
                    }
                    if let mesh=mesh {
                        cost+=mesh.bytes
                    }
                    result=Prepared(geometry:mesh,material:Material(material,diffuse:diffuse),image:diffuse,contact:source.relief.contactHeights["\(id)/\(frame)"] ?? 0)
                } catch {fputs("HD stream \(id)/\(frame): \(error)\n",stderr)}
                os_signpost(.begin,log:XRFrameMetrics.log,name:"Stream publish")
                defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Stream publish")}
                lock.lock()
                pending.remove(key)
                if let result=result,cost<=resident.limit {resident.insert(result,key:key,bytes:cost)}else {failed.insert(key)}
                lock.unlock()
                if result != nil {onReady?()}
            }
        }
    }
}
