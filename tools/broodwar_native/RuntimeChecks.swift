import Cocoa
import SceneKit
import Metal
import GLTFKit2

// Deterministic renderer regression tests; live native-engine acceptance is
// recorded separately. No commands are issued to the game by these checks.
func runtimeChecks()throws {
    let commander=try Commander(art:Art())
    commander.requireFidelity=false // Exercise rig lifecycle independently of the live fidelity gate.
    let map=commander.art.manifest["map"] as! Obj
    commander.mapWidth=Int(number(map,"width"));commander.mapHeight=Int(number(map,"height"))
    commander.mapTiles=map["tiles"] as! [Int]
    let x=100.0,y=100.0
    func unit(_ id:String)->Obj {["id":id,"type":0,"owner":6,"x":x,"y":y,"hp":40,"completed":true,"order":2,"image":["animation":0]]}
    func state(_ frame:Double,_ units:[Obj],_ events:[Obj]=[],session:String="check")->Obj {
        ["ok":true,"protocol":2,"session":session,"frame":frame,"player":6,
         "map":["width":commander.mapWidth*32,"height":commander.mapHeight*32],
         "units":units,"events":events,"eventSeq":events.map{number($0,"seq")}.max() ?? commander.eventSequence]
    }
    var passed=[String]()
    func require(_ ok:Bool,_ message:String)throws {if !ok {throw SceneError.invalid("Runtime check: "+message)};passed.append(message)}
    let timing=XRFrameMetrics()
    timing.completed(frame:1,eye:1,milliseconds:2)
    timing.completed(frame:1,eye:0,milliseconds:3)
    timing.completed(frame:2,eye:0,milliseconds:4)
    timing.completed(frame:2,eye:1,milliseconds:5)
    try require(timing.completedStereoSamples==2,"GPU timing pairs either completion callback order")
    commander.apply(state(1,[unit("a"),unit("b")]),[:])
    try require(commander.models.count==2,"two independent live models")
    let aMaterials=Set(commander.models["a"]!.source.materials.map{ObjectIdentifier($0)})
    let bMaterials=Set(commander.models["b"]!.source.materials.map{ObjectIdentifier($0)})
    try require(aMaterials.isDisjoint(with:bMaterials),"team tint materials are isolated between units")
    let attack:Obj=["kind":"attack","unit":"a","seq":1,"x":x,"y":y]
    commander.apply(state(12,[unit("a"),unit("b")],[attack]),[:])
    try require(commander.models["a"]?.action=="attack","native fire selects attack animation")
    let authoredDeath=commander.models["b"]?.clip(for:"death") != nil
    let death:Obj=["kind":"death","unit":"b","seq":2,"state":unit("b")]
    commander.apply(state(24,[unit("a")],[death]),[:])
    try require(commander.models["b"]==nil && (authoredDeath ? commander.retiring["b"]?.dying==true : commander.retiring["b"]==nil) && commander.deathCount==(authoredDeath ? 1:0),"confirmed death retires only an authored death clip")
    commander.apply(state(25,[unit("a")],[death]),[:])
    try require(commander.deathCount==(authoredDeath ? 1:0),"replayed event is ignored")
    commander.apply(state(26,[]),[:])
    try require(commander.models.isEmpty && commander.deathCount==(authoredDeath ? 1:0),"fog disappearance does not create death")
    commander.apply(state(1000,[]),[:])
    try require(commander.retiring.isEmpty,"death model is released")
    let model=try commander.modelLibrary.make(0)
    var u=unit("air");u["airborne"]=true
    model.apply(u,height:2,frame:1)
    try require(abs(Double(model.targetPosition.y)-3.5)<0.001,"airborne model clears ground")
    u["renderHidden"]=true;u["transported"]=true;model.apply(u,height:0,frame:2)
    try require(model.root.isHidden,"transported model is hidden")
    u["renderHidden"]=false;u["cloaked"]=true;model.apply(u,height:0,frame:3)
    try require(!model.root.isHidden && model.root.opacity<1,"visible cloaked model is translucent")
    var deathDefinition=model.definition.raw
    var mapping=deathDefinition["animations"] as? Obj ?? [:]
    mapping["death"]=model.clip(for:"attack") ?? model.clip(for:"idle")
    deathDefinition["animations"]=mapping
    let deathModel=try ModelInstance(asset:commander.modelLibrary.assets[0]!.1,
        definition:ModelDefinition(kind:0,object:deathDefinition),revision:"one-shot-regression")
    deathModel.beginDeath(frame:1)
    try require(!deathModel.activePlayers.isEmpty && deathModel.activePlayers.allSatisfy{$0.animation.repeatCount<=1},"death clip is bound with one-shot playback")
    deathModel.apply(unit("dead"),height:0,frame:2)
    try require(deathModel.action=="death","later state cannot restart a dying rig")
    for (order,action) in [(116,"burrow"),(117,"burrowed"),(118,"unburrow"),(98,"siege"),(99,"unsiege"),(42,"morph"),(72,"lift"),(71,"land"),(34,"repair")] {
        var sample=unit("state");sample["order"]=order;sample["burrowed"]=order==117
        try require(ModelInstance.stateAction(sample,moving:false,attacking:false)==action,"native order \(order) maps to \(action)")
    }
    let bullet:Obj=["id":1,"weapon":0,"x":x,"y":y]
    commander.effects.apply(bullets:[bullet],events:[],units:[:],frame:1001,ground:commander.ground)
    try require(commander.effects.projectiles.count==1,"native projectile is displayed")
    commander.effects.apply(bullets:[],events:[],units:[:],frame:1002,ground:commander.ground)
    try require(commander.effects.projectiles.isEmpty,"expired native projectile is removed")
    commander.fog[0]=50;commander.controlGroups[4]=["a"]
    commander.apply(state(1,[unit("c")],session:"new-game"),[:])
    try require(commander.fog.isEmpty && commander.controlGroups.isEmpty && commander.effects.sprites.renderedFrames==0,"new game clears fog control groups and sprite counters")
    try require(commander.deathCount==0 && commander.effects.counts.isEmpty && Set(commander.models.keys)==["c"],"new game clears old effects and identities")
    var nativeDeath=unit("fallback");nativeDeath["image"]=["id":239,"frame":0,"flags":0]
    commander.worldSprites.apply(["dyingUnits":[nativeDeath]],ground:commander.ground,authored:[])
    try require(commander.worldSprites.nodes.count==1,"native death image creates fallback geometry")
    commander.worldSprites.apply(["dyingUnits":[nativeDeath]],ground:commander.ground,authored:["fallback"])
    try require(commander.worldSprites.nodes.isEmpty,"authored death suppresses duplicate sprite")
    commander.worldSprites.apply(["loneSprites":[nativeDeath]],ground:commander.ground,authored:[])
    try require(commander.worldSprites.nodes.count==1,"visible independent native sprite is rendered")
    commander.worldSprites.apply([:],ground:commander.ground,authored:[])
    try require(commander.worldSprites.nodes.isEmpty,"native disappearance removes sprite without a fabricated timeline")
    let fallback=UnitBillboard(id:"fallback-live")
    var sprite=unit("fallback-live")
    sprite["image"]=["id":239,"frame":77,"frameset":68,"direction":9,"flags":2]
    fallback.apply(sprite,ground:commander.ground,art:commander.worldSprites)
    try require(fallback.layers.count==1 && fallback.evidence.first?["frame"] as? Int == 77 && fallback.layers[0]?.scale.x == -1,
        "fallback preserves native directional frame and mirror flag")
    sprite["image"]=["id":239,"frame":68,"frameset":68,"direction":0,"flags":8]
    fallback.apply(sprite,ground:commander.ground,art:commander.worldSprites)
    for (angle,frame,mirror) in [(0.0,68,false),(Double.pi/2,76,false),(Double.pi,84,false),(-Double.pi/2,76,true)] {
        let origin=fallback.root.simdWorldPosition
        fallback.updateView(eye:origin+SIMD3<Float>(Float(sin(angle)*4),2,Float(cos(angle)*4)),orientation:simd_quatf(),perspective:true,art:commander.worldSprites)
        try require(fallback.evidence.first?["frame"] as? Int == frame && fallback.evidence.first?["mirrored"] as? Bool == mirror,
            "observer angle \(angle) selects the native animation group and mirrored direction")
        try require(fallback.layers[0]!.renderingOrder==0,"relief uses depth ordering rather than the 2D sprite bucket")
        let facing=fallback.layers[0]!.simdOrientation.act(SIMD3<Float>(0,0,1))
        try require(abs(facing.x-Float(sin(angle)*cos(Terrain3D.canonicalElevation)))<0.0001 && abs(facing.z-Float(cos(angle)*cos(Terrain3D.canonicalElevation)))<0.0001,"relief preserves source elevation and faces observer azimuth at \(angle)")
    }
    try require(WorldSprites.playerColor([[0,0,0,0]],owner:0)==Models3D.playerColors[0],"zero initialization colors use player palette")
    if let mesh=commander.worldSprites.relief.geometry(239,frame:68,art:commander.worldSprites) {
        try require(!(mesh is SCNPlane) && mesh.sources(for:.vertex).first!.vectorCount>100,"HD depth produces tessellated geometry")
        let box=mesh.boundingBox
        try require(box.max.z-box.min.z>0.01,"relief has nonzero physical depth")
        let material=commander.worldSprites.relief.material(239,frame:68,color:[244,4,4],art:commander.worldSprites)
        try require(material.readsFromDepthBuffer && material.writesToDepthBuffer,"relief participates in scene occlusion")
        try require(material.shaderModifiers?[.surface]?.contains("relief_specular")==true,"relief shader binds material layers")
    } else {try require(false,"prepared Marine relief required")}
    var profileContinuous=true
    for y in stride(from:1,to:commander.ground.height-1,by:3) {
        for x in stride(from:1,to:commander.ground.width-1,by:3) {
            let px=Double(x)/4,pz=(Double(y)+0.37)/4
            if abs(commander.ground.profile(px-0.000001,pz)-commander.ground.profile(px+0.000001,pz))>0.001 {profileContinuous=false}
        }
    }
    try require(profileContinuous,"subdivided cliff profiles meet across minitile boundaries")
    let fixed=UnitBillboard.viewFrame(["frame":3,"frameset":0,"flags":0],azimuth:.pi,count:50)
    try require(fixed.0==3 && !fixed.1,"non-directional building animation does not become a directional frame")
    let short=UnitBillboard.viewFrame(["frame":0,"frameset":0,"flags":8],azimuth:.pi,count:1)
    try require(short.0==0,"incomplete directional group keeps the valid native frame")
    sprite["renderHidden"]=true;fallback.apply(sprite,ground:commander.ground,art:commander.worldSprites)
    try require(fallback.root.isHidden,"hidden native unit cannot leak through fallback")
    sprite["image"]=["id":239,"frame":999999,"flags":0]
    fallback.apply(sprite,ground:commander.ground,art:commander.worldSprites)
    try require(fallback.layers.isEmpty,"invalid native frame removes stale fallback geometry")
    try require(!commander.devOverlay,"developer overlay defaults off")
    try require(commander.worldSprites.creepEdgeFrame(tileset:"twilight",neighbors:0)==0 &&
        commander.worldSprites.creepEdgeFrame(tileset:"twilight",neighbors:64)==1 &&
        commander.worldSprites.creepEdgeFrame(tileset:"twilight",neighbors:255)==37,
        "creep border lookup distinguishes empty north and surrounded tiles")
    try require(commander.worldSprites.creepEdgeMaterial(tileset:"twilight",frame:1) != nil,
        "owned HD creep border is loadable")
    try require(commander.wrists.ray.categoryBitMask==2 && commander.wrists.box.categoryBitMask==2,
        "decorative rays and drag outlines cannot intercept command hits")
    let tileFiles=((commander.art.manifest["tiles"] as? [String:Obj]) ?? [:]).values.compactMap{$0["file"] as? String}.sorted()
    if tileFiles.count>=2 {
        let neighbors=[tileFiles[1]]+[String](repeating:tileFiles[0],count:8)
        let material=try commander.art.terrainMaterial(neighbors)
        let padded=material.diffuse.contents as! MTLTexture,original=try commander.art.texture(tileFiles[1])
        var border=[UInt8](repeating:0,count:4),expected=[UInt8](repeating:0,count:4)
        padded.getBytes(&border,bytesPerRow:4,from:MTLRegionMake2D(0,0,1,1),mipmapLevel:0)
        original.getBytes(&expected,bytesPerRow:4,from:MTLRegionMake2D(original.width-8,original.height-8,1,1),mipmapLevel:0)
        try require(padded.width==original.width+16 && border==expected,"terrain mip gutter contains the real diagonal neighbor")
    }
    let card=CommandCard();card.accept(["types":[["id":7,"mineralCost":75,"gasCost":9]]])
    let production=card.choices(["type":106,"building":true])
    try require(production.contains{$0.payload["op"] as? String == "train" && $0.title.hasSuffix("75/9")},"command card uses native DAT prices")
    let scvChoice=production.first{$0.payload["op"] as? String == "train"}!
    try require(scvChoice.unavailable(["minerals":74,"gas":9],[:])=="Not enough minerals","native price disables an unaffordable command")
    try require(scvChoice.unavailable(["minerals":75,"gas":8],[:])=="Not enough gas","gas shortages disable commands")
    try require(scvChoice.unavailable(["minerals":75,"gas":9],[:])==nil,"exact resources enable commands")
    try require(scvChoice.hotkey=="S","SCV training displays its command hotkey")
    try require(card.choices(["type":0]).contains{$0.payload["op"] as? String == "patrol"},"Marine card includes patrol")
    try require(!production.contains{$0.payload["op"] as? String == "cancel"},"idle producers do not offer cancellation")
    try require(card.choices(["type":106,"building":true,"buildQueue":[7,228,228,228,228]]).contains{$0.payload["action"] as? String == "train"},"active native queue offers cancellation")
    let repair=card.choices(["type":7,"worker":true]).first{$0.payload["op"] as? String == "repair"}
    try require(repair?.target == .unit,"repair requires a unit target")
    card.accept(["spells":[["action":"storm","title":"Psionic storm","casters":[67],"target":"ground","energy":75]]])
    let storm=card.choices(["type":67]).first{$0.payload["op"] as? String == "spell"}
    try require(storm?.target == .ground && storm?.title.hasSuffix("75 energy")==true,"spell targeting and energy come from native metadata")
    let defiler=card.choices(["type":136,"building":true])
    try require(defiler.contains{$0.title=="Consume" && number($0.payload,"type")==16} && defiler.contains{$0.title=="Plague" && number($0.payload,"type")==15},"Defiler research uses correct technology IDs")
    let larva=card.choices(["type":35])
    try require(larva.contains{$0.payload["op"] as? String == "morph" && number($0.payload,"type")==47} && !larva.contains{number($0.payload,"type")==50},"larva produces Scourge rather than Infested Terrans")
    let evolution=card.choices(["type":139,"building":true]),pool=card.choices(["type":142,"building":true])
    try require(evolution.contains{$0.payload["op"] as? String == "upgrade" && number($0.payload,"type")==10} && !pool.contains{$0.title=="Melee attacks"},"Zerg weapon upgrades belong to Evolution Chamber")
    commander.wrists.pending=CommandChoice("Old repair",["op":"repair"],.unit);commander.selected=["new-selection"]
    commander.wrists.update(commander)
    try require(commander.wrists.pending==nil,"selection changes cancel stale targeting")
    commander.tabletop=true;commander.wrists.zoom=10;let closeHeight=commander.commanderHeight;commander.wrists.zoom=20
    try require(closeHeight==9 && commander.commanderHeight==18,"tabletop zoom changes world viewing distance at a centered head pose")
    commander.tabletop=false;commander.placedEyeHeight=1.65
    commander.camera.position=Terrain3D.worldPosition(7,commander.ground.elevation(7,11),11)
    commander.camera.position.y += 1.65
    commander.toggleTabletop();commander.walk(0,0)
    try require(abs(commander.cameraMapPosition.x-7)<0.00001 && abs(commander.cameraMapPosition.z-11)<0.00001,
        "changing commander scale keeps native ground location fixed")
    for (top,bottom,expected) in [([2.0,0.0],[0.0,2.0],0.0...0.5),([0.0,2.0],[2.0,0.0],0.5...1.0),([2.0,2.0],[0.0,0.0],0.0...1.0)] {
        try require(Terrain3D.exposedEdge(top,bottom)==expected,"cliff edge clips exposed interval \(top)")
    }
    try require(Terrain3D.exposedEdge([0,0],[2,2])==nil,"lower cliff neighbor emits no duplicate wall")
    for height in [0.0,1.0,2.0,4.0] {
        let placed=Terrain3D.worldPosition(7,height,11),native=Terrain3D.mapPosition(placed)
        try require(abs(native.x-7)<0.00001 && abs(native.z-11)<0.00001,"relief hit returns native map coordinates at height \(height)")
        let screenY=Double(placed.z)*sin(Terrain3D.canonicalElevation)-Double(placed.y)*cos(Terrain3D.canonicalElevation)
        try require(abs(screenY-11*sin(Terrain3D.canonicalElevation))<0.00001,"relief preserves native projected ground anchor at height \(height)")
    }
    var continuous=true
    for y in 0..<commander.ground.height-1 {for x in 0..<commander.ground.width-1 {
        let a=commander.ground.surface(x,y),right=commander.ground.surface(x+1,y),below=commander.ground.surface(x,y+1)
        continuous = continuous && a[3]==right[0] && a[2]==right[1] && a[1]==below[0] && a[2]==below[3]
    }}
    try require(continuous,"all loaded cliff and ramp edges share both height endpoints")
    let red=commander.worldSprites.texture(239,frame:0,color:[255,0,0])!
    let blue=commander.worldSprites.texture(239,frame:0,color:[0,0,255])!
    let redPixels=NSBitmapImageRep(data:red.tiffRepresentation!)!,bluePixels=NSBitmapImageRep(data:blue.tiffRepresentation!)!
    var teamPixel=false
    for y in 0..<redPixels.pixelsHigh where !teamPixel {for x in 0..<redPixels.pixelsWide {
        let a=redPixels.colorAt(x:x,y:y)!.usingColorSpace(.deviceRGB)!,b=bluePixels.colorAt(x:x,y:y)!.usingColorSpace(.deviceRGB)!
        if a.redComponent>b.redComponent+0.1 && b.blueComponent>a.blueComponent+0.1 && abs(a.alphaComponent-b.alphaComponent)<0.001 {teamPixel=true;break}
    }}
    try require(teamPixel,"HD team mask changes color while preserving alpha and cache isolation")
    commander.requireFidelity=true
    commander.apply(state(1001,[unit("unapproved")]),[:])
    try require(commander.models.isEmpty && commander.billboards["unapproved"] != nil,"unapproved live model uses the HD image fallback")
    let report:Obj=["passed":true,"checks":passed,"source":"deterministic renderer inputs; not live-game acceptance"]
    let path=argument("--runtime-report").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("runtime-checks.json")
    try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:path,options:.atomic)
    print("Runtime checks passed: \(passed.count)")
}
