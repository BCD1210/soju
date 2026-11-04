import Cocoa
import QuartzCore

final class CaptureFrames {
    let folder:URL?,duration:Double,interval:Double
    var started:Double?,next=0.0,sequence=0
    init() {
        folder=argument("--record-dir").map{URL(fileURLWithPath:$0)}
        duration=Double(argument("--record-seconds") ?? "120") ?? 120
        interval=1/max(1,min(30,Double(argument("--record-fps") ?? "1") ?? 1))
        if let folder=folder {try? FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true)}
    }
    func reserve()->Int? {
        guard folder != nil else {return nil}
        let now=CACurrentMediaTime()
        if started==nil {started=now;next=now}
        guard now>=next,now-(started ?? now)<=duration else {return nil}
        next=now+interval
        let result=sequence;sequence+=1;return result
    }
    func save(_ image:CGImage,index:Int,metadata:Obj) {
        guard let folder=folder else {return}
        do {
            let name=String(format:"frame-%05d",index),file=folder.appendingPathComponent(name+".png")
            guard !FileManager.default.fileExists(atPath:file.path) else {throw SceneError.invalid("Capture already exists: \(file.path)")}
            guard let png=NSBitmapImageRep(cgImage:image).representation(using:.png,properties:[:]) else {throw SceneError.invalid("PNG encoding failed")}
            try png.write(to:file,options:.atomic)
            try JSONSerialization.data(withJSONObject:metadata,options:.prettyPrinted).write(to:folder.appendingPathComponent(name+".json"),options:.atomic)
        } catch {fputs("Capture failed: \(error)\n",stderr)}
    }
}
