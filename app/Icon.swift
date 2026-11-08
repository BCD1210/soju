import AppKit
// App icon: a hand-drawn soju bottle and shot glass clinking ("jjan"), pen lines on warm paper,
// colour laid slightly off the lines like a quick marker sketch. Usage: swift Icon.swift <out dir>
let output = CommandLine.arguments[1]
let dir = URL(fileURLWithPath: output).appendingPathComponent("Soju.iconset")
try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)

func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> NSColor {
    NSColor(calibratedRed: r, green: g, blue: b, alpha: a)
}

// small deterministic random so every size gets the same wobble
struct Rand {
    var s: UInt64
    mutating func next() -> CGFloat {
        s = s &* 6364136223846793005 &+ 1442695040888963407
        return CGFloat((s >> 33) % 100000) / 100000.0
    }
}

let INK = rgb(0.13, 0.12, 0.11)

/// A hand-drawn line through `pts` (unit coordinates, y up): resampled, nudged sideways by a
/// slow wobble, then smoothed with Catmull-Rom so it reads like a pen line, not a vector path.
func pen(_ pts: [CGPoint], _ n: CGFloat, closed: Bool, seed: UInt64, wobble: CGFloat = 0.006) -> NSBezierPath {
    var r = Rand(s: seed)
    var src = pts
    if closed { src.append(pts[0]) }
    var out: [CGPoint] = []
    let phase1 = r.next() * 6.28, phase2 = r.next() * 6.28
    var dist: CGFloat = 0
    for i in 0..<(src.count - 1) {
        let a = src[i], b = src[i + 1]
        let len = hypot(b.x - a.x, b.y - a.y)
        let steps = max(2, Int(len / 0.025))
        for k in 0..<steps {
            let t = CGFloat(k) / CGFloat(steps)
            let x = a.x + (b.x - a.x) * t, y = a.y + (b.y - a.y) * t
            let nx = -(b.y - a.y) / max(len, 0.0001), ny = (b.x - a.x) / max(len, 0.0001)
            let w = wobble * (sin(dist * 23 + phase1) * 0.6 + sin(dist * 61 + phase2) * 0.4)
            out.append(CGPoint(x: (x + nx * w) * n, y: (y + ny * w) * n))
            dist += len / CGFloat(steps)
        }
    }
    if !closed, let last = src.last { out.append(CGPoint(x: last.x * n, y: last.y * n)) }
    if closed { out.append(out[0]); out.append(out[1]) }
    let p = NSBezierPath()
    p.move(to: out[0])
    for i in 1..<(out.count - 1) {
        let p0 = out[max(0, i - 2)], p1 = out[i - 1], p2 = out[i], p3 = out[min(out.count - 1, i + 1)]
        p.curve(to: p2, controlPoint1: CGPoint(x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6),
                controlPoint2: CGPoint(x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6))
    }
    if closed { p.close() }
    p.lineCapStyle = .round
    p.lineJoinStyle = .round
    return p
}

func ink(_ p: NSBezierPath, _ n: CGFloat, width: CGFloat = 0.024) {
    INK.setStroke()
    // fine pen at full size; small icons keep the heavier line so the shapes still read
    let w = n >= 256 ? width * 0.72 : width
    p.lineWidth = max(1.2, n * w)
    p.stroke()
}

func rotate(_ pts: [CGPoint], by deg: CGFloat, around c: CGPoint) -> [CGPoint] {
    let a = deg * .pi / 180
    return pts.map { p in
        let dx = p.x - c.x, dy = p.y - c.y
        return CGPoint(x: c.x + dx * cos(a) - dy * sin(a), y: c.y + dx * sin(a) + dy * cos(a))
    }
}

func shifted(_ p: NSBezierPath, _ dx: CGFloat, _ dy: CGFloat) -> NSBezierPath {
    let q = p.copy() as! NSBezierPath
    q.transform(using: AffineTransform(translationByX: dx, byY: dy))
    return q
}

