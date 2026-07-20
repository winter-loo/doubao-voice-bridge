import Foundation

struct AppPreferences: Equatable {
    private enum Key {
        static let controlPort = "controlPort"
        static let audioPort = "audioPort"
        static let virtualAudioDevice = "virtualAudioDevice"
        static let restoreDefaultInput = "restoreDefaultInput"
        static let showCaptureWindow = "showCaptureWindow"
        static let launchAtLogin = "launchAtLogin"
        static let setupCompleted = "setupCompleted"
    }

    var controlPort = 4387
    var audioPort = 5004
    var virtualAudioDevice = "Soundflower (2ch)"
    var restoreDefaultInput = true
    var showCaptureWindow = false
    var launchAtLogin = false
    var setupCompleted = false

    init() {}

    init(defaults: UserDefaults) {
        if let value = defaults.object(forKey: Key.controlPort) as? NSNumber {
            controlPort = value.intValue
        }
        if let value = defaults.object(forKey: Key.audioPort) as? NSNumber {
            audioPort = value.intValue
        }
        if let value = defaults.string(forKey: Key.virtualAudioDevice), !value.isEmpty {
            virtualAudioDevice = value
        }
        if defaults.object(forKey: Key.restoreDefaultInput) != nil {
            restoreDefaultInput = defaults.bool(forKey: Key.restoreDefaultInput)
        }
        if defaults.object(forKey: Key.showCaptureWindow) != nil {
            showCaptureWindow = defaults.bool(forKey: Key.showCaptureWindow)
        }
        launchAtLogin = defaults.bool(forKey: Key.launchAtLogin)
        setupCompleted = defaults.bool(forKey: Key.setupCompleted)
    }

    func save(to defaults: UserDefaults) {
        defaults.set(controlPort, forKey: Key.controlPort)
        defaults.set(audioPort, forKey: Key.audioPort)
        defaults.set(virtualAudioDevice, forKey: Key.virtualAudioDevice)
        defaults.set(restoreDefaultInput, forKey: Key.restoreDefaultInput)
        defaults.set(showCaptureWindow, forKey: Key.showCaptureWindow)
        defaults.set(launchAtLogin, forKey: Key.launchAtLogin)
        defaults.set(setupCompleted, forKey: Key.setupCompleted)
    }
}
