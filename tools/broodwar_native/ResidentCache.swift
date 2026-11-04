import Foundation

// Eviction drops cache ownership only; a draw's referenced resources stay alive.
// Byte limits cover cached resources, not resources pinned by active scene nodes.
final class ResidentCache<Value> {
    private struct Entry {var value:Value;var bytes:Int;var age:UInt64}
    private var entries=[String:Entry](),clock:UInt64=0
    let limit:Int
    private(set) var bytes=0
    private(set) var evictions=0
    init(limit:Int) {self.limit=max(0,limit)}
    func get(_ key:String)->Value? {
        guard var entry=entries[key] else {return nil}
        clock &+= 1;entry.age=clock;entries[key]=entry;return entry.value
    }
    func insert(_ value:Value,key:String,bytes cost:Int) {
        if let old=entries.removeValue(forKey:key) {bytes-=old.bytes}
        let cost=max(0,cost)
        guard cost<=limit else {return}
        while bytes+cost>limit,let oldest=entries.min(by:{$0.value.age<$1.value.age}) {
            bytes-=oldest.value.bytes;entries.removeValue(forKey:oldest.key);evictions+=1
        }
        clock &+= 1;entries[key]=Entry(value:value,bytes:cost,age:clock);bytes+=cost
    }
}

import MetalKit
import ObjectiveC

// A lease follows the texture's lifetime, including scene, cache and batch owners.
final class TextureResidency {
    static let shared=TextureResidency(limit:min(65536,max(64,Int(ProcessInfo.processInfo.environment["SOJU_BW_TEXTURE_MIB"] ?? "1536") ?? 1536))*1024*1024)
    private let lock=NSLock()
    private var used=0,peak=0,rejected=0
    let limit:Int
    init(limit:Int) {self.limit=max(0,limit)}
    func reserve(_ bytes:Int)->Bool {
        lock.lock();defer{lock.unlock()}
        guard bytes>=0,bytes<=limit-used else {rejected+=1;return false}
        used+=bytes;peak=max(peak,used);return true
    }
    func release(_ bytes:Int) {lock.lock();used-=bytes;lock.unlock()}
    func report()->Obj {
        lock.lock();defer{lock.unlock()}
        return ["limitBytes":limit,"residentBytes":used,"peakBytes":peak,"rejections":rejected,
                "scope":"Renderer-owned Metal texture storage; runtime swapchains and framework-private resources are external"]
    }
    private final class Lease {
        let owner:TextureResidency,bytes:Int
        init(_ owner:TextureResidency,_ bytes:Int) {self.owner=owner;self.bytes=bytes}
        deinit {owner.release(bytes)}
    }
    private static var association:UInt8=0
    private func allocate(_ device:MTLDevice,_ descriptor:MTLTextureDescriptor,_ create:()->MTLTexture?)->MTLTexture? {
        let size=device.heapTextureSizeAndAlign(descriptor:descriptor).size
        guard reserve(size) else {return nil}
        guard let texture=create() else {release(size);return nil}
        let actual=max(size,texture.allocatedSize)
        if actual>size,!reserve(actual-size) {release(size);return nil}
        objc_setAssociatedObject(texture,&Self.association,Lease(self,actual),.OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return texture
    }
    func make(_ device:MTLDevice,_ descriptor:MTLTextureDescriptor)->MTLTexture? {
        allocate(device,descriptor){device.makeTexture(descriptor:descriptor)}
    }
    func load(_ loader:MTKTextureLoader,_ image:CGImage,options:[MTKTextureLoader.Option:Any])throws->MTLTexture {
        let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm_srgb,width:image.width,height:image.height,mipmapped:true)
        descriptor.storageMode = .private;descriptor.usage = .shaderRead
        var failure:Error?
        let texture=allocate(loader.device,descriptor) {
            do {return try loader.newTexture(cgImage:image,options:options)}
            catch {failure=error;return nil}
        }
        if let failure=failure {throw failure}
        guard let texture=texture else {throw SceneError.invalid("Texture residency limit reached")}
        return texture
    }
}
