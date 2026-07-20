import XCTest
@testable import DoubaoBridgeMac

final class AudioToolboxDeviceTests: XCTestCase {
    private let deviceListing = """
    [AudioToolbox @ 0x1234] CoreAudio devices:
    [AudioToolbox @ 0x1234] [0]                         (null), default-output
    [AudioToolbox @ 0x1234] [1]                         (null), BuiltInMicrophoneDevice
    [AudioToolbox @ 0x1234] [2]                         (null), BuiltInSpeakerDevice
    [AudioToolbox @ 0x1234] [3]         OrayVirtualAudioDevice, OrayVirtualAudioDevice_UID
    [AudioToolbox @ 0x1234] [4]              Soundflower (2ch), SoundflowerEngine:0
    """

    func testResolvesSoundflowerAfterAnotherVirtualDeviceChangesItsIndex() throws {
        XCTAssertEqual(
            try audioToolboxDeviceIndex(named: "Soundflower (2ch)", in: deviceListing),
            4
        )
        XCTAssertEqual(
            try audioToolboxDeviceIndex(named: "Soundflower", in: deviceListing),
            4
        )
    }

    func testReportsAvailableDevicesWhenNameDoesNotMatch() {
        XCTAssertThrowsError(
            try audioToolboxDeviceIndex(named: "Missing Device", in: deviceListing)
        ) { error in
            XCTAssertTrue(String(describing: error).contains("Soundflower (2ch)"))
        }
    }
}
