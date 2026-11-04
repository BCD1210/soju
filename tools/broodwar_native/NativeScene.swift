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

final class MirrorView: NSImageView { override func hitTest(_ point:NSPoint)->NSView? { nil } }
final class WalkView: SCNView {
    weak var commander: Commander?
    var keys=Set<UInt16>();var dragStart=NSPoint.zero;var dragged=false
    let selectionBox=CAShapeLayer()
    override var acceptsFirstResponder:Bool { true }
    override func keyDown(with e:NSEvent) {
        keys.insert(e.keyCode)
        if !e.isARepeat {
            let key=e.charactersIgnoringModifiers?.lowercased() ?? ""
            if e.keyCode==111 {commander?.captureDesktop();return}
            if commander?.groupKey(key,modifiers:e.modifierFlags) != true {commander?.key(key)}
        }
    }
    override func keyUp(with e:NSEvent) { keys.remove(e.keyCode) }
    override func resignFirstResponder()->Bool { keys.removeAll();return super.resignFirstResponder() }
    override func mouseDown(with e:NSEvent) { dragStart=convert(e.locationInWindow,from:nil);dragged=false }
    override func mouseDragged(with e:NSEvent) {
        let point=convert(e.locationInWindow,from:nil)
        guard hypot(point.x-dragStart.x,point.y-dragStart.y)>4 else {return}
        dragged=true
        if e.modifierFlags.contains(.option) {
            commander?.yaw-=CGFloat(e.deltaX)*0.006
            commander?.pitch=max(-1.25,min(1.1,(commander?.pitch ?? 0)-CGFloat(e.deltaY)*0.006))
        } else {
            wantsLayer=true
            if selectionBox.superlayer==nil {layer?.addSublayer(selectionBox)}
            selectionBox.fillColor=NSColor.green.withAlphaComponent(0.08).cgColor
            selectionBox.strokeColor=NSColor.green.cgColor;selectionBox.lineWidth=1
            selectionBox.path=CGPath(rect:NSRect(x:min(point.x,dragStart.x),y:min(point.y,dragStart.y),width:abs(point.x-dragStart.x),height:abs(point.y-dragStart.y)),transform:nil)
        }
    }
    override func mouseUp(with e:NSEvent) {
        let point=convert(e.locationInWindow,from:nil)
        selectionBox.removeFromSuperlayer()
        if dragged && !e.modifierFlags.contains(.option) {
            commander?.selectBox(NSRect(x:min(point.x,dragStart.x),y:min(point.y,dragStart.y),width:abs(point.x-dragStart.x),height:abs(point.y-dragStart.y)),add:e.modifierFlags.contains(.shift))
        } else if !dragged {
            commander?.select(at:point)
            if e.clickCount==2 {commander?.selectSameType()}
        }
    }
    override func rightMouseDown(with e:NSEvent) { commander?.move(to:convert(e.locationInWindow,from:nil)) }
}
final class Commander: NSObject, NSApplicationDelegate {
    var window:NSWindow!;var view:WalkView!;var label:NSTextField!
    var desktopCard=NSView(),desktopCardSignature=""
    let art:Art;let bridge=Bridge();let world=SCNScene();let camera=SCNNode()
    var yaw:CGFloat = -0.54,pitch:CGFloat = -0.14
    var units=[String:Obj](),nodes=[String:SCNNode](),selected=[String]()
    var controlGroups=[Int:[String]]();var tabletop=false
    var terrain=[Int:SCNNode]();var fog=[Int:UInt8]();let fogPresentation=FogPresentation()
    let modelLibrary:Models3D;var models=[String:ModelInstance]();let ground:Terrain3D
    let effects:CombatEffects;var retiring=[String:ModelInstance]()
    let wrists=WristPanels()
    let captureFrames=CaptureFrames()
    let worldSprites=WorldSprites();var creepTiles=Set<Int>(),creepNodes=[Int:SCNNode]()
    var eventSequence:Double=0,session="",lostEvents=0,deathCount=0
    var targetingOrder="move"
    var devOverlay=CommandLine.arguments.contains("--dev-overlay")
    var billboards=[String:UnitBillboard]()
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
    var controllerHistory=[Obj](),lastControllerLog=0.0
    let actionNames=["Point / move","Mine","Follow SCV","Train SCV","Build basic structure","Attack / attack move","Stop","Hold"]
    let hud=SCNNode(geometry:SCNPlane(width:1.05,height:0.16));var hudText=""
    var mirror:MirrorView?;var mirrorContext:CIContext?
    #endif
    init(art:Art)throws { self.art=art;ground=try Terrain3D(url:assets.appendingPathComponent("terrain3d.json"));modelLibrary=try Models3D(folder:argument("--model-dir").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("models-3d"),catalogURL:argument("--catalog").map{URL(fileURLWithPath:$0)});effects=try CombatEffects();super.init() }
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
        camera.camera!.zNear=0.03;camera.camera!.zFar=150;Models3D.exposure(camera.camera!)
        if CommandLine.arguments.contains("--top-down") {
            camera.camera!.usesOrthographicProjection=true
            camera.camera!.orthographicScale=Double(argument("--view-height") ?? "24") ?? 24
        }
        world.rootNode.addChildNode(camera);view.pointOfView=camera
        Models3D.lights(world,tileset:(art.manifest["map"] as? Obj)?["tileset"] as? String ?? "badlands");world.rootNode.addChildNode(effects.root)
        world.rootNode.addChildNode(worldSprites.root)
        label=NSTextField(wrappingLabelWithString:"");label.textColor = .white
        label.backgroundColor=NSColor.black.withAlphaComponent(0.75);label.drawsBackground=true
        label.font=NSFont.monospacedSystemFont(ofSize:13,weight:.medium)
        label.isHidden = !devOverlay
        label.frame=NSRect(x:16,y:16,width:1370,height:64);label.autoresizingMask=[.width]
        window.contentView=view;view.addSubview(label);
        if !xrRequested {desktopCard.frame=NSRect(x:1010,y:100,width:410,height:320);desktopCard.autoresizingMask=[.minXMargin,.maxYMargin];view.addSubview(desktopCard)}
        window.center();window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(view);NSApp.activate(ignoringOtherApps:true)
        tick=Timer.scheduledTimer(withTimeInterval:1/60,repeats:true) { [weak self] _ in self?.step() }
        poll=Timer.scheduledTimer(withTimeInterval:0.1,repeats:true) { [weak self] _ in self?.refresh() }
        refresh()
        #if SOJU_OPENXR
        if xrRequested {
            xrRenderer=SCNRenderer(device:art.device,options:nil);xrRenderer!.scene=world
            xrRenderer!.usesReverseZ=false;xrRenderer!.isJitteringEnabled=false;xrRenderer!.isPlaying=true
            xrCamera.camera=SCNCamera();xrCamera.camera!.zNear=0.03;xrCamera.camera!.zFar=150;Models3D.exposure(xrCamera.camera!)
            world.rootNode.addChildNode(xrCamera);xrRenderer!.pointOfView=xrCamera
            view.pointOfView=xrCamera;view.isPlaying=false;view.rendersContinuously=false;view.scene=nil
            let mirror=MirrorView(frame:view.bounds);mirror.autoresizingMask=[.width,.height];mirror.imageScaling = .scaleProportionallyUpOrDown
            view.addSubview(mirror,positioned:.below,relativeTo:label);self.mirror=mirror;mirrorContext=CIContext(mtlDevice:art.device)
            let panel=SCNMaterial();panel.lightingModel = .constant;panel.isDoubleSided=true;panel.readsFromDepthBuffer=false;panel.writesToDepthBuffer=false
            hud.geometry?.materials=[panel];hud.renderingOrder=100;hud.name="hud";world.rootNode.addChildNode(hud)
            let marker=SCNMaterial();marker.lightingModel = .constant;marker.diffuse.contents=NSColor.green
            reticle.categoryBitMask=2;reticle.geometry?.materials=[marker];reticle.isHidden=true;reticle.name="aim-marker"
            world.rootNode.addChildNode(reticle)
            for node in [wrists.commands,wrists.resources,wrists.ray,wrists.box] {world.rootNode.addChildNode(node)}
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
        let regionWidth=min(tabletop ? 64:48,mapWidth),regionHeight=min(tabletop ? 64:48,mapHeight)
        let x=max(0,min(mapWidth-regionWidth,cx-regionWidth/2)),y=max(0,min(mapHeight-regionHeight,cy-regionHeight/2))
        let since=eventSequence,knownSession=session
        bridge.queue.async {
            do {
                var request:Obj=["op":"scene"]
                if !knownSession.isEmpty {request["since"]=since}
                let s=try self.bridge.request(request)
                let metadata=knownSession != s["session"] as? String ? try? self.bridge.request(["op":"unit_types"]):nil
                guard s["truncated"] as? Bool != true else { throw SceneError.invalid("Visible unit limit exceeded; native snapshot was not applied") }
                let loaded=s["map"] as? Obj ?? [:]
                if Int(number(loaded,"width")) != self.mapWidth*32 || Int(number(loaded,"height")) != self.mapHeight*32 {
                    DispatchQueue.main.async {self.apply(s,[:]);self.busy=false};return
                }
                let f=try self.bridge.request(["op":"terrain","x":x,"y":y,"width":regionWidth,"height":regionHeight])
                DispatchQueue.main.async { if let metadata=metadata {self.wrists.card.accept(metadata)};self.apply(s,f);self.busy=false }
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
}


do {
    if CommandLine.arguments.contains("--effect-proof") {try effectSpriteProof();exit(0)}
    if CommandLine.arguments.contains("--live-proof") {try liveModelProof();exit(0)}
    if argument("--command-audit") != nil {try commandAudit();exit(0)}
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
