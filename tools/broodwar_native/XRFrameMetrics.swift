import Foundation
import QuartzCore

final class XRFrameMetrics {
    private let lock=NSLock()
    private var previous=0.0,start=0.0,intervals=[Double](),cpu=[Double](),gpu=[Double]()
    private var eyeGPU=[Int:Double]()
    private var unitCount=0
    func begin(units:Int) {
        lock.lock();defer{lock.unlock()}
        let now=CACurrentMediaTime()
        unitCount=units;start=now
        if previous>0,intervals.count<10000 {intervals.append((now-previous)*1000)}
        previous=now
    }
    func submitted() {
        lock.lock();defer{lock.unlock()}
        if cpu.count<10000 {cpu.append((CACurrentMediaTime()-start)*1000)}
    }
    func completed(frame:Int,eye:Int,milliseconds:Double) {
        lock.lock();defer{lock.unlock()}
        guard milliseconds>0,gpu.count<10000 else {return}
        if eye==0 {eyeGPU[frame]=milliseconds}
        else if let first=eyeGPU.removeValue(forKey:frame) {gpu.append(first+milliseconds)}
        if eye==1,gpu.count%90==0,let path=argument("--frame-metrics") {
            func summary(_ raw:[Double])->Obj {
                let values=Array(raw.dropFirst(min(90,raw.count/2))).sorted()
                guard !values.isEmpty else {return [:]}
                return ["count":values.count,"meanMs":values.reduce(0,+)/Double(values.count),
                        "p50Ms":values[values.count/2],"p95Ms":values[min(values.count-1,Int(Double(values.count)*0.95))],
                        "p99Ms":values[min(values.count-1,Int(Double(values.count)*0.99))],
                        "over11_111ms":values.filter{$0>1000/90}.count]
            }
            let data:Obj=["targetHz":90,"budgetMs":1000.0/90,"unitCount":unitCount,
                "snapshotSource":argument("--snapshot") ?? "live","frameIntervals":summary(intervals),
                "stereoCPUEncode":summary(cpu),"stereoGPU":summary(gpu),
                "captureEnabled":argument("--record-dir") != nil,
                "method":"Consecutive left-eye starts; CPU both-eye encoding; sum of both Metal command-buffer GPU durations. Warm-up excluded. Includes simulator pacing, not network receive latency."]
            if let bytes=try? JSONSerialization.data(withJSONObject:data,options:.prettyPrinted) {
                try? bytes.write(to:URL(fileURLWithPath:path),options:.atomic)
            }
        }
    }
}
