import Cocoa
import SceneKit
import simd

final class WristPanels {
    let commands=SCNNode(),resources=SCNNode(),ray=SCNNode(),box=SCNNode()
    let card=CommandCard()
    var selection=[String]()
    var choices=[CommandChoice](),page=0,signature="",lastMap=0.0
    var dragUnit:String?
    var pending:CommandChoice?,dragStart:SCNVector3?,lastTap=0.0,lastUnit=""
    var grab:SIMD3<Float>?,grabDistance:Float?,zoom:Float=20
    var cardClicks=0,boxSelections=0,doubleTaps=0,grabs=0
    init() {commands.name="wrist-commands";resources.name="wrist-resources";ray.name="target-ray";box.name="selection-box";ray.categoryBitMask=2;box.categoryBitMask=2;resources.categoryBitMask=2}
    func panel(_ title:String,size:CGSize,enabled:Bool=true)->SCNGeometry {
        let image=NSImage(size:NSSize(width:600,height:180))
        image.lockFocus();NSColor(calibratedRed:0.025,green:0.045,blue:0.065,alpha:0.96).setFill()
        NSRect(x:0,y:0,width:600,height:180).fill()
        (title as NSString).draw(in:NSRect(x:16,y:12,width:570,height:156),withAttributes:[.font:NSFont.systemFont(ofSize:38,weight:.medium),.foregroundColor:enabled ? NSColor.white:NSColor.gray])
        image.unlockFocus()
        let geometry=SCNPlane(width:size.width,height:size.height),material=SCNMaterial()
        material.lightingModel = .constant;material.diffuse.contents=image;material.isDoubleSided=true
        geometry.materials=[material];return geometry
    }
    func update(_ commander:Commander) {
        if selection != commander.selected {pending=nil;dragStart=nil;selection=commander.selected;page=0}
        let selected=commander.selected.first.flatMap{commander.units[$0]}
        let next=selected.map{card.choices($0)} ?? []
        let key="\(commander.selected):\(next.map{$0.label}):\(next.map{$0.unavailable(commander.state,selected ?? [:]) ?? ""}):\(page):\(commander.tabletop)"
        if signature != key {
            signature=key;choices=next
            let pages=max(1,(choices.count+8)/9);page=min(page,pages-1)
            for child in commands.childNodes {child.removeFromParentNode()}
            let title=selected.map{card.names[String(Int(number($0,"type")))] ?? "Unit"} ?? "Select units"
            let header=SCNNode(geometry:panel(title,size:CGSize(width:0.42,height:0.055)))
            header.categoryBitMask=2;header.position.y=0.18;commands.addChildNode(header)
            for i in 0..<9 where page*9+i<choices.count {
                let entry=choices[page*9+i],node=SCNNode(geometry:panel(entry.label,size:CGSize(width:0.135,height:0.075),enabled:entry.unavailable(commander.state,selected ?? [:])==nil))
                node.name="card-\(page*9+i)";node.position=SCNVector3(Double(i%3-1)*0.14,0.10-Double(i/3)*0.08,0)
                commands.addChildNode(node)
            }
            for (i,title) in ["Page \(page+1)/\(pages)",commander.tabletop ? "Unit scale":"Tabletop","Cancel target"].enumerated() {
                let node=SCNNode(geometry:panel(title,size:CGSize(width:0.135,height:0.055)))
                node.name="card-\(-i-1)";node.position=SCNVector3(Double(i-1)*0.14,-0.14,0);commands.addChildNode(node)
            }
        }
        guard CACurrentMediaTime()-lastMap>0.25 else {return};lastMap=CACurrentMediaTime()
        let image=NSImage(size:NSSize(width:512,height:600));image.lockFocus()
        NSColor(calibratedWhite:0.025,alpha:0.96).setFill();NSRect(x:0,y:0,width:512,height:600).fill()
        let info="Minerals \(Int(number(commander.state,"minerals")))   Gas \(Int(number(commander.state,"gas")))"
        (info as NSString).draw(at:NSPoint(x:15,y:550),withAttributes:[.font:NSFont.systemFont(ofSize:24,weight:.semibold),.foregroundColor:NSColor.white])
        let supplies=(commander.state["supplies"] as? [Obj] ?? []).map{"\(Int(number($0,"used"))/2)/\(Int(number($0,"provided"))/2)"}.joined(separator:"   ")
        ("Supply Z / T / P: "+supplies as NSString).draw(at:NSPoint(x:15,y:514),withAttributes:[.font:NSFont.systemFont(ofSize:21),.foregroundColor:NSColor.white])
        let width=max(1,commander.mapWidth),height=max(1,commander.mapHeight)
        for (index,visibility) in commander.fog where visibility != 48 {
            NSColor(calibratedWhite:visibility==50 ? 0.23:0.10,alpha:1).setFill()
            NSRect(x:Double(index%width)*480/Double(width)+16,y:Double(height-1-index/width)*480/Double(height)+16,width:480/Double(width)+1,height:480/Double(height)+1).fill()
        }
        for unit in commander.units.values where unit["renderHidden"] as? Bool != true {
            (number(unit,"owner")==number(commander.state,"player") ? NSColor.green:NSColor.red).setFill()
            NSRect(x:number(unit,"x")/32/Double(width)*480+14,y:(1-number(unit,"y")/32/Double(height))*480+14,width:5,height:5).fill()
        }
        image.unlockFocus()
        if resources.geometry==nil {resources.geometry=panel("",size:CGSize(width:0.34,height:0.4))}
        resources.geometry?.firstMaterial?.diffuse.contents=image
    }
    func line(from:SCNVector3,to:SCNVector3,color:NSColor) {
        let geometry=SCNGeometry(sources:[SCNGeometrySource(vertices:[from,to])],elements:[SCNGeometryElement(indices:[Int32(0),1],primitiveType:.line)])
        let material=SCNMaterial();material.lightingModel = .constant;material.diffuse.contents=color
        geometry.materials=[material];ray.geometry=geometry
    }
}
