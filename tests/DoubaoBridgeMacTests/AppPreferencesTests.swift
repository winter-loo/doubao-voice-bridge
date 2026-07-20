import XCTest
@testable import DoubaoBridgeMac

final class AppPreferencesTests: XCTestCase {
    func testDefaultsMatchConsumerAppConfiguration() {
        let suiteName = "AppPreferencesTests.defaults.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        let preferences = AppPreferences(defaults: defaults)

        XCTAssertEqual(preferences.controlPort, 4387)
        XCTAssertEqual(preferences.audioPort, 5004)
        XCTAssertEqual(preferences.virtualAudioDevice, "BlackHole 2ch")
        XCTAssertTrue(preferences.restoreDefaultInput)
        XCTAssertFalse(preferences.showCaptureWindow)
    }

    func testPreferencesRoundTrip() {
        let suiteName = "AppPreferencesTests.roundTrip.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }

        var expected = AppPreferences()
        expected.controlPort = 9000
        expected.audioPort = 9001
        expected.virtualAudioDevice = "Virtual Test Device"
        expected.restoreDefaultInput = false
        expected.showCaptureWindow = true
        expected.launchAtLogin = true
        expected.setupCompleted = true
        expected.save(to: defaults)

        XCTAssertEqual(AppPreferences(defaults: defaults), expected)
    }

    func testLegacySoundflowerPreferenceMigratesToBlackHole() {
        let suiteName = "AppPreferencesTests.migration.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        defaults.set("Soundflower (2ch)", forKey: "virtualAudioDevice")

        XCTAssertEqual(
            AppPreferences(defaults: defaults).virtualAudioDevice,
            "BlackHole 2ch"
        )
    }
}
