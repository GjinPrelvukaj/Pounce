import AppKit
import CoreGraphics

// The mark, redrawn at 1024 so the generated icon set is crisp at every size.
// Geometry read off the existing 512px source and scaled 2x: a rounded-square
// field in the brand indigo, four toes on an arc, one pad below.
let S = 1024
let cs = CGColorSpaceCreateDeviceRGB()
guard let ctx = CGContext(data: nil, width: S, height: S, bitsPerComponent: 8,
                          bytesPerRow: 0, space: cs,
                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
else { fatalError("no context") }

let indigo = CGColor(red: 94/255, green: 106/255, blue: 210/255, alpha: 1)
let white = CGColor(red: 1, green: 1, blue: 1, alpha: 1)

ctx.setFillColor(indigo)
let field = CGPath(roundedRect: CGRect(x: 0, y: 0, width: 1024, height: 1024),
                   cornerWidth: 224, cornerHeight: 224, transform: nil)
ctx.addPath(field)
ctx.fillPath()

ctx.setFillColor(white)
// Toes: (centre x, centre y from the top, radius). y is flipped below.
let toes: [(CGFloat, CGFloat, CGFloat)] = [
    (250, 396, 78),
    (404, 268, 96),
    (620, 268, 96),
    (774, 396, 78),
]
for (x, yTop, r) in toes {
    ctx.addEllipse(in: CGRect(x: x - r, y: CGFloat(S) - yTop - r, width: r * 2, height: r * 2))
}
ctx.fillPath()

// The pad: an ellipse, wider than tall.
ctx.addEllipse(in: CGRect(x: 322, y: CGFloat(S) - 812, width: 380, height: 316))
ctx.fillPath()

guard let image = ctx.makeImage() else { fatalError("no image") }
let rep = NSBitmapImageRep(cgImage: image)
guard let data = rep.representation(using: .png, properties: [:]) else { fatalError("no png") }
try! data.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
print("wrote \(CommandLine.arguments[1])")
