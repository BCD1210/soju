import Cocoa
import SceneKit
import os.signpost

// Construction objects stay on this serial queue. Published geometries and images
// are immutable; changing a mask creates a new material before publication.
final class TerrainWorker {
    struct Request {
        let session:String,center:SIMD2<Int>,radius:Int
        let fog:[Int:UInt8],creep:Set<Int>
    }
    struct Tile {let index:Int,geometry:SCNGeometry,name:String}
    struct Result {let session:String,terrain:[Tile],creep:[Tile],removedTerrain:[Int],removedCreep:[Int],error:String}
    private let queue=DispatchQueue(label:"soju.bw.terrain",qos:.userInitiated)
    private var busy=false,pending:Request?
    private var builder:Builder?
    func submit(_ request:Request,completion:@escaping(Result)->Void) {
        dispatchPrecondition(condition:.onQueue(.main))
        if busy {pending=request;return}
        busy=true
        queue.async { [self] in
            let result:Result=autoreleasepool {
                do {
                    if builder==nil {builder=try Builder()}
                    return builder!.apply(request)
                } catch {return Result(session:request.session,terrain:[],creep:[],removedTerrain:[],removedCreep:[],error:String(describing:error))}
            }
            DispatchQueue.main.async { [self] in
                completion(result);busy=false
                if let next=pending {pending=nil;submit(next,completion:completion)}
            }
        }
    }
    final class Builder {
        let art:Art,ground:Terrain3D,worldSprites=WorldSprites(),fogPresentation=FogPresentation(),world=SCNScene()
        let mapWidth:Int,mapHeight:Int,mapTiles:[Int]
        var fog=[Int:UInt8](),creepTiles=Set<Int>(),terrain=[Int:SCNNode](),creepNodes=[Int:SCNNode]()
        var center=SIMD2<Int>(0,0),radius=20,session="",notice=""
        init()throws {
            art=try Art();ground=try Terrain3D(url:assets.appendingPathComponent("terrain3d.json"))
            let map=art.manifest["map"] as! Obj
            mapWidth=Int(number(map,"width"));mapHeight=Int(number(map,"height"));mapTiles=map["tiles"] as! [Int]
        }
        func apply(_ request:Request)->Result {
            os_signpost(.begin,log:XRFrameMetrics.log,name:"Terrain construct")
            defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Terrain construct")}
            let oldTerrain=terrain.mapValues{$0.geometry!},oldCreep=creepNodes.mapValues{$0.geometry!}
            var dirty=Set<Int>()
            for index in Set(fog.keys).union(request.fog.keys).union(creepTiles).union(request.creep) {
                if fog[index]==request.fog[index] && creepTiles.contains(index)==request.creep.contains(index) {continue}
                let x=index%mapWidth,y=index/mapWidth
                for dy in -1...1 {for dx in -1...1 {
                    let nx=x+dx,ny=y+dy
                    if nx>=0 && nx<mapWidth && ny>=0 && ny<mapHeight {dirty.insert(ny*mapWidth+nx)}
                }}
            }
            let full=session != request.session || center != request.center || radius != request.radius
            if session != request.session {
                for n in terrain.values {n.removeFromParentNode()};for n in creepNodes.values {n.removeFromParentNode()}
                terrain.removeAll();creepNodes.removeAll();fogPresentation.cache.removeAll()
            }
            session=request.session;center=request.center;radius=request.radius;fog=request.fog;creepTiles=request.creep;notice=""
            updateTerrain(dirty:full ? nil:dirty)
            return Result(session:session,
                terrain:terrain.compactMap {i,n in guard let g=n.geometry,oldTerrain[i] !== g else {return nil};return Tile(index:i,geometry:g,name:n.name ?? "terrain")},
                creep:creepNodes.compactMap {i,n in guard let g=n.geometry,oldCreep[i] !== g else {return nil};return Tile(index:i,geometry:g,name:n.name ?? "creep")},
                removedTerrain:oldTerrain.keys.filter{terrain[$0]==nil},removedCreep:oldCreep.keys.filter{creepNodes[$0]==nil},error:notice)
        }
    func updateTerrain(dirty:Set<Int>?=nil) {
        let tiles=art.tileCatalog
        let cx=center.x,cy=center.y,radius=self.radius
        var keep=dirty==nil ? Set<Int>():Set(terrain.keys)
        let candidates:Set<Int>
        if let dirty=dirty {candidates=dirty}
        else {
            var region=Set<Int>()
            for y in max(0,cy-radius)..<max(0,min(mapHeight,cy+radius+1)) {
                for x in max(0,cx-radius)..<max(0,min(mapWidth,cx+radius+1)) {region.insert(y*mapWidth+x)}
            }
            candidates=region
        }
        for index in candidates {
                let x=index%mapWidth,y=index/mapWidth
                guard abs(x-cx)<=radius,abs(y-cy)<=radius,let visibility=fog[index],visibility != 48 else {keep.remove(index);continue}
                keep.insert(index)
                if let n=terrain[index] {
                    let mask=fogPresentation.texture(x:x,y:y,width:mapWidth,fog:fog)
                    if (n.geometry?.firstMaterial?.multiply.contents as? NSImage) !== mask {
                        let geometry=n.geometry!.copy() as! SCNGeometry
                        geometry.materials=n.geometry!.materials.map{$0.copy() as! SCNMaterial}
                        geometry.firstMaterial?.multiply.contents=mask;n.geometry=geometry
                    }
                    continue
                }
                guard let a=tiles[String(mapTiles[index])],let file=a["file"] as? String else { continue }
                do {
                    // HD megatiles already contain the original terrain lighting.
                    let neighbors=(-1...1).flatMap { dy in (-1...1).map { dx -> String in
                        let nx=max(0,min(mapWidth-1,x+dx)),ny=max(0,min(mapHeight-1,y+dy))
                        return tiles[String(mapTiles[ny*mapWidth+nx])]?["file"] as? String ?? file
                    }}
                    let material=try art.terrainMaterial(neighbors)
                    material.multiply.wrapS = .clamp;material.multiply.wrapT = .clamp
                    material.multiply.minificationFilter = .linear;material.multiply.magnificationFilter = .linear
                    let p=ground.geometry(tileX:x,tileY:y,material:material)
                    let n=SCNNode(geometry:p);n.name="terrain"
                    n.castsShadow=false
                    n.position=SCNVector3(CGFloat(x),0,CGFloat(y))
                    p.firstMaterial?.multiply.contents=fogPresentation.texture(x:x,y:y,width:mapWidth,fog:fog)
                    world.rootNode.addChildNode(n);terrain[index]=n
                } catch { notice="Terrain: \(error)" }
        }
        for (i,n) in terrain where !keep.contains(i) { n.removeFromParentNode();terrain.removeValue(forKey:i) }
        let tileset=(art.manifest["map"] as? Obj)?["tileset"] as? String ?? "badlands"
        let visibleCreep=creepTiles.filter{fog[$0]==50}
        var creepKeep=dirty==nil ? Set<Int>():Set(creepNodes.keys)
        for index in candidates {
            guard keep.contains(index),fog[index]==50 else {creepKeep.remove(index);continue}
            let x=index%mapWidth,y=index/mapWidth
            let interior=visibleCreep.contains(index)
            var neighbors=0
            for (bit,offset) in [(1,1),(0,1),(-1,1),(1,0),(-1,0),(1,-1),(0,-1),(-1,-1)].enumerated() {
                let nx=x+offset.0,ny=y+offset.1
                if nx>=0 && nx<mapWidth && ny>=0 && ny<mapHeight && visibleCreep.contains(ny*mapWidth+nx) {neighbors |= 1<<bit}
            }
            let edge=interior ? 0:worldSprites.creepEdgeFrame(tileset:tileset,neighbors:neighbors)
            if !interior && edge==0 {creepKeep.remove(index);continue}
            creepKeep.insert(index)
            let signature=interior ? "creep":"creep-edge-\(edge)"
            if creepNodes[index]?.name==signature {continue}
            creepNodes.removeValue(forKey:index)?.removeFromParentNode()
            let material=interior ? worldSprites.creepMaterial(tileset:tileset,index:x*7+y*13):worldSprites.creepEdgeMaterial(tileset:tileset,frame:edge)
            if let material=material {
                let node=SCNNode(geometry:ground.geometry(tileX:x,tileY:y,material:material))
                node.position=SCNVector3(Double(x),0,Double(y));node.name=signature;node.castsShadow=false
                node.renderingOrder=1
                world.rootNode.addChildNode(node);creepNodes[index]=node
            }
        }
        for (index,node) in creepNodes where !creepKeep.contains(index) {node.removeFromParentNode();creepNodes.removeValue(forKey:index)}
    }
    }
}
