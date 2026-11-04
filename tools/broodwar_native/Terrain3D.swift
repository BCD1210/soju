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
    // Lift HD map samples along the canonical 32-degree comparison ray.
    // This is a reconstruction convention, not recovered source geometry.
    // Perspective views retain VF4 relief; native map footprints stay fixed.
    static let canonicalElevation=32.0 * Double.pi/180
    static let reliefDepth=1/tan(canonicalElevation)
    static func worldPosition(_ x:Double,_ height:Double,_ z:Double)->SCNVector3 {
        SCNVector3(x,height,z+height*reliefDepth)
    }
    static func mapPosition(_ point:SCNVector3)->SCNVector3 {
        SCNVector3(point.x,0,point.z-point.y*CGFloat(reliefDepth))
    }
    // Shared corners give every cliff texel real surface area. Extra extruded
    // walls duplicated the baked cliff artwork and had zero-area texture UVs.
    func corner(_ x:Int,_ y:Int,_ cx:Int,_ cy:Int)->Double {
        [index(cx-1,cy-1),index(cx,cy-1),index(cx-1,cy),index(cx,cy)].map{heights[$0]}.reduce(0,+)/4
    }

    func surface(_ x:Int,_ y:Int)->[Double] {
        [corner(x,y,x,y),corner(x,y,x,y+1),corner(x,y,x+1,y+1),corner(x,y,x+1,y)]
    }
    func profile(_ x:Double,_ z:Double)->Double {
        let sx=x*4,sy=z*4,ix=Int(floor(sx)),iy=Int(floor(sy)),v=surface(ix,iy)
        var fx=sx-floor(sx),fy=sy-floor(sy)
        fx=fx*fx*(3-2*fx);fy=fy*fy*(3-2*fy)
        return v[0]*(1-fx)*(1-fy)+v[1]*(1-fx)*fy+v[2]*fx*fy+v[3]*fx*(1-fy)
    }
    func elevation(_ x:Double,_ z:Double)->Double {
        let sx=x*16,sy=z*16,ix=floor(sx),iy=floor(sy),fx=sx-ix,fy=sy-iy
        let a=profile(ix/16,iy/16),b=profile(ix/16,(iy+1)/16)
        let c=profile((ix+1)/16,(iy+1)/16),d=profile((ix+1)/16,iy/16)
        return fy>=fx ? a*(1-fy)+b*(fy-fx)+c*fx:a*(1-fx)+c*fy+d*(fx-fy)
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
            let lifted=p.map{SIMD3<Float>($0.x,$0.y,$0.z+$0.y*Float(Self.reliefDepth))}
            let base=Int32(vertices.count),first=simd_cross(lifted[1]-lifted[0],lifted[2]-lifted[0]),second=simd_cross(lifted[2]-lifted[0],lifted[3]-lifted[0])
            let normal=simd_normalize(simd_length_squared(first)>simd_length_squared(second) ? first:second)
            for i in 0..<4 { vertices.append(Self.worldPosition(Double(p[i].x),Double(p[i].y),Double(p[i].z)));normals.append(SCNVector3(normal.x,normal.y,normal.z));uv.append(t[i]) }
            indices += [base,base+1,base+2,base,base+2,base+3]
        }
        let values=(0...4).flatMap{y in (0...4).map{x in profile(Double(tileX)+Double(x)/4,Double(tileY)+Double(y)/4)}}
        let segments=(values.max()!-values.min()!)>0.001 ? 16:4
        for y in 0..<segments {for x in 0..<segments {
            let ax=Double(x)/Double(segments),bx=Double(x+1)/Double(segments)
            let az=Double(y)/Double(segments),bz=Double(y+1)/Double(segments)
            let points=[(ax,az),(ax,bz),(bx,bz),(bx,az)]
            quad(points.map{px,pz in SIMD3<Float>(Float(px),Float(profile(Double(tileX)+px,Double(tileY)+pz)),Float(pz))},
                 points.map{px,pz in CGPoint(x:px,y:1-pz)})
        }}
        let geometry=SCNGeometry(sources:[SCNGeometrySource(vertices:vertices),SCNGeometrySource(normals:normals),SCNGeometrySource(textureCoordinates:uv)],
            elements:[SCNGeometryElement(indices:indices,primitiveType:.triangles)])
        geometry.materials=[material];return geometry
    }
}
