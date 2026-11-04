import Cocoa
import SceneKit
import Metal
import QuartzCore
import CoreImage
import Darwin
import simd
extension Commander {
    func cardCommand(_ choice:CommandChoice,hit:SCNHitTestResult?) {
        guard !selected.isEmpty else {return}
        guard let unit=selected.first.flatMap({units[$0]}) else {return}
        if let reason=choice.unavailable(state,unit) {notice=reason;return}
        if choice.payload["op"] as? String == "select_larvae" {
            selected=Array(owned(35).filter{dist($0,unit)<128*128}.compactMap{$0["id"] as? String}.sorted().prefix(12));return
        }
        var payload=choice.payload
        payload["units"]=["move","attack_move","stop","hold","patrol","gather","repair"].contains(payload["op"] as? String ?? "") ? selected:Array(selected.prefix(1))
        switch choice.target {
        case .none:break
        case .unit:
            guard let hit=hit,let id=unitID(hit.node) else {notice="Aim at a visible unit";return};payload["target"]=id
        case .ground,.rally:
            guard let hit=hit else {return}
            let point=Terrain3D.mapPosition(hit.worldCoordinates)
            if choice.target == .rally,let id=unitID(hit.node) {payload["target"]=id}
            else if ["build","addon","land"].contains(payload["op"] as? String ?? "") {
                payload["tileX"]=max(0,Int(point.x)-1);payload["tileY"]=max(0,Int(point.z)-1)
            } else {payload["x"]=max(0,Int(point.x*32));payload["y"]=max(0,Int(point.z*32))}
        }
        command(payload);wrists.pending=nil
    }

    func updateDesktopCard() {
        wrists.update(self)
        guard desktopCardSignature != wrists.signature else {return}
        desktopCardSignature=wrists.signature
        for child in desktopCard.subviews {child.removeFromSuperview()}
        guard let unit=selected.first.flatMap({units[$0]}) else {return}
        let heading=NSTextField(labelWithString:wrists.card.names[String(Int(number(unit,"type")))] ?? "Commands")
        heading.textColor = .white;heading.frame=NSRect(x:0,y:280,width:400,height:25);desktopCard.addSubview(heading)
        for i in 0..<9 where wrists.page*9+i<wrists.choices.count {
            let index=wrists.page*9+i,choice=wrists.choices[index]
            let button=NSButton(title:choice.label,target:self,action:#selector(desktopCardClick(_:)))
            button.bezelStyle = .regularSquare;button.font=NSFont.systemFont(ofSize:12);button.cell?.wraps=true;button.cell?.isScrollable=false
            button.tag=index;button.isEnabled=choice.unavailable(state,unit)==nil
            button.toolTip=choice.unavailable(state,unit) ?? choice.label
            button.frame=NSRect(x:(i%3)*135,y:200-(i/3)*75,width:130,height:70)
            desktopCard.addSubview(button)
        }
        let pages=max(1,(wrists.choices.count+8)/9)
        for (i,title) in ["Page \(wrists.page+1)/\(pages)","Cancel target"].enumerated() {
            let button=NSButton(title:title,target:self,action:#selector(desktopCardClick(_:)))
            button.tag = -i-1;button.frame=NSRect(x:i*200,y:0,width:195,height:35);desktopCard.addSubview(button)
        }
    }
    @objc func desktopCardClick(_ sender:NSButton) {
        if sender.tag == -1 {wrists.page=(wrists.page+1)%max(1,(wrists.choices.count+8)/9);return}
        if sender.tag == -2 {wrists.pending=nil;return}
        guard wrists.choices.indices.contains(sender.tag) else {return}
        let choice=wrists.choices[sender.tag]
        if choice.target == .none {cardCommand(choice,hit:nil)}
        else {wrists.pending=choice;notice="Right-click target: \(choice.title)"}
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
        if key=="`" {devOverlay.toggle();label?.isHidden = !devOverlay;return}
        if let unit=selected.first.flatMap({units[$0]}) {
            let choices=wrists.card.choices(unit),start=min(wrists.page*9,choices.count),end=min(start+9,choices.count)
            if let choice=choices[start..<end].first(where:{!$0.hotkey.isEmpty && $0.hotkey.lowercased()==key && $0.hotkey != "Esc"}) {
                if let reason=choice.unavailable(state,unit) {notice=reason;return}
                if choice.target == .none {cardCommand(choice,hit:nil)} else {wrists.pending=choice;notice="Aim target: \(choice.title)"}
                return
            }
        }
        if key=="c",let id=selected.first,let unit=units[id] {
            let spells=wrists.card.choices(unit).filter{$0.payload["op"] as? String == "spell"}
            if !spells.isEmpty {
                let previous=spells.firstIndex{$0.title==wrists.pending?.title} ?? -1
                wrists.pending=spells[(previous+1)%spells.count]
            }
            return
        }
        if key=="\u{1b}" {wrists.pending=nil;return}
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
        if let hit=view.hitTest(p,options:[SCNHitTestOption.categoryBitMask:5]).first(where:{unitID($0.node) != nil}),let id=unitID(hit.node),
            let u=units[id],number(u,"owner")==number(state,"player") {
            selected=[id];notice="Selected unit \(Int(number(u,"type")))"
        }
    }
    func move(to p:NSPoint) {
        let hits=view.hitTest(p,options:[SCNHitTestOption.categoryBitMask:5])
        if let choice=wrists.pending {
            let hit=hits.first{choice.target == .unit ? unitID($0.node) != nil:$0.node.name=="terrain"}
            cardCommand(choice,hit:hit);return
        }
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
    func groundCommand(_ worldPoint:SCNVector3) {
        let point=Terrain3D.mapPosition(worldPoint)
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
        diagnosticQueue.async {
            if let data=try? JSONSerialization.data(withJSONObject:object,options:.prettyPrinted) { try? data.write(to:runtime.appendingPathComponent(file),options:.atomic) }
        }
    }
    func command(_ c:Obj) {
        guard argument("--snapshot")==nil else {notice="Recorded snapshot is read-only";return}
        bridge.queue.async {
            do { let ack=try self.bridge.request(c);DispatchQueue.main.async { self.notice="\(c["op"]!) sent at game frame \(Int(number(ack,"frame")))";self.record("scene-last-command.json",["command":c,"ack":ack,"time":Date().timeIntervalSince1970]) } }
            catch { DispatchQueue.main.async { self.notice="Command: \(error)" } }
        }
    }
}
