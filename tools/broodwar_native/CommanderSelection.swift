import Cocoa
import SceneKit

extension Commander {
    func selectBox(_ rect:NSRect,add:Bool) {
        let candidates=units.keys.sorted().filter { id in
            guard let unit=units[id],number(unit,"owner")==number(state,"player"),
                let node=nodes[id],!node.isHidden,view.isNode(node,insideFrustumOf:camera) else {return false}
            let p=view.projectPoint(node.worldPosition)
            return p.z>=0 && p.z<=1 && rect.contains(NSPoint(x:p.x,y:p.y))
        }
        selected=Array((add ? selected+candidates.filter{!selected.contains($0)}:candidates).prefix(12))
        notice="Selected \(selected.count)"
    }
    func selectSameType() {
        guard let first=selected.first,let original=units[first] else {return}
        selected=Array(units.keys.sorted().filter { id in
            guard let unit=units[id],number(unit,"type")==number(original,"type"),
                number(unit,"owner")==number(state,"player"),let node=nodes[id],!node.isHidden else {return false}
            return view.isNode(node,insideFrustumOf:camera)
        }.prefix(12))
        notice="Selected \(selected.count) of type \(Int(number(original,"type")))"
    }
    func groupKey(_ key:String,modifiers:NSEvent.ModifierFlags)->Bool {
        guard let index=Int(key),index>=0,index<=9 else {return false}
        if modifiers.contains(.control) {
            controlGroups[index]=selected;notice="Group \(index) assigned (\(selected.count))";return true
        }
        guard let group=controlGroups[index] else {return false}
        let alive=group.filter{units[$0] != nil && number(units[$0]!,"owner")==number(state,"player")}
        selected=Array((modifiers.contains(.shift) ? selected+alive.filter{!selected.contains($0)}:alive).prefix(12))
        notice="Group \(index): \(selected.count)";return true
    }
    func toggleTabletop() {
        tabletop.toggle()
        pitch=tabletop ? -1.0:-0.14
        notice=tabletop ? "Tabletop commander · V returns to unit scale":"Unit scale"
    }
    func captureDesktop() {
        let folder=argument("--proof-dir").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("3d-proof/m4/desktop")
        if let desktop=desktopMetal {desktop.requestedCapture=folder;return}
        do {
            try FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true)
            let image=view.snapshot()
            guard let tiff=image.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),
                let data=bitmap.representation(using:.png,properties:[:]) else {throw SceneError.invalid("Desktop capture")}
            let name="frame-\(Int(number(state,"frame")))"
            try data.write(to:folder.appendingPathComponent(name+".png"))
            let evidence:Obj=["frame":number(state,"frame"),"selected":selected,"tabletop":tabletop,
                "controlGroups":Dictionary(uniqueKeysWithValues:controlGroups.map{(String($0.key),$0.value)}),
                "spriteFrames":effects.sprites.renderedFrames]
            try JSONSerialization.data(withJSONObject:evidence,options:.prettyPrinted).write(to:folder.appendingPathComponent(name+".json"))
            notice="Captured \(name)"
        } catch {notice="Capture: \(error)"}
    }
}
