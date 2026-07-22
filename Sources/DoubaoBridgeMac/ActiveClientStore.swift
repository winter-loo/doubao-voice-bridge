import Foundation

final class ActiveClientStore<Value> {
    var onCountChange: ((Int) -> Void)?

    private var storage: [UUID: Value] = [:]

    var values: Dictionary<UUID, Value>.Values {
        storage.values
    }

    func insert(_ value: Value, forKey key: UUID) {
        let previousCount = storage.count
        storage[key] = value
        publishCountIfChanged(from: previousCount)
    }

    @discardableResult
    func removeValue(forKey key: UUID) -> Value? {
        let previousCount = storage.count
        let value = storage.removeValue(forKey: key)
        publishCountIfChanged(from: previousCount)
        return value
    }

    private func publishCountIfChanged(from previousCount: Int) {
        guard storage.count != previousCount else {
            return
        }
        onCountChange?(storage.count)
    }
}
