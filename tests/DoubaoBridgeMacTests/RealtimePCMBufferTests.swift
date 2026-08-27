import Foundation
import XCTest
@testable import DoubaoBridgeMac

final class RealtimePCMBufferTests: XCTestCase {
    func testDecodesLittleEndianPCMAndPadsUnderflowWithSilence() {
        let buffer = RealtimePCMBuffer(capacityFrames: 8)
        let bytes = Data([0x00, 0x40, 0x00, 0xC0])

        XCTAssertEqual(buffer.enqueueS16LE(bytes), 0)
        XCTAssertEqual(buffer.dequeue(frameCount: 3), [0.5, -0.5, 0])
    }

    func testDropsOldestFramesInsteadOfGrowingLatency() {
        let buffer = RealtimePCMBuffer(capacityFrames: 2)
        let bytes = Data([0x00, 0x10, 0x00, 0x20, 0x00, 0x30])

        XCTAssertEqual(buffer.enqueueS16LE(bytes), 2)
        XCTAssertEqual(buffer.dequeue(frameCount: 2), [0.25, 0.375])
    }

    func testPreservesSampleSplitAcrossEnqueues() {
        let buffer = RealtimePCMBuffer(capacityFrames: 4)

        XCTAssertEqual(buffer.enqueueS16LE(Data([0x00])), 0)
        XCTAssertEqual(buffer.dequeue(frameCount: 1), [0])
        XCTAssertEqual(buffer.enqueueS16LE(Data([0x40, 0x00, 0xC0])), 0)
        XCTAssertEqual(buffer.dequeue(frameCount: 2), [0.5, -0.5])
    }

    func testClearRemovesQueuedFramesAndPendingSplitSample() {
        let buffer = RealtimePCMBuffer(capacityFrames: 4)
        XCTAssertEqual(buffer.enqueueS16LE(Data([0x00, 0x40, 0x7F])), 0)

        buffer.clear()

        XCTAssertEqual(buffer.dequeue(frameCount: 2), [0, 0])
        XCTAssertEqual(buffer.enqueueS16LE(Data([0x00, 0x20])), 0)
        XCTAssertEqual(buffer.dequeue(frameCount: 1), [0.25])
    }
}
