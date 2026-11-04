import Foundation
import Metal

struct Failure: Error, CustomStringConvertible { let description: String; init(_ s: String) { description = s } }
func need(_ b: Bool, _ s: String) throws { if !b { throw Failure(s) } }
typealias Object = [String: Any]
func number(_ d: Object, _ k: String) -> Int { (d[k] as? NSNumber)?.intValue ?? 0 }
func array(_ d: Object, _ k: String) -> [Object] { d[k] as? [Object] ?? [] }
func string(_ d: Object, _ k: String) throws -> String { guard let s = d[k] as? String else { throw Failure("missing string \(k)") }; return s }
func compare(_ v: Int) throws -> MTLCompareFunction {
    let n = (0x200...0x207).contains(v) ? v - 0x200 : v - 1
    guard (0...7).contains(n), let c = MTLCompareFunction(rawValue: UInt(n)) else { throw Failure("compare function \(v)") }; return c
}
func blendOp(_ v: Int) throws -> MTLBlendOperation {
    switch v { case 1,0x8006:return .add;case 2,0x800a:return .subtract;case 3,0x800b:return .reverseSubtract;case 4,0x8007:return .min;case 5,0x8008:return .max;default:throw Failure("blend op \(v)") }
}
func blendFactor(_ v: Int) throws -> MTLBlendFactor {
    switch v {case 1,0x4000:return .zero;case 2,0x4001:return .one;case 3,0x4300:return .sourceColor;case 4,0x4301:return .oneMinusSourceColor;case 5,0x4302:return .sourceAlpha;case 6,0x4303:return .oneMinusSourceAlpha;case 7,0x4304:return .destinationAlpha;case 8,0x4305:return .oneMinusDestinationAlpha;case 9,0x4306:return .destinationColor;case 10,0x4307:return .oneMinusDestinationColor;case 11,0x4308:return .sourceAlphaSaturated;case 14,0xc001:return .blendColor;case 15,0xc002:return .oneMinusBlendColor;case 0xc003:return .blendAlpha;case 0xc004:return .oneMinusBlendAlpha;default:throw Failure("blend factor \(v)")}
}
func stencilOp(_ v: Int) throws -> MTLStencilOperation {
    switch v {case 1,0x1e00:return .keep;case 2,0:return .zero;case 3,0x1e01:return .replace;case 4,0x1e02:return .incrementClamp;case 5,0x1e03:return .decrementClamp;case 6,0x150a:return .invert;case 7,0x8507:return .incrementWrap;case 8,0x8508:return .decrementWrap;default:throw Failure("stencil op \(v)")}
}
final class Worker {
    let device: MTLDevice
    let queue: MTLCommandQueue
    var targets = [String: MTLTexture](), generations = [String: Int](), depths = [String: MTLTexture](), depthGenerations = [String: Int]()
    var occlusion: UInt64 = 0
    var queryValues = [String: UInt32]()
    var targetBytes = [String: Int]()
    var pipelines = [String: MTLRenderPipelineState](), depthStates = [String: MTLDepthStencilState]()
    var functions = [String: MTLFunction](), textureCache = [String: MTLTexture]()
    init() throws {
        guard let d = MTLCreateSystemDefaultDevice(), let q = d.makeCommandQueue() else { throw Failure("no default Metal device/queue") }
        device = d; queue = q
    }
    func run(_ directory: String) throws -> String {
    let root = URL(fileURLWithPath: directory)
    let data = try Data(contentsOf: root.appendingPathComponent("commands.json"))
    guard let commands = try JSONSerialization.jsonObject(with: data) as? [Object] else { throw Failure("invalid command protocol") }
    var pipelineHits = 0, textureHits = 0, draws = 0, clears = 0, encoderCount = 0, bufferCount = 0, depthHits = 0
    let started=ProcessInfo.processInfo.systemUptime
    var cb: MTLCommandBuffer? = nil
    var activeEncoder: MTLRenderCommandEncoder? = nil
    var activeKey=""
    var retainedSamplers=[MTLSamplerState]()
    func endPass() {activeEncoder?.endEncoding();activeEncoder=nil;activeKey=""}
    func flush() throws {
        endPass()
        if let pending=cb {
            pending.commit();pending.waitUntilCompleted()
            try need(pending.status == .completed,"GPU command failed: \(String(describing:pending.error))")
            cb=nil;retainedSamplers.removeAll()
        }
    }
    func commandBuffer() throws -> MTLCommandBuffer {
        if cb==nil {cb=queue.makeCommandBuffer();bufferCount += 1}
        guard let buffer=cb else {throw Failure("command buffer")};return buffer
    }
    func file(_ name: String) throws -> Data { try Data(contentsOf: root.appendingPathComponent(name)) }
    func buffer(_ name: String) throws -> MTLBuffer {
        let bytes = try file(name)
        guard !bytes.isEmpty, let b = bytes.withUnsafeBytes({ p in device.makeBuffer(bytes:p.baseAddress!,length:p.count,options:.storageModeShared) }) else { throw Failure("buffer allocation") }; return b
    }
    func function(_ path: String, _ entry: String) throws -> MTLFunction {
        let source = try String(contentsOf: root.appendingPathComponent(path), encoding:.utf8)
        if let f = functions[source] { return f }
        let options = MTLCompileOptions(); options.mathMode = .safe
        let library = try device.makeLibrary(source:source, options:options)
        guard let f = library.makeFunction(name:entry) else { throw Failure("entry \(entry)") }; functions[source] = f; return f
    }
    for command in commands {
        if command["query_reset"] != nil {try flush();occlusion=0;continue}
        if let key=command["query_report"] as? String {try flush();queryValues[key]=UInt32(truncatingIfNeeded:occlusion);continue}
        guard let raw = command["registers"] as? Object, let attachments = command["targets"] as? [Object], let target = attachments.first else { throw Failure("missing state") }
        func r(_ a: Int) -> Int { number(raw,String(a)) }
        func f(_ a: Int) -> Double { Double(Float(bitPattern:UInt32(r(a)))) }
        let w = number(target,"width"), h = number(target,"height")
        var colors = [MTLTexture]()
        for target in attachments {
            let key = try string(target,"key")
        if targets[key] == nil {
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:MTLPixelFormat(rawValue:UInt(number(target,"metal")))!,width:w,height:h,mipmapped:false)
            descriptor.storageMode = .shared; descriptor.usage = [.renderTarget,.shaderRead]
            guard let texture = device.makeTexture(descriptor:descriptor) else { throw Failure("color allocation") }; targets[key] = texture; targetBytes[key] = number(target,"bytes")
        }
        let color = targets[key]!
        if generations[key] != number(target,"generation") {
            if generations[key] != nil {try flush()}
            let initial = try file(string(target,"file"));try need(initial.count == w*h*number(target,"bytes"),"target initial bytes")
            initial.withUnsafeBytes { p in color.replace(region:MTLRegionMake2D(0,0,w,h),mipmapLevel:0,withBytes:p.baseAddress!,bytesPerRow:w*number(target,"bytes")) }
            generations[key] = number(target,"generation")
        }
            colors.append(color)
        }
        let mask = number(command,"clear"), isClear = command["clear"] != nil
        let pass = MTLRenderPassDescriptor()
        for (slot, color) in colors.enumerated() {
            let a = pass.colorAttachments[slot]!
            a.texture = color; a.loadAction = isClear && mask & 60 != 0 && (mask >> 6) == slot ? .clear : .load; a.storeAction = .store
            a.clearColor = MTLClearColor(red:f(0xd80),green:f(0xd84),blue:f(0xd88),alpha:f(0xd8c))
        }
        try need(r(0x1514)<=1,"invalid occlusion enable")
        let visibility = r(0x1514)==1 && !isClear ? device.makeBuffer(length:8,options:.storageModeShared) : nil
        if let visibility {visibility.contents().storeBytes(of:UInt64(0),as:UInt64.self);pass.visibilityResultBuffer=visibility}
        var depthFormat = MTLPixelFormat.invalid
        if let depth = command["depth"] as? Object {
            let dk = try string(depth,"key")
            depthFormat = [0x19,0x14].contains(number(depth,"format")) ? .depth32Float_stencil8 : (number(depth,"format")==0x13 ? .depth16Unorm : .depth32Float)
            if depthGenerations[dk] != number(depth,"generation") {
                if depthGenerations[dk] != nil {try flush()}
                depths.removeValue(forKey:dk)
            }
            if depths[dk] == nil {
                try need(isClear && mask & 1 != 0 && (depthFormat != .depth32Float_stencil8 || mask & 2 != 0),"new zeta target requires clear before use")
                let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:depthFormat,width:w,height:h,mipmapped:false)
                descriptor.storageMode = .private;descriptor.usage = .renderTarget
                guard let t = device.makeTexture(descriptor:descriptor) else { throw Failure("depth allocation") };depths[dk] = t; depthGenerations[dk] = number(depth,"generation")
            }
            pass.depthAttachment.texture = depths[dk];pass.depthAttachment.loadAction = isClear && mask & 1 != 0 ? .clear : .load;pass.depthAttachment.storeAction = .store
            pass.depthAttachment.clearDepth = f(0xd90)
            if depthFormat == .depth32Float_stencil8 {
                pass.stencilAttachment.texture = depths[dk];pass.stencilAttachment.loadAction = isClear && mask & 2 != 0 ? .clear : .load;pass.stencilAttachment.storeAction = .store
                pass.stencilAttachment.clearStencil = UInt32(r(0xda0))
            }
        }
        let depthKey=(command["depth"] as? Object)?["key"] as? String ?? "none"
        let passKey=try attachments.map{try string($0,"key")}.joined(separator:"|")+"|"+depthKey
        if isClear || visibility != nil || activeKey != passKey {endPass()}
        let current=try commandBuffer()
        if isClear {
            try need(r(0xe00)==0,"scissored clear unsupported")
            guard let encoder = current.makeRenderCommandEncoder(descriptor:pass) else { throw Failure("clear encoder") }; encoderCount += 1; encoder.endEncoding();clears += 1
        } else {
            let pk = try string(command,"pipeline")
            let vs = try function(string(command,"vertex"),"soju_vertex"), fs = try function(string(command,"fragment"),"soju_fragment")
            if pipelines[pk] == nil {
                let descriptor = MTLRenderPipelineDescriptor();descriptor.vertexFunction = vs;descriptor.fragmentFunction = fs
                for slot in colors.indices { descriptor.colorAttachments[slot].pixelFormat = colors[slot].pixelFormat }
                descriptor.depthAttachmentPixelFormat = depthFormat
                if depthFormat == .depth32Float_stencil8 { descriptor.stencilAttachmentPixelFormat = depthFormat }
                try need(r(0x12e4)<=1,"invalid per-target blend enable")
                for slot in colors.indices {
                    let physical = (r(0x121c) >> (4 + slot * 3)) & 7
                    let blend = descriptor.colorAttachments[slot]!
                    try need(r(0x1360 + physical*4)<=1,"invalid blend enable")
                    blend.isBlendingEnabled = r(0x1360 + physical*4)==1
                    if blend.isBlendingEnabled {
                        let per = r(0x12e4)==1, base = 0x1e00 + physical*32
                        let separate = r(per ? base : 0x133c) != 0
                        blend.rgbBlendOperation = try blendOp(r(per ? base+4 : 0x1340))
                        blend.sourceRGBBlendFactor = try blendFactor(r(per ? base+8 : 0x1344))
                        blend.destinationRGBBlendFactor = try blendFactor(r(per ? base+12 : 0x1348))
                        blend.alphaBlendOperation = try blendOp(r(per ? base+(separate ? 16 : 4) : (separate ? 0x134c : 0x1340)))
                        blend.sourceAlphaBlendFactor = try blendFactor(r(per ? base+(separate ? 20 : 8) : (separate ? 0x1350 : 0x1344)))
                        blend.destinationAlphaBlendFactor = try blendFactor(r(per ? base+(separate ? 24 : 12) : (separate ? 0x1358 : 0x1348)))
                    }
                }
                let vertex = MTLVertexDescriptor()
                for a in array(command,"attributes") {
                    let slot = number(a,"slot"), stream = number(a,"stream")
                    guard let format = MTLVertexFormat(rawValue:UInt(number(a,"format"))) else {throw Failure("vertex format")}
                    vertex.attributes[slot].format = format;vertex.attributes[slot].offset = number(a,"offset");vertex.attributes[slot].bufferIndex = stream
                    vertex.layouts[stream].stride = number(a,"stride");vertex.layouts[stream].stepFunction = number(a,"instanced")==1 ? .constant : .perVertex;vertex.layouts[stream].stepRate = number(a,"instanced")==1 ? 0 : 1
                }
                descriptor.vertexDescriptor = vertex
                pipelines[pk] = try device.makeRenderPipelineState(descriptor:descriptor)
            } else {pipelineHits += 1}
            if depthStates[pk] == nil {
                let d = MTLDepthStencilDescriptor()
                try need(r(0x12cc)<=1 && r(0x12e8)<=1 && r(0x1380)<=1,"invalid depth/stencil enable")
                d.depthCompareFunction = r(0x12cc)==0 ? .always : try compare(r(0x130c));d.isDepthWriteEnabled = r(0x12e8)==1 && r(0x12cc)==1
                if r(0x1380)==1 {
                    try need(depthFormat == .depth32Float_stencil8,"stencil format")
                    func stencil(_ back: Bool) throws -> MTLStencilDescriptor {
                        let s = MTLStencilDescriptor();s.stencilCompareFunction = try compare(r(back ? 0x15a4 : 0x1390))
                        s.stencilFailureOperation = try stencilOp(r(back ? 0x1598 : 0x1384));s.depthFailureOperation = try stencilOp(r(back ? 0x159c : 0x1388));s.depthStencilPassOperation = try stencilOp(r(back ? 0x15a0 : 0x138c))
                        s.readMask = UInt32(r(back ? 0xf5c : 0x1398));s.writeMask = UInt32(r(back ? 0xf58 : 0x139c));return s
                    }
                    d.frontFaceStencil = try stencil(false);d.backFaceStencil = try stencil(r(0x1594)==1)
                }
                guard let state = device.makeDepthStencilState(descriptor:d) else {throw Failure("depth/stencil state")};depthStates[pk] = state
            } else {depthHits += 1}
            if activeEncoder==nil {
                activeEncoder=current.makeRenderCommandEncoder(descriptor:pass);activeKey=passKey;encoderCount += 1
            }
            guard let encoder=activeEncoder else {throw Failure("render encoder")}
            if visibility != nil {encoder.setVisibilityResultMode(.counting,offset:0)}
            encoder.setRenderPipelineState(pipelines[pk]!);encoder.setDepthStencilState(depthStates[pk]!)
            let sx=f(0xa00),sy=f(0xa04),sz=f(0xa08),ox=f(0xa0c),oy=f(0xa10),oz=f(0xa14)
            let near = r(0xd7c)==0 ? oz-sz : oz, far=oz+sz
            try need([sx,sy,sz,ox,oy,oz].allSatisfy{$0.isFinite} && sx != 0 && sy != 0 && near>=0 && near<=1 && far>=0 && far<=1,"invalid viewport/depth range")
            encoder.setViewport(MTLViewport(originX:ox-abs(sx),originY:oy-abs(sy),width:2*abs(sx),height:2*abs(sy),znear:near,zfar:far))
            let offset = r(0xdac)==0x1b01 ? r(0xdc4) : r(0xdc8)
            try need([r(0xdc0),r(0xdc4),r(0xdc8)].allSatisfy{$0<=1},"invalid polygon offset enable")
            if offset != 0 {
                try need(r(0x1110)==1,"depth bias requires format-dependent units")
                encoder.setDepthBias(Float(f(0x15bc)),slopeScale:Float(f(0x156c)),clamp:Float(f(0x187c)))
            } else {encoder.setDepthBias(0,slopeScale:0,clamp:0)}
            if r(0xe00) != 0 {
                let x=r(0xe04)&65535,y=r(0xe08)&65535,endX=r(0xe04)>>16,endY=r(0xe08)>>16
                try need(x<endX && y<endY && endX<=w && endY<=h,"invalid scissor")
                encoder.setScissorRect(MTLScissorRect(x:x,y:y,width:endX-x,height:endY-y))
            } else {encoder.setScissorRect(MTLScissorRect(x:0,y:0,width:w,height:h))}
            if r(0x1918) != 0 {
                switch r(0x1920) {case 0x404:encoder.setCullMode(.front);case 0x405:encoder.setCullMode(.back);default:throw Failure("cull face unsupported")}
            } else {encoder.setCullMode(.none)}
            switch r(0x191c) {case 0,0x900:encoder.setFrontFacing(.clockwise);case 0x901:encoder.setFrontFacing(.counterClockwise);default:throw Failure("front winding")}
            try need(r(0xdac)==r(0xdb0) && [0,0x1b01,0x1b02].contains(r(0xdac)),"polygon mode unsupported")
            encoder.setTriangleFillMode(r(0xdac)==0x1b01 ? .lines : .fill)
            encoder.setBlendColor(red:Float(f(0x131c)),green:Float(f(0x1320)),blue:Float(f(0x1324)),alpha:Float(f(0x1328)))
            encoder.setStencilReferenceValues(front:UInt32(r(0x1394)),back:UInt32(r(r(0x1594)==1 ? 0xf54 : 0x1394)))
            for b in array(command,"buffers") {
                let slot=number(b,"slot")
                let a=array(command,"attributes").first{number($0,"stream")==slot}!
                let offset=number(a,"instanced")==1 ? (number(command,"instance")/number(a,"rate"))*number(a,"stride") : 0
                encoder.setVertexBuffer(try buffer(string(b,"file")),offset:offset,index:slot)
            }
            for c in array(command,"constants") {
                let b=try buffer(string(c,"file"));if number(c,"stage")==0 {encoder.setVertexBuffer(b,offset:0,index:number(c,"slot"))}else{encoder.setFragmentBuffer(b,offset:0,index:number(c,"slot"))}
            }
            for stage in 0...1 {
                let items=array(command,"textures").filter{number($0,"stage")==stage}
                if items.isEmpty {continue}
                let argument=(stage==0 ? vs : fs).makeArgumentEncoder(bufferIndex:30)
                guard let table=device.makeBuffer(length:argument.encodedLength,options:.storageModeShared) else {throw Failure("argument allocation")};argument.setArgumentBuffer(table,offset:0)
                for item in items {
                    if item["image"] != nil {
                        let tk=try string(item,"key")
                        if textureCache[tk]==nil {
                            let tw=number(item,"width"),th=number(item,"height")
                            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:MTLPixelFormat(rawValue:UInt(number(item,"metal")))!,width:tw,height:th,mipmapped:false)
                            d.storageMode = .shared;d.usage = [.shaderRead,.pixelFormatView]
                            guard let t=device.makeTexture(descriptor:d) else {throw Failure("sample texture allocation (BC1 device support required)")}
                            let bytes=try file(string(item,"file"));let row=number(item,"row")
                            try need(bytes.count == row*number(item,"rows"),"sample texture size")
                            bytes.withUnsafeBytes{p in t.replace(region:MTLRegionMake2D(0,0,tw,th),mipmapLevel:0,withBytes:p.baseAddress!,bytesPerRow:row)}
                            let components = (item["swizzle"] as! [NSNumber]).map { MTLTextureSwizzle(rawValue:$0.uint8Value)! }
                            let swizzle = MTLTextureSwizzleChannels(red:components[0],green:components[1],blue:components[2],alpha:components[3])
                            guard let view=t.makeTextureView(pixelFormat:t.pixelFormat,textureType:.type2D,levels:0..<1,slices:0..<1,swizzle:swizzle) else {throw Failure("texture view")}
                            textureCache[tk]=view
                        }else{textureHits += 1}
                        let t=textureCache[tk]!;argument.setTexture(t,index:number(item,"image"));encoder.useResource(t,usage:.read,stages:stage==0 ? .vertex : .fragment)
                    }else{
                        let d=MTLSamplerDescriptor();let modes:[MTLSamplerAddressMode]=[.repeat,.mirrorRepeat,.clampToEdge]
                        d.sAddressMode=modes[number(item,"u")];d.tAddressMode=modes[number(item,"v")];d.normalizedCoordinates=true
                        d.minFilter=number(item,"min")==1 ? .nearest : .linear;d.magFilter=number(item,"mag")==1 ? .nearest : .linear;d.mipFilter = .notMipmapped;d.supportArgumentBuffers=true
                        guard let sampler=device.makeSamplerState(descriptor:d) else {throw Failure("sampler allocation")};retainedSamplers.append(sampler);argument.setSamplerState(sampler,index:9*number(item,"count")+number(item,"sampler"))
                    }
                }
                if stage==0 {encoder.setVertexBuffer(table,offset:0,index:30)}else{encoder.setFragmentBuffer(table,offset:0,index:30)}
            }
            let topology:MTLPrimitiveType
            switch number(command,"topology") {case 0:topology = .point;case 1:topology = .line;case 3:topology = .lineStrip;case 4:topology = .triangle;case 5:topology = .triangleStrip;default:throw Failure("primitive")}
            if let indexFile=command["indices"] as? String {
                let indices=try buffer(indexFile)
                for segment in array(command,"segments") {
                    encoder.drawIndexedPrimitives(type:topology,indexCount:number(segment,"count"),indexType:number(command,"indexSize")==2 ? .uint16 : .uint32,indexBuffer:indices,indexBufferOffset:number(segment,"start")*number(command,"indexSize"),instanceCount:1,baseVertex:number(command,"baseVertex"),baseInstance:number(command,"instance"))
                }
            }else{encoder.drawPrimitives(type:topology,vertexStart:number(command,"first"),vertexCount:number(command,"count"),instanceCount:1,baseInstance:number(command,"instance"))}
            draws += 1
        }
        if let visibility {try flush();occlusion &+= visibility.contents().load(as:UInt64.self)}
    }
    try flush()
    let elapsed=ProcessInfo.processInfo.systemUptime-started
    return "encoders=\(encoderCount) command_buffers=\(bufferCount) depth_hits=\(depthHits) seconds=\(elapsed) draws_per_second=\(Double(draws)/max(elapsed,0.000001)) device=\(device.name) draws=\(draws) clears=\(clears) pipelines=\(pipelines.count) pipeline_hits=\(pipelineHits) textures=\(textureCache.count) texture_hits=\(textureHits)"
}
    func readback(_ directory: String) throws -> String {
        let root = URL(fileURLWithPath:directory)
    for (key,t) in targets {
        var bytes=[UInt8](repeating:0,count:t.width*t.height*targetBytes[key]!)
        t.getBytes(&bytes,bytesPerRow:t.width*targetBytes[key]!,from:MTLRegionMake2D(0,0,t.width,t.height),mipmapLevel:0)
        try Data(bytes).write(to:root.appendingPathComponent("\(key).rgba"))
    }
        for (key,value) in queryValues {
            var little=value.littleEndian
            try withUnsafeBytes(of:&little) {try Data($0).write(to:root.appendingPathComponent("\(key).query"))}
        }
        return "readback_targets=\(targets.count)"
    }
}
do {
    let worker = try Worker()
    if CommandLine.arguments[1] == "--service" {
        while let line = readLine() {
            do {
                let parts = line.split(separator:"\t",maxSplits:1).map(String.init)
                try need(parts.count == 2,"invalid request")
                let result = try parts[0] == "submit" ? worker.run(parts[1]) : worker.readback(parts[1])
                print("OK \(result)"); fflush(stdout)
            } catch { print("ERROR \(String(describing:error).replacingOccurrences(of:"\n",with:" "))"); fflush(stdout) }
        }
    } else {
        print(try worker.run(CommandLine.arguments[1]))
        print(try worker.readback(CommandLine.arguments[1]))
    }
} catch { fputs("\(error)\n",stderr); exit(1) }
