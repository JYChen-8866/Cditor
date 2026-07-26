import AppKit
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let sourcePath = "/Users/jychen/Downloads/人像转绘本图标.png"
let outputDirectory = URL(fileURLWithPath: "assets/branding", isDirectory: true)

guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: sourcePath) as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    fatalError("Unable to load portrait at \(sourcePath)")
}

func roundedRect(_ rect: CGRect, radius: CGFloat) -> CGPath {
    CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
}

func renderIcon(size: Int) -> CGImage {
    let scale = CGFloat(size) / 1024
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let bitmapInfo = CGImageAlphaInfo.premultipliedLast.rawValue
    guard let context = CGContext(
        data: nil,
        width: size,
        height: size,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: colorSpace,
        bitmapInfo: bitmapInfo
    ) else { fatalError("Unable to create bitmap context") }

    context.scaleBy(x: scale, y: scale)
    context.setShouldAntialias(true)
    context.setAllowsAntialiasing(true)

    let shell = CGRect(x: 64, y: 64, width: 896, height: 896)
    context.saveGState()
    context.addPath(roundedRect(shell, radius: 208))
    context.clip()

    context.setFillColor(CGColor(red: 25 / 255, green: 26 / 255, blue: 24 / 255, alpha: 1))
    context.fill(shell)

    // Crop close around the face and shoulders while preserving the original likeness.
    let crop = CGRect(x: 255, y: 220, width: 1538, height: 1538)
    guard let portrait = image.cropping(to: crop) else { fatalError("Unable to crop portrait") }
    let portraitRect = CGRect(x: 84, y: 84, width: 856, height: 856)
    context.draw(portrait, in: portraitRect)

    // A quiet lower vignette gives the badge enough contrast without covering the portrait.
    let colors = [
        CGColor(red: 25 / 255, green: 26 / 255, blue: 24 / 255, alpha: 0),
        CGColor(red: 25 / 255, green: 26 / 255, blue: 24 / 255, alpha: 0.82),
    ] as CFArray
    let locations: [CGFloat] = [0, 1]
    if let gradient = CGGradient(colorsSpace: colorSpace, colors: colors, locations: locations) {
        context.drawLinearGradient(
            gradient,
            start: CGPoint(x: 512, y: 430),
            end: CGPoint(x: 512, y: 72),
            options: []
        )
    }

    // Editing cursor: the acid green is the existing Cditor brand accent.
    context.setFillColor(CGColor(red: 200 / 255, green: 1, blue: 61 / 255, alpha: 1))
    context.addPath(roundedRect(CGRect(x: 850, y: 286, width: 24, height: 278), radius: 12))
    context.fillPath()
    context.fillEllipse(in: CGRect(x: 850, y: 250, width: 24, height: 24))

    // Small editorial seal derived from the website's circular C mark.
    context.setFillColor(CGColor(red: 25 / 255, green: 26 / 255, blue: 24 / 255, alpha: 0.96))
    context.fillEllipse(in: CGRect(x: 132, y: 124, width: 178, height: 178))
    context.setStrokeColor(CGColor(red: 200 / 255, green: 1, blue: 61 / 255, alpha: 1))
    context.setLineWidth(8)
    context.strokeEllipse(in: CGRect(x: 132, y: 124, width: 178, height: 178))

    let font = NSFont(name: "Georgia-Italic", size: 120) ?? NSFont.systemFont(ofSize: 120, weight: .medium)
    let attributes: [NSAttributedString.Key: Any] = [
        .font: font,
        .foregroundColor: NSColor(red: 200 / 255, green: 1, blue: 61 / 255, alpha: 1),
    ]
    let mark = NSAttributedString(string: "C", attributes: attributes)
    let markSize = mark.size()
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
    mark.draw(at: CGPoint(x: 221 - markSize.width / 2, y: 207 - markSize.height / 2 - 3))
    NSGraphicsContext.restoreGraphicsState()

    context.restoreGState()

    context.setStrokeColor(CGColor(gray: 1, alpha: 0.14))
    context.setLineWidth(8)
    context.addPath(roundedRect(shell.insetBy(dx: 4, dy: 4), radius: 204))
    context.strokePath()

    guard let result = context.makeImage() else { fatalError("Unable to render icon") }
    return result
}

func writePNG(_ image: CGImage, to url: URL) {
    guard let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
        fatalError("Unable to create PNG destination")
    }
    CGImageDestinationAddImage(destination, image, nil)
    guard CGImageDestinationFinalize(destination) else { fatalError("Unable to write \(url.path)") }
}

for size in [16, 32, 64, 128, 256, 512, 1024] {
    let name = size == 1024 ? "cditor-portrait-app-icon.png" : "cditor-portrait-app-icon-\(size).png"
    writePNG(renderIcon(size: size), to: outputDirectory.appendingPathComponent(name))
}
