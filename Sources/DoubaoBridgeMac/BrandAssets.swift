import AppKit
import Foundation

enum BrandPalette {
    static let coreBlueRGB = (red: UInt8(24), green: UInt8(121), blue: UInt8(255))
    static let coreBlue = NSColor(
        srgbRed: CGFloat(coreBlueRGB.red) / 255.0,
        green: CGFloat(coreBlueRGB.green) / 255.0,
        blue: CGFloat(coreBlueRGB.blue) / 255.0,
        alpha: 1
    )
}

enum BrandAssets {
    private static let menuBarPointSize = NSSize(width: 20, height: 20)

    static func menuBarTemplateImage() -> NSImage? {
        guard let standardURL = resourceURL(
            named: "logo-template-20",
            subdirectory: "Brand/MenuBar"
        ),
              let retinaURL = resourceURL(
                  named: "logo-template-20@2x",
                  subdirectory: "Brand/MenuBar"
              ),
              let standardData = try? Data(contentsOf: standardURL),
              let retinaData = try? Data(contentsOf: retinaURL),
              let standard = NSBitmapImageRep(data: standardData),
              let retina = NSBitmapImageRep(data: retinaData)
        else {
            return nil
        }

        standard.size = menuBarPointSize
        retina.size = menuBarPointSize
        let image = NSImage(size: menuBarPointSize)
        image.addRepresentation(standard)
        image.addRepresentation(retina)
        image.isTemplate = true
        return image
    }

    static func menuBarActiveImage() -> NSImage? {
        guard let template = menuBarTemplateImage() else {
            return nil
        }

        let image = NSImage(size: menuBarPointSize)
        for case let source as NSBitmapImageRep in template.representations {
            guard let tinted = NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: source.pixelsWide,
                pixelsHigh: source.pixelsHigh,
                bitsPerSample: 8,
                samplesPerPixel: 4,
                hasAlpha: true,
                isPlanar: false,
                colorSpaceName: .deviceRGB,
                bitmapFormat: [.alphaNonpremultiplied, .thirtyTwoBitBigEndian],
                bytesPerRow: source.pixelsWide * 4,
                bitsPerPixel: 32
            ), let bitmapData = tinted.bitmapData else {
                continue
            }

            tinted.size = menuBarPointSize
            let alphaIndex = source.bitmapFormat.contains(.alphaFirst)
                ? 0
                : source.samplesPerPixel - 1
            var sourceSamples = [Int](repeating: 0, count: source.samplesPerPixel)
            for y in 0..<source.pixelsHigh {
                for x in 0..<source.pixelsWide {
                    source.getPixel(&sourceSamples, atX: x, y: y)
                    let alpha = source.hasAlpha ? sourceSamples[alphaIndex] : 255
                    guard alpha > 0 else {
                        continue
                    }
                    let offset = y * tinted.bytesPerRow + x * 4
                    bitmapData[offset] = BrandPalette.coreBlueRGB.red
                    bitmapData[offset + 1] = BrandPalette.coreBlueRGB.green
                    bitmapData[offset + 2] = BrandPalette.coreBlueRGB.blue
                    bitmapData[offset + 3] = UInt8(clamping: alpha)
                }
            }
            image.addRepresentation(tinted)
        }
        guard !image.representations.isEmpty else {
            return nil
        }
        image.isTemplate = false
        return image
    }

    static func logoMarkImage() -> NSImage? {
        image(named: "logo-mark-1024", subdirectory: "Brand")
    }

    private static func image(named name: String, subdirectory: String) -> NSImage? {
        guard let url = resourceURL(named: name, subdirectory: subdirectory) else {
            return nil
        }
        return NSImage(contentsOf: url)
    }

    private static func resourceURL(named name: String, subdirectory: String) -> URL? {
        Bundle.main.url(
            forResource: name,
            withExtension: "png",
            subdirectory: subdirectory
        ) ?? Bundle.module.url(
            forResource: name,
            withExtension: "png",
            subdirectory: subdirectory
        )
    }
}
