#!/usr/bin/env swift
//
// Draws Sparagne's macOS app icon and writes every size the
// AppIcon.appiconset needs, plus its Contents.json.
//
// Usage:
//   swift scripts/make-icon.swift <output-dir>
//
// <output-dir> is normally
// apple/Sparagne/Sparagne/Resources/Assets.xcassets/AppIcon.appiconset. The
// script draws a 1024x1024 idea of the icon — the macOS rounded-square
// shape, the palette's dark background and a simple ledger glyph (a ruled
// margin bar and a few row lines, the bottom one — the "total" — picked out
// in the accent color) — then renders each of the ten slots fresh at its own
// pixel size (not resampled from the master), so edges stay crisp down to
// 16x16.
//
// Colors are copied from `Support/Palette.swift`'s `Ink` enum: keep the two
// in sync if the palette changes. The geometry below (in a 1024x1024 canvas)
// is the whole design; change it and rerun to replace every PNG and
// Contents.json.
//
// AppKit/CoreGraphics only, no external dependencies (run with `swift`,
// Xcode's command-line tool).

import CoreGraphics
import Foundation
import ImageIO

// MARK: - Palette (copied from Support/Palette.swift's `Ink`)

private enum Palette {
    static let bg = CGColor(red: 0x0D / 255, green: 0x0D / 255, blue: 0x0D / 255, alpha: 1)
    static let dim = CGColor(red: 0x7A / 255, green: 0x7A / 255, blue: 0x7A / 255, alpha: 1)
    static let accent = CGColor(red: 0xFF / 255, green: 0x5A / 255, blue: 0x3C / 255, alpha: 1)
}

// MARK: - The ten slots Xcode's AppIcon.appiconset expects

private struct IconSlot {
    /// The point size (16, 32, 128, 256 or 512).
    let size: Int
    let scale: Int

    var pixels: Int { size * scale }
    var filename: String { "icon_\(size)x\(size)\(scale == 2 ? "@2x" : "").png" }
}

private let slots: [IconSlot] = [
    IconSlot(size: 16, scale: 1), IconSlot(size: 16, scale: 2),
    IconSlot(size: 32, scale: 1), IconSlot(size: 32, scale: 2),
    IconSlot(size: 128, scale: 1), IconSlot(size: 128, scale: 2),
    IconSlot(size: 256, scale: 1), IconSlot(size: 256, scale: 2),
    IconSlot(size: 512, scale: 1), IconSlot(size: 512, scale: 2),
]

// MARK: - Drawing

/// Renders the icon at `pixels`×`pixels`, drawn fresh (not scaled from a
/// master) so every size stays crisp. All geometry below is expressed in a
/// notional 1024×1024 canvas and scaled uniformly to `pixels`.
private func drawIcon(pixels: Int) -> CGImage {
    let canvas: CGFloat = 1024
    let scale = CGFloat(pixels) / canvas

    guard
        let context = CGContext(
            data: nil,
            width: pixels,
            height: pixels,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
    else {
        fatalError("could not create a \(pixels)x\(pixels) graphics context")
    }

    /// A rect given in the 1024-canvas coordinate space, scaled to `pixels`.
    func rect(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat) -> CGRect {
        CGRect(x: x, y: y, width: w, height: h).applying(CGAffineTransform(scaleX: scale, y: scale))
    }

    /// A capsule: fully rounded on its short side, whichever that is, so a
    /// tall bar gets rounded caps rather than the lens shape a fixed corner
    /// radius would give it.
    func pill(_ r: CGRect) -> CGPath {
        let radius = min(r.width, r.height) / 2
        return CGPath(roundedRect: r, cornerWidth: radius, cornerHeight: radius, transform: nil)
    }

    // The macOS icon shape: ~824x824 centred in the 1024 canvas, ~185pt
    // corner radius. Transparent outside it; the palette's background fills
    // it, and everything drawn after is clipped to it.
    let shapeRect = rect(100, 100, 824, 824)
    let shapePath = CGPath(
        roundedRect: shapeRect,
        cornerWidth: 185 * scale,
        cornerHeight: 185 * scale,
        transform: nil
    )
    context.addPath(shapePath)
    context.setFillColor(Palette.bg)
    context.fillPath()

    context.addPath(shapePath)
    context.clip()

    // The ledger glyph: a ruled margin bar on the left, a few row lines to
    // its right of uneven length (like written rows), the bottom one — the
    // "total" — picked out in the accent color.
    let barRect = rect(232, 260, 46, 504)
    context.addPath(pill(barRect))
    context.setFillColor(Palette.accent)
    context.fillPath()

    let rowCount = 4
    let rowHeight: CGFloat = 46
    let rowsTopY: CGFloat = 718
    let rowsBottomY: CGFloat = 260
    let rowLeftX: CGFloat = 342
    let rowFullRightX: CGFloat = 792
    let rowShortRightX: CGFloat = 652

    for index in 0..<rowCount {
        let t = CGFloat(index) / CGFloat(rowCount - 1)
        let centerY = rowsTopY - t * (rowsTopY - rowsBottomY)
        let isTotal = index == rowCount - 1
        let rightX = isTotal ? rowFullRightX : rowShortRightX
        let rowRect = rect(rowLeftX, centerY - rowHeight / 2, rightX - rowLeftX, rowHeight)
        context.addPath(pill(rowRect))
        context.setFillColor(isTotal ? Palette.accent : Palette.dim)
        context.fillPath()
    }

    guard let image = context.makeImage() else {
        fatalError("could not render the \(pixels)x\(pixels) icon")
    }
    return image
}

private func writePNG(_ image: CGImage, to url: URL) {
    guard let destination = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil) else {
        fatalError("could not create a PNG destination at \(url.path)")
    }
    CGImageDestinationAddImage(destination, image, nil)
    guard CGImageDestinationFinalize(destination) else {
        fatalError("could not write \(url.path)")
    }
}

// MARK: - Contents.json

/// Built by hand (not `JSONSerialization`) to match Xcode's own formatting:
/// two-space indent, a space before each colon, keys in alphabetical order.
private func contentsJSON(for slots: [IconSlot]) -> String {
    let images = slots.map { slot in
        """
          {
            "filename" : "\(slot.filename)",
            "idiom" : "mac",
            "scale" : "\(slot.scale)x",
            "size" : "\(slot.size)x\(slot.size)"
          }
        """
    }.joined(separator: ",\n")

    return """
    {
      "images" : [
    \(images)
      ],
      "info" : {
        "author" : "xcode",
        "version" : 1
      }
    }

    """
}

// MARK: - Main

let arguments = CommandLine.arguments
guard arguments.count == 2 else {
    FileHandle.standardError.write("usage: swift scripts/make-icon.swift <output-dir>\n".data(using: .utf8)!)
    exit(1)
}

let outputDir = URL(fileURLWithPath: arguments[1], isDirectory: true)
try? FileManager.default.createDirectory(at: outputDir, withIntermediateDirectories: true)

for slot in slots {
    let image = drawIcon(pixels: slot.pixels)
    writePNG(image, to: outputDir.appendingPathComponent(slot.filename))
    print("wrote \(slot.filename) (\(slot.pixels)x\(slot.pixels))")
}

let contentsURL = outputDir.appendingPathComponent("Contents.json")
try contentsJSON(for: slots).write(to: contentsURL, atomically: true, encoding: .utf8)
print("wrote \(contentsURL.lastPathComponent)")
