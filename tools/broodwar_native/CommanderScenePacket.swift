import Cocoa
import SceneKit
import simd

extension Commander {
    func scenePick(_ hit:SCNHitTestResult,origin:SIMD3<Float> = .zero)->ScenePickHit? {
        let target:ScenePickHit.Target
        if let id=unitID(hit.node) {target = .unit(id)}
        else if hit.node.name=="terrain" {target = .terrain}
        else if let name=hit.node.name,name.hasPrefix("card-"),let index=Int(name.dropFirst(5)) {target = .card(index)}
        else {return nil}
        let position=SIMD3<Float>(Float(hit.worldCoordinates.x),Float(hit.worldCoordinates.y),Float(hit.worldCoordinates.z))
        return ScenePickHit(target:target,position:position,distance:simd_distance(position,origin))
    }
    func sceneCamera(aspect:Float)->MetalBatches.CameraPacket {
        let near=Float(camera.camera!.zNear)*(tabletop ? wrists.zoom:1),far=Float(camera.camera!.zFar)
        var projection=matrix_identity_float4x4
        if camera.camera!.usesOrthographicProjection {
            let scale=Float(camera.camera!.orthographicScale)
            projection.columns.0.x=1/(scale*aspect);projection.columns.1.y=1/scale
            projection.columns.2.z = -2/(far-near);projection.columns.3.z = -(far+near)/(far-near)
        } else {
            let f=1/tan(Float(camera.camera!.fieldOfView)*Float.pi/360)
            projection.columns.0.x=f/aspect;projection.columns.1.y=f
            projection.columns.2=SIMD4<Float>(0,0,-(far+near)/(far-near),-1)
            projection.columns.3=SIMD4<Float>(0,0,-2*far*near/(far-near),0)
        }
        return MetalBatches.CameraPacket(transform:camera.simdWorldTransform,projection:projection,orientation:camera.simdWorldOrientation,orthographic:camera.camera!.usesOrthographicProjection)
    }
    func sendSceneView() {
        guard let worker=sceneWorker,let batches=metalBatches else {return}
        let aspect=Float(max(1,view.bounds.width)/max(1,view.bounds.height))
        desktopCameraPacket=sceneCamera(aspect:aspect)
        #if SOJU_OPENXR
        if xrRequested,let feedback=metalXRLoop?.latest(),let pose=feedback.camera,let center=feedback.center {
            hud.isHidden = !devOverlay
            hud.simdPosition=center+pose.orientation.act(SIMD3<Float>(0,0.45,-1.5)*worldUnitsPerMeter)
            hud.simdOrientation=pose.orientation;hud.simdScale=SIMD3<Float>(repeating:worldUnitsPerMeter)
            if devOverlay {
                let text="Selected: \(selected.count)   Minerals: \(Int(number(state,"minerals")))   Action: \(actionNames[actionMode])\nLeft trigger: select / drag   Right trigger: command   Left grip: scale\n\(notice)"
                if text != hudText {
                    hudText=text
                    let image=NSImage(size:NSSize(width:1200,height:180));image.lockFocus()
                    NSColor.black.withAlphaComponent(0.8).setFill();NSRect(x:0,y:0,width:1200,height:180).fill()
                    (text as NSString).draw(in:NSRect(x:20,y:12,width:1160,height:158),withAttributes:[.font:NSFont.monospacedSystemFont(ofSize:30,weight:.medium),.foregroundColor:NSColor.white])
                    image.unlockFocus();hud.geometry?.firstMaterial?.diffuse.contents=image
                }
            }
        }
        #endif
        wrists.overlayImages.begin()
        var overlays=[SceneOverlay]()
        func capture(_ node:SCNNode,opacity:Float=1) {
            guard !node.isHidden else {return}
            let alpha=opacity*Float(node.opacity)
            if let geometry=node.geometry,let material=geometry.firstMaterial {
                let (vertices,indices)=batches.mesh(geometry,uvTransform:material.diffuse.contentsTransform)
                if !indices.isEmpty {
                    let image=wrists.overlayImages.capture(material.diffuse.contents as? NSImage)
                    overlays.append(SceneOverlay(transform:node.simdWorldTransform,vertices:vertices.map{SCNVector3($0.position.x,$0.position.y,$0.position.z)},
                        uv:vertices.map{CGPoint(x:Double($0.uv.x),y:Double($0.uv.y))},indices:indices.map{Int32($0)},image:image,
                        color:SceneOverlay.tint(material.diffuse.contents),opacity:alpha,
                        depth:material.readsFromDepthBuffer,order:node.renderingOrder,line:geometry.elements.first?.primitiveType == .line,name:node.name))
                }
            }
            for child in node.childNodes {capture(child,opacity:alpha)}
        }
        #if SOJU_OPENXR
        if xrRequested {for node in [wrists.commands,wrists.resources,wrists.ray,wrists.box,reticle,hud] {capture(node)}}
        #endif
        wrists.overlayImages.end()
        let center=terrainCenter
        var sceneCamera=desktopCameraPacket!
        #if SOJU_OPENXR
        if let loop=metalXRLoop {
            loop.update(MetalXRLoop.Body(position:camera.simdPosition,yaw:Float(yaw),scale:worldUnitsPerMeter,tabletop:tabletop,fade:comfortFade,vignette:motionVignette))
            if let latest=loop.latest().camera {sceneCamera=latest}
        }
        #endif
        worker.view(SceneView(session:session,viewport:SIMD2<Float>(Float(view.bounds.width),Float(view.bounds.height)),camera:sceneCamera,center:SIMD2<Int>(Int(center.x),Int(center.z)),radius:terrainRadius,selected:Set(selected),overlays:overlays,hiddenUnit:possessedUnit))
    }
    func sceneRay(_ point:NSPoint)->(SIMD3<Float>,SIMD3<Float>)? {
        guard let presentation=desktopMetal?.presentation else {return nil}
        let camera=presentation.camera,viewport=presentation.viewport
        let inverse=simd_inverse(camera.projection*simd_inverse(camera.transform))
        let x=Float(point.x)/max(1,viewport.x)*2-1,y=Float(point.y)/max(1,viewport.y)*2-1
        let a=inverse*SIMD4<Float>(x,y,-1,1),b=inverse*SIMD4<Float>(x,y,1,1)
        let start=SIMD3<Float>(a.x,a.y,a.z)/a.w,end=SIMD3<Float>(b.x,b.y,b.z)/b.w
        return (start,simd_normalize(end-start))
    }
    func sceneHit(_ point:NSPoint,accept:(ScenePickHit.Target)->Bool)->ScenePickHit? {
        guard let ray=sceneRay(point) else {return nil}
        return sceneWorker?.mailbox.presented()?.hit(origin:ray.0,direction:ray.1,accept:accept)
    }
    func sceneProject(_ position:SIMD3<Float>)->NSPoint? {
        guard let presentation=desktopMetal?.presentation else {return nil}
        let camera=presentation.camera,viewport=presentation.viewport
        let clip=camera.projection*simd_inverse(camera.transform)*SIMD4<Float>(position,1)
        guard clip.w>0,abs(clip.x)<=clip.w,abs(clip.y)<=clip.w,abs(clip.z)<=clip.w else {return nil}
        return NSPoint(x:CGFloat((clip.x/clip.w+1)*viewport.x/2),y:CGFloat((clip.y/clip.w+1)*viewport.y/2))
    }
}

