import Metal
import Foundation

// A slot's payload is reusable only after its command-buffer callback returns.
final class GPUCompletionPool {
    private final class Slot {
        let available=DispatchSemaphore(value:1)
        var frame=0,eye=0
        var handler:MTLCommandBufferHandler!
    }
    private let slots=(0..<16).map{_ in Slot()}
    private var next=0
    private(set) var exhausted=0
    init(metrics:XRFrameMetrics) {
        for slot in slots {
            let available=slot.available
            slot.handler={ [weak slot,metrics,available] command in
                defer {available.signal()}
                guard let slot=slot else {return}
                if command.status == .completed {metrics.completed(frame:slot.frame,eye:slot.eye,milliseconds:(command.gpuEndTime-command.gpuStartTime)*1000)}
                else if let error=command.error {fputs("GPU completion: \(error)\n",stderr)}
            }
        }
    }
    @discardableResult func attach(_ command:MTLCommandBuffer,frame:Int,eye:Int)->Bool {
        for _ in slots.indices {
            let slot=slots[next];next=(next+1)%slots.count
            guard slot.available.wait(timeout:.now()) == .success else {continue}
            slot.frame=frame;slot.eye=eye;command.addCompletedHandler(slot.handler);return true
        }
        exhausted+=1;return false
    }
}
