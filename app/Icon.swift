import AppKit
// App icon: a green soju bottle and a filled shot glass on a deep navy tile.
// Drawn as vectors so every size of the iconset stays sharp. Usage: swift Icon.swift <out dir>
let output = CommandLine.arguments[1]
let dir = URL(fileURLWithPath: output).appendingPathComponent("Soju.iconset")
try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)

func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> NSColor {
    NSColor(calibratedRed: r, green: g, blue: b, alpha: a)
}

func draw(_ n: CGFloat) {
    // tile
    let tile = NSBezierPath(roundedRect: NSRect(x: n * 0.08, y: n * 0.08, width: n * 0.84, height: n * 0.84),
                            xRadius: n * 0.2, yRadius: n * 0.2)
    NSGradient(starting: rgb(0.13, 0.19, 0.31), ending: rgb(0.06, 0.09, 0.16))!.draw(in: tile, angle: -90)
    NSGraphicsContext.saveGraphicsState()
    tile.addClip()
    // soft light behind the bottle
    let glow = NSBezierPath(ovalIn: NSRect(x: n * 0.12, y: n * 0.22, width: n * 0.62, height: n * 0.62))
    NSGradient(starting: rgb(0.35, 0.75, 0.55, 0.28), ending: rgb(0.35, 0.75, 0.55, 0))!.draw(in: glow, relativeCenterPosition: .zero)

    // bottle: body, shoulder, neck, cap
    let bx = n * 0.25, bw = n * 0.28, by = n * 0.16, bh = n * 0.42
    let body = NSBezierPath()
    let neckW = bw * 0.36, neckX = bx + (bw - neckW) / 2
    body.move(to: NSPoint(x: bx + n * 0.03, y: by))
    body.line(to: NSPoint(x: bx + bw - n * 0.03, y: by))
    body.curve(to: NSPoint(x: bx + bw, y: by + n * 0.03), controlPoint1: NSPoint(x: bx + bw, y: by), controlPoint2: NSPoint(x: bx + bw, y: by))
    body.line(to: NSPoint(x: bx + bw, y: by + bh))
    body.curve(to: NSPoint(x: neckX + neckW, y: by + bh + n * 0.13),
               controlPoint1: NSPoint(x: bx + bw, y: by + bh + n * 0.07), controlPoint2: NSPoint(x: neckX + neckW, y: by + bh + n * 0.06))
    body.line(to: NSPoint(x: neckX + neckW, y: by + bh + n * 0.22))
    body.line(to: NSPoint(x: neckX, y: by + bh + n * 0.22))
    body.line(to: NSPoint(x: neckX, y: by + bh + n * 0.13))
    body.curve(to: NSPoint(x: bx, y: by + bh),
               controlPoint1: NSPoint(x: neckX, y: by + bh + n * 0.06), controlPoint2: NSPoint(x: bx, y: by + bh + n * 0.07))
    body.line(to: NSPoint(x: bx, y: by + n * 0.03))
    body.curve(to: NSPoint(x: bx + n * 0.03, y: by), controlPoint1: NSPoint(x: bx, y: by), controlPoint2: NSPoint(x: bx, y: by))
    body.close()
    NSGradient(colors: [rgb(0.10, 0.45, 0.27), rgb(0.27, 0.72, 0.46), rgb(0.12, 0.50, 0.30)])!.draw(in: body, angle: 0)
    rgb(0.05, 0.25, 0.15, 0.9).setStroke()
    body.lineWidth = max(1, n * 0.008)
    body.stroke()
    // cap
    let cap = NSBezierPath(roundedRect: NSRect(x: neckX - n * 0.006, y: by + bh + n * 0.21, width: neckW + n * 0.012, height: n * 0.06),
                           xRadius: n * 0.012, yRadius: n * 0.012)
    NSGradient(starting: rgb(0.20, 0.62, 0.38), ending: rgb(0.08, 0.38, 0.22))!.draw(in: cap, angle: -90)
    // label
    let label = NSBezierPath(roundedRect: NSRect(x: bx + n * 0.015, y: by + bh * 0.28, width: bw - n * 0.03, height: bh * 0.36),
                             xRadius: n * 0.015, yRadius: n * 0.015)
    rgb(0.95, 0.97, 0.98).setFill()
    label.fill()
    rgb(0.20, 0.42, 0.78).setFill()
    NSBezierPath(rect: NSRect(x: bx + n * 0.015, y: by + bh * 0.28, width: bw - n * 0.03, height: bh * 0.06)).fill()
    if n >= 64 {
        let s = NSAttributedString(string: "소주", attributes: [
            .font: NSFont.systemFont(ofSize: n * 0.065, weight: .heavy), .foregroundColor: rgb(0.12, 0.34, 0.62)])
        let ss = s.size()
        s.draw(at: NSPoint(x: bx + (bw - ss.width) / 2, y: by + bh * 0.39))
    }
    // highlight
    rgb(1, 1, 1, 0.32).setFill()
    NSBezierPath(roundedRect: NSRect(x: bx + bw * 0.14, y: by + n * 0.03, width: bw * 0.09, height: bh * 0.82),
                 xRadius: n * 0.01, yRadius: n * 0.01).fill()

    // shot glass, front right, filled
    let gx = n * 0.555, gy = n * 0.16, gw = n * 0.18, gh = n * 0.19
    let glass = NSBezierPath()
    glass.move(to: NSPoint(x: gx + gw * 0.12, y: gy))
    glass.line(to: NSPoint(x: gx + gw * 0.88, y: gy))
    glass.line(to: NSPoint(x: gx + gw, y: gy + gh))
    glass.line(to: NSPoint(x: gx, y: gy + gh))
    glass.close()
    rgb(0.85, 0.93, 1.0, 0.22).setFill()
    glass.fill()
    // liquid
    let lh = gh * 0.62
    let liquid = NSBezierPath()
    liquid.move(to: NSPoint(x: gx + gw * 0.15, y: gy + gh * 0.08))
    liquid.line(to: NSPoint(x: gx + gw * 0.85, y: gy + gh * 0.08))
    liquid.line(to: NSPoint(x: gx + gw * 0.85 + gw * 0.12 * (lh / gh), y: gy + lh))
    liquid.line(to: NSPoint(x: gx + gw * 0.15 - gw * 0.12 * (lh / gh), y: gy + lh))
    liquid.close()
    rgb(0.80, 0.95, 0.92, 0.75).setFill()
    liquid.fill()
    // thick glass base
    rgb(0.90, 0.96, 1.0, 0.55).setFill()
    NSBezierPath(rect: NSRect(x: gx + gw * 0.13, y: gy, width: gw * 0.74, height: gh * 0.08)).fill()
    rgb(0.92, 0.97, 1.0, 0.95).setStroke()
    glass.lineWidth = max(1, n * 0.01)
    glass.stroke()
    // rim shine
    rgb(1, 1, 1, 0.6).setFill()
    NSBezierPath(rect: NSRect(x: gx + gw * 0.08, y: gy + gh * 0.2, width: gw * 0.06, height: gh * 0.6)).fill()
    NSGraphicsContext.restoreGraphicsState()
}

for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = size * scale
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        draw(CGFloat(pixels))
        NSGraphicsContext.restoreGraphicsState()
        let suffix = scale == 2 ? "@2x" : ""
        try rep.representation(using: .png, properties: [:])!.write(to:
            dir.appendingPathComponent("icon_\(size)x\(size)\(suffix).png"))
    }
}
