import AppKit
import CoreGraphics
import CoreText
import Foundation
import ImageIO
import UniformTypeIdentifiers

let sourceURL = URL(fileURLWithPath: "/Users/jychen/Downloads/人像转绘本图标.png")
let outputURL = URL(fileURLWithPath: "assets/branding/cditor-portrait-app-icon-v2.png")

guard let source = CGImageSourceCreateWithURL(sourceURL as CFURL, nil),
      let portrait = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    fatalError("Unable to load portrait")
}

let canvasSize = 1024
let colorSpace = CGColorSpaceCreateDeviceRGB()
guard let context = CGContext(
    data: nil,
    width: canvasSize,
    height: canvasSize,
    bitsPerComponent: 8,
    bytesPerRow: 0,
    space: colorSpace,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { fatalError("Unable to create context") }

func roundedRect(_ rect: CGRect, radius: CGFloat) -> CGPath {
    CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
}

let shell = CGRect(x: 64, y: 64, width: 896, height: 896)
context.addPath(roundedRect(shell, radius: 208))
context.clip()
context.setFillColor(CGColor(red: 22 / 255, green: 23 / 255, blue: 21 / 255, alpha: 1))
context.fill(shell)

// One oversized letterform owns the silhouette; the portrait becomes its material.
let font = CTFontCreateWithName("HelveticaNeue-CondensedBlack" as CFString, 850, nil)
var character: UniChar = 67
var glyph: CGGlyph = 0
guard CTFontGetGlyphsForCharacters(font, &character, &glyph, 1),
      let rawGlyphPath = CTFontCreatePathForGlyph(font, glyph, nil) else {
    fatalError("Unable to build C glyph")
}

let bounds = rawGlyphPath.boundingBoxOfPath
let target = CGRect(x: 128, y: 132, width: 720, height: 760)
let glyphScale = min(target.width / bounds.width, target.height / bounds.height)
var transform = CGAffineTransform(
    a: glyphScale,
    b: 0,
    c: 0,
    d: glyphScale,
    tx: target.midX - bounds.midX * glyphScale,
    ty: target.midY - bounds.midY * glyphScale
)
guard let glyphPath = rawGlyphPath.copy(using: &transform) else { fatalError("Unable to transform glyph") }

context.saveGState()
context.addPath(glyphPath)
context.clip()

guard let crop = portrait.cropping(to: CGRect(x: 325, y: 270, width: 1400, height: 1400)) else {
    fatalError("Unable to crop portrait")
}
context.draw(crop, in: CGRect(x: 58, y: 68, width: 900, height: 900))

// Gentle warm glaze ties the source illustration to the product palette.
context.setFillColor(CGColor(red: 200 / 255, green: 1, blue: 61 / 255, alpha: 0.09))
context.fill(shell)
context.restoreGState()

context.addPath(glyphPath)
context.setStrokeColor(CGColor(red: 200 / 255, green: 1, blue: 61 / 255, alpha: 1))
context.setLineWidth(13)
context.setLineJoin(.round)
context.strokePath()

// The C opening resolves into a single insertion caret.
context.setFillColor(CGColor(red: 200 / 255, green: 1, blue: 61 / 255, alpha: 1))
context.addPath(roundedRect(CGRect(x: 817, y: 421, width: 28, height: 190), radius: 14))
context.fillPath()

// Folded page corner, kept outside the portrait letterform.
context.setFillColor(CGColor(red: 242 / 255, green: 240 / 255, blue: 233 / 255, alpha: 1))
context.move(to: CGPoint(x: 774, y: 64))
context.addLine(to: CGPoint(x: 960, y: 250))
context.addLine(to: CGPoint(x: 960, y: 64))
context.closePath()
context.fillPath()
context.setFillColor(CGColor(red: 200 / 255, green: 1, blue: 61 / 255, alpha: 1))
context.move(to: CGPoint(x: 850, y: 64))
context.addLine(to: CGPoint(x: 960, y: 174))
context.addLine(to: CGPoint(x: 960, y: 64))
context.closePath()
context.fillPath()

context.setStrokeColor(CGColor(gray: 1, alpha: 0.12))
context.setLineWidth(8)
context.addPath(roundedRect(shell.insetBy(dx: 4, dy: 4), radius: 204))
context.strokePath()

guard let result = context.makeImage(),
      let destination = CGImageDestinationCreateWithURL(outputURL as CFURL, UTType.png.identifier as CFString, 1, nil) else {
    fatalError("Unable to prepare output")
}
CGImageDestinationAddImage(destination, result, nil)
guard CGImageDestinationFinalize(destination) else { fatalError("Unable to write output") }
