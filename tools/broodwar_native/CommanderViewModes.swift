import Cocoa
import SceneKit
import simd

struct CommanderViewBookmark {
    let position:SIMD3<Float>,yaw:CGFloat,pitch:CGFloat,eyeHeight:Double,tabletop:Bool
}

enum PossessionPresentation {
    static let hiddenCategory = 8
    static let pickCategories = 1 | 4 | hiddenCategory
    static let nearDistance:Float = 0.60
    static func nearBounds(lower:SIMD3<Float>,upper:SIMD3<Float>,transform:simd_float4x4,eye:SIMD3<Float>)->Bool {
        var lo=SIMD3<Float>(repeating:.infinity),hi=SIMD3<Float>(repeating:-.infinity)
        for x in [lower.x,upper.x] {for y in [lower.y,upper.y] {for z in [lower.z,upper.z] {
            let p=transform*SIMD4<Float>(x,y,z,1),v=SIMD3<Float>(p.x,p.y,p.z)
            lo=simd_min(lo,v);hi=simd_max(hi,v)
        }}}
        return simd_distance(eye,simd_clamp(eye,lo,hi))<=nearDistance
    }
    static func near(_ node:SCNNode,eye:SIMD3<Float>)->Bool {
        let box=node.boundingBox
        return nearBounds(lower:SIMD3<Float>(box.min),upper:SIMD3<Float>(box.max),transform:node.simdWorldTransform,eye:eye)
    }
    // Enclose the complete near rectangle and eye, including asymmetric stereo projections.
    static func eyeVolume(transform:simd_float4x4,projection:simd_float4x4)->(SIMD3<Float>,Float) {
        let p=transform.columns.3,eye=SIMD3<Float>(p.x,p.y,p.z),inverse=simd_inverse(projection)
        var radius:Float=0
        for x:Float in [-1,1] {for y:Float in [-1,1] {
            let h=inverse*SIMD4<Float>(x,y,-1,1),world=transform*(h/h.w)
            radius=max(radius,simd_distance(eye,SIMD3<Float>(world.x,world.y,world.z)))
        }}
        return (eye,radius+0.02)
    }
    static func intersectsEye(center:SIMD3<Float>,radius:Float,volume:(SIMD3<Float>,Float))->Bool {
        simd_distance(center,volume.0)<=radius+volume.1
    }
    private static let originalCategories=NSMapTable<SCNNode,NSNumber>.weakToStrongObjects()
    static func mask(_ node:SCNNode,hidden:Bool) {
        guard node.geometry != nil else {return}
        if hidden {
            if originalCategories.object(forKey:node)==nil {
                originalCategories.setObject(NSNumber(value:node.categoryBitMask),forKey:node)
            }
            node.categoryBitMask=hiddenCategory
        } else if let original=originalCategories.object(forKey:node) {
            node.categoryBitMask=original.intValue;originalCategories.removeObject(forKey:node)
        }
    }
    static func hiddenBodies(_ billboards:[UnitBillboard],eye:SIMD3<Float>,possessed:String?)->Set<String> {
        guard let possessed=possessed else {return []}
        return Set(billboards.compactMap { b in
            guard let id=b.root.name else {return nil}
            return id==possessed || near(b.root,eye:eye) ? id:nil
        })
    }
}

