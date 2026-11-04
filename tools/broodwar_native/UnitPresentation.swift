import Cocoa
import SceneKit

// Segmented bars remain legible from commander distance and track native HP.
final class UnitBars {
    let root=SCNNode(),health=SCNNode(),shield=SCNNode()
    var lastHP = -1.0,lastShield = -1.0
    init() {
        let billboard=SCNBillboardConstraint();billboard.freeAxes = .all
        root.constraints=[billboard];root.categoryBitMask=2;root.castsShadow=false
        for (index,node) in [health,shield].enumerated() {
            let plane=SCNPlane(width:0.9,height:0.065)
            let material=SCNMaterial();material.lightingModel = .constant
            material.isDoubleSided=true;material.writesToDepthBuffer=false
            plane.materials=[material];node.geometry=plane;node.position.y=CGFloat(index)*0.09
            node.castsShadow=false;node.categoryBitMask=2;root.addChildNode(node)
        }
    }
    static func texture(_ fraction:Double,_ color:NSColor)->NSImage {
        let image=NSImage(size:NSSize(width:120,height:10))
        image.lockFocus();NSColor.black.setFill();NSRect(x:0,y:0,width:120,height:10).fill()
        color.setFill()
        for i in 0..<12 where Double(i)/12 < fraction {
            NSRect(x:CGFloat(i)*10+1,y:2,width:8,height:6).fill()
        }
        image.unlockFocus();return image
    }
    func apply(_ unit:Obj) {
        let hp=max(0,min(1,number(unit,"hp")/max(1,number(unit,"maxHP"))))
        let sp=max(0,min(1,number(unit,"shields")/max(1,number(unit,"maxShields"))))
        if hp != lastHP {
            health.geometry?.firstMaterial?.diffuse.contents=Self.texture(hp,hp>0.66 ? .green:hp>0.33 ? .yellow:.red)
            lastHP=hp
        }
        if sp != lastShield {
            shield.geometry?.firstMaterial?.diffuse.contents=Self.texture(sp,.cyan);lastShield=sp
        }
        shield.isHidden=number(unit,"maxShields")<=0
    }
}

// Shared corner values make adjacent revealed tiles continuous. Corners touching
// unexplored space are black, so smoothing never reveals unexplored terrain.
final class FogPresentation {
    var cache=[String:NSImage]()
    func texture(x:Int,y:Int,width:Int,fog:[Int:UInt8])->NSImage {
        func value(_ x:Int,_ y:Int)->Double {
            guard x>=0,x<width,y>=0 else {return 0}
            let state=fog[y*width+x] ?? 48
            return state==50 ? 1:state==49 ? 0.35:0
        }
        func corner(_ x:Int,_ y:Int)->Double {
            let v=[value(x-1,y-1),value(x,y-1),value(x-1,y),value(x,y)]
            return v.contains(0) ? 0:v.reduce(0,+)/4
        }
        let values=[corner(x,y),corner(x+1,y),corner(x,y+1),corner(x+1,y+1)]
        let key=values.map{String($0)}.joined(separator:",")
        if let image=cache[key] {return image}
        let image=NSImage(size:NSSize(width:32,height:32));image.lockFocus()
        for py in 0..<32 {for px in 0..<32 {
            let u=(Double(px)+0.5)/32,v=(Double(py)+0.5)/32
            let a=u*u*(3-2*u),b=v*v*(3-2*v)
            let shade=(values[0]*(1-a)+values[1]*a)*(1-b)+(values[2]*(1-a)+values[3]*a)*b
            NSColor(white:shade,alpha:1).setFill();NSRect(x:px,y:31-py,width:1,height:1).fill()
        }}
        image.unlockFocus()
        if cache.count>512 {cache.removeAll()}
        cache[key]=image;return image
    }
}
