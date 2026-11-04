import Foundation
import QuartzCore
import os.signpost

// Fixed storage keeps arbitrarily long sessions measurable without retaining frames.
final class FrameDistribution {
    private var bins=[UInt64](repeating:0,count:100001)
    private(set) var count:UInt64=0
    private var total=0.0,maximum=0.0
    private var overBudget:UInt64=0,missed:UInt64=0
    func add(_ milliseconds:Double) {
        guard milliseconds.isFinite,milliseconds>=0 else {return}
        count+=1;total+=milliseconds;maximum=max(maximum,milliseconds)
        bins[min(bins.count-1,Int(min(milliseconds,1000)*100))]+=1
        if milliseconds>1000.0/90 {overBudget+=1}
        // Count absent nominal refresh slots separately from ordinary timer jitter.
        missed+=UInt64(max(0,(milliseconds/(1000.0/90)).rounded()-1))
    }
    func summary()->Obj {
        guard count>0 else {return [:]}
        func percentile(_ fraction:Double)->Double {
            let target=UInt64(ceil(Double(count)*fraction));var accumulated:UInt64=0
            for (i,n) in bins.enumerated() {
                accumulated+=n
                if accumulated>=target {return i==bins.count-1 ? maximum:Double(i+1)/100}
            }
            return maximum
        }
        return ["count":count,"meanMs":total/Double(count),"p50Ms":percentile(0.5),
                "p95Ms":percentile(0.95),"p99Ms":percentile(0.99),"maxMs":maximum,
                "over11_111ms":overBudget,"estimatedMissedRefreshSlots":missed]
    }
}

final class XRFrameMetrics {
    static let log=OSLog(subsystem:"soju.broodwar",category:"XRFrame")
    private let lock=NSLock()
    private let intervals=FrameDistribution(),cpu=FrameDistribution(),gpu=FrameDistribution()
    private let stages: [String:FrameDistribution]=Dictionary(uniqueKeysWithValues:
        ["billboardUpdate","eyeSetup","renderEncodeEye","eyePostRender"].map{($0,FrameDistribution())})
    private var previous=0.0,start=0.0,first=0.0
    private struct Pair {var frame = -1;var left=0.0;var right=0.0}
    private var eyeGPU=[Pair](repeating:Pair(),count:512)
    private var unitCount=0
    private var enabled=argument("--metrics-start-file")==nil
    private var reporter:DispatchSourceTimer?
    init() {
        guard argument("--frame-metrics") != nil else {return}
        let timer=DispatchSource.makeTimerSource(queue:DispatchQueue(label:"soju.bw.metrics",qos:.utility))
        timer.schedule(deadline:.now()+5,repeating:5)
        timer.setEventHandler { [weak self] in self?.writeReport() }
        timer.resume();reporter=timer
    }
    deinit {reporter?.cancel()}
    var completedStereoSamples:Int {lock.lock();defer{lock.unlock()};return Int(gpu.count)}
    func stage(_ name:String,milliseconds:Double) {
        lock.lock();defer{lock.unlock()}
        if enabled {stages[name]?.add(milliseconds)}
    }
    func begin(units:Int) {
        lock.lock();defer{lock.unlock()}
        if !enabled {
            guard let path=argument("--metrics-start-file"),FileManager.default.fileExists(atPath:path) else {return}
            enabled=true
        }
        let now=CACurrentMediaTime();unitCount=units;start=now
        if first==0 {first=now}
        if previous>0 {intervals.add((now-previous)*1000)}
        previous=now
    }
    func submitted() {
        lock.lock();defer{lock.unlock()}
        if enabled,start>0 {cpu.add((CACurrentMediaTime()-start)*1000)}
    }
    func completed(frame:Int,eye:Int,milliseconds:Double) {
        lock.lock();defer{lock.unlock()}
        guard enabled,milliseconds.isFinite,milliseconds>0 else {return}
        let slot=frame%eyeGPU.count
        if eyeGPU[slot].frame != frame {eyeGPU[slot]=Pair(frame:frame,left:0,right:0)}
        if eye==0 {eyeGPU[slot].left=milliseconds}else {eyeGPU[slot].right=milliseconds}
        guard eyeGPU[slot].left>0,eyeGPU[slot].right>0 else {return}
        gpu.add(eyeGPU[slot].left+eyeGPU[slot].right);eyeGPU[slot]=Pair()
    }
    func writeReport() {
        guard let path=argument("--frame-metrics") else {return}
        lock.lock()
        let data:Obj=["renderer":CommandLine.arguments.contains("--metal-batches") ? "Metal instanced amplified stereo":"SceneKit per eye",
            "targetHz":90,"budgetMs":1000.0/90,"unitCount":unitCount,"elapsedSeconds":first>0 ? previous-first:0,
            "snapshotSource":argument("--snapshot") ?? "live","frameIntervals":intervals.summary(),
            "stereoCPUEncode":cpu.summary(),"stereoGPU":gpu.summary(),"stagesCPU":stages.mapValues{$0.summary()},
            "captureEnabled":argument("--record-dir") != nil,
            "method":"Consecutive left-eye starts, both-eye CPU submission and summed GPU duration. No sample cap or implicit warm-up exclusion. Arm with metrics-start-file after warm-up. Percentiles are upper histogram edges (0.01 ms); values >=1000 ms use maximum. Missed slots are cadence estimates, not compositor telemetry."]
        lock.unlock()
        if let bytes=try? JSONSerialization.data(withJSONObject:data,options:.prettyPrinted) {
            try? bytes.write(to:URL(fileURLWithPath:path),options:.atomic)
        }
    }
}
