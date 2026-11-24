// Draws the Xilo app icon. Usage: swift make-icon.swift out.png
import AppKit

let size: CGFloat = 1024
let out = CommandLine.arguments[1]
let space = CGColorSpaceCreateDeviceRGB()
let ctx = CGContext(data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8,
                    bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!

let inset: CGFloat = 100
let rect = CGRect(x: inset, y: inset, width: size - inset * 2, height: size - inset * 2)
let tile = CGPath(roundedRect: rect, cornerWidth: 185, cornerHeight: 185, transform: nil)

ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 30, color: CGColor(gray: 0, alpha: 0.3))
ctx.addPath(tile)
ctx.setFillColor(CGColor(red: 0, green: 0.6, blue: 0.75, alpha: 1))
ctx.fillPath()
ctx.restoreGState()

ctx.saveGState()
ctx.addPath(tile)
ctx.clip()
let gradient = CGGradient(colorsSpace: space, colors: [
    CGColor(red: 0.0, green: 0.80, blue: 0.92, alpha: 1),
    CGColor(red: 0.0, green: 0.45, blue: 0.70, alpha: 1),
] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(gradient, start: CGPoint(x: 0, y: size), end: CGPoint(x: size, y: 0), options: [])
ctx.restoreGState()

let bolt = CGMutablePath()
let points: [(CGFloat, CGFloat)] = [(560, 820), (330, 480), (480, 480), (430, 204), (694, 572), (540, 572), (600, 820)]
bolt.move(to: CGPoint(x: points[0].0, y: points[0].1))
for p in points.dropFirst() { bolt.addLine(to: CGPoint(x: p.0, y: p.1)) }
bolt.closeSubpath()
ctx.addPath(bolt)
ctx.setFillColor(CGColor(gray: 1, alpha: 1))
ctx.fillPath()

let image = NSBitmapImageRep(cgImage: ctx.makeImage()!)
try! image.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
