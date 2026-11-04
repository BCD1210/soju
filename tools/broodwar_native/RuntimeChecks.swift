import Cocoa
import SceneKit
import Metal
import GLTFKit2

// Deterministic renderer regression tests; live native-engine acceptance is
// recorded separately. No commands are issued to the game by these checks.
func runtimeChecks()throws {
    let commander=try Commander(art:Art())
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
    let card=CommandCard();card.accept(["types":[["id":7,"mineralCost":75,"gasCost":9]]])
    let production=card.choices(["type":106,"building":true])
    try require(production.contains{$0.payload["op"] as? String == "train" && $0.title.hasSuffix("75/9")},"command card uses native DAT prices")
    let repair=card.choices(["type":7,"worker":true]).first{$0.payload["op"] as? String == "repair"}
    try require(repair?.target == .unit,"repair requires a unit target")
    let evolution=card.choices(["type":139,"building":true]),pool=card.choices(["type":142,"building":true])
    try require(evolution.contains{$0.payload["op"] as? String == "upgrade" && number($0.payload,"type")==10} && !pool.contains{$0.title=="Melee attacks"},"Zerg weapon upgrades belong to Evolution Chamber")
    commander.wrists.pending=CommandChoice("Old repair",["op":"repair"],.unit);commander.selected=["new-selection"]
    commander.wrists.update(commander)
    try require(commander.wrists.pending==nil,"selection changes cancel stale targeting")
    commander.tabletop=true;commander.wrists.zoom=10;let closeHeight=commander.commanderHeight;commander.wrists.zoom=20
    try require(closeHeight==9 && commander.commanderHeight==18,"tabletop zoom changes world viewing distance at a centered head pose")
    let report:Obj=["passed":true,"checks":passed,"source":"deterministic renderer inputs; not live-game acceptance"]
    let path=argument("--runtime-report").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("runtime-checks.json")
    try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:path,options:.atomic)
    print("Runtime checks passed: \(passed.count)")
}
