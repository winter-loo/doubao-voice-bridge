import AppKit
import XCTest
@testable import DoubaoBridgeMac

final class CaptureWindowPresentationTests: XCTestCase {
    func testInternalCaptureTargetHasNoVisibleWindowChrome() {
        let frame = CaptureWindowPresentation.frame(showDeveloperWindow: false)

        XCTAssertEqual(
            CaptureWindowPresentation.styleMask(showDeveloperWindow: false),
            [.borderless]
        )
        XCTAssertEqual(CaptureWindowPresentation.alphaValue(showDeveloperWindow: false), 0)
        XCTAssertEqual(frame.origin, CaptureWindowPresentation.hiddenOrigin)
    }

    func testExplicitDeveloperModeRetainsInspectableCaptureWindow() {
        let style = CaptureWindowPresentation.styleMask(showDeveloperWindow: true)

        XCTAssertTrue(style.contains(.titled))
        XCTAssertTrue(style.contains(.closable))
        XCTAssertEqual(CaptureWindowPresentation.alphaValue(showDeveloperWindow: true), 1)
    }

    @MainActor
    func testCaptureHostAcceptsInputWithoutBecomingMainWindow() {
        let window = VoiceCaptureHostWindow(
            contentRect: .zero,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        XCTAssertTrue(window.canBecomeKey)
        XCTAssertFalse(window.canBecomeMain)
    }
}
