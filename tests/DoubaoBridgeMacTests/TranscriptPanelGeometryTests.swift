import AppKit
import XCTest
@testable import DoubaoBridgeMac

final class TranscriptPanelGeometryTests: XCTestCase {
    func testPanelCentersBelowStatusItemWhenThereIsRoom() {
        let anchor = NSRect(x: 590, y: 875, width: 24, height: 24)
        let visibleFrame = NSRect(x: 0, y: 0, width: 1200, height: 900)
        let size = TranscriptPanelLayout.expanded.size

        let frame = TranscriptPanelGeometry.frame(anchor: anchor, visibleFrame: visibleFrame, size: size)

        XCTAssertEqual(frame.midX, anchor.midX, accuracy: 0.001)
        XCTAssertEqual(frame.maxY, anchor.minY - TranscriptPanelGeometry.anchorGap, accuracy: 0.001)
    }

    func testPanelStaysOnScreenNearRightEdge() {
        let anchor = NSRect(x: 1170, y: 875, width: 24, height: 24)
        let visibleFrame = NSRect(x: 0, y: 0, width: 1200, height: 900)
        let size = TranscriptPanelLayout.expanded.size

        let frame = TranscriptPanelGeometry.frame(anchor: anchor, visibleFrame: visibleFrame, size: size)

        XCTAssertEqual(frame.maxX, visibleFrame.maxX - TranscriptPanelGeometry.edgeInset, accuracy: 0.001)
    }

    func testLayoutExpandsOnlyAfterTranscriptAppears() {
        XCTAssertEqual(
            TranscriptPanelLayout.resolve(transcriptText: ""),
            .compact
        )
        XCTAssertEqual(
            TranscriptPanelLayout.resolve(transcriptText: "识别结果"),
            .expanded
        )
    }

    func testLayoutMetricsMatchTheApprovedPrototypeParameters() {
        XCTAssertEqual(TranscriptPanelStyle.compact.size, NSSize(width: 180, height: 38))
        XCTAssertEqual(TranscriptPanelStyle.compact.cornerRadius, 36)
        XCTAssertEqual(TranscriptPanelStyle.compact.horizontalPadding, 14)
        XCTAssertEqual(TranscriptPanelStyle.compact.verticalPadding, 10)
        XCTAssertEqual(TranscriptPanelStyle.compact.statusOpacity, 0.92)

        XCTAssertEqual(TranscriptPanelStyle.expanded.size, NSSize(width: 420, height: 104))
        XCTAssertEqual(TranscriptPanelStyle.expanded.cornerRadius, 24)
        XCTAssertEqual(TranscriptPanelStyle.expanded.horizontalPadding, 16)
        XCTAssertEqual(TranscriptPanelStyle.expanded.verticalPadding, 8)
        XCTAssertEqual(TranscriptPanelStyle.expanded.statusOpacity, 0.62)

        XCTAssertEqual(TranscriptPanelStyle.anchorGap, 0)
        XCTAssertEqual(TranscriptPanelStyle.iconSize, 26)
        XCTAssertEqual(TranscriptPanelStyle.contentGap, 10)
        XCTAssertEqual(TranscriptPanelStyle.statusFontSize, 12)
        XCTAssertEqual(TranscriptPanelStyle.transcriptFontSize, 16)
        XCTAssertEqual(TranscriptPanelStyle.surfaceOpacity, 0.97)
        XCTAssertEqual(TranscriptPanelStyle.outlineOpacity, 0.10)

        XCTAssertLessThan(
            TranscriptPanelLayout.compact.size.height,
            TranscriptPanelLayout.expanded.size.height
        )
    }

    func testExpandedLayoutKeepsTheSameTopAnchor() {
        let anchor = NSRect(x: 590, y: 875, width: 24, height: 24)
        let visibleFrame = NSRect(x: 0, y: 0, width: 1200, height: 900)
        let compact = TranscriptPanelGeometry.frame(
            anchor: anchor,
            visibleFrame: visibleFrame,
            size: TranscriptPanelLayout.compact.size
        )
        let expanded = TranscriptPanelGeometry.frame(
            anchor: anchor,
            visibleFrame: visibleFrame,
            size: TranscriptPanelLayout.expanded.size
        )

        XCTAssertEqual(compact.maxY, expanded.maxY, accuracy: 0.001)
    }
}
