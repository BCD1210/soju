import Foundation
import Metal

// The test process owns all generated artifacts. No persistent GPU cache is used.
func run() throws {
    let args = CommandLine.arguments
    guard args.count == 7, let count = Int(args[4]), let width = Int(args[5]), let entry = args.last else {
        throw NSError(domain: "usage: runner library input output count width entry", code: 1)
    }
    guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {
        throw NSError(domain: "Metal device unavailable", code: 2)
    }
    let library = try device.makeLibrary(URL: URL(fileURLWithPath: args[1]))
    guard let function = library.makeFunction(name: entry) else { throw NSError(domain: "entry missing", code: 3) }
    let pipeline = try device.makeComputePipelineState(function: function)
    let input = try Data(contentsOf: URL(fileURLWithPath: args[2]))
    let buffer = input.withUnsafeBytes { device.makeBuffer(bytes: $0.baseAddress!, length: input.count, options: .storageModeShared) }!
    guard let command = queue.makeCommandBuffer(), let encoder = command.makeComputeCommandEncoder() else { throw NSError(domain: "command unavailable", code: 4) }
    encoder.setComputePipelineState(pipeline)
    encoder.setBuffer(buffer, offset: 0, index: 0)
    encoder.dispatchThreadgroups(MTLSize(width: count / width, height: 1, depth: 1), threadsPerThreadgroup: MTLSize(width: width, height: 1, depth: 1))
    encoder.endEncoding()
    command.commit()
    command.waitUntilCompleted()
    if let error = command.error { throw error }
    guard command.status == .completed else { throw NSError(domain: "GPU execution did not complete", code: 5) }
    try Data(bytes: buffer.contents(), count: input.count).write(to: URL(fileURLWithPath: args[3]))
    print("Metal \(device.name): \(entry), \(count) invocations")
}
do { try run() } catch { fputs("\(error)\n", stderr); exit(1) }
