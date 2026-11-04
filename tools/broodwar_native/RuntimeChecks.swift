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
    let death:Obj=["kind":"death","unit":"b","seq":2,"state":unit("b")]
    commander.apply(state(24,[unit("a")],[death]),[:])
    try require(commander.models["b"]==nil && commander.retiring["b"]?.dying==true && commander.deathCount==1,"confirmed death retires exactly one model")
    commander.apply(state(25,[unit("a")],[death]),[:])
    try require(commander.deathCount==1,"replayed event is ignored")
    commander.apply(state(26,[]),[:])
    try require(commander.models.isEmpty && commander.deathCount==1,"fog disappearance does not create death")
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
    let report:Obj=["passed":true,"checks":passed,"source":"deterministic renderer inputs; not live-game acceptance"]
    let path=argument("--runtime-report").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("runtime-checks.json")
    try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:path,options:.atomic)
    print("Runtime checks passed: \(passed.count)")
}
