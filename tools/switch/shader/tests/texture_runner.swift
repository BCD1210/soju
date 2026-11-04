import Foundation
import Metal
func run() throws {
    let args = CommandLine.arguments
    guard args.count == 4, let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else { throw NSError(domain:"Metal unavailable",code:1) }
    let library = try device.makeLibrary(URL: URL(fileURLWithPath: args[1]))
    let vertexLibrary = try device.makeLibrary(URL: URL(fileURLWithPath: args[3]))
    let fragment = library.makeFunction(name: "soju_fragment")!
    let descriptor = MTLRenderPipelineDescriptor()
    descriptor.vertexFunction = vertexLibrary.makeFunction(name: "soju_vertex")
    descriptor.fragmentFunction = fragment
    descriptor.colorAttachments[0].pixelFormat = .rgba32Float
    let layout = MTLVertexDescriptor()
    layout.attributes[0].format = .float4; layout.attributes[0].bufferIndex = 0
    layout.layouts[0].stride = 16
    descriptor.vertexDescriptor = layout
    let pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
    let imageDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba32Float, width: 2, height: 2, mipmapped: false)
    imageDescriptor.storageMode = .shared
    imageDescriptor.usage = .shaderRead
    var images: [MTLTexture] = []
    for image in 0..<2 {
        let texture = device.makeTexture(descriptor: imageDescriptor)!
        let pixels: [Float] = (0..<16).map { Float(image * 100 + $0 + 1) }
        pixels.withUnsafeBytes { texture.replace(region: MTLRegionMake2D(0,0,2,2), mipmapLevel: 0, withBytes: $0.baseAddress!, bytesPerRow: 32) }
        images.append(texture)
    }
    var samplers: [MTLSamplerState] = []
    for repeatMode in [false,true] {
        let descriptor = MTLSamplerDescriptor()
        descriptor.minFilter = .nearest; descriptor.magFilter = .nearest
        descriptor.sAddressMode = repeatMode ? .repeat : .clampToEdge
        descriptor.tAddressMode = repeatMode ? .repeat : .clampToEdge
        descriptor.supportArgumentBuffers = true
        samplers.append(device.makeSamplerState(descriptor: descriptor)!)
    }
    let arguments = fragment.makeArgumentEncoder(bufferIndex: 30)
    let argumentBuffer = device.makeBuffer(length: arguments.encodedLength, options: .storageModeShared)!
    arguments.setArgumentBuffer(argumentBuffer, offset: 0)
    for i in 0..<2 { arguments.setTexture(images[i], index: i); arguments.setSamplerState(samplers[i], index: 18+i) }
    let outputDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba32Float, width: 4, height: 4, mipmapped: false)
    outputDescriptor.storageMode = .shared; outputDescriptor.usage = [.renderTarget]
    let output = device.makeTexture(descriptor: outputDescriptor)!
    let pass = MTLRenderPassDescriptor()
    pass.colorAttachments[0].texture = output
    pass.colorAttachments[0].clearColor = MTLClearColorMake(0,0,0,0)
    pass.colorAttachments[0].loadAction = .clear; pass.colorAttachments[0].storeAction = .store
    let command = queue.makeCommandBuffer()!
    let encoder = command.makeRenderCommandEncoder(descriptor: pass)!
    encoder.setRenderPipelineState(pipeline)
    let positions: [Float] = [-1,-1,0,1, 3,-1,0,1, -1,3,0,1]
    let vertices = positions.withUnsafeBytes { device.makeBuffer(bytes: $0.baseAddress!, length: 48, options: .storageModeShared)! }
    encoder.setVertexBuffer(vertices, offset: 0, index: 0)
    encoder.setFragmentBuffer(argumentBuffer, offset: 0, index: 30)
    for image in images { encoder.useResource(image, usage: .read, stages: .fragment) }
    encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
    encoder.endEncoding(); command.commit(); command.waitUntilCompleted()
    if let error = command.error { throw error }
    guard command.status == .completed else { throw NSError(domain:"incomplete GPU command",code:2) }
    var pixels = [Float](repeating: 0, count: 64)
    pixels.withUnsafeMutableBytes { output.getBytes($0.baseAddress!, bytesPerRow: 64, from: MTLRegionMake2D(0,0,4,4), mipmapLevel: 0) }
    try pixels.withUnsafeBytes { try Data($0).write(to: URL(fileURLWithPath: args[2])) }
}
do { try run() } catch { fputs("\(error)\n",stderr); exit(1) }
