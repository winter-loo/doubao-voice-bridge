import Foundation
import XCTest
@testable import DoubaoBridgeMac

final class DefaultInputRecoveryStoreTests: XCTestCase {
    func testRecoveryRecordRoundTripAndClear() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let store = DefaultInputRecoveryStore(
            fileURL: directory.appendingPathComponent("recovery.json")
        )
        let record = DefaultInputRecoveryRecord(
            originalDeviceUID: "builtin-mic",
            originalDeviceName: "MacBook Pro Microphone",
            remoteDeviceUID: "soundflower-2ch",
            remoteDeviceName: "Soundflower (2ch)",
            createdAt: Date(timeIntervalSince1970: 1_700_000_000)
        )

        try store.save(record)
        let restored = try XCTUnwrap(store.load())
        XCTAssertEqual(restored.originalDeviceUID, record.originalDeviceUID)
        XCTAssertEqual(restored.remoteDeviceUID, record.remoteDeviceUID)
        XCTAssertEqual(restored.createdAt, record.createdAt)

        try store.clear()
        XCTAssertNil(try store.load())
    }
}
