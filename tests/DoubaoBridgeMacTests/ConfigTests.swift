import XCTest
@testable import DoubaoBridgeMac

final class ConfigTests: XCTestCase {
    func testExplicitAudioDeviceIndexOverridesPreferredDeviceName() throws {
        var preferences = AppPreferences()
        preferences.virtualAudioDevice = "Preferred Virtual Device"

        let config = try Config.parse(
            arguments: ["--audio-device-index", "7"],
            preferences: preferences
        )

        XCTAssertEqual(config.audioDeviceIndex, 7)
        XCTAssertNil(config.audioDeviceName)
    }

    func testPerformanceDefaultsUseStablePollingAndBoundedWarmup() throws {
        let config = try Config.parse(arguments: [], preferences: AppPreferences())

        XCTAssertEqual(config.voiceActivationCheckDelay, 0.05)
        XCTAssertEqual(config.voiceActivationAttemptTimeout, 1.0)
        XCTAssertEqual(config.voiceActivationStableSamples, 2)
        XCTAssertEqual(config.asrWarmupDelay, 0.3)
        XCTAssertEqual(config.finalDelay, 1.8)
    }

    func testPerformanceTimingOverridesAreParsed() throws {
        let config = try Config.parse(
            arguments: [
                "--voice-activation-attempt-timeout", "0.8",
                "--voice-activation-stable-samples", "3",
                "--asr-warmup-delay", "0.25",
            ],
            preferences: AppPreferences()
        )

        XCTAssertEqual(config.voiceActivationAttemptTimeout, 0.8)
        XCTAssertEqual(config.voiceActivationStableSamples, 3)
        XCTAssertEqual(config.asrWarmupDelay, 0.25)
    }
}
