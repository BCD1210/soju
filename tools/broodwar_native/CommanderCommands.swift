import Cocoa
import SceneKit
import Metal
import QuartzCore
import CoreImage
import Darwin
import simd
extension Commander {
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
        if key=="v" {toggleTabletop();return}
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