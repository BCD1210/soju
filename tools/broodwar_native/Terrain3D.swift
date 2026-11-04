import Cocoa
import SceneKit
import simd

final class Terrain3D {
    let width:Int,height:Int,flags:[Int],heights:[Double],ramps:Set<Int>
    init(url:URL)throws {
        let data=try loadObject(url)
        width=Int(number(data,"width"));height=Int(number(data,"height"))
        guard width>0,height>0,let f=data["flags"] as? [Int],let h=data["heights"] as? [Double],
            f.count==width*height,h.count==f.count else { throw SceneError.invalid("Terrain minitile dimensions") }
        flags=f;heights=h;ramps=Set(data["rampCells"] as? [Int] ?? [])
    }
    func index(_ x:Int,_ y:Int)->Int { max(0,min(height-1,y))*width+max(0,min(width-1,x)) }
    // Shared plateau vertices stay flat. Only confirmed ramp cells interpolate.
    func corner(_ x:Int,_ y:Int,_ cx:Int,_ cy:Int)->Double {
        let own=index(x,y)
        guard ramps.contains(own) else { return heights[own] }
        let adjacent=[index(cx-1,cy-1),index(cx,cy-1),index(cx-1,cy),index(cx,cy)].filter{flags[$0]&1 != 0}
        let fixed=adjacent.filter{!ramps.contains($0)}
        if let h=fixed.first.map({heights[$0]}),fixed.allSatisfy({heights[$0]==h}) { return h }
        let ramp=adjacent.filter{ramps.contains($0)}
        return ramp.isEmpty ? heights[own]:ramp.map{heights[$0]}.reduce(0,+)/Double(ramp.count)
    }
    func surface(_ x:Int,_ y:Int)->[Double] {
        [corner(x,y,x,y),corner(x,y,x,y+1),corner(x,y,x+1,y+1),corner(x,y,x+1,y)]
    }
    func elevation(_ x:Double,_ z:Double)->Double {
        let sx=x*4,sy=z*4,ix=Int(floor(sx)),iy=Int(floor(sy)),fx=sx-floor(sx),fy=sy-floor(sy),v=surface(ix,iy)
        return (v[0]*(1-fx)+v[3]*fx)*(1-fy)+(v[1]*(1-fx)+v[2]*fx)*fy
    }
    func walkable(_ x:Double,_ z:Double)->Bool {
        x>=0 && z>=0 && x<Double(width)/4 && z<Double(height)/4 && flags[index(Int(x*4),Int(z*4))]&1 != 0
    }
    static func exposedEdge(_ top:[Double],_ bottom:[Double])->ClosedRange<Double>? {
        let first=top[0]-bottom[0],last=top[1]-bottom[1]
        if first<=0 && last<=0 {return nil}
        if first>=0 && last>=0 {return 0...1}
        let crossing=first/(first-last)
        return first>0 ? 0...crossing:crossing...1
    }
    func geometry(tileX:Int,tileY:Int,material:SCNMaterial)->SCNGeometry {
        var vertices=[SCNVector3](),normals=[SCNVector3](),uv=[CGPoint](),indices=[Int32]()
        func quad(_ p:[SIMD3<Float>],_ t:[CGPoint]) {
            let base=Int32(vertices.count),first=simd_cross(p[1]-p[0],p[2]-p[0]),second=simd_cross(p[2]-p[0],p[3]-p[0])
            let normal=simd_normalize(simd_length_squared(first)>simd_length_squared(second) ? first:second)
            for i in 0..<4 { vertices.append(SCNVector3(p[i].x,p[i].y,p[i].z));normals.append(SCNVector3(normal.x,normal.y,normal.z));uv.append(t[i]) }
            indices += [base,base+1,base+2,base,base+2,base+3]
        }
        for y in 0..<4 { for x in 0..<4 {
            let mx=tileX*4+x,my=tileY*4+y,v=surface(mx,my)
            let ax=Float(x)/4,bx=ax+0.25,az=Float(y)/4,bz=az+0.25
            let points=[SIMD3<Float>(ax,Float(v[0]),az),SIMD3<Float>(ax,Float(v[1]),bz),SIMD3<Float>(bx,Float(v[2]),bz),SIMD3<Float>(bx,Float(v[3]),az)]
            let u=CGFloat(x)/4,w=u+0.25,t=1-CGFloat(y)/4,b=t-0.25
            quad(points,[CGPoint(x:u,y:t),CGPoint(x:u,y:b),CGPoint(x:w,y:b),CGPoint(x:w,y:t)])
            // Separate vertices form actual vertical cliff faces, with no smoothing across levels.
            for (a,c,dx,dy,na,nc) in [(0,1,-1,0,3,2),(1,2,0,1,0,3),(2,3,1,0,1,0),(3,0,0,-1,2,1)] {
                let other=surface(mx+dx,my+dy)
                guard let exposed=Self.exposedEdge([v[a],v[c]],[other[na],other[nc]]) else {continue}
                let start=Float(exposed.lowerBound),end=Float(exposed.upperBound)
                let highA=points[a]+(points[c]-points[a])*start,highC=points[a]+(points[c]-points[a])*end
                var lowA=highA,lowC=highC
                lowA.y=Float(other[na])+(Float(other[nc])-Float(other[na]))*start
                lowC.y=Float(other[na])+(Float(other[nc])-Float(other[na]))*end
                // Crossed ramp edges expose a triangle on each side. Clipping
                // at their intersection closes the hole without overlapping faces.
                let edgeUV=[CGPoint(x:u,y:t),CGPoint(x:u,y:b),CGPoint(x:w,y:b),CGPoint(x:w,y:t)]
                let ta=edgeUV[a],tc=edgeUV[c]
                let ua=CGPoint(x:ta.x+(tc.x-ta.x)*CGFloat(start),y:ta.y+(tc.y-ta.y)*CGFloat(start))
                let uc=CGPoint(x:ta.x+(tc.x-ta.x)*CGFloat(end),y:ta.y+(tc.y-ta.y)*CGFloat(end))
                quad([highC,highA,lowA,lowC],[uc,ua,ua,uc])
            }
        }}
        let geometry=SCNGeometry(sources:[SCNGeometrySource(vertices:vertices),SCNGeometrySource(normals:normals),SCNGeometrySource(textureCoordinates:uv)],
            elements:[SCNGeometryElement(indices:indices,primitiveType:.triangles)])
        geometry.materials=[material];return geometry
    }
}
