import Foundation
import Darwin
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

enum FrameCreation:Int,CaseIterable {
    case commandBuffer,renderEncoder,blitEncoder,textureDescriptor,renderTexture,captureTexture,textureView,targetRebuild
    var name:String {
        switch self {
        case .commandBuffer:return "metalCommandBufferRequests"
        case .renderEncoder:return "metalRenderEncoderRequests"
        case .blitEncoder:return "metalBlitEncoderRequests"
        case .textureDescriptor:return "applicationTextureDescriptorCreations"
        case .renderTexture:return "applicationRenderTextureRequests"
        case .captureTexture:return "applicationCaptureTextureRequests"
        case .textureView:return "applicationTextureViewRequests"
        case .targetRebuild:return "applicationRenderTargetRebuilds"
        }
    }
}
final class XRFrameMetrics {
    static let log=OSLog(subsystem:"soju.broodwar",category:"PointsOfInterest")
    private let lock=NSLock()
    private let latency=Dictionary(uniqueKeysWithValues:["bridgeRoundTrip","nativeToBridge","bridgeToWorker","workerToPacket","nativeToPacket","packetAge","packetToPresented","packetToDisplayEstimate","nativeToGPUComplete","nativeToPresented","nativeToPredictedDisplay","nativeToDisplayEstimate"].map{($0,FrameDistribution())})
    private var lastLatencyNative=0.0
    func presented(_ timing:SceneTiming,at time:Double) {
        lock.lock();defer{lock.unlock()}
        if enabled,timing.native>0,time>0 {
            latency["nativeToPresented"]?.add((time-timing.native)*1000)
            if timing.published>0 {latency["packetToPresented"]?.add((time-timing.published)*1000)}
        }
    }
    func latencySample(_ timing:SceneTiming,completed:Double) {
        lock.lock();defer{lock.unlock()}
        guard enabled,timing.native>0,timing.bridgeRead>0,timing.published>0 else {return}
        if timing.native != lastLatencyNative {
            lastLatencyNative=timing.native
            if timing.bridgeWireRead>0 {
                stages["nativeToWire"]?.add((timing.bridgeWireRead-timing.native)*1000)
                stages["bridgeJSONDecode"]?.add((timing.bridgeRead-timing.bridgeWireRead)*1000)
            }
            for (key,value) in [("bridgeRoundTrip",timing.bridgeRead-timing.bridgeStarted),("nativeToBridge",timing.bridgeRead-timing.native),("bridgeToWorker",timing.workerStarted-timing.bridgeRead),("workerToPacket",timing.published-timing.workerStarted)] {latency[key]?.add(value*1000)}
        }
        latency["nativeToPacket"]?.add((timing.published-timing.native)*1000)
        latency["packetAge"]?.add((completed-timing.published)*1000)
        latency["nativeToGPUComplete"]?.add((completed-timing.native)*1000)
        if timing.predictedDisplay>0 {
            latency["nativeToPredictedDisplay"]?.add((timing.predictedDisplay-timing.native)*1000)
            latency["nativeToDisplayEstimate"]?.add((max(completed,timing.predictedDisplay)-timing.native)*1000)
            latency["packetToDisplayEstimate"]?.add((max(completed,timing.predictedDisplay)-timing.published)*1000)
        }
    }
    private let intervals=FrameDistribution(),cpu=FrameDistribution(),gpu=FrameDistribution()
    private let stages: [String:FrameDistribution]=Dictionary(uniqueKeysWithValues:
        ["billboardUpdate","eyeSetup","renderEncodeEye","eyePostRender","desktopDrawableWait","desktopSubmitToPresented","nativeToWire","bridgeJSONDecode"].map{($0,FrameDistribution())})
    private let waitTiming=FrameDistribution(),acquireTiming=FrameDistribution(),endTiming=FrameDistribution(),afterWaitTiming=FrameDistribution()
    private var periodMs=0.0,lateSubmissions:UInt64=0,lifecycleSamples:UInt64=0
    private var allocationBefore=malloc_statistics_t()
    private var allocationFrames:UInt64=0,allocationPositive:UInt64=0,allocationZero:UInt64=0
    private var allocationMax:Int64=0,allocationMin:Int64=0,allocationTotal:Int64=0
    private var previous=0.0,start=0.0,first=0.0
    private struct Pair {var frame = -1;var left=0.0;var right=0.0}
    private var eyeGPU=[Pair](repeating:Pair(),count:512)
    private var unitCount=0
    private var enabled=argument("--metrics-start-file")==nil
    private let metricsStartFile=argument("--metrics-start-file")
    private let traceCreations=CommandLine.arguments.contains("--trace-frame-creations")
    private var creations=[UInt64](repeating:0,count:FrameCreation.allCases.count)
    private var allCreations=[UInt64](repeating:0,count:FrameCreation.allCases.count)
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
    func created(_ kind:FrameCreation,_ count:Int=1) {
        lock.lock();allCreations[kind.rawValue]+=UInt64(count)
        if enabled {creations[kind.rawValue]+=UInt64(count)}
        lock.unlock()
        if traceCreations {os_signpost(.event,log:Self.log,name:"Frame creation request","kind=%d count=%d",kind.rawValue,count)}
    }
    func beginFrameAllocationSample() {
        malloc_zone_statistics(nil,&allocationBefore)
    }
    func endFrameAllocationSample() {
        var after=malloc_statistics_t();malloc_zone_statistics(nil,&after)
        let delta=Int64(after.blocks_in_use)-Int64(allocationBefore.blocks_in_use)
        lock.lock();defer{lock.unlock()}
        guard enabled else {return}
        allocationFrames+=1;allocationTotal+=delta
        allocationMin=min(allocationMin,delta);allocationMax=max(allocationMax,delta)
        if delta>0 {allocationPositive+=1};if delta==0 {allocationZero+=1}
    }
    func lifecycle(wait:Double,acquire:Double,end:Double,afterWait:Double,period:Double) {
        lock.lock();defer{lock.unlock()}
        guard enabled,period>0 else {return}
        waitTiming.add(wait);acquireTiming.add(acquire);endTiming.add(end);afterWaitTiming.add(afterWait)
        periodMs=period;lifecycleSamples+=1
        if afterWait>period {lateSubmissions+=1}
    }
    func stage(_ name:String,milliseconds:Double) {
        lock.lock();defer{lock.unlock()}
        if enabled {stages[name]?.add(milliseconds)}
    }
    func begin(units:Int) {
        lock.lock();defer{lock.unlock()}
        if !enabled {
            guard let path=metricsStartFile,FileManager.default.fileExists(atPath:path) else {return}
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
        var allocation=malloc_statistics_t()
        malloc_zone_statistics(nil,&allocation)
        lock.lock()
        let data:Obj=["renderer":useMetalBatches ? "Metal instanced amplified stereo":"SceneKit per eye",
            "targetHz":90,"budgetMs":1000.0/90,"unitCount":unitCount,"elapsedSeconds":first>0 ? previous-first:0,
            "snapshotSource":argument("--snapshot") ?? "live","frameIntervals":intervals.summary(),
            "stereoCPUEncode":cpu.summary(),"stereoGPU":gpu.summary(),"stagesCPU":stages.mapValues{$0.summary()},
            "textureResidency":TextureResidency.shared.report(),"latencyMs":latency.mapValues{$0.summary()},
            "latencyScope":"Native frame hook to GPU completion and display estimate (later of GPU readiness and runtime predicted display via CLOCK_MONOTONIC). Desktop uses compositor presentedTime. Physical photon telemetry is unavailable.",
            "frameOwnerCreationRequests":["measured":Dictionary(uniqueKeysWithValues:FrameCreation.allCases.map{($0.name,creations[$0.rawValue])}),
                "includingWarmup":Dictionary(uniqueKeysWithValues:FrameCreation.allCases.map{($0.name,allCreations[$0.rawValue])}),
                "method":"Instrumented creation requests issued by the native frame owner. Metal command buffers and encoders remain framework object requests. These counters are not malloc interception, exclude worker/UI creation, and do not prove zero transient allocations."],
            "frameAllocationDelta":["samples":allocationFrames,"positiveFrames":allocationPositive,"zeroNetFrames":allocationZero,
                "minNetBlocks":allocationMin,"maxNetBlocks":allocationMax,"totalNetBlocks":allocationTotal,
                "method":"All-zone blocks-in-use delta across each frame loop. Includes concurrent worker activity; frees can cancel allocations. Not a transient allocation count or proof of zero allocations."],
            "runtimePacing":["samples":lifecycleSamples,"periodMs":periodMs,"afterWaitOverPeriod":lateSubmissions,
                "wait":waitTiming.summary(),"acquire":acquireTiming.summary(),"endFrame":endTiming.summary(),"afterWait":afterWaitTiming.summary(),
                "method":"Host monotonic durations. After-wait budget uses predictedDisplayPeriod, not a converted display timestamp; xrEndFrame is submission, not physical presentation."],
            "mallocZoneStatistics":["blocksInUse":allocation.blocks_in_use,"bytesInUse":allocation.size_in_use,
                "bytesAllocated":allocation.size_allocated,"peakBytesInUse":allocation.max_size_in_use,
                "method":"All malloc zones sampled on the metrics queue; retained allocation counts, not transient allocation traffic or a zero-allocation proof"],
            "captureEnabled":argument("--record-dir") != nil,
            "method":"Consecutive left-eye starts, both-eye CPU submission and summed GPU duration. No sample cap or implicit warm-up exclusion. Arm with metrics-start-file after warm-up. Percentiles are upper histogram edges (0.01 ms); values >=1000 ms use maximum. Missed slots are cadence estimates, not compositor telemetry."]
        lock.unlock()
        if let bytes=try? JSONSerialization.data(withJSONObject:data,options:.prettyPrinted) {
            try? bytes.write(to:URL(fileURLWithPath:path),options:.atomic)
        }
    }
}
