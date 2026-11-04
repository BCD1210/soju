import Foundation
import QuartzCore
import os.signpost

final class XRFrameMetrics {
    static let log=OSLog(subsystem:"soju.broodwar",category:"XRFrame")
    private var stages=[String:[Double]]()
    func stage(_ name:String, milliseconds:Double) {
        lock.lock();defer{lock.unlock()}
        if enabled,stages[name,default:[]].count<10000 {stages[name,default:[]].append(milliseconds)}
    }
    private let lock=NSLock()
    private var previous=0.0,start=0.0,intervals=[Double](),cpu=[Double](),gpu=[Double]()
    private struct Pair {var frame = -1;var left=0.0;var right=0.0}
    private var eyeGPU=[Pair](repeating:Pair(),count:512)
    init() {
        intervals.reserveCapacity(10000);cpu.reserveCapacity(10000);gpu.reserveCapacity(10000)
        for name in ["billboardUpdate","eyeSetup","renderEncodeEye","eyePostRender"] {
            var values=[Double]();values.reserveCapacity(10000);stages[name]=values
        }
    }
    var completedStereoSamples:Int {lock.lock();defer{lock.unlock()};return gpu.count}
    private var unitCount=0
    private var enabled=argument("--metrics-start-file")==nil
    func begin(units:Int) {
        lock.lock();defer{lock.unlock()}
        if !enabled {
            guard let path=argument("--metrics-start-file"),FileManager.default.fileExists(atPath:path) else {return}
            enabled=true
        }
        let now=CACurrentMediaTime()
        unitCount=units;start=now
        if previous>0,intervals.count<10000 {intervals.append((now-previous)*1000)}
        previous=now
    }
    func submitted() {
        lock.lock();defer{lock.unlock()}
        if enabled,cpu.count<10000 {cpu.append((CACurrentMediaTime()-start)*1000)}
    }
    func completed(frame:Int,eye:Int,milliseconds:Double) {
        lock.lock();defer{lock.unlock()}
        guard enabled,milliseconds>0,gpu.count<10000 else {return}
        let slot=frame%eyeGPU.count
        if eyeGPU[slot].frame != frame {eyeGPU[slot]=Pair(frame:frame,left:0,right:0)}
        if eye==0 {eyeGPU[slot].left=milliseconds}else {eyeGPU[slot].right=milliseconds}
        guard eyeGPU[slot].left>0,eyeGPU[slot].right>0 else {return}
        gpu.append(eyeGPU[slot].left+eyeGPU[slot].right);eyeGPU[slot]=Pair()
        if gpu.count%90==0,let path=argument("--frame-metrics") {
            func summary(_ raw:[Double])->Obj {
                let values=Array(raw.dropFirst(min(90,raw.count/2))).sorted()
                guard !values.isEmpty else {return [:]}
                return ["count":values.count,"meanMs":values.reduce(0,+)/Double(values.count),
                        "p50Ms":values[values.count/2],"p95Ms":values[min(values.count-1,Int(Double(values.count)*0.95))],
                        "p99Ms":values[min(values.count-1,Int(Double(values.count)*0.99))],
                        "over11_111ms":values.filter{$0>1000/90}.count]
            }
            let data:Obj=["renderer":CommandLine.arguments.contains("--metal-batches") ? "Metal instanced amplified stereo":"SceneKit per eye","targetHz":90,"budgetMs":1000.0/90,"unitCount":unitCount,
                "snapshotSource":argument("--snapshot") ?? "live","frameIntervals":summary(intervals),
                "stereoCPUEncode":summary(cpu),"stereoGPU":summary(gpu),
                "stagesCPU":stages.mapValues{summary($0)},
                "captureEnabled":argument("--record-dir") != nil,
                "method":"Consecutive left-eye starts; CPU both-eye encoding; sum of both Metal command-buffer GPU durations. Warm-up excluded. Includes simulator pacing, not network receive latency."]
            if let bytes=try? JSONSerialization.data(withJSONObject:data,options:.prettyPrinted) {
                try? bytes.write(to:URL(fileURLWithPath:path),options:.atomic)
            }
        }
    }
}