extension Commander {
    var comfortFade:Float {Float(max(0,1-(CACurrentMediaTime()-comfortStarted)/0.35))}
    var viewBookmark:CommanderViewBookmark {
        CommanderViewBookmark(position:camera.simdPosition,yaw:yaw,pitch:pitch,eyeHeight:placedEyeHeight,tabletop:tabletop)
    }
    func restoreView(_ bookmark:CommanderViewBookmark) {
        camera.simdPosition=bookmark.position;yaw=bookmark.yaw;pitch=bookmark.pitch
        placedEyeHeight=bookmark.eyeHeight;tabletop=bookmark.tabletop
    }
    var terrainRadius:Int {tabletop ? max(mapWidth,mapHeight):camera.camera?.usesOrthographicProjection == true ? 44:20}
    func recenterTable() {
        let center=SIMD3<Float>(Float(mapWidth)*0.5,0,Float(mapHeight)*0.5)
        let offset=simd_quatf(angle:Float(yaw),axis:SIMD3<Float>(0,1,0)).act(SIMD3<Float>(0,tableHeight,1.25)*wrists.zoom)
        camera.camera?.zFar=Double(max(Float(max(mapWidth,mapHeight)),wrists.zoom))*4
        camera.simdPosition=center+offset
        placedEyeHeight=Double(offset.y)
    }
    func adjustTable(scale:Float,height:Float,rotation:Float) {
        guard tabletop else {return}
        wrists.zoom=max(4,min(256,wrists.zoom*exp(scale)))
        tableHeight=max(0.25,min(1.5,tableHeight+height));yaw+=CGFloat(rotation)
        recenterTable()
    }
    func enterPossession(_ id:String) {
        guard possessedUnit==nil,let unit=units[id],canPossess(unit) else {return}
        comfortStarted=CACurrentMediaTime()
        possessionReturn=viewBookmark;possessedUnit=id;tabletop=false;selected=[id]
        wrists.pending=nil;wrists.dragStart=nil
        let turret=unit["turret"] as? Obj ?? unit
        yaw = -CGFloat(number(turret,"facing"))*2 * .pi/256;pitch=0
        camera.simdPosition=possessionEye(unit);placedEyeHeight=Double(camera.position.y)-ground.elevation(number(unit,"x")/32,number(unit,"y")/32)
        updatePossessionVisibility()
        notice="Unit view · Escape or right grip exits"
    }
    func canPossess(_ unit:Obj)->Bool {
        number(unit,"owner")==number(state,"player") && number(unit,"hp")>0 &&
        unit["building"] as? Bool != true && unit["dead"] as? Bool != true &&
        unit["transported"] as? Bool != true && unit["burrowed"] as? Bool != true && unit["renderHidden"] as? Bool != true
    }
    func possessionEye(_ unit:Obj)->SIMD3<Float> {
        let x=number(unit,"x")/32,z=number(unit,"y")/32
        let definition=modelLibrary.catalog.definitions[Int(number(unit,"type"))]
        let eyeHeight=max(0.20,(definition?.targetHeight ?? 1.0)*0.86)
        let flight=unit["airborne"] as? Bool == true ? (definition?.flightHeight ?? 1.5):0
        let height=ground.elevation(x,z)+(definition?.elevation ?? 0)+flight+eyeHeight
        return SIMD3<Float>(Float(x),Float(height),Float(z))
    }
    func exitPossession() {
        guard possessedUnit != nil else {return}
        comfortStarted=CACurrentMediaTime();motionVignette=0
        possessedUnit=nil;updatePossessionVisibility()
        if let bookmark=possessionReturn {restoreView(bookmark)}
        possessionReturn=nil;wrists.pending=nil;notice="Returned to commander view"
    }
    func updatePossession(_ dt:Double) {
        guard let id=possessedUnit else {return}
        guard let unit=units[id],canPossess(unit) else {exitPossession();return}
        let target=possessionEye(unit)
        motionVignette=min(0.7,simd_distance(target,camera.simdPosition)*0.6)
        camera.simdPosition += (target-camera.simdPosition)*Float(1-exp(-dt/0.08))
        selected=[id]
        updatePossessionVisibility()
    }
    func updatePossessionVisibility(eye:SIMD3<Float>?=nil) {
        guard sceneWorker==nil else {return}
        camera.camera?.categoryBitMask = ~PossessionPresentation.hiddenCategory
        #if SOJU_OPENXR
        xrCamera.camera?.categoryBitMask = ~PossessionPresentation.hiddenCategory
        #endif
        let eye=eye ?? camera.simdWorldPosition
        for (id,model) in models {
            let hide=possessedUnit != nil && (id==possessedUnit ||
                PossessionPresentation.near(model.root,eye:eye))
            for node in model.allNodes {PossessionPresentation.mask(node,hidden:hide)}
            PossessionPresentation.mask(model.selection,hidden:hide)
            model.bars.root.enumerateChildNodes {node,_ in PossessionPresentation.mask(node,hidden:hide)}
        }
        let hidden=PossessionPresentation.hiddenBodies(Array(billboards.values),eye:eye,possessed:possessedUnit)
        for (id,billboard) in billboards {
            for (index,node) in billboard.layers {
                let shadow=number(billboard.nativeLayers[index] ?? [:],"drawfunc")==10
                PossessionPresentation.mask(node,hidden:hidden.contains(id) && !shadow)
            }
        }
    }
    func possessionMove(side:Float,forward:Float,heading:Float,now:Double) {
        guard let id=possessedUnit,let unit=units[id],canPossess(unit),now-lastPossessionOrder>=0.15,
              abs(side)>0.15 || abs(forward)>0.15 else {return}
        let dx = -sin(heading)*forward+cos(heading)*side
        let dz = -cos(heading)*forward-sin(heading)*side
        let x=max(0,min(Float(mapWidth*32-1),Float(number(unit,"x"))+dx*96))
        let y=max(0,min(Float(mapHeight*32-1),Float(number(unit,"y"))+dz*96))
        command(["op":"move","units":[id],"x":Int(x),"y":Int(y)]);lastPossessionOrder=now
    }
    func possessionAttack(_ hit:ScenePickHit) {
        guard let id=possessedUnit else {return}
        if let target=hit.unitID,target != id {command(["op":"attack","units":[id],"target":target])}
        else {
            let p=Terrain3D.mapPosition(hit.worldCoordinates)
            command(["op":"attack_move","units":[id],"x":Int(p.x*32),"y":Int(p.z*32)])
        }
    }
}
