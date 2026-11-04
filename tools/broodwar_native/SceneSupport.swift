import Cocoa
import SceneKit
import Metal
import QuartzCore
import CoreImage
import Darwin
import simd
typealias Obj = [String: Any]
func number(_ o: Obj, _ k: String) -> Double { (o[k] as? NSNumber)?.doubleValue ?? 0 }
let runtime = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".local/share/soju/broodwar-native")
let assets = runtime.appendingPathComponent("scene-assets")

enum SceneError: Error { case invalid(String) }
func loadObject(_ url: URL) throws -> Obj {
    guard let o = try JSONSerialization.jsonObject(with: Data(contentsOf:url)) as? Obj else { throw SceneError.invalid("JSON object") }
    return o
}
func u32(_ b: [UInt8], _ p: Int) -> UInt32 {
    UInt32(b[p]) | UInt32(b[p+1]) << 8 | UInt32(b[p+2]) << 16 | UInt32(b[p+3]) << 24
}
// A snapshot is applied only after every bounded chunk of the same response arrives.
struct SnapshotWire {
    var parts=[Int:[UInt8]]();var identity:[UInt32]?
    mutating func accept(_ bytes:[UInt8]) throws -> Data? {
        if !bytes.starts(with:[83,66,87,49]) {
            guard identity==nil else { throw SceneError.invalid("Mixed snapshot response") }
            return Data(bytes)
        }
        guard bytes.count>=16 else { throw SceneError.invalid("Short snapshot chunk") }
        let response=u32(bytes,4),index=Int(bytes[8])|Int(bytes[9])<<8
        let count=Int(bytes[10])|Int(bytes[11])<<8,total=Int(u32(bytes,12)),chunk=8176
        guard total>0,total<=2*1024*1024,count==(total+chunk-1)/chunk,index<count else { throw SceneError.invalid("Invalid snapshot bounds") }
        let current=[response,UInt32(count),UInt32(total)]
        if identity==nil { identity=current }
        guard identity==current,parts[index]==nil,bytes.count-16==min(chunk,total-index*chunk) else { throw SceneError.invalid("Mixed, duplicate or malformed snapshot chunk") }
        parts[index]=Array(bytes.dropFirst(16))
        if parts.count==count { return Data((0..<count).flatMap{parts[$0]!}) }
        return nil
    }
}
final class Bridge {
    let queue = DispatchQueue(label:"soju.bw.native.state")
    func request(_ payload: Obj) throws -> Obj {
        var o=payload
        o["token"]=try String(contentsOf:runtime.appendingPathComponent("session-token"),encoding:.utf8)
        if o["op"] as? String != "state" && o["op"] as? String != "scene" && o["op"] as? String != "terrain" {
            o["seq"]=Int64(Date().timeIntervalSince1970*1_000_000)
        }
        let fd=socket(AF_INET,SOCK_DGRAM,IPPROTO_UDP)
        guard fd>=0 else { throw SceneError.invalid("socket") };defer { close(fd) }
        var receiveSize:Int32=1024*1024
        setsockopt(fd,SOL_SOCKET,SO_RCVBUF,&receiveSize,socklen_t(MemoryLayout<Int32>.size))
        var timeout=timeval(tv_sec:1,tv_usec:0)
        setsockopt(fd,SOL_SOCKET,SO_RCVTIMEO,&timeout,socklen_t(MemoryLayout.size(ofValue:timeout)))
        var address=sockaddr_in()
        address.sin_len=UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family=sa_family_t(AF_INET);address.sin_port=UInt16(8795).bigEndian
        address.sin_addr=in_addr(s_addr:inet_addr("127.0.0.1"))
        let connected=withUnsafePointer(to:&address) {
            $0.withMemoryRebound(to:sockaddr.self,capacity:1) { Darwin.connect(fd,$0,socklen_t(MemoryLayout<sockaddr_in>.size)) }
        }
        guard connected==0 else { throw SceneError.invalid("connect") }
        let data=try JSONSerialization.data(withJSONObject:o)
        let sent=data.withUnsafeBytes { Darwin.send(fd,$0.baseAddress,data.count,0) }
        guard sent==data.count else { throw SceneError.invalid("send") }
        var buffer=[UInt8](repeating:0,count:65535)
        var wire=SnapshotWire();var complete:Data?
        let deadline=CACurrentMediaTime()+1.5
        while complete==nil && CACurrentMediaTime()<deadline {
            let n=buffer.withUnsafeMutableBytes { recv(fd,$0.baseAddress,$0.count,0) }
            guard n>0 else { throw SceneError.invalid("Game is paused, in menus, or snapshot is incomplete") }
            complete=try wire.accept(Array(buffer.prefix(n)))
        }
        guard let data=complete,let result=try JSONSerialization.jsonObject(with:data) as? Obj else { throw SceneError.invalid("Incomplete snapshot") }
        guard result["ok"] as? Bool == true else { throw SceneError.invalid(result["error"] as? String ?? "Rejected") }
        return result
    }
}
final class Art {
    let device: MTLDevice
    let manifest: Obj
    var textures=[String:MTLTexture]()
    init(device preferred:MTLDevice? = nil) throws {
        guard let d=preferred ?? MTLCreateSystemDefaultDevice(),d.supportsBCTextureCompression else { throw SceneError.invalid("Metal BC texture support required") }
        device=d;manifest=try loadObject(assets.appendingPathComponent("manifest.json"))
    }
    func texture(_ name:String) throws -> MTLTexture {
        if let t=textures[name] { return t }
        let b=[UInt8](try Data(contentsOf:assets.appendingPathComponent(name)))
        guard b.count>=128,Array(b[0..<4])==[68,68,83,32] else { throw SceneError.invalid("Unsupported texture "+name) }
        let w=Int(u32(b,16)),h=Int(u32(b,12)),four=String(bytes:b[84..<88],encoding:.ascii) ?? ""
        let block:Int
        switch four {
        case "DXT1":block=8
        case "DXT5":block=16
        default:throw SceneError.invalid("Unsupported DDS encoding "+four)
        }
        let row=((w+3)/4)*block,size=row*((h+3)/4)
        guard w>0,h>0,w<=16384,h<=16384,b.count>=128+size else { throw SceneError.invalid("Invalid DDS bounds") }
        // Decode BC tiles once so Metal can build a complete filtered RGBA mip chain.
        var rgba=[UInt8](repeating:0,count:w*h*4)
        func rgb565(_ n:Int)->[Int] {[(n>>11)*255/31,((n>>5)&63)*255/63,(n&31)*255/31]}
        for by in 0..<((h+3)/4) { for bx in 0..<((w+3)/4) {
            let offset=128+(by*((w+3)/4)+bx)*block,co=offset+(block==16 ? 8:0)
            let c0=Int(b[co])|Int(b[co+1])<<8,c1=Int(b[co+2])|Int(b[co+3])<<8
            let first=rgb565(c0),second=rgb565(c1)
            var colors=[first,second,[Int](repeating:0,count:3),[Int](repeating:0,count:3)]
            for c in 0..<3 {
                colors[2][c]=c0>c1 || block==16 ? (2*first[c]+second[c])/3:(first[c]+second[c])/2
                colors[3][c]=c0>c1 || block==16 ? (first[c]+2*second[c])/3:0
            }
            var alpha=[255,255,255,255,255,255,255,255],alphaBits:UInt64=0
            if block==16 {
                alpha[0]=Int(b[offset]);alpha[1]=Int(b[offset+1])
                if alpha[0]>alpha[1] {for i in 2...7 {alpha[i]=((8-i)*alpha[0]+(i-1)*alpha[1])/7}}
                else {for i in 2...5 {alpha[i]=((6-i)*alpha[0]+(i-1)*alpha[1])/5};alpha[6]=0;alpha[7]=255}
                for i in 0..<6 {alphaBits |= UInt64(b[offset+2+i]) << (8*i)}
            }
            let colorBits=u32(b,co+4)
            for py in 0..<4 {for px in 0..<4 {
                let x=bx*4+px,y=by*4+py
                if x>=w || y>=h {continue}
                let pixel=py*4+px,ci=Int((colorBits >> (2*pixel))&3),dst=(y*w+x)*4
                for c in 0..<3 {rgba[dst+c]=UInt8(colors[ci][c])}
                rgba[dst+3]=block==16 ? UInt8(alpha[Int((alphaBits >> (3*pixel))&7)]):(c0<=c1 && ci==3 ? 0:255)
            }}
        }}
        let desc=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm_srgb,width:w,height:h,mipmapped:true)
        desc.usage = .shaderRead;desc.storageMode = .shared
        guard let t=device.makeTexture(descriptor:desc),let queue=device.makeCommandQueue(),let command=queue.makeCommandBuffer(),let blit=command.makeBlitCommandEncoder() else { throw SceneError.invalid("Metal texture allocation") }
        rgba.withUnsafeBytes {t.replace(region:MTLRegionMake2D(0,0,w,h),mipmapLevel:0,withBytes:$0.baseAddress!,bytesPerRow:w*4)}
        blit.generateMipmaps(for:t);blit.endEncoding();command.commit();command.waitUntilCompleted()
        textures[name]=t;return t
    }
    // Filter across the actual map neighbors instead of repeating each tile's
    // edge texel. The gutter participates in mip generation at oblique angles.
    func terrainMaterial(_ files:[String]) throws -> SCNMaterial {
        guard files.count==9 else {throw SceneError.invalid("Terrain neighborhood")}
        let key="neighbors:"+files.joined(separator:"|")
        let center=try texture(files[4]),gutter=8
        let width=center.width,height=center.height,paddedWidth=width+gutter*2,paddedHeight=height+gutter*2
        let padded:MTLTexture
        if let cached=textures[key] {padded=cached}
        else {
            var pixels=[UInt8](repeating:0,count:paddedWidth*paddedHeight*4)
            for row in 0..<3 {for column in 0..<3 {
                let tile=try texture(files[row*3+column])
                guard tile.width==width && tile.height==height else {throw SceneError.invalid("Mixed terrain texture sizes")}
                let copyWidth=column==1 ? width:gutter,copyHeight=row==1 ? height:gutter
                let sx=column==0 ? width-gutter:0,sy=row==0 ? height-gutter:0
                let dx=column==0 ? 0:column==1 ? gutter:gutter+width
                let dy=row==0 ? 0:row==1 ? gutter:gutter+height
                pixels.withUnsafeMutableBytes {buffer in
                    tile.getBytes(buffer.baseAddress!.advanced(by:(dy*paddedWidth+dx)*4),bytesPerRow:paddedWidth*4,
                        from:MTLRegionMake2D(sx,sy,copyWidth,copyHeight),mipmapLevel:0)
                }
            }}
            let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm_srgb,width:paddedWidth,height:paddedHeight,mipmapped:true)
            descriptor.storageMode = .shared;descriptor.usage = .shaderRead
            guard let image=device.makeTexture(descriptor:descriptor),let queue=device.makeCommandQueue(),let command=queue.makeCommandBuffer(),let blit=command.makeBlitCommandEncoder() else {throw SceneError.invalid("Terrain gutter allocation")}
            pixels.withUnsafeBytes {image.replace(region:MTLRegionMake2D(0,0,paddedWidth,paddedHeight),mipmapLevel:0,withBytes:$0.baseAddress!,bytesPerRow:paddedWidth*4)}
            blit.generateMipmaps(for:image);blit.endEncoding();command.commit();command.waitUntilCompleted()
            textures[key]=image;padded=image
        }
        let material=try self.material(files[4]);material.diffuse.contents=padded
        var transform=SCNMatrix4MakeScale(CGFloat(width)/CGFloat(paddedWidth),-CGFloat(height)/CGFloat(paddedHeight),1)
        transform.m41=CGFloat(gutter)/CGFloat(paddedWidth);transform.m42=CGFloat(height+gutter)/CGFloat(paddedHeight)
        material.diffuse.contentsTransform=transform
        return material
    }
    func material(_ file:String) throws -> SCNMaterial {
        let m=SCNMaterial();m.lightingModel = .constant;m.diffuse.contents=try texture(file)
        // DDS rows start at the top; direct Metal textures do not receive the
        // NSImage orientation conversion. Flip only diffuse, not the fog mask.
        var orientation=SCNMatrix4MakeScale(1,-1,1);orientation.m42=1
        m.diffuse.contentsTransform=orientation
        m.diffuse.mipFilter = .linear;m.diffuse.maxAnisotropy=8;m.diffuse.minificationFilter = .linear;m.diffuse.magnificationFilter = .linear
        m.diffuse.wrapS = .clamp;m.diffuse.wrapT = .clamp;m.isDoubleSided=true
        return m
    }
}
