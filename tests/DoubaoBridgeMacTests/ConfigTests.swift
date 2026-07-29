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
}
