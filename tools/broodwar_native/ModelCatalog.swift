import Foundation
import GLTFKit2

// Unit ID, placement and animation conventions are data, never engine code.
struct ModelDefinition {
    enum TeamColorBlend:String { case multiply, replace }
    let kind:Int, file:String, name:String
    let scale:Double, targetHeight:Double?, yawDegrees:Double, elevation:Double,flightHeight:Double
    let faceHeading:Bool, footPivot:Bool
    let teamMask:String
    let teamColorBlend:TeamColorBlend
    let animations:[String:[String]]
    let requiredAnimations:[String]
    let raw:Obj
    init(kind:Int,object:Obj)throws {
        guard (0...65535).contains(kind),let file=object["file"] as? String,
            !file.isEmpty,!file.hasPrefix("/"),!file.split(separator:"/").contains(".."),
            file.lowercased().hasSuffix(".glb") else { throw SceneError.invalid("Invalid model path/type \(kind)") }
        teamMask=object["teamMask"] as? String ?? "none"
        guard ["alpha","texture","none"].contains(teamMask) else {throw SceneError.invalid("Invalid teamMask")}
        guard let blend=TeamColorBlend(rawValue:object["teamColorBlend"] as? String ?? "multiply") else {
            throw SceneError.invalid("Invalid teamColorBlend")
        }
        teamColorBlend=blend
        if teamMask=="texture" {
            guard let path=object["teamMaskTexture"] as? String,!path.hasPrefix("/"),!path.split(separator:"/").contains("..") else {throw SceneError.invalid("Missing or unsafe teamMaskTexture")}
        }
        self.kind=kind;self.file=file;raw=object
        name=object["name"] as? String ?? URL(fileURLWithPath:file).deletingPathExtension().lastPathComponent
        scale=(object["scale"] as? NSNumber)?.doubleValue ?? 1
        targetHeight=(object["targetHeight"] as? NSNumber)?.doubleValue
        yawDegrees=(object["yawDegrees"] as? NSNumber)?.doubleValue ?? 180
        elevation=(object["elevation"] as? NSNumber)?.doubleValue ?? 0
        flightHeight=(object["flightHeight"] as? NSNumber)?.doubleValue ?? 1.5
        faceHeading=object["faceHeading"] as? Bool ?? true
        footPivot=object["footPivot"] as? Bool ?? false
        guard scale.isFinite,scale>0,scale<=1000,yawDegrees.isFinite,elevation.isFinite,
            abs(elevation)<1000,flightHeight.isFinite,flightHeight>=0,flightHeight<1000,targetHeight.map({$0.isFinite && $0>0 && $0<=1000}) ?? true else {
            throw SceneError.invalid("Invalid model transform \(kind)")
        }
        for key in ["teamMaterials","turretNodes"] where object[key] != nil {
            guard let names=object[key] as? [String],names.allSatisfy({!$0.isEmpty}) else {throw SceneError.invalid("Invalid \(key) for model \(kind)")}
        }
        var mappings=[String:[String]]()
        for (key,value) in object["animations"] as? Obj ?? [:] {
            if let names=value as? [String] { mappings[key]=names }
            else if let name=value as? String { mappings[key]=[name] }
            else { throw SceneError.invalid("Invalid animation mapping \(kind)/\(key)") }
        }
        animations=mappings;requiredAnimations=object["requiredAnimations"] as? [String] ?? []
    }
}
final class ModelCatalog {
    let folder:URL,defaultURL:URL,overrideURL:URL,includeDefaults:Bool
    var definitions=[Int:ModelDefinition](),signatures=[Int:String]()
    var lastError=""
    private var lastCheck=0.0
    init(folder:URL=runtime.appendingPathComponent("models-3d"),catalog:URL?=nil,includeDefaults:Bool=true)throws {
        self.includeDefaults=includeDefaults
        self.folder=folder;defaultURL=runtime.appendingPathComponent("default-model-catalog.json")
        overrideURL=catalog ?? folder.appendingPathComponent("catalog.json")
        try reload()
    }
    func url(_ d:ModelDefinition)throws->URL {
        let root=folder.resolvingSymlinksInPath().standardizedFileURL
        let file=folder.appendingPathComponent(d.file).resolvingSymlinksInPath().standardizedFileURL
        guard file.path.hasPrefix(root.path+"/"),FileManager.default.fileExists(atPath:file.path) else {
            throw SceneError.invalid("Missing model inside model directory: \(d.file)")
        }
        return file
    }
    func reload()throws {
        var objects=[String:Obj]()
        for file in [defaultURL] where includeDefaults && FileManager.default.fileExists(atPath:file.path) {
            let object=try loadObject(file)
            guard number(object,"schemaVersion")==1,let models=object["models"] as? [String:Obj] else {
                throw SceneError.invalid("Invalid model catalog: \(file.lastPathComponent)")
            }
            for (key,value) in models { objects[key,default:[:]].merge(value){_,new in new} }
        }
        // Drop-in convention for new units, buildings and map objects.
        for file in try FileManager.default.contentsOfDirectory(at:folder,includingPropertiesForKeys:nil) {
            let name=file.deletingPathExtension().lastPathComponent
            guard file.pathExtension.lowercased()=="glb",name.hasPrefix("unit-"),let kind=Int(name.dropFirst(5)) else { continue }
            let sidecar=file.deletingPathExtension().appendingPathExtension("json")
            var object:Obj=["file":file.lastPathComponent]
            if FileManager.default.fileExists(atPath:sidecar.path) { object.merge(try loadObject(sidecar)){_,new in new} }
            objects[String(kind),default:[:]].merge(object){_,new in new}
        }
        // Explicit active release wins over loose unit-ID files.
        if FileManager.default.fileExists(atPath:overrideURL.path) {
            let object=try loadObject(overrideURL)
            guard number(object,"schemaVersion")==1,let models=object["models"] as? [String:Obj] else {
                throw SceneError.invalid("Invalid model catalog: \(overrideURL.lastPathComponent)")
            }
            for (key,value) in models { objects[key,default:[:]].merge(value){_,new in new} }
        }
        var next=[Int:ModelDefinition](),sigs=[Int:String]()
        for (key,object) in objects {
            guard let kind=Int(key) else { throw SceneError.invalid("Non-numeric unit ID: \(key)") }
            let definition=try ModelDefinition(kind:kind,object:object);next[kind]=definition
            let file=folder.appendingPathComponent(definition.file)
            let attrs=try? FileManager.default.attributesOfItem(atPath:file.path)
            let changed=(attrs?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
            let size=(attrs?[.size] as? NSNumber)?.int64Value ?? -1
            let encoded=try JSONSerialization.data(withJSONObject:object,options:.sortedKeys)
            sigs[kind]="\(changed):\(size):"+encoded.base64EncodedString()
        }
        definitions=next;signatures=sigs;lastError=""
    }
    func poll(force:Bool=false) {
        let now=Date().timeIntervalSince1970
        guard force || now-lastCheck>1 else { return };lastCheck=now
        do { try reload() } catch { lastError="Model catalog: \(error)" }
    }
}
