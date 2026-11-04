import Cocoa
import SceneKit
import simd

#if SOJU_OPENXR
extension Commander {
    var worldUnitsPerMeter:Float {tabletop ? wrists.zoom:1}
    var xrBody:simd_quatf {
        simd_quatf(angle:Float(yaw)-xrOriginYaw,axis:SIMD3<Float>(0,1,0))*simd_quatf(angle:tabletop ? -0.75:0,axis:SIMD3<Float>(1,0,0))
    }
    func handPosition(_ position:SIMD3<Float>)->SIMD3<Float> {
        camera.simdPosition+xrBody.act((position-(xrOrigin ?? .zero))*worldUnitsPerMeter)
    }
    func controls(_ input:SojuXRInput) {
        let now=CACurrentMediaTime(),dt=Float(min(0.05,now-lastXRInput));lastXRInput=now
        defer {
            let edge=input.leftPressed != 0 || input.rightPressed != 0 || input.leftReleased != 0 ||
                input.rightReleased != 0 || input.leftGripPressed != 0 || input.rightGripPressed != 0
            if edge || now-lastControllerLog>1 {
                lastControllerLog=now
                let sample:Obj=["time":Date().timeIntervalSince1970,"frame":number(state,"frame"),
                    "leftPressed":input.leftPressed,"leftReleased":input.leftReleased,"rightPressed":input.rightPressed,
                    "leftHeld":input.leftHeld,"rightHeld":input.rightHeld,"leftGrip":input.leftGripHeld,"rightGrip":input.rightGripHeld,
                    "selected":selected,"boxSelections":wrists.boxSelections,"doubleTaps":wrists.doubleTaps,
                    "grabs":wrists.grabs,"zoom":wrists.zoom,"camera":[camera.position.x,camera.position.y,camera.position.z],
                    "aimValid":input.aimValid,"leftValid":input.leftValid,"pendingSpell":wrists.pending?.title ?? "",
                    "dragging":wrists.dragStart != nil]
                record("xr-controller-diagnostics.json",sample)
                if edge {
                    controllerHistory.append(sample)
                    if controllerHistory.count>256 {controllerHistory.removeFirst()}
                    record("xr-controller-events.json",["events":controllerHistory])
                }
            }
        }
        guard xrOrigin != nil else {return}
        if input.leftGripPressed != 0 && input.rightGripHeld==0 {toggleTabletop()}
        let raw=SIMD3<Float>(input.px,input.py,input.pz),left=SIMD3<Float>(input.leftX,input.leftY,input.leftZ)
        if input.rightGripHeld != 0 && input.aimValid != 0 {
            if input.leftGripHeld != 0 && input.leftValid != 0 {
                let distance=simd_distance(raw,left)
                if let old=wrists.grabDistance,distance>0.08 {wrists.zoom=max(3,min(60,wrists.zoom*old/distance))}
                wrists.grabDistance=distance
            } else if let old=wrists.grab {
                let delta=xrBody.act((old-raw)*worldUnitsPerMeter)
                camera.position.x+=CGFloat(delta.x);camera.position.z+=CGFloat(delta.z)
            } else {wrists.grabs+=1}
            wrists.grab=raw
        } else {wrists.grab=nil;wrists.grabDistance=nil}
        if abs(input.turnX)<0.4 {turnHeld=false}
        if abs(input.turnX)>0.7 && !turnHeld {yaw-=input.turnX>0 ? .pi/6 : -.pi/6;turnHeld=true}
        let dx=abs(input.moveX)>0.15 ? input.moveX:0,dy=abs(input.moveY)>0.15 ? input.moveY:0
        walk(CGFloat((-sin(Float(yaw))*dy+cos(Float(yaw))*dx)*3*dt*worldUnitsPerMeter),CGFloat((-cos(Float(yaw))*dy-sin(Float(yaw))*dx)*3*dt*worldUnitsPerMeter))
        wrists.update(self)
        for (node,valid,position,rotation) in [(wrists.commands,input.leftValid,left,simd_quatf(ix:input.leftQx,iy:input.leftQy,iz:input.leftQz,r:input.leftQw)),
            (wrists.resources,input.aimValid,raw,simd_quatf(ix:input.qx,iy:input.qy,iz:input.qz,r:input.qw))] {
            node.isHidden=valid==0;node.simdScale=SIMD3<Float>(repeating:worldUnitsPerMeter*0.65)
            node.simdPosition=handPosition(position)+xrBody.act(rotation.act(SIMD3<Float>(0,0.12,-0.25))*worldUnitsPerMeter)
            node.simdOrientation=xrBody*rotation
        }
        guard input.aimValid != 0 else {reticle.isHidden=true;wrists.ray.isHidden=true;wrists.dragStart=nil;wrists.box.geometry=nil;return}
        let from=handPosition(raw),direction=(xrBody*simd_quatf(ix:input.qx,iy:input.qy,iz:input.qz,r:input.qw)).act(SIMD3<Float>(0,0,-1))
        let end=from+direction*100
        let hits=world.rootNode.hitTestWithSegment(from:SCNVector3(from.x,from.y,from.z),to:SCNVector3(end.x,end.y,end.z),options:[SCNHitTestOption.searchMode.rawValue:SCNHitTestSearchMode.closest.rawValue,SCNHitTestOption.categoryBitMask.rawValue:5])
        let hit=hits.first(where:{$0.node.name?.hasPrefix("card-")==true || $0.node.name=="terrain" || unitID($0.node) != nil})
        wrists.ray.isHidden=false
        wrists.line(from:SCNVector3(from.x,from.y,from.z),to:hit?.worldCoordinates ?? SCNVector3(end.x,end.y,end.z),color:wrists.pending==nil ? .cyan:.orange)
        guard let hit=hit else {
            if input.leftHeld==0 {wrists.dragStart=nil;wrists.box.geometry=nil}
            reticle.isHidden=true;return
        }
        reticle.isHidden=false;reticle.position=hit.worldCoordinates;reticle.position.y+=0.07
        if input.rightPressed != 0,let name=hit.node.name,name.hasPrefix("card-"),let index=Int(name.dropFirst(5)) {
            wrists.cardClicks+=1
            switch index {
            case -1:wrists.page=(wrists.page+1)%max(1,(wrists.choices.count+8)/9)
            case -2:toggleTabletop()
            case -3:wrists.pending=nil
            default:
                guard wrists.choices.indices.contains(index) else {return}
                let choice=wrists.choices[index]
                if let unit=selected.first.flatMap({units[$0]}),let reason=choice.unavailable(state,unit) {notice=reason;return}
                if choice.target == .none {cardCommand(choice,hit:nil)}
                else {wrists.pending=choice;notice="Aim target: \(choice.title)"}
            }
            return
        }
        let point=hit.worldCoordinates
        if input.leftPressed != 0 || (input.leftHeld != 0 && wrists.dragStart==nil) {wrists.dragStart=point;wrists.dragUnit=unitID(hit.node)}
        if let start=wrists.dragStart,input.leftHeld != 0 {
            let points=[start,SCNVector3(point.x,start.y,start.z),point,SCNVector3(start.x,point.y,point.z),start].map{SCNVector3($0.x,$0.y+0.1,$0.z)}
            let geometry=SCNGeometry(sources:[SCNGeometrySource(vertices:points)],elements:[SCNGeometryElement(indices:[Int32(0),1,1,2,2,3,3,4],primitiveType:.line)])
            let material=SCNMaterial();material.lightingModel = .constant;material.diffuse.contents=NSColor.green;geometry.materials=[material];wrists.box.geometry=geometry
        }
        if input.leftReleased != 0,let start=wrists.dragStart {
            wrists.dragStart=nil;wrists.box.geometry=nil
            if hypot(point.x-start.x,point.z-start.z)>0.5 && (wrists.dragUnit==nil || wrists.dragUnit != unitID(hit.node)) {
                selected=Array(units.keys.sorted().filter {id in
                    guard let u=units[id],number(u,"owner")==number(state,"player"),let node=nodes[id],!node.isHidden else {return false}
                    return node.position.x>=min(start.x,point.x) && node.position.x<=max(start.x,point.x) && node.position.z>=min(start.z,point.z) && node.position.z<=max(start.z,point.z) && xrRenderer?.isNode(node,insideFrustumOf:xrCamera)==true
                }.prefix(12));wrists.boxSelections+=1
            } else if let id=unitID(hit.node),let unit=units[id],number(unit,"owner")==number(state,"player") {
                selected=[id]
                if now-wrists.lastTap<0.4 && wrists.lastUnit==id {
                    selected=Array(units.keys.sorted().filter {candidate in
                        guard let u=units[candidate],let node=nodes[candidate],!node.isHidden else {return false}
                        return number(u,"owner")==number(state,"player") && number(u,"type")==number(unit,"type") && xrRenderer?.isNode(node,insideFrustumOf:xrCamera)==true
                    }.prefix(12));wrists.doubleTaps+=1
                }
                wrists.lastTap=now;wrists.lastUnit=id;wrists.page=0
            }
        }
        if input.rightPressed != 0 && hit.node.name?.hasPrefix("card-") != true {
            if let choice=wrists.pending {cardCommand(choice,hit:hit)}
            else if let id=unitID(hit.node),let unit=units[id] {
                if !contextCommand(id),number(unit,"owner")==number(state,"player") {selected=[id];wrists.page=0}
            } else if !selected.isEmpty {groundCommand(point)}
        }
        if now-lastAimLog>1 || input.leftPressed != 0 || input.rightPressed != 0 || input.leftReleased != 0 {
            lastAimLog=now
            record("xr-input-diagnostics.json",["time":Date().timeIntervalSince1970,"selected":selected,"leftPressed":input.leftPressed,"rightPressed":input.rightPressed,
                "leftHeld":input.leftHeld,"rightHeld":input.rightHeld,"leftReleased":input.leftReleased,"hit":hit.node.name ?? "","hitUnit":unitID(hit.node) ?? "","point":[point.x,point.y,point.z],
                "leftValid":input.leftValid,"aimValid":input.aimValid,"worldUnitsPerMeter":worldUnitsPerMeter,"boxSelections":wrists.boxSelections,"doubleTaps":wrists.doubleTaps,"cardClicks":wrists.cardClicks])
        }
    }
}
#endif
