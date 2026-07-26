import AppKit
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let sourceURL = URL(fileURLWithPath: "/Users/jychen/Downloads/人像转绘本图标.png")
let outputURL = URL(fileURLWithPath: "assets/branding/cditor-effie-portrait-icon.png")

guard let source = CGImageSourceCreateWithURL(sourceURL as CFURL, nil),
      let portrait = CGImageSourceCreateImageAtIndex(source, 0, nil),
      let rawCrop = portrait.cropping(to: CGRect(x: 360, y: 430, width: 1320, height: 1320)) else {
    fatalError("Unable to load or crop portrait")
}

func recolorPortrait(_ image: CGImage) -> CGImage {
    let width = image.width
    let height = image.height
    let bytesPerRow = width * 4
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    guard let buffer = calloc(height, bytesPerRow),
          let bitmap = CGContext(
            data: buffer,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: bytesPerRow,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
          ) else { fatalError("Unable to create recolor buffer") }
    defer { free(buffer) }

    bitmap.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    let pixels = buffer.bindMemory(to: UInt8.self, capacity: height * bytesPerRow)

    func isPaper(_ pixel: Int) -> Bool {
        let offset = pixel * 4
        let red = Int(pixels[offset])
        let green = Int(pixels[offset + 1])
        let blue = Int(pixels[offset + 2])
        return red > 195 && green > 184 && blue > 158 && red - green < 24 && green - blue < 38
    }

    var visited = [Bool](repeating: false, count: width * height)
    var queue: [Int] = []
    queue.reserveCapacity(width * height / 2)

    func enqueue(_ pixel: Int) {
        guard !visited[pixel], isPaper(pixel) else { return }
        visited[pixel] = true
        queue.append(pixel)
    }

    for x in 0..<width {
        enqueue(x)
        enqueue((height - 1) * width + x)
    }
    for y in 0..<height {
        enqueue(y * width)
        enqueue(y * width + width - 1)
    }

    var cursor = 0
    while cursor < queue.count {
        let pixel = queue[cursor]
        cursor += 1
        let x = pixel % width
        let y = pixel / width
        let offset = pixel * 4
        pixels[offset] = 255
        pixels[offset + 1] = 255
        pixels[offset + 2] = 255

        if x > 0 { enqueue(pixel - 1) }
        if x + 1 < width { enqueue(pixel + 1) }
        if y > 0 { enqueue(pixel - width) }
        if y + 1 < height { enqueue(pixel + width) }
    }

    func isWarmPaper(_ pixel: Int) -> Bool {
        let offset = pixel * 4
        let red = Int(pixels[offset])
        let green = Int(pixels[offset + 1])
        let blue = Int(pixels[offset + 2])
        return red > 180 && green > 155 && blue > 115 && red - green < 58 && green - blue < 68
    }

    var warmVisited = [Bool](repeating: false, count: width * height)
    var warmQueue: [Int] = []
    func warmEnqueue(_ pixel: Int) {
        guard !warmVisited[pixel], isWarmPaper(pixel) else { return }
        warmVisited[pixel] = true
        warmQueue.append(pixel)
    }
    func warmSeed(outputX: CGFloat, outputY: CGFloat) {
        let sourceX = Int((outputX - 64) / 896 * CGFloat(width))
        let sourceY = Int((1 - (outputY - 64) / 896) * CGFloat(height))
        guard sourceX >= 0, sourceX < width, sourceY >= 0, sourceY < height else { return }
        warmEnqueue(sourceY * width + sourceX)
    }

    warmSeed(outputX: 302, outputY: 495)
    warmSeed(outputX: 620, outputY: 340)

    var warmCursor = 0
    while warmCursor < warmQueue.count {
        let pixel = warmQueue[warmCursor]
        warmCursor += 1
        let x = pixel % width
        let y = pixel / width
        let offset = pixel * 4
        pixels[offset] = 255
        pixels[offset + 1] = 255
        pixels[offset + 2] = 255
        if x > 0 { warmEnqueue(pixel - 1) }
        if x + 1 < width { warmEnqueue(pixel + 1) }
        if y > 0 { warmEnqueue(pixel - width) }
        if y + 1 < height { warmEnqueue(pixel + width) }
    }

    guard let result = bitmap.makeImage() else { fatalError("Unable to recolor portrait") }
    return result
}

let crop = recolorPortrait(rawCrop)

let size = 1024
let colorSpace = CGColorSpaceCreateDeviceRGB()
guard let context = CGContext(
    data: nil,
    width: size,
    height: size,
    bitsPerComponent: 8,
    bytesPerRow: 0,
    space: colorSpace,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { fatalError("Unable to create bitmap context") }

func roundedRect(_ rect: CGRect, radius: CGFloat) -> CGPath {
    CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
}

let shell = CGRect(x: 64, y: 64, width: 896, height: 896)

context.saveGState()
context.setShadow(offset: CGSize(width: 0, height: -24), blur: 34, color: CGColor(gray: 0, alpha: 0.25))
context.setFillColor(CGColor(red: 242 / 255, green: 240 / 255, blue: 233 / 255, alpha: 1))
context.addPath(roundedRect(shell, radius: 208))
context.fillPath()
context.restoreGState()

context.saveGState()
context.addPath(roundedRect(shell, radius: 208))
context.clip()
context.draw(crop, in: shell)

// Preserve the original warm illustration; only ground the lower edge slightly.
let colors = [
    CGColor(red: 25 / 255, green: 26 / 255, blue: 24 / 255, alpha: 0),
    CGColor(red: 25 / 255, green: 26 / 255, blue: 24 / 255, alpha: 0.12),
] as CFArray
let locations: [CGFloat] = [0, 1]
if let gradient = CGGradient(colorsSpace: colorSpace, colors: colors, locations: locations) {
    context.drawLinearGradient(
        gradient,
        start: CGPoint(x: 512, y: 390),
        end: CGPoint(x: 512, y: 64),
        options: []
    )
}
context.restoreGState()

context.setStrokeColor(CGColor(red: 25 / 255, green: 26 / 255, blue: 24 / 255, alpha: 0.32))
context.setLineWidth(10)
context.addPath(roundedRect(shell.insetBy(dx: 5, dy: 5), radius: 203))
context.strokePath()

guard let result = context.makeImage(),
      let destination = CGImageDestinationCreateWithURL(outputURL as CFURL, UTType.png.identifier as CFString, 1, nil) else {
    fatalError("Unable to prepare output")
}
CGImageDestinationAddImage(destination, result, nil)
guard CGImageDestinationFinalize(destination) else { fatalError("Unable to write output") }
