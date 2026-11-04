import Foundation
#if SOJU_OPENXR
// Continuous samples replace only another continuous sample. Every trigger or
// grip transition occupies its own FIFO entry until the main owner consumes it.
final class XRInputQueue {
    private let lock=NSLock()
    private var inputs=[SojuXRInput](repeating:SojuXRInput(),count:128)
    private var edges=[Bool](repeating:false,count:128)
    private var readIndex=0,writeIndex=0,previous=SojuXRInput()
    private var turnHeld=false
    func submit(_ input:SojuXRInput)->Bool {
        lock.lock();defer{lock.unlock()}
        var turnEdge=false
        if abs(input.turnX)<0.4 && turnHeld {turnHeld=false;turnEdge=true}
        if abs(input.turnX)>0.7 && !turnHeld {turnHeld=true;turnEdge=true}
        let edge=turnEdge || input.leftPressed != 0 || input.rightPressed != 0 || input.leftReleased != 0 || input.rightReleased != 0 ||
            input.leftGripPressed != 0 || input.rightGripPressed != 0 || input.leftHeld != previous.leftHeld || input.rightHeld != previous.rightHeld ||
            input.leftGripHeld != previous.leftGripHeld || input.rightGripHeld != previous.rightGripHeld
        previous=input
        let last=(writeIndex+inputs.count-1)%inputs.count
        if !edge,readIndex != writeIndex,!edges[last] {inputs[last]=input;return true}
        let next=(writeIndex+1)%inputs.count
        guard next != readIndex else {return false}
        inputs[writeIndex]=input;edges[writeIndex]=edge;writeIndex=next;return true
    }
    func take()->SojuXRInput? {
        lock.lock();defer{lock.unlock()}
        guard readIndex != writeIndex else {return nil}
        let value=inputs[readIndex];readIndex=(readIndex+1)%inputs.count;return value
    }
}
#endif
