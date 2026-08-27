import XCTest
@testable import DoubaoBridgeMac

final class MenuBarSnapshotTests: XCTestCase {
    func testCoreBlueMatchesCanonicalBrandToken() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let data = try Data(
            contentsOf: repositoryRoot
                .appendingPathComponent("branding/voice-t/brand-colors.json")
        )
        let document = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data) as? [String: Any]
        )
        let colors = try XCTUnwrap(document["colors"] as? [String: Any])
        let coreBlue = try XCTUnwrap(colors["core_blue"] as? [String: Any])
        let expectedRGB = try XCTUnwrap(coreBlue["rgb"] as? [Int])

        let actual = BrandPalette.coreBlue.usingColorSpace(.sRGB)
        XCTAssertEqual(
            [actual?.redComponent, actual?.greenComponent, actual?.blueComponent]
                .compactMap { $0 }
                .map { Int(($0 * 255).rounded()) },
            expectedRGB
        )
    }

    func testBrandMenuBarImageLoadsAsTwentyPointTemplate() {
        let image = BrandAssets.menuBarTemplateImage()

        XCTAssertNotNil(image)
        XCTAssertEqual(image?.size, NSSize(width: 20, height: 20))
        XCTAssertEqual(image?.isTemplate, true)
        XCTAssertEqual(
            Set(image?.representations.map(\.pixelsWide) ?? []),
            Set([20, 40])
        )
    }

    func testBrandLogoLoadsForSetupAssistant() {
        XCTAssertNotNil(BrandAssets.logoMarkImage())
    }

    func testPublishedReadySnapshotDoesNotRetainOptimizingPresentation() {
        let optimizing = MenuBarSnapshot(
            phase: .optimizing,
            connectedClientCount: 1,
            launchAtLogin: false
        )
        let ready = MenuBarSnapshot(
            phase: .ready,
            connectedClientCount: 1,
            launchAtLogin: false
        )

        XCTAssertEqual(optimizing.phase.symbolName, "text.bubble")
        XCTAssertEqual(ready.phase.symbolName, "waveform.circle")
        XCTAssertEqual(ready.phase.title, "Ready")
        XCTAssertEqual(ready.connectionSummary, "1 Windows client connected")
    }

    func testSnapshotFormatsMultipleConnectedClients() {
        let snapshot = MenuBarSnapshot(
            phase: .ready,
            connectedClientCount: 3,
            launchAtLogin: true
        )

        XCTAssertEqual(snapshot.connectionSummary, "3 Windows clients connected")
        XCTAssertTrue(snapshot.launchAtLogin)
    }
}
