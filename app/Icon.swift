import AppKit
let output = CommandLine.arguments[1]
let dir = URL(fileURLWithPath: output).appendingPathComponent("Soju.iconset")
try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = size * scale
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        let n = CGFloat(pixels)
        NSColor(calibratedRed: 0.12, green: 0.34, blue: 0.28, alpha: 1).setFill()
        NSBezierPath(roundedRect: NSRect(x: n * 0.08, y: n * 0.08, width: n * 0.84, height: n * 0.84),
                     xRadius: n * 0.2, yRadius: n * 0.2).fill()
        let text = NSAttributedString(string: "S", attributes: [
            .font: NSFont.systemFont(ofSize: n * 0.63, weight: .bold),
            .foregroundColor: NSColor(calibratedRed: 0.94, green: 0.97, blue: 0.86, alpha: 1)])
        let ts = text.size()
        text.draw(at: NSPoint(x: (n - ts.width) / 2, y: (n - ts.height) / 2))
        NSGraphicsContext.restoreGraphicsState()
        let suffix = scale == 2 ? "@2x" : ""
        try rep.representation(using: .png, properties: [:])!.write(to:
            dir.appendingPathComponent("icon_\(size)x\(size)\(suffix).png"))
    }
}