func dot(_ c: CGPoint, _ r: CGFloat, _ n: CGFloat, _ color: NSColor = INK) {
    color.setFill()
    NSBezierPath(ovalIn: NSRect(x: (c.x - r) * n, y: (c.y - r) * n, width: r * 2 * n, height: r * 2 * n)).fill()
}

func sparkle(_ c: CGPoint, _ s: CGFloat, _ n: CGFloat, seed: UInt64) {
    // a four-point doodle star
    let pts = [CGPoint(x: c.x, y: c.y + s), CGPoint(x: c.x + s * 0.22, y: c.y + s * 0.22), CGPoint(x: c.x + s, y: c.y),
               CGPoint(x: c.x + s * 0.22, y: c.y - s * 0.22), CGPoint(x: c.x, y: c.y - s), CGPoint(x: c.x - s * 0.22, y: c.y - s * 0.22),
               CGPoint(x: c.x - s, y: c.y), CGPoint(x: c.x - s * 0.22, y: c.y + s * 0.22)]
    ink(pen(pts, n, closed: true, seed: seed, wobble: 0.002), n, width: 0.016)
}

func draw(_ n: CGFloat) {
    // paper tile
    let tile = NSBezierPath(roundedRect: NSRect(x: n * 0.08, y: n * 0.08, width: n * 0.84, height: n * 0.84),
                            xRadius: n * 0.2, yRadius: n * 0.2)
    rgb(0.975, 0.965, 0.935).setFill()
    tile.fill()
    NSGraphicsContext.saveGraphicsState()
    tile.addClip()
    var grain = Rand(s: 7)
    for _ in 0..<Int(n * 1.2) {  // a little paper tooth
        let x = grain.next(), y = grain.next(), a = 0.03 + grain.next() * 0.05
        rgb(0.55, 0.5, 0.42, a).setFill()
        NSBezierPath(rect: NSRect(x: x * n, y: y * n, width: max(1, n * 0.003), height: max(1, n * 0.003))).fill()
    }

    let small = n < 64

    // --- bottle, leaning in toward the glass ---
    let bc = CGPoint(x: 0.40, y: 0.47)
    let bottle = rotate([
        CGPoint(x: 0.31, y: 0.20), CGPoint(x: 0.49, y: 0.20), CGPoint(x: 0.50, y: 0.24), CGPoint(x: 0.50, y: 0.55),
        CGPoint(x: 0.47, y: 0.62), CGPoint(x: 0.435, y: 0.67), CGPoint(x: 0.435, y: 0.77),
        CGPoint(x: 0.365, y: 0.77), CGPoint(x: 0.365, y: 0.67), CGPoint(x: 0.33, y: 0.62), CGPoint(x: 0.30, y: 0.55),
        CGPoint(x: 0.30, y: 0.24)], by: -14, around: bc)
    let bottleLine = pen(bottle, n, closed: true, seed: 11)
    // marker colour laid a little off the line
    rgb(0.55, 0.82, 0.58).setFill()
    shifted(pen(bottle, n, closed: true, seed: 12, wobble: 0.01), n * 0.014, -n * 0.012).fill()
    // cap
    let cap = rotate([CGPoint(x: 0.36, y: 0.77), CGPoint(x: 0.44, y: 0.77), CGPoint(x: 0.44, y: 0.83), CGPoint(x: 0.36, y: 0.83)], by: -14, around: bc)
    rgb(0.30, 0.62, 0.42).setFill()
    shifted(pen(cap, n, closed: true, seed: 13), n * 0.01, -n * 0.008).fill()
    ink(bottleLine, n)
    ink(pen(cap, n, closed: true, seed: 14), n)
    // label with a face
    let label = rotate([CGPoint(x: 0.32, y: 0.32), CGPoint(x: 0.48, y: 0.32), CGPoint(x: 0.48, y: 0.47), CGPoint(x: 0.32, y: 0.47)], by: -14, around: bc)
    rgb(1, 1, 0.98).setFill()
    pen(label, n, closed: true, seed: 15).fill()
    ink(pen(label, n, closed: true, seed: 16), n, width: 0.018)
    if !small {
        let face = rotate([CGPoint(x: 0.365, y: 0.41), CGPoint(x: 0.435, y: 0.41), CGPoint(x: 0.40, y: 0.375)], by: -14, around: bc)
        dot(face[0], 0.012, n); dot(face[1], 0.012, n)
        let smile = rotate([CGPoint(x: 0.385, y: 0.375), CGPoint(x: 0.40, y: 0.362), CGPoint(x: 0.415, y: 0.375)], by: -14, around: bc)
        ink(pen(smile, n, closed: false, seed: 17, wobble: 0.001), n, width: 0.014)
        let blush = rotate([CGPoint(x: 0.345, y: 0.385), CGPoint(x: 0.455, y: 0.385)], by: -14, around: bc)
        dot(blush[0], 0.014, n, rgb(0.97, 0.6, 0.62, 0.85)); dot(blush[1], 0.014, n, rgb(0.97, 0.6, 0.62, 0.85))
        // highlight stroke on the glass of the bottle
        let hi = rotate([CGPoint(x: 0.325, y: 0.50), CGPoint(x: 0.325, y: 0.57)], by: -14, around: bc)
        rgb(1, 1, 1, 0.9).setStroke()
        let h = pen(hi, n, closed: false, seed: 18, wobble: 0.001); h.lineWidth = n * 0.018; h.stroke()
    }

    // --- shot glass, leaning back toward the bottle ---
    let gc = CGPoint(x: 0.66, y: 0.30)
    let glass = rotate([CGPoint(x: 0.585, y: 0.40), CGPoint(x: 0.735, y: 0.40), CGPoint(x: 0.715, y: 0.20), CGPoint(x: 0.605, y: 0.20)], by: 10, around: gc)
    let liquid = rotate([CGPoint(x: 0.595, y: 0.34), CGPoint(x: 0.725, y: 0.34), CGPoint(x: 0.712, y: 0.215), CGPoint(x: 0.608, y: 0.215)], by: 10, around: gc)
    rgb(0.80, 0.92, 0.95).setFill()
    shifted(pen(liquid, n, closed: true, seed: 21, wobble: 0.008), n * 0.01, -n * 0.008).fill()
    ink(pen(glass, n, closed: true, seed: 22), n)
    // thick base line
    let base = rotate([CGPoint(x: 0.61, y: 0.235), CGPoint(x: 0.71, y: 0.235)], by: 10, around: gc)
    ink(pen(base, n, closed: false, seed: 23, wobble: 0.002), n, width: 0.016)
    if !small {
        let ge = rotate([CGPoint(x: 0.635, y: 0.30), CGPoint(x: 0.685, y: 0.30)], by: 10, around: gc)
        dot(ge[0], 0.011, n); dot(ge[1], 0.011, n)
        let gm = rotate([CGPoint(x: 0.65, y: 0.275), CGPoint(x: 0.66, y: 0.268), CGPoint(x: 0.67, y: 0.275)], by: 10, around: gc)
        ink(pen(gm, n, closed: false, seed: 24, wobble: 0.001), n, width: 0.012)
        // a splash where they touch
        ink(pen([CGPoint(x: 0.555, y: 0.47), CGPoint(x: 0.575, y: 0.51)], n, closed: false, seed: 31, wobble: 0.002), n, width: 0.016)
        ink(pen([CGPoint(x: 0.59, y: 0.465), CGPoint(x: 0.62, y: 0.49)], n, closed: false, seed: 32, wobble: 0.002), n, width: 0.016)
        ink(pen([CGPoint(x: 0.605, y: 0.43), CGPoint(x: 0.64, y: 0.44)], n, closed: false, seed: 33, wobble: 0.002), n, width: 0.016)
        sparkle(CGPoint(x: 0.70, y: 0.66), 0.045, n, seed: 41)
        sparkle(CGPoint(x: 0.77, y: 0.55), 0.028, n, seed: 42)
        sparkle(CGPoint(x: 0.22, y: 0.70), 0.03, n, seed: 43)
    }
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
