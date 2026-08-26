import XCTest
@testable import DoubaoBridgeMac

final class MenuBarSnapshotTests: XCTestCase {
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
