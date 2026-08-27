import XCTest
@testable import DoubaoBridgeMac

final class VoiceSessionPerformanceTests: XCTestCase {
    func testStableCommitGateInvalidatesEarlierTextRevisions() {
        var gate = StableTextCommitGate()

        XCTAssertNil(gate.observe("   "))
        let first = gate.observe("一二三")!
        let revised = gate.observe("一二三四五")!

        XCTAssertFalse(gate.isCurrent(first))
        XCTAssertTrue(gate.isCurrent(revised))
        gate.reset()
        XCTAssertFalse(gate.isCurrent(revised))
    }

    func testStopRequiresClientOwnershipOfTheCurrentSession() {
        XCTAssertEqual(
            SessionCommandOwnership.authorizedStopSessionID(
                argument: "42",
                clientSessionID: 42,
                activeSessionID: 42
            ),
            42
        )
        XCTAssertEqual(
            SessionCommandOwnership.authorizedStopSessionID(
                argument: "",
                clientSessionID: 42,
                activeSessionID: 42
            ),
            42
        )
        XCTAssertNil(SessionCommandOwnership.authorizedStopSessionID(
            argument: "42",
            clientSessionID: 42,
            activeSessionID: 43
        ))
        XCTAssertNil(SessionCommandOwnership.authorizedStopSessionID(
            argument: "43",
            clientSessionID: 42,
            activeSessionID: 43
        ))
    }

    func testStableReadyGateRequiresConsecutiveDataPlaneReadySamples() {
        var gate = StableVoiceReadyGate(requiredSamples: 2)

        XCTAssertFalse(gate.observe(doubaoUIActive: true, captureFocused: true, transportReady: false))
        XCTAssertFalse(gate.observe(doubaoUIActive: true, captureFocused: true, transportReady: true))
        XCTAssertTrue(gate.observe(doubaoUIActive: true, captureFocused: true, transportReady: true))
    }

    func testStableReadyGateResetsAfterAnyReadinessRegression() {
        var gate = StableVoiceReadyGate(requiredSamples: 2)

        XCTAssertFalse(gate.observe(doubaoUIActive: true, captureFocused: true, transportReady: true))
        XCTAssertFalse(gate.observe(doubaoUIActive: false, captureFocused: true, transportReady: true))
        XCTAssertFalse(gate.observe(doubaoUIActive: true, captureFocused: true, transportReady: true))
        XCTAssertTrue(gate.observe(doubaoUIActive: true, captureFocused: true, transportReady: true))
    }

    func testCompletionClassifiesSpeechWithoutTextAsEmptyResult() {
        XCTAssertEqual(
            VoiceSessionCompletion.classify(text: "", hadText: false, containsSpeech: true),
            .emptyResult
        )
    }

    func testCompletionClassifiesSilenceWithoutTextAsNoSpeech() {
        XCTAssertEqual(
            VoiceSessionCompletion.classify(text: "", hadText: false, containsSpeech: false),
            .noSpeech
        )
    }

    func testCompletionPreservesFinalText() {
        XCTAssertEqual(
            VoiceSessionCompletion.classify(text: "一二三", hadText: true, containsSpeech: true),
            .final("一二三")
        )
    }

    func testTraceUsesSessionRelativeMonotonicMilliseconds() {
        let trace = VoiceSessionTrace(sessionID: 42, startedNanoseconds: 1_000_000_000)

        XCTAssertEqual(
            trace.event("capture_ready", nowNanoseconds: 1_125_000_000),
            VoiceTraceEvent(sessionID: 42, elapsedMilliseconds: 125, name: "capture_ready")
        )
    }

    func testAudioEvidenceRequiresEnoughSignalToCountAsSpeech() {
        XCTAssertTrue(
            VoiceAudioEvidence(
                bytes: 9_600,
                rmsDBFS: -55,
                maxWindowRMSDBFS: -37,
                peakDBFS: -18
            ).containsSpeech
        )
        XCTAssertFalse(
            VoiceAudioEvidence(
                bytes: 9_600,
                rmsDBFS: -58,
                maxWindowRMSDBFS: -55,
                peakDBFS: -43
            ).containsSpeech
        )
        XCTAssertFalse(
            VoiceAudioEvidence(
                bytes: 400,
                rmsDBFS: -20,
                maxWindowRMSDBFS: -18,
                peakDBFS: -10
            ).containsSpeech
        )
    }
}
