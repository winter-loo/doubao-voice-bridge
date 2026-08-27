import Combine
import XCTest
@testable import DoubaoBridgeMac

@MainActor
final class TranscriptPanelSessionTests: XCTestCase {
    private var observations = Set<AnyCancellable>()

    override func tearDown() {
        observations.removeAll()
        super.tearDown()
    }

    func testEachNewBridgeSessionPublishesANewPanelAfterDiscardingTheOldOne() throws {
        let model = try makeModel()
        var publishedSessionIDs: [Int?] = []
        model.$transcriptPanelSession
            .sink { publishedSessionIDs.append($0?.id) }
            .store(in: &observations)

        model.handleBridgeEvent(status("arming", sessionID: 41))
        model.updateTranscript("上一轮文字", sessionID: 41)
        model.handleBridgeEvent(status("arming", sessionID: 42))

        XCTAssertEqual(publishedSessionIDs, [nil, 41, nil, 42])
        XCTAssertEqual(model.transcriptPanelSession?.id, 42)
        XCTAssertEqual(model.transcriptText, "")
    }

    func testRecoveryArmingWithinSameBridgeSessionKeepsTheCurrentPanel() throws {
        let model = try makeModel()
        var publishedSessionIDs: [Int?] = []
        model.$transcriptPanelSession
            .sink { publishedSessionIDs.append($0?.id) }
            .store(in: &observations)

        model.handleBridgeEvent(status("arming", sessionID: 7))
        model.updateTranscript("保留中的实时文字", sessionID: 7)
        model.handleBridgeEvent(status("arming", sessionID: 7))

        XCTAssertEqual(publishedSessionIDs, [nil, 7])
        XCTAssertEqual(model.transcriptText, "保留中的实时文字")
    }

    func testLateEventsFromPreviousSessionCannotOverwriteCurrentPanel() throws {
        let model = try makeModel()
        model.handleBridgeEvent(status("arming", sessionID: 10))
        model.handleBridgeEvent(status("arming", sessionID: 11))

        model.handleBridgeEvent(["type": "final", "session_id": 10, "text": "过期文字"])
        model.handleBridgeEvent(status("idle", sessionID: 10))

        XCTAssertEqual(model.transcriptPanelSession?.id, 11)
        XCTAssertEqual(model.transcriptText, "")
        XCTAssertEqual(model.phase, .activating)
    }

    func testUnscopedLegacyEventsCannotMutateAnActiveSession() throws {
        let model = try makeModel()
        model.handleBridgeEvent(status("arming", sessionID: 12))

        model.handleBridgeEvent(["type": "final", "text": "没有会话标识的文字"])
        model.handleBridgeEvent(["type": "status", "phase": "idle"])

        XCTAssertEqual(model.transcriptPanelSession?.id, 12)
        XCTAssertEqual(model.transcriptText, "")
        XCTAssertEqual(model.phase, .activating)
    }

    func testReadyGatePhasesRemainActivatingUntilRecording() throws {
        let model = try makeModel()
        model.handleBridgeEvent(status("arming", sessionID: 51))

        model.handleBridgeEvent(status("ui_ready", sessionID: 51))
        XCTAssertEqual(model.phase, .activating)
        XCTAssertEqual(model.statusMessage, "Doubao voice UI is stable")

        model.handleBridgeEvent(status("asr_warmup", sessionID: 51))
        XCTAssertEqual(model.phase, .activating)
        XCTAssertEqual(model.statusMessage, "Verifying the audio data path")

        model.handleBridgeEvent(status("recording", sessionID: 51))
        XCTAssertEqual(model.phase, .listening)
    }

    func testIdleGracePeriodOnlyEndsTheSessionThatScheduledIt() async throws {
        let model = try makeModel(transcriptDismissDelay: 0.01)
        model.handleBridgeEvent(status("arming", sessionID: 20))
        model.handleBridgeEvent(status("idle", sessionID: 20))

        model.handleBridgeEvent(status("arming", sessionID: 21))
        try await Task.sleep(nanoseconds: 50_000_000)

        XCTAssertEqual(model.transcriptPanelSession?.id, 21)
    }

    func testIdleGracePeriodReleasesTheCurrentPanelSession() async throws {
        let model = try makeModel(transcriptDismissDelay: 0.01)
        model.handleBridgeEvent(status("arming", sessionID: 30))
        model.handleBridgeEvent(status("idle", sessionID: 30))

        try await Task.sleep(nanoseconds: 50_000_000)

        XCTAssertNil(model.transcriptPanelSession)
    }

    func testSessionErrorRemainsVisibleThroughIdleUntilGracePeriodEnds() async throws {
        let model = try makeModel(transcriptDismissDelay: 0.01)
        model.handleBridgeEvent(status("arming", sessionID: 31))

        model.handleBridgeEvent([
            "type": "error",
            "phase": "empty_result",
            "message": "No speech result was produced",
            "session_id": 31,
        ])
        model.handleBridgeEvent(status("idle", sessionID: 31))

        XCTAssertEqual(model.phase, .error("No speech result was produced"))
        XCTAssertEqual(model.transcriptPanelSession?.id, 31)

        try await Task.sleep(nanoseconds: 50_000_000)

        XCTAssertNil(model.transcriptPanelSession)
        XCTAssertEqual(model.phase, .error("Setup is incomplete"))
    }

    private func status(_ phase: String, sessionID: Int) -> [String: Any] {
        ["type": "status", "phase": phase, "session_id": sessionID]
    }

    private func makeModel(transcriptDismissDelay: TimeInterval = 2.5) throws -> BridgeAppModel {
        let defaults = UserDefaults(suiteName: #function)!
        defaults.removePersistentDomain(forName: #function)
        return BridgeAppModel(
            preferences: AppPreferences(),
            defaults: defaults,
            audioDeviceManager: CoreAudioDeviceManager(),
            controller: DoubaoController(
                inputSourceID: "invalid.test.input-source",
                hotKey: HotKey(keyCode: 0x3F, flags: [], name: "fn")
            ),
            ffmpegPath: "/usr/bin/true",
            transcriptDismissDelay: transcriptDismissDelay
        )
    }
}
