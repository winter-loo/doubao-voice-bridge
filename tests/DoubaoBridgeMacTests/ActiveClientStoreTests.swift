import XCTest
@testable import DoubaoBridgeMac

final class ActiveClientStoreTests: XCTestCase {
    func testCountChangesTrackInsertAndRemoval() {
        let store = ActiveClientStore<String>()
        let firstID = UUID()
        let secondID = UUID()
        var observedCounts: [Int] = []
        store.onCountChange = { observedCounts.append($0) }

        store.insert("first", forKey: firstID)
        store.insert("second", forKey: secondID)
        store.removeValue(forKey: firstID)
        store.removeValue(forKey: secondID)

        XCTAssertEqual(observedCounts, [1, 2, 1, 0])
        XCTAssertTrue(store.values.isEmpty)
    }

    func testRemovingUnknownClientDoesNotPublishDuplicateCount() {
        let store = ActiveClientStore<String>()
        let clientID = UUID()
        var observedCounts: [Int] = []
        store.onCountChange = { observedCounts.append($0) }

        store.insert("client", forKey: clientID)
        store.removeValue(forKey: clientID)
        store.removeValue(forKey: clientID)

        XCTAssertEqual(observedCounts, [1, 0])
    }
}
