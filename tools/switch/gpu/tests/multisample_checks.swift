@main struct MultisampleChecks {
    static func main() throws {
        guard let device = MTLCreateSystemDefaultDevice() else { throw Failure("no Metal device") }
        let transfer = try MultisampleTransfer(device)
        let w = 17, h = 19, count = w*h*4
        for format: MTLPixelFormat in [.rgba8Unorm, .bgra8Unorm, .rgba8Unorm_srgb, .bgra8Unorm_srgb, .depth32Float, .depth32Float_stencil8] {
            let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: format, width: w, height: h, mipmapped: false)
            d.textureType = .type2DMultisample; d.sampleCount = 4; d.storageMode = .private
            d.usage = [.renderTarget,.shaderRead,.pixelFormatView]
            guard let texture = device.makeTexture(descriptor: d) else { throw Failure("multisample texture") }
            let depth = format == .depth32Float || format == .depth32Float_stencil8
            var bytes = Data()
            for i in 0..<count {
                if depth {
                    var f = Float(i)/Float(count)
                    withUnsafeBytes(of: &f) { bytes.append(contentsOf: $0) }
                } else {
                    bytes.append(contentsOf: [UInt8(truncatingIfNeeded: i*3), UInt8(truncatingIfNeeded: i*7), UInt8(truncatingIfNeeded: i*11), UInt8(truncatingIfNeeded: i*13)])
                }
            }
            let stencil = format == .depth32Float_stencil8 ? Data((0..<count).map { UInt8(truncatingIfNeeded: $0*17) }) : Data()
            try transfer.upload(texture, bytes: bytes, stencil: stencil)
            try need(try transfer.download(texture) == bytes, "sample round trip \(format)")
            if !stencil.isEmpty { try need(try transfer.download(texture, stencil: true) == stencil, "stencil sample round trip") }
            do {
                try transfer.upload(texture, bytes: bytes.dropLast(), stencil: stencil)
                throw Failure("short upload accepted")
            } catch let failure as Failure {
                try need(failure.description == "sample upload byte count", "unexpected short upload result")
            }
            try need(try transfer.download(texture) == bytes, "invalid upload changed samples")
            if !depth {
                let resolvedDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: format, width: w, height: h, mipmapped: false)
                resolvedDescriptor.storageMode = .shared; resolvedDescriptor.usage = [.renderTarget]
                guard let resolved = device.makeTexture(descriptor: resolvedDescriptor), let cb = transfer.queue.makeCommandBuffer() else { throw Failure("resolve resources") }
                let pass = MTLRenderPassDescriptor()
                pass.colorAttachments[0].texture = texture; pass.colorAttachments[0].resolveTexture = resolved
                pass.colorAttachments[0].loadAction = .load; pass.colorAttachments[0].storeAction = .storeAndMultisampleResolve
                guard let encoder = cb.makeRenderCommandEncoder(descriptor: pass) else { throw Failure("resolve encoder") }
                encoder.endEncoding(); cb.commit(); cb.waitUntilCompleted()
                try need(cb.status == .completed, "resolve failed")
                var output = [UInt8](repeating: 0, count: w*h*4)
                resolved.getBytes(&output, bytesPerRow: w*4, from: MTLRegionMake2D(0,0,w,h), mipmapLevel: 0)
                for y in 0..<h { for x in 0..<w { for channel in 0..<4 {
                    let sum = (0..<4).reduce(0) { result, s in result + Int(bytes[((y*2+s/2)*w*2+x*2+s%2)*4+channel]) }
                    if channel < 3 && (format == .rgba8Unorm_srgb || format == .bgra8Unorm_srgb) {
                        let linear = (0..<4).reduce(0.0) { result, s in
                            let c = Double(bytes[((y*2+s/2)*w*2+x*2+s%2)*4+channel])/255.0
                            return result + (c <= 0.04045 ? c/12.92 : pow((c+0.055)/1.055,2.4))
                        } / 4.0
                        let encoded = linear <= 0.0031308 ? linear*12.92 : 1.055*pow(linear,1/2.4)-0.055
                        try need(abs(Double(output[(y*w+x)*4+channel])-encoded*255) <= 1, "sRGB linear resolve")
                    } else {
                        try need(abs(Int(output[(y*w+x)*4+channel])*4-sum) <= 2, "resolve sample average")
                    }
                } } }
                try need(try transfer.download(texture) == bytes, "resolve destroyed stored samples")
            }
        }
        print("multisample_transfer formats=6 samples=7752 round_trip=passed stencil=passed resolve=passed invalid_upload=passed")
    }
}
