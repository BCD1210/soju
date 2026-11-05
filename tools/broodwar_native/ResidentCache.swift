import Foundation

// Eviction drops cache ownership only; a draw's referenced resources stay alive.
// Byte limits cover cached resources, not resources pinned by active scene nodes.
final class ResidentCache<Value> {
    private final class Entry {
        let key:String,value:Value,bytes:Int
        weak var previous:Entry?
        var next:Entry?
        init(key:String,value:Value,bytes:Int) {self.key=key;self.value=value;self.bytes=bytes}
    }
    private var entries=[String:Entry]()
    private var oldest:Entry?,newest:Entry?
    let limit:Int
    private(set) var bytes=0
    private(set) var evictions=0
    init(limit:Int) {self.limit=max(0,limit)}
    deinit {while let entry=oldest {detach(entry)}}
    private func detach(_ entry:Entry) {
        if let previous=entry.previous {previous.next=entry.next}else {oldest=entry.next}
        if let next=entry.next {next.previous=entry.previous}else {newest=entry.previous}
        entry.previous=nil;entry.next=nil
    }
    private func append(_ entry:Entry) {
        entry.previous=newest;newest?.next=entry
        if oldest==nil {oldest=entry};newest=entry
    }
    func get(_ key:String)->Value? {
        guard let entry=entries[key] else {return nil}
        if newest !== entry {detach(entry);append(entry)}
        return entry.value
    }
    @discardableResult func trim(to target:Int)->Int {
        let before=bytes
        while bytes>max(0,target),let old=oldest {
            detach(old);entries.removeValue(forKey:old.key);bytes-=old.bytes;evictions+=1
        }
        return before-bytes
    }
    func insert(_ value:Value,key:String,bytes cost:Int) {
        if let old=entries.removeValue(forKey:key) {detach(old);bytes-=old.bytes}
        let cost=max(0,cost)
        guard cost<=limit else {return}
        while bytes+cost>limit,let old=oldest {
            detach(old);entries.removeValue(forKey:old.key);bytes-=old.bytes;evictions+=1
        }
        let entry=Entry(key:key,value:value,bytes:cost)
        entries[key]=entry;append(entry);bytes+=cost
    }
}

import MetalKit
import ObjectiveC

// A lease follows the texture's lifetime, including scene, cache and batch owners.
final class TextureResidency {
    static let shared=TextureResidency(limit:min(65536,max(64,Int(ProcessInfo.processInfo.environment["SOJU_BW_TEXTURE_MIB"] ?? "1536") ?? 1536))*1024*1024)
    private let lock=NSLock()
    private var used=0,peak=0,rejected=0
    private var owners=[String:Int](),ownerPeaks=[String:Int]()
    let limit:Int
    init(limit:Int) {self.limit=max(0,limit)}
    func reserve(_ bytes:Int,owner:String="other")->Bool {
        lock.lock();defer{lock.unlock()}
        guard bytes>=0,bytes<=limit-used else {rejected+=1;return false}
        used+=bytes;peak=max(peak,used)
        owners[owner,default:0]+=bytes;ownerPeaks[owner]=max(ownerPeaks[owner,default:0],owners[owner,default:0]);return true
    }
    func release(_ bytes:Int,owner:String="other") {lock.lock();used-=bytes;owners[owner,default:0]-=bytes;lock.unlock()}
    func report()->Obj {
        lock.lock();defer{lock.unlock()}
        return ["limitBytes":limit,"residentBytes":used,"peakBytes":peak,"rejections":rejected,
                "owners":owners,"ownerPeakBytes":ownerPeaks,
                "scope":"Renderer-owned Metal texture storage; runtime swapchains and framework-private resources are external"]
    }
    private final class Lease {
        let owner:TextureResidency,bytes:Int,category:String
        init(_ owner:TextureResidency,_ bytes:Int,_ category:String) {self.owner=owner;self.bytes=bytes;self.category=category}
        deinit {owner.release(bytes,owner:category)}
    }
    private static var association:UInt8=0
    private func allocate(_ device:MTLDevice,_ descriptor:MTLTextureDescriptor,owner:String,_ create:()->MTLTexture?)->MTLTexture? {
        let size=device.heapTextureSizeAndAlign(descriptor:descriptor).size
        guard reserve(size,owner:owner) else {return nil}
        guard let texture=create() else {release(size,owner:owner);return nil}
        let actual=max(size,texture.allocatedSize)
        if actual>size,!reserve(actual-size,owner:owner) {release(size,owner:owner);return nil}
        objc_setAssociatedObject(texture,&Self.association,Lease(self,actual,owner),.OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return texture
    }
    func make(_ device:MTLDevice,_ descriptor:MTLTextureDescriptor,owner:String="other")->MTLTexture? {
        allocate(device,descriptor,owner:owner){device.makeTexture(descriptor:descriptor)}
    }
    func load(_ loader:MTKTextureLoader,_ image:CGImage,options:[MTKTextureLoader.Option:Any],owner:String="other")throws->MTLTexture {
        let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm_srgb,width:image.width,height:image.height,mipmapped:true)
        descriptor.storageMode = .private;descriptor.usage = .shaderRead
        var failure:Error?
        let texture=allocate(loader.device,descriptor,owner:owner) {
            do {return try loader.newTexture(cgImage:image,options:options)}
            catch {failure=error;return nil}
        }
        if let failure=failure {throw failure}
        guard let texture=texture else {throw SceneError.invalid("Texture residency limit reached")}
        return texture
    }
}
