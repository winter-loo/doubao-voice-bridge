import AppKit
import Foundation

enum BrandPalette {
    static let coreBlue = NSColor(
        srgbRed: 24.0 / 255.0,
        green: 121.0 / 255.0,
        blue: 255.0 / 255.0,
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
