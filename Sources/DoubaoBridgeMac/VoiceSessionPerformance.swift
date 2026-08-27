import Foundation

struct StableVoiceReadyGate {
    let requiredSamples: Int
    private(set) var consecutiveReadySamples = 0

    init(requiredSamples: Int) {
        self.requiredSamples = max(1, requiredSamples)
    }

    mutating func observe(
        doubaoUIActive: Bool,
        captureFocused: Bool,
        transportReady: Bool
    ) -> Bool {
        guard doubaoUIActive, captureFocused, transportReady else {
            consecutiveReadySamples = 0
            return false
        }
        consecutiveReadySamples += 1
        return consecutiveReadySamples >= requiredSamples
    }

    mutating func reset() {
        consecutiveReadySamples = 0
    }
}

enum VoiceSessionCompletion: Equatable {
    case final(String)
    case emptyResult
    case noSpeech

    static func classify(text: String, hadText: Bool, containsSpeech: Bool) -> Self {
        let finalText = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !finalText.isEmpty {
            return .final(finalText)
        }
        return hadText || containsSpeech ? .emptyResult : .noSpeech
    }
}

struct VoiceTraceEvent: Equatable {
    let sessionID: Int
    let elapsedMilliseconds: Int
    let name: String

    var dictionary: [String: Any] {
        [
            "type": "trace",
            "session_id": sessionID,
            "elapsed_ms": elapsedMilliseconds,
            "event": name,
        ]
    }
}

struct VoiceSessionTrace {
    let sessionID: Int
    let startedNanoseconds: UInt64

    init(
        sessionID: Int,
        startedNanoseconds: UInt64 = DispatchTime.now().uptimeNanoseconds
    ) {
        self.sessionID = sessionID
        self.startedNanoseconds = startedNanoseconds
    }

    func event(
        _ name: String,
        nowNanoseconds: UInt64 = DispatchTime.now().uptimeNanoseconds
    ) -> VoiceTraceEvent {
        let elapsed = nowNanoseconds >= startedNanoseconds
            ? nowNanoseconds - startedNanoseconds
            : 0
        return VoiceTraceEvent(
            sessionID: sessionID,
            elapsedMilliseconds: Int(elapsed / 1_000_000),
            name: name
        )
    }
}

struct StableTextCommitGate {
    private(set) var revision = 0

    mutating func observe(_ text: String) -> Int? {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }
        revision += 1
        return revision
    }

    func isCurrent(_ candidate: Int) -> Bool {
        candidate == revision
    }

    mutating func reset() {
        revision = 0
    }
}

enum SessionCommandOwnership {
    static func authorizedStopSessionID(
        argument: String,
        clientSessionID: Int?,
        activeSessionID: Int?
    ) -> Int? {
        let requestedSessionID: Int?
        if argument.isEmpty {
            requestedSessionID = clientSessionID
        } else {
            requestedSessionID = Int(argument)
        }
        guard let requestedSessionID,
              requestedSessionID == clientSessionID,
              requestedSessionID == activeSessionID
        else {
            return nil
        }
        return requestedSessionID
    }
}

struct VoiceAudioEvidence: Equatable {
    let bytes: UInt64
    let rmsDBFS: Double
    let maxWindowRMSDBFS: Double
    let peakDBFS: Double

    var containsSpeech: Bool {
        bytes >= 1_920 && maxWindowRMSDBFS >= -48 && peakDBFS >= -36
    }
}
