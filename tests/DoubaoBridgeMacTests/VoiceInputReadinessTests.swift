import XCTest
@testable import DoubaoBridgeMac

final class VoiceInputReadinessTests: XCTestCase {
    func testRecordingRequiresDoubaoUIAndCaptureFocus() {
        XCTAssertTrue(
            VoiceInputReadiness.isReady(
                doubaoUIActive: true,
                captureFocused: true
            )
        )
        XCTAssertFalse(
            VoiceInputReadiness.isReady(
                doubaoUIActive: true,
                captureFocused: false
            )
        )
        XCTAssertFalse(
            VoiceInputReadiness.isReady(
                doubaoUIActive: false,
                captureFocused: true
            )
        )
    }

    func testActiveRecordingRecoversWhenCaptureFocusIsLost() {
        XCTAssertTrue(
            VoiceInputReadiness.needsRecovery(
                shortcutActive: true,
                captureFocused: false
            )
        )
        XCTAssertFalse(
            VoiceInputReadiness.needsRecovery(
                shortcutActive: true,
                captureFocused: true
            )
        )
        XCTAssertFalse(
            VoiceInputReadiness.needsRecovery(
                shortcutActive: false,
                captureFocused: false
            )
        )
    }
}
