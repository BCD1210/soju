// Native Blizzard simulation with rigged 3D meshes and map-derived terrain.
// Desktop and OpenXR share unit instances, geometry, commands and game state.
import Cocoa
import SceneKit
import Metal
import QuartzCore
import CoreImage
import Darwin
import simd
let xrRequested=CommandLine.arguments.contains("--xr")

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
    func material(_ file:String) throws -> SCNMaterial {
        let m=SCNMaterial();m.lightingModel = .constant;m.diffuse.contents=try texture(file)
        m.diffuse.mipFilter = .linear;m.diffuse.maxAnisotropy=8;m.diffuse.minificationFilter = .linear;m.diffuse.magnificationFilter = .linear
        m.diffuse.wrapS = .clamp;m.diffuse.wrapT = .clamp;m.isDoubleSided=true
        return m
    }
}
final class MirrorView: NSImageView { override func hitTest(_ point:NSPoint)->NSView? { nil } }
final class WalkView: SCNView {
    weak var commander: Commander?
    var keys=Set<UInt16>();var dragStart=NSPoint.zero;var dragged=false
    override var acceptsFirstResponder:Bool { true }
    override func keyDown(with e:NSEvent) {
        keys.insert(e.keyCode)
        if !e.isARepeat { commander?.key(e.charactersIgnoringModifiers?.lowercased() ?? "") }
    }
    override func keyUp(with e:NSEvent) { keys.remove(e.keyCode) }
    override func resignFirstResponder()->Bool { keys.removeAll();return super.resignFirstResponder() }
    override func mouseDown(with e:NSEvent) { dragStart=convert(e.locationInWindow,from:nil);dragged=false }
    override func mouseDragged(with e:NSEvent) {
        dragged=true;commander?.yaw-=CGFloat(e.deltaX)*0.006
        commander?.pitch=max(-1.25,min(1.1,(commander?.pitch ?? 0)-CGFloat(e.deltaY)*0.006))
    }
    override func mouseUp(with e:NSEvent) {
        if !dragged { commander?.select(at:convert(e.locationInWindow,from:nil)) }
    }
    override func rightMouseDown(with e:NSEvent) { commander?.move(to:convert(e.locationInWindow,from:nil)) }
}
final class Commander: NSObject, NSApplicationDelegate {
    var window:NSWindow!;var view:WalkView!;var label:NSTextField!
    let art:Art;let bridge=Bridge();let world=SCNScene();let camera=SCNNode()
    var yaw:CGFloat = -0.54,pitch:CGFloat = -0.14
    var units=[String:Obj](),nodes=[String:SCNNode](),selected=[String]()
    var terrain=[Int:SCNNode]();var fog=[Int:UInt8]()
    let modelLibrary:Models3D;var models=[String:ModelInstance]();let ground:Terrain3D
    let effects:CombatEffects;var retiring=[String:ModelInstance]()
    var eventSequence:Double=0,session="",lostEvents=0,deathCount=0
    var targetingOrder="move"
    var mapWidth=0,mapHeight=0,mapTiles=[Int]()
    var state:Obj=[:];var busy=false;var initialized=false;var notice="Connecting to original client"
    var lastTick=CACurrentMediaTime();var lastSceneDiagnostic=0.0;var tick:Timer?;var poll:Timer?;var placingDepot=false
    #if SOJU_OPENXR
    var xrTimer:Timer?;var xrRenderer:SCNRenderer?;var xrCamera=SCNNode()
    var xrColor=[Int:MTLTexture]();var xrDepth=[Int:MTLTexture]();var lastXRInput=CACurrentMediaTime();var turnHeld=false
    var eyePositions=[SIMD3<Float>](repeating:.zero,count:2);var headCenter:SIMD3<Float>?
    var xrOrigin:SIMD3<Float>?;var xrOriginYaw:Float=0;var xrDrawCount=0;var xrFrameTime=0.0
    let reticle=SCNNode(geometry:SCNSphere(radius:0.06))
    var lastAimLog=0.0;var selectionGroup = -1;var actionMode=0
    let actionNames=["Point / move","Mine","Follow SCV","Train SCV","Build basic structure","Attack / attack move","Stop","Hold"]
    let hud=SCNNode(geometry:SCNPlane(width:1.05,height:0.16));var hudText=""
    var mirror:MirrorView?;var mirrorContext:CIContext?
    #endif
    init(art:Art)throws { self.art=art;ground=try Terrain3D(url:assets.appendingPathComponent("terrain3d.json"));modelLibrary=try Models3D();effects=try CombatEffects();super.init() }
    func applicationDidFinishLaunching(_ notification:Notification) {
        let map=art.manifest["map"] as! Obj
        mapWidth=Int(number(map,"width"));mapHeight=Int(number(map,"height"));mapTiles=map["tiles"] as! [Int]
        window=NSWindow(contentRect:NSRect(x:0,y:0,width:1440,height:900),styleMask:[.titled,.closable,.resizable,.miniaturizable],backing:.buffered,defer:false)
        window.title=xrRequested ? "Soju • Brood War Remastered — OpenXR commander" : "Soju • Brood War Remastered — desktop commander"
        view=WalkView(frame:window.contentView!.bounds,options:[SCNView.Option.preferredRenderingAPI.rawValue:SCNRenderingAPI.metal.rawValue])
        view.autoresizingMask=[.width,.height];view.commander=self;view.scene=world
        view.preferredFramesPerSecond=60;view.rendersContinuously=true;view.isPlaying=true
        view.antialiasingMode = .multisampling4X
        world.background.contents=NSColor(calibratedRed:0.025,green:0.035,blue:0.05,alpha:1)
        camera.camera=SCNCamera();camera.camera!.fieldOfView=80
        camera.camera!.zNear=0.03;camera.camera!.zFar=150
        world.rootNode.addChildNode(camera);view.pointOfView=camera
        Models3D.lights(world,tileset:(art.manifest["map"] as? Obj)?["tileset"] as? String ?? "badlands");world.rootNode.addChildNode(effects.root)
        label=NSTextField(wrappingLabelWithString:"");label.textColor = .white
        label.backgroundColor=NSColor.black.withAlphaComponent(0.75);label.drawsBackground=true
        label.font=NSFont.monospacedSystemFont(ofSize:13,weight:.medium)
        label.frame=NSRect(x:16,y:16,width:1370,height:64);label.autoresizingMask=[.width]
        window.contentView=view;view.addSubview(label);window.center();window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(view);NSApp.activate(ignoringOtherApps:true)
        tick=Timer.scheduledTimer(withTimeInterval:1/60,repeats:true) { [weak self] _ in self?.step() }
        poll=Timer.scheduledTimer(withTimeInterval:0.1,repeats:true) { [weak self] _ in self?.refresh() }
        refresh()
        #if SOJU_OPENXR
        if xrRequested {
            xrRenderer=SCNRenderer(device:art.device,options:nil);xrRenderer!.scene=world
            xrRenderer!.usesReverseZ=false;xrRenderer!.isJitteringEnabled=false;xrRenderer!.isPlaying=true
            xrCamera.camera=SCNCamera();xrCamera.camera!.zNear=0.03;xrCamera.camera!.zFar=150
            world.rootNode.addChildNode(xrCamera);xrRenderer!.pointOfView=xrCamera
            view.pointOfView=xrCamera;view.isPlaying=false;view.rendersContinuously=false
            let mirror=MirrorView(frame:view.bounds);mirror.autoresizingMask=[.width,.height];mirror.imageScaling = .scaleProportionallyUpOrDown
            view.addSubview(mirror,positioned:.below,relativeTo:label);self.mirror=mirror;mirrorContext=CIContext(mtlDevice:art.device)
            let panel=SCNMaterial();panel.lightingModel = .constant;panel.isDoubleSided=true;panel.readsFromDepthBuffer=false;panel.writesToDepthBuffer=false
            hud.geometry?.materials=[panel];hud.renderingOrder=100;hud.name="hud";world.rootNode.addChildNode(hud)
            let marker=SCNMaterial();marker.lightingModel = .constant;marker.diffuse.contents=NSColor.green
            reticle.geometry?.materials=[marker];reticle.isHidden=true;reticle.name="aim-marker"
            world.rootNode.addChildNode(reticle)
            xrTimer=Timer.scheduledTimer(withTimeInterval:1/90,repeats:true) { [weak self] _ in
                guard let self=self else { return }
                if soju_xr_step(Unmanaged.passUnretained(self).toOpaque(),xrDraw,xrControls)==0 {
                    self.notice=String(cString:soju_xr_status());self.xrTimer?.invalidate();soju_xr_close()
                }
            }
        }
        #endif
    }
    func applicationWillTerminate(_ notification:Notification) {
        #if SOJU_OPENXR
        if xrRequested { xrTimer?.invalidate();soju_xr_close() }
        #endif
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender:NSApplication)->Bool { true }
    func refresh() {
        guard !busy else { return };busy=true
        let center=terrainCenter
        let cx=max(0,min(mapWidth-1,Int(center.x))),cy=max(0,min(mapHeight-1,Int(center.z)))
        let regionWidth=min(48,mapWidth),regionHeight=min(48,mapHeight)
        let x=max(0,min(mapWidth-regionWidth,cx-regionWidth/2)),y=max(0,min(mapHeight-regionHeight,cy-regionHeight/2))
        let since=eventSequence,knownSession=session
        bridge.queue.async {
            do {
                var request:Obj=["op":"state"]
                if !knownSession.isEmpty {request["since"]=since}
                let s=try self.bridge.request(request)
                guard s["truncated"] as? Bool != true else { throw SceneError.invalid("Visible unit limit exceeded; native snapshot was not applied") }
                let loaded=s["map"] as? Obj ?? [:]
                if Int(number(loaded,"width")) != self.mapWidth*32 || Int(number(loaded,"height")) != self.mapHeight*32 {
                    DispatchQueue.main.async {self.apply(s,[:]);self.busy=false};return
                }
                let f=try self.bridge.request(["op":"terrain","x":x,"y":y,"width":regionWidth,"height":regionHeight])
                DispatchQueue.main.async { self.apply(s,f);self.busy=false }
            } catch {
                DispatchQueue.main.async { self.notice="Connection: \(error)";self.busy=false }
            }
        }
    }
    func restartForMap() {
        notice="Preparing changed map";tick?.invalidate();poll?.invalidate()
        #if SOJU_OPENXR
        if xrRequested {xrTimer?.invalidate();soju_xr_close()}
        #endif
        exit(78) // Launcher prepares the new map, then restarts this viewer.
    }
    func apply(_ s:Obj,_ f:Obj) {
        let liveMap=s["map"] as? Obj ?? [:]
        guard Int(number(liveMap,"width"))==mapWidth*32,Int(number(liveMap,"height"))==mapHeight*32 else {
            restartForMap();return
        }
        let prepared=art.manifest["map"] as? Obj ?? [:]
        if let expected=prepared["nativePath"] as? String,!expected.isEmpty,
            let actual=liveMap["path"] as? String,expected.lowercased() != actual.lowercased() {
            restartForMap();return
        }
        modelLibrary.catalog.poll()
        let newSession=s["session"] as? String ?? "legacy"
        let changed=newSession != session
        if changed {
            for n in nodes.values {n.removeFromParentNode()}
            for m in retiring.values {m.root.removeFromParentNode()}
            nodes.removeAll();models.removeAll();retiring.removeAll();effects.clear();selected=[]
            initialized=false;session=newSession;eventSequence=0;deathCount=0;lostEvents=0
        }
        let events=changed ? []:(s["events"] as? [Obj] ?? []).filter{number($0,"seq")>eventSequence}
        eventSequence=number(s,"eventSeq")
        if s["eventsLost"] as? Bool == true {lostEvents+=1}
        state=s
        let list=s["units"] as? [Obj] ?? []
        units=Dictionary(uniqueKeysWithValues:list.compactMap { u in (u["id"] as? String).map { ($0,u) } })
        if !initialized,let base=list.first(where:{($0["townHall"] as? Bool == true || number($0,"type")==106) && number($0,"owner")==number(s,"player")}) {
            camera.position=SCNVector3(number(base,"x")/32-3,1.65,number(base,"y")/32+5)
            initialized=true
        }
        let x=Int(number(f,"x")),y=Int(number(f,"y")),w=Int(number(f,"width"))
        for (i,c) in (f["fog"] as? String ?? "").utf8.enumerated() { fog[(y+i/w)*mapWidth+x+i%w]=c }
        updateTerrain()
        selected=selected.filter { units[$0] != nil }
        for e in events where e["kind"] as? String == "death" {
            guard let id=e["unit"] as? String,units[id]==nil,retiring[id]==nil,
                let dead=e["state"] as? Obj,dead["renderHidden"] as? Bool != true else {continue}
            if let model=models[id] {
                model.apply(dead,height:ground.elevation(number(dead,"x")/32,number(dead,"y")/32),frame:number(s,"frame"))
                model.beginDeath(frame:number(s,"frame"));retiring[id]=model;deathCount+=1
                models.removeValue(forKey:id);nodes.removeValue(forKey:id)
            }
        }
        for (id,n) in nodes where units[id]==nil {
            n.removeFromParentNode();nodes.removeValue(forKey:id);models.removeValue(forKey:id)
        }
        var unsupported=Set<Int>()
        for (id,u) in units {
            let kind=Int(number(u,"type"))
            do {
                let instance:ModelInstance
                if let old=models[id],old.kind==kind,old.revision==modelLibrary.catalog.signatures[kind] { instance=old }
                else {
                    instance=try modelLibrary.make(kind);instance.root.name=id
                    // Commit a replacement only after import and rig binding succeed.
                    nodes[id]?.removeFromParentNode()
                    models[id]=instance;nodes[id]=instance.root;world.rootNode.addChildNode(instance.root)
                }
                for e in events where e["kind"] as? String == "attack" && e["unit"] as? String == id {instance.attackUntilFrame=number(s,"frame")+5}
                instance.apply(u,height:ground.elevation(number(u,"x")/32,number(u,"y")/32),frame:number(s,"frame"))
                let colors=s["playerColors"] as? [[Int]] ?? [],owner=Int(number(u,"owner"))
                if colors.indices.contains(owner),colors[owner].prefix(3).contains(where:{$0>0}) {instance.teamColor(colors[owner])}
                else if Models3D.playerColors.indices.contains(owner) {instance.teamColor(Models3D.playerColors[owner])}
            } catch {
                if modelLibrary.errors[kind]==nil { modelLibrary.errors[kind]=String(describing:error) }
                if let old=models[id],old.kind==kind {
                    old.apply(u,height:ground.elevation(number(u,"x")/32,number(u,"y")/32),frame:number(s,"frame"))
                } else { unsupported.insert(kind);nodes[id]?.isHidden=true }
            }
        }
        for (id,model) in retiring where model.updateDeath(frame:number(s,"frame"),now:CACurrentMediaTime()) {
            model.root.removeFromParentNode();retiring.removeValue(forKey:id)
        }
        effects.apply(bullets:s["bullets"] as? [Obj] ?? [],events:events,units:units,frame:number(s,"frame"),ground:ground)
        if !unsupported.isEmpty { notice="3D models unavailable for types: \(unsupported.sorted()). These units are not rendered." }
        else if !modelLibrary.catalog.lastError.isEmpty { notice=modelLibrary.catalog.lastError }
        else if !modelLibrary.errors.isEmpty { notice="Asset update rejected; previous models retained: \(modelLibrary.errors.keys.sorted())" }
        else if ["Connecting","Connection:","3D models unavailable","Asset update rejected","Model catalog:","Reloading 3D assets"].contains(where:{notice.hasPrefix($0)}) { notice="Original engine connected · rigged 3D models" }
    }
    var terrainCenter:SCNVector3 {
        #if SOJU_OPENXR
        if xrRequested,let h=headCenter { return SCNVector3(h.x,h.y,h.z) }
        #endif
        return camera.position
    }
    func updateTerrain() {
        let tiles=art.manifest["tiles"] as? [String:Obj] ?? [:]
        let cx=Int(terrainCenter.x),cy=Int(terrainCenter.z)
        var keep=Set<Int>()
        for y in max(0,cy-20)..<max(0,min(mapHeight,cy+21)) {
            for x in max(0,cx-20)..<max(0,min(mapWidth,cx+21)) {
                let index=y*mapWidth+x
                guard let visibility=fog[index],visibility != 48 else { continue }
                keep.insert(index)
                if let n=terrain[index] { n.geometry?.firstMaterial?.multiply.contents=NSColor(white:visibility==50 ? 1:0.35,alpha:1);continue }
                guard let a=tiles[String(mapTiles[index])],let file=a["file"] as? String else { continue }
                do {
                    let material=try art.material(file);material.lightingModel = .lambert
                    let p=ground.geometry(tileX:x,tileY:y,material:material)
                    let n=SCNNode(geometry:p);n.name="terrain"
                    n.position=SCNVector3(CGFloat(x),0,CGFloat(y))
                    p.firstMaterial?.multiply.contents=NSColor(white:visibility==50 ? 1:0.35,alpha:1)
                    world.rootNode.addChildNode(n);terrain[index]=n
                } catch { notice="Terrain: \(error)" }
            }
        }
        for (i,n) in terrain where !keep.contains(i) { n.removeFromParentNode();terrain.removeValue(forKey:i) }
    }
    func walk(_ dx:CGFloat,_ dz:CGFloat) {
        for axis in 0..<2 {
            let x=camera.position.x+(axis==0 ? dx:0),z=camera.position.z+(axis==1 ? dz:0)
            let before=ground.elevation(Double(camera.position.x),Double(camera.position.z))
            guard ground.walkable(Double(x),Double(z)),abs(ground.elevation(Double(x),Double(z))-before)<0.4 else { continue }
            camera.position.x=x;camera.position.z=z
        }
    }
    func step() {
        let now=CACurrentMediaTime(),dt=CGFloat(min(0.1,now-lastTick));lastTick=now
        SCNTransaction.begin();SCNTransaction.disableActions=true
        for (id,model) in models { model.advance(dt:Double(dt),now:now,selected:selected.contains(id)) }
        for model in retiring.values {model.advance(dt:Double(dt),now:now,selected:false)}
        SCNTransaction.commit()
        var forward:CGFloat=0,side:CGFloat=0
        if view.keys.contains(13) { forward+=1 };if view.keys.contains(1) { forward-=1 }
        if view.keys.contains(0) { side-=1 };if view.keys.contains(2) { side+=1 }
        let speed:CGFloat=4
        walk((-sin(yaw)*forward+cos(yaw)*side)*speed*dt,(-cos(yaw)*forward-sin(yaw)*side)*speed*dt)
        camera.position.x=max(0.1,min(CGFloat(mapWidth)-0.1,camera.position.x))
        camera.position.z=max(0.1,min(CGFloat(mapHeight)-0.1,camera.position.z))
        camera.position.y=CGFloat(ground.elevation(Double(camera.position.x),Double(camera.position.z))+1.65)
        camera.eulerAngles=SCNVector3(pitch,yaw,0)
        Models3D.followLight(world,position:camera.position)
        if now-lastSceneDiagnostic>1 {
            lastSceneDiagnostic=now
            let rendered:[Obj]=models.map { id,m in
                ["id":id,"type":m.kind,"model":m.modelName,"animation":m.playing,
                 "position":[m.root.position.x,m.root.position.y,m.root.position.z],"yaw":m.root.eulerAngles.y,
                 "hidden":m.root.isHidden,"geometry":"rigged triangles","action":m.action,
                 "animationWarning":m.animationError,"animationPlayers":m.activePlayers.count,
                 "file":m.definition.file]
            }
            let diagnostics:Obj=["time":Date().timeIntervalSince1970,"frame":number(state,"frame"),
                "units":rendered,"camera":[camera.position.x,camera.position.y,camera.position.z],"xr":xrRequested,
                "protocol":number(state,"protocol"),"eventSequence":eventSequence,"eventsLost":lostEvents,
                "combatEvents":effects.counts,"deathAnimations":deathCount,"retiringModels":retiring.count,"projectiles":effects.projectiles.count,
                "effectErrors":Dictionary(uniqueKeysWithValues:effects.library.errors.map{(String($0.key),$0.value)}),
                "catalogError":modelLibrary.catalog.lastError,"assetErrors":Dictionary(uniqueKeysWithValues:modelLibrary.errors.map{(String($0.key),$0.value)}),
                "missingTypes":Array(Set(units.values.compactMap { u -> Int? in let id=u["id"] as? String ?? "";return models[id]==nil ? Int(number(u,"type")):nil })).sorted()]
            record("3d-scene-diagnostics.json",diagnostics)
            record(xrRequested ? "3d-xr-diagnostics.json":"3d-desktop-diagnostics.json",diagnostics)
        }
        label.stringValue="LIVE BROOD WAR · RIGGED 3D · \(xrRequested ? "OpenXR commander" : "Desktop commander")\nWASD walk · Drag look · 1 workers / 2 army / 3 bases · G mine · F follow · T train worker · B structure · Q attack · H hold · X stop · R recenter · Right-click command\nFrame \(Int(number(state,"frame")))  Minerals \(Int(number(state,"minerals")))  Selected \(selected.count)  |  \(notice)"
    }
    func owned(_ kind:Int)->[Obj] { units.values.filter { Int(number($0,"type"))==kind && number($0,"owner")==number(state,"player") } }
    func group(_ name:String)->[Obj] {
        units.values.filter { u in
            guard number(u,"owner")==number(state,"player"),u["renderHidden"] as? Bool != true else {return false}
            if name=="workers" {return u["worker"] as? Bool == true || [7,41,64].contains(Int(number(u,"type")))}
            if name=="bases" {return u["townHall"] as? Bool == true || [106,131,132,133,154].contains(Int(number(u,"type")))}
            return u["building"] as? Bool != true && u["worker"] as? Bool != true && u["resource"] as? Bool != true
        }.sorted {($0["id"] as? String ?? "")<($1["id"] as? String ?? "")}
    }
    func key(_ key:String) {
        if key=="q" {targetingOrder="attack_move";notice="Point at enemy or ground to attack"}
        if key=="h",!selected.isEmpty {command(["op":"hold","units":Array(selected.prefix(12))])}
        if key=="p" {targetingOrder="patrol";notice="Point at ground to patrol"}
        if key=="l" { modelLibrary.catalog.poll(force:true);notice="Reloading 3D assets";refresh() }
        if key=="x",!selected.isEmpty { command(["op":"stop","units":Array(selected.prefix(12))]) }
        #if SOJU_OPENXR
        if key=="r" { xrOrigin=nil;notice="Recenter requested" }
        #endif
        if key=="b" { placingDepot.toggle();notice=placingDepot ? "Point at ground: SCV Depot / Probe Pylon / Drone Hatchery" : "Build cancelled" }
        if ["1","2","3"].contains(key) {
            selected=Array(group(key=="1" ? "workers":key=="2" ? "army":"bases").compactMap{$0["id"] as? String}.prefix(12));notice="Selected \(selected.count)"
        }
        if key=="g",let worker=selected.compactMap({units[$0]}).first(where:{$0["worker"] as? Bool == true || [7,41,64].contains(Int(number($0,"type")))}) {
            let minerals=units.values.filter{[176,177,178].contains(Int(number($0,"type")))}
            if let target=minerals.min(by:{dist($0,worker)<dist($1,worker)}),let id=target["id"] as? String {
                command(["op":"gather","units":selected,"target":id])
            } else { notice="No visible mineral field. Move a worker to explore first." }
        }
        if key=="f",let worker=group("workers").first,let id=worker["id"] as? String { command(["op":"follow","units":selected,"target":id]) }
        if key=="t",let base=group("bases").first,let id=base["id"] as? String {
            let race=Int(number(base,"race"))
            if race==0 {notice="Zerg units require larva morph; select the original client's production command"}
            else {command(["op":"train","units":[id],"type":race==2 ? 64:7])}
        }
    }
    func dist(_ a:Obj,_ b:Obj)->Double { pow(number(a,"x")-number(b,"x"),2)+pow(number(a,"y")-number(b,"y"),2) }
    func unitID(_ node:SCNNode)->String? {
        var candidate:SCNNode?=node
        while let n=candidate { if let id=n.name,units[id] != nil { return id };candidate=n.parent }
        return nil
    }
    func select(at p:NSPoint) {
        if let hit=view.hitTest(p,options:nil).first(where:{unitID($0.node) != nil}),let id=unitID(hit.node),
            let u=units[id],number(u,"owner")==number(state,"player") {
            selected=[id];notice="Selected unit \(Int(number(u,"type")))"
        }
    }
    func move(to p:NSPoint) {
        let hits=view.hitTest(p,options:nil)
        if !placingDepot,let hit=hits.first(where:{unitID($0.node) != nil}),let id=unitID(hit.node),contextCommand(id) {return}
        guard !selected.isEmpty,let hit=hits.first(where:{$0.node.name=="terrain"}) else { notice="Select units and right-click explored ground";return }
        groundCommand(hit.worldCoordinates)
    }
    @discardableResult func contextCommand(_ id:String)->Bool {
        guard !selected.isEmpty,let target=units[id] else {return false}
        let owner=Int(number(target,"owner")),player=Int(number(state,"player")),allies=state["alliances"] as? [Int] ?? []
        if target["resource"] as? Bool == true {command(["op":"gather","units":selected,"target":id]);return true}
        if owner != player && owner<8 && (owner>=allies.count || allies[owner]==0) {
            command(["op":"attack","units":selected,"target":id]);targetingOrder="move";return true
        }
        return false
    }
    func groundCommand(_ point:SCNVector3) {
        if placingDepot {
            guard let worker=selected.compactMap({units[$0]}).first(where:{$0["worker"] as? Bool == true || [7,41,64].contains(Int(number($0,"type")))}),let id=worker["id"] as? String else { notice="Select a worker first";return }
            command(["op":"build","units":[id],"type":number(worker,"type")==64 ? 156:number(worker,"type")==41 ? 131:109,"tileX":Int(point.x)-1,"tileY":Int(point.z)-1])
            placingDepot=false
            #if SOJU_OPENXR
            actionMode=0
            #endif
        } else { command(["op":targetingOrder,"units":selected,"x":Int(point.x*32),"y":Int(point.z*32)]);targetingOrder="move" }
    }
    func record(_ file:String,_ object:Obj) {
        if let data=try? JSONSerialization.data(withJSONObject:object,options:.prettyPrinted) { try? data.write(to:runtime.appendingPathComponent(file),options:.atomic) }
    }
    func command(_ c:Obj) {
        bridge.queue.async {
            do { let ack=try self.bridge.request(c);DispatchQueue.main.async { self.notice="\(c["op"]!) sent at game frame \(Int(number(ack,"frame")))";self.record("scene-last-command.json",["command":c,"ack":ack,"time":Date().timeIntervalSince1970]) } }
            catch { DispatchQueue.main.async { self.notice="Command: \(error)" } }
        }
    }
}
#if SOJU_OPENXR
func xrDraw(_ context:UnsafeMutableRawPointer?,_ texture:UnsafeMutableRawPointer?,_ queue:UnsafeMutableRawPointer?,_ packet:UnsafePointer<SojuXREye>?) {
    guard let context=context,let texture=texture,let queue=queue,let packet=packet else { return }
    let c=Unmanaged<Commander>.fromOpaque(context).takeUnretainedValue()
    c.renderEye(Unmanaged<AnyObject>.fromOpaque(texture).takeUnretainedValue() as! MTLTexture,
        Unmanaged<AnyObject>.fromOpaque(queue).takeUnretainedValue() as! MTLCommandQueue,packet.pointee)
}
func xrControls(_ context:UnsafeMutableRawPointer?,_ packet:UnsafePointer<SojuXRInput>?) {
    guard let context=context,let packet=packet else { return }
    Unmanaged<Commander>.fromOpaque(context).takeUnretainedValue().controls(packet.pointee)
}
extension Commander {
    func renderEye(_ texture:MTLTexture,_ queue:MTLCommandQueue,_ eye:SojuXREye) {
        guard let renderer=xrRenderer,let buffer=queue.makeCommandBuffer() else { return }
        if eye.eye==0 { xrFrameTime=CACurrentMediaTime() }
        if xrOrigin==nil && eye.tracked != 0 {
            xrOrigin=SIMD3<Float>(eye.headX,eye.headY,eye.headZ)
            let forward=simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw).act(SIMD3<Float>(0,0,-1))
            xrOriginYaw=atan2(-forward.x,-forward.z)
        }
        let body=simd_quatf(angle:Float(yaw)-xrOriginYaw,axis:SIMD3<Float>(0,1,0))
        let base=SIMD3<Float>(Float(camera.position.x),Float(camera.position.y),Float(camera.position.z))
        let position=base+body.act(SIMD3<Float>(eye.px,eye.py,eye.pz)-(xrOrigin ?? .zero))
        if eye.eye==0 {
            let center=base+body.act(SIMD3<Float>(eye.headX,eye.headY,eye.headZ)-(xrOrigin ?? .zero))
            let headRotation=body*simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw)
            hud.simdPosition=center+headRotation.act(SIMD3<Float>(0,-0.38,-0.9));hud.simdOrientation=headRotation
            let text="Selected: \(selected.count)   Minerals: \(Int(number(state,"minerals")))   Action: \(actionNames[actionMode])\nLeft trigger: unit group   Right grip: action   Right trigger: confirm\n\(notice)"
            if text != hudText {
                hudText=text
                let size=NSSize(width:1200,height:180),image=NSImage(size:NSSize(width:1200,height:180))
                image.lockFocus();NSColor.black.withAlphaComponent(0.8).setFill();NSRect(origin:.zero,size:size).fill()
                (text as NSString).draw(in:NSRect(x:20,y:12,width:1160,height:158),withAttributes:[.font:NSFont.monospacedSystemFont(ofSize:30,weight:.medium),.foregroundColor:NSColor.white])
                image.unlockFocus();hud.geometry?.firstMaterial?.diffuse.contents=image
            }
        }
        eyePositions[Int(eye.eye)]=position
        if eye.eye==1 { headCenter=(eyePositions[0]+eyePositions[1])*0.5 }
        SCNTransaction.begin();SCNTransaction.disableActions=true
        xrCamera.simdPosition=position
        xrCamera.simdOrientation=body*simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw)
        let l=CGFloat(tan(eye.left)),r=CGFloat(tan(eye.right)),u=CGFloat(tan(eye.up)),d=CGFloat(tan(eye.down))
        let near:CGFloat=0.03,far:CGFloat=150
        var projection=SCNMatrix4()
        projection.m11=2/(r-l);projection.m22=2/(u-d)
        projection.m31=(r+l)/(r-l);projection.m32=(u+d)/(u-d)
        projection.m33 = -(far+near)/(far-near);projection.m34 = -1
        projection.m43 = -2*far*near/(far-near);projection.m44=0
        xrCamera.camera!.projectionTransform=projection
        SCNTransaction.commit();SCNTransaction.flush()
        let index=Int(eye.eye)
        if xrDepth[index]?.width != texture.width || xrDepth[index]?.height != texture.height {
            let desc=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.depth32Float_stencil8,width:texture.width,height:texture.height,mipmapped:false)
            desc.storageMode = .private;desc.usage = .renderTarget
            desc.textureType = .type2DMultisample;desc.sampleCount=4
            xrDepth[index]=art.device.makeTexture(descriptor:desc)
            desc.pixelFormat=texture.pixelFormat;xrColor[index]=art.device.makeTexture(descriptor:desc)
        }
        let pass=MTLRenderPassDescriptor();pass.colorAttachments[0].texture=xrColor[index]
        pass.colorAttachments[0].resolveTexture=texture
        pass.colorAttachments[0].loadAction = .clear;pass.colorAttachments[0].storeAction = .multisampleResolve
        pass.colorAttachments[0].clearColor=MTLClearColorMake(0.025,0.035,0.05,1)
        pass.depthAttachment.texture=xrDepth[index];pass.depthAttachment.loadAction = .clear
        pass.depthAttachment.storeAction = .dontCare;pass.depthAttachment.clearDepth=1
        pass.stencilAttachment.texture=xrDepth[index];pass.stencilAttachment.loadAction = .clear
        pass.stencilAttachment.storeAction = .dontCare;pass.stencilAttachment.clearStencil=0
        if eye.eye==0 { xrDrawCount+=1 }
        if eye.eye==0 && xrDrawCount%72==0 {
            let diagnostic:Obj=["eye":[eye.px,eye.py,eye.pz],"desired":[position.x,position.y,position.z],
                "presentation":[xrCamera.presentation.simdWorldPosition.x,xrCamera.presentation.simdWorldPosition.y,xrCamera.presentation.simdWorldPosition.z],
                "desktop":[camera.simdWorldPosition.x,camera.simdWorldPosition.y,camera.simdWorldPosition.z],
                "fov":[eye.left,eye.right,eye.up,eye.down],"unitCount":nodes.count,"terrainCount":terrain.count,
                "tracked":eye.tracked,"selected":selected,"frame":number(state,"frame"),"yaw":yaw,
                "orientation":[xrCamera.simdOrientation.imag.x,xrCamera.simdOrientation.imag.y,xrCamera.simdOrientation.imag.z,xrCamera.simdOrientation.real]]
            if let data=try? JSONSerialization.data(withJSONObject:diagnostic,options:.prettyPrinted) { try? data.write(to:runtime.appendingPathComponent("xr-view-diagnostics.json"),options:.atomic) }
        }
        renderer.render(atTime:xrFrameTime,viewport:CGRect(x:0,y:0,width:texture.width,height:texture.height),commandBuffer:buffer,passDescriptor:pass)
        if eye.eye==0 && xrDrawCount%10==0,let context=mirrorContext {
            let desc=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:texture.pixelFormat,width:texture.width,height:texture.height,mipmapped:false)
            desc.usage = .shaderRead;desc.storageMode = .private
            if let copy=art.device.makeTexture(descriptor:desc),let blit=buffer.makeBlitCommandEncoder() {
                blit.copy(from:texture,to:copy);blit.endEncoding()
                buffer.addCompletedHandler { [weak self] command in
                    guard command.status == .completed,let ci=CIImage(mtlTexture:copy,options:nil),let cg=context.createCGImage(ci.transformed(by:CGAffineTransform(translationX:0,y:CGFloat(copy.height)).scaledBy(x:1,y:-1)),from:ci.extent) else { return }
                    DispatchQueue.main.async { self?.mirror?.image=NSImage(cgImage:cg,size:NSSize(width:copy.width,height:copy.height)) }
                }
            }
        }
        buffer.addCompletedHandler { command in if let error=command.error { fputs("XR GPU: \(error)\n",stderr) } }
        buffer.commit()
    }
    func controls(_ input:SojuXRInput) {
        let now=CACurrentMediaTime(),dt=Float(min(0.05,now-lastXRInput));lastXRInput=now
        if abs(input.turnX)<0.4 { turnHeld=false }
        if abs(input.turnX)>0.7 && !turnHeld { yaw -= input.turnX>0 ? .pi/6 : -.pi/6;turnHeld=true }
        let dx=abs(input.moveX)>0.15 ? input.moveX:0,dy=abs(input.moveY)>0.15 ? input.moveY:0
        walk(CGFloat((-sin(Float(yaw))*dy+cos(Float(yaw))*dx)*3*dt),CGFloat((-cos(Float(yaw))*dy-sin(Float(yaw))*dx)*3*dt))
        if input.leftPressed != 0 || input.leftGripPressed != 0 { selectionGroup=(selectionGroup+1)%3;key(String(selectionGroup+1)) }
        if input.rightGripPressed != 0 { actionMode=(actionMode+1)%actionNames.count;placingDepot=actionMode==4;targetingOrder=actionMode==5 ? "attack_move":"move";notice=actionNames[actionMode] }
        if input.rightPressed != 0 && (1...3).contains(actionMode) {
            key(actionMode==1 ? "g":actionMode==2 ? "f":"t");return
        }
        if input.rightPressed != 0 && [6,7].contains(actionMode) {key(actionMode==6 ? "x":"h");return}
        guard input.aimValid != 0,xrOrigin != nil else { reticle.isHidden=true;return }
        let body=simd_quatf(angle:Float(yaw)-xrOriginYaw,axis:SIMD3<Float>(0,1,0))
        let pose=body*simd_quatf(ix:input.qx,iy:input.qy,iz:input.qz,r:input.qw)
        let from=SIMD3<Float>(Float(camera.position.x),Float(camera.position.y),Float(camera.position.z))+body.act(SIMD3<Float>(input.px,input.py,input.pz)-(xrOrigin ?? .zero))
        let to=from+pose.act(SIMD3<Float>(0,0,-1))*50
        let hits=world.rootNode.hitTestWithSegment(from:SCNVector3(from.x,from.y,from.z),to:SCNVector3(to.x,to.y,to.z),options:nil)
        guard let hit=hits.first(where:{$0.node.name=="terrain" || self.unitID($0.node) != nil}) else { reticle.isHidden=true;return }
        reticle.isHidden=false;reticle.position=hit.worldCoordinates;reticle.position.y+=0.07
        if now-lastAimLog>1 || input.rightPressed != 0 || input.leftPressed != 0 {
            lastAimLog=now
            record("xr-input-diagnostics.json",["time":Date().timeIntervalSince1970,"selected":selected,
                "leftPressed":input.leftPressed,"rightPressed":input.rightPressed,"hit":hit.node.name ?? "",
                "point":[hit.worldCoordinates.x,hit.worldCoordinates.y,hit.worldCoordinates.z],
                "origin":[from.x,from.y,from.z],"move":[input.moveX,input.moveY],"turn":input.turnX])
        }
        guard input.rightPressed != 0 else { return }
        if placingDepot,hit.node.name=="terrain" { groundCommand(hit.worldCoordinates);return }
        if let id=unitID(hit.node),let u=units[id] {
            if contextCommand(id) {} else if number(u,"owner")==number(state,"player") { selected=[id];notice="Controller selected unit" }
        } else if hit.node.name=="terrain" && !selected.isEmpty {
            groundCommand(hit.worldCoordinates)
        }
    }
}
#endif
do {
    if CommandLine.arguments.contains("--runtime-check") {try runtimeChecks();exit(0)}
    if CommandLine.arguments.contains("--model-check") || CommandLine.arguments.contains("--model-proof") {
        guard let device=MTLCreateSystemDefaultDevice() else { throw SceneError.invalid("Metal device unavailable") }
        let folder=argument("--model-dir").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("models-3d")
        let library=try Models3D(folder:folder,catalogURL:argument("--catalog").map{URL(fileURLWithPath:$0)},includeDefaults:!CommandLine.arguments.contains("--no-default-catalog"))
        try library.proof(device:device);exit(0)
    }
    if CommandLine.arguments.contains("--bridge-probe") {
        let state=try Bridge().request(["op":"scene"])
        print("Native scene response decoded: frame \(Int(number(state,"frame"))) units \((state["units"] as? [Obj] ?? []).count)")
        exit(0)
    }
    var preferred:MTLDevice?
    #if SOJU_OPENXR
    if CommandLine.arguments.contains("--xr-probe") {
        let ok=soju_xr_probe();print(String(cString:soju_xr_status()));exit(ok != 0 ? 0:1)
    }
    if xrRequested {
        guard soju_xr_init() != 0,let device=soju_xr_device() else { throw SceneError.invalid(String(cString:soju_xr_status())) }
        preferred=Unmanaged<AnyObject>.fromOpaque(device).takeUnretainedValue() as? MTLDevice
    }
    #else
    if xrRequested { throw SceneError.invalid("Build with OpenXR support before using --xr") }
    #endif
    let art=try Art(device:preferred)
    let app=NSApplication.shared;app.setActivationPolicy(.regular)
    let delegate=try Commander(art:art);app.delegate=delegate;app.run()
            } catch {
    #if SOJU_OPENXR
    if xrRequested { soju_xr_close() }
    #endif
    fputs("soju native scene: \(error)\n",stderr);exit(1)
}