extension UnitBillboard {
    func projectedLayers(camera:MetalBatches.CameraPacket,viewport:SIMD2<Float>,art:WorldSprites)->[Obj] {
        projectedSpriteBounds(projection:camera.projection*simd_inverse(camera.transform),viewport:viewport,art:art).map{$0.object}
    }
    func projectedSpriteBounds(projection:simd_float4x4,viewport:SIMD2<Float>,art:WorldSprites)->[SceneSpriteBounds] {
        return nativeLayers.keys.sorted().compactMap {index in
            guard !root.isHidden,let native=nativeLayers[index],let node=layers[index],!node.isHidden,
                  let entry=art.images[String(Int(number(native,"id")))],let records=art.frameRecords(Int(number(native,"id"))),
                  let frame=shown[index],records.indices.contains(frame) else {return nil}
            let record=records[frame],scale=number(entry,"scale"),width=number(entry,"grpWidth"),height=number(entry,"grpHeight")
            let left=number(record,"ox")/(width*scale)-0.5,top=0.5-number(record,"oy")/(height*scale)
            let right=left+number(record,"w")/(width*scale),bottom=top-number(record,"h")/(height*scale)
            let transform=projection*node.simdWorldTransform
            let points=[(left,top),(right,top),(right,bottom),(left,bottom)].map {x,y -> SIMD2<Float> in
                let clip=transform*SIMD4<Float>(Float(x*width/32),Float(y*height/32),0,1)
                return SIMD2<Float>((clip.x/clip.w+1)*viewport.x/2,(1-clip.y/clip.w)*viewport.y/2)
            }
            return SceneSpriteBounds(image:number(native,"id"),frame:frame,shadow:number(native,"drawfunc")==10,
                left:points.map{$0.x}.min()!,top:points.map{$0.y}.min()!,right:points.map{$0.x}.max()!,bottom:points.map{$0.y}.max()!)
        }
    }
}
