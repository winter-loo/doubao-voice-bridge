import AppKit
import ApplicationServices
import Combine
import Foundation
import ServiceManagement

enum BridgeAppPhase: Equatable {
    case starting
    case ready
    case activating
    case listening
    case optimizing
    case error(String)

    var title: String {
        switch self {
        case .starting:
            return "Starting"
        case .ready:
            return "Ready"
        case .activating:
            return "Activating Doubao"
        case .listening:
            return "Listening"
        case .optimizing:
            return "Optimizing recognition"
        case .error:
            return "Needs attention"
        }
    }

    var symbolName: String {
        switch self {
        case .starting:
            return "ellipsis.circle"
        case .ready:
            return "waveform.circle"
        case .activating:
            return "hourglass.circle"
        case .listening:
            return "waveform.circle.fill"
        case .optimizing:
            return "text.bubble"
        case .error:
            return "exclamationmark.triangle.fill"
        }
    }
}

struct AppReadiness: Equatable {
    var accessibilityGranted = false
    var doubaoInputAvailable = false
    var virtualAudioAvailable = false
    var ffmpegAvailable = false

    var isReady: Bool {
        accessibilityGranted
            && doubaoInputAvailable
            && virtualAudioAvailable
    }
}

@MainActor
final class BridgeAppModel: ObservableObject {
    @Published private(set) var phase: BridgeAppPhase = .starting
    @Published private(set) var connectedClientCount = 0
    @Published private(set) var readiness = AppReadiness()
    @Published private(set) var preferences: AppPreferences
    @Published private(set) var restartRequired = false
    @Published private(set) var statusMessage = "Starting the bridge"

    var testDoubaoAction: (() -> Void)?
    var openDoubaoSettingsAction: (() -> Void)?

    private let defaults: UserDefaults
    private let audioDeviceManager: CoreAudioDeviceManager
    private let controller: DoubaoController
    private let ffmpegPath: String

    init(
        preferences: AppPreferences,
        defaults: UserDefaults = .standard,
        audioDeviceManager: CoreAudioDeviceManager,
        controller: DoubaoController,
        ffmpegPath: String
    ) {
        self.preferences = preferences
        self.defaults = defaults
        self.audioDeviceManager = audioDeviceManager
        self.controller = controller
        self.ffmpegPath = ffmpegPath

        if Bundle.main.bundleURL.pathExtension == "app" {
            self.preferences.launchAtLogin = SMAppService.mainApp.status == .enabled
        }
        refreshReadiness()
    }

    var shouldPresentSetup: Bool {
        !preferences.setupCompleted || !readiness.isReady
    }

    var connectionSummary: String {
        switch connectedClientCount {
        case 0:
            return "No Windows clients"
        case 1:
            return "1 Windows client connected"
        default:
            return "\(connectedClientCount) Windows clients connected"
        }
    }

    var networkAddresses: [String] {
        NetworkAddressProvider.ipv4Addresses()
    }

    func refreshReadiness() {
        let virtualAudioAvailable = (
            try? (
                audioDeviceManager.findInputDevice(matching: preferences.virtualAudioDevice),
                audioDeviceManager.findOutputDevice(matching: preferences.virtualAudioDevice)
            )
        ) != nil
        readiness = AppReadiness(
            accessibilityGranted: AXIsProcessTrusted(),
            doubaoInputAvailable: controller.isInputSourceAvailable(),
            virtualAudioAvailable: virtualAudioAvailable,
            ffmpegAvailable: Self.executableIsAvailable(ffmpegPath)
        )

        if readiness.isReady, case .starting = phase {
            phase = .ready
            statusMessage = "Waiting for a Windows client"
        } else if !readiness.isReady, case .starting = phase {
            phase = .error("Setup is incomplete")
            statusMessage = "Complete setup before using voice input"
        }
    }

    func handleBridgeEvent(_ object: [String: Any]) {
        let type = object["type"] as? String
        if type == "error" {
            let message = object["message"] as? String ?? "Unknown bridge error"
            phase = .error(message)
            statusMessage = message
            return
        }

        guard type == "status", let value = object["phase"] as? String else {
            return
        }

        switch value {
        case "arming", "voice_retry":
            phase = .activating
            statusMessage = "Preparing Doubao voice input"
        case "recording":
            phase = .listening
            statusMessage = "Receiving microphone audio from Windows"
        case "optimizing":
            phase = .optimizing
            statusMessage = "Waiting for Doubao to commit the final text"
        case "idle":
            phase = readiness.isReady ? .ready : .error("Setup is incomplete")
            statusMessage = connectedClientCount == 0
                ? "Waiting for a Windows client"
                : connectionSummary
        default:
            break
        }
    }

    func serverStateChanged(_ state: String) {
        if state.contains("failed") {
            phase = .error(state)
            statusMessage = "The bridge could not start its network listener"
        } else if state.contains("ready"), readiness.isReady {
            phase = .ready
            statusMessage = connectionSummary
        }
    }

    func clientCountChanged(_ count: Int) {
        connectedClientCount = count
        if phase == .ready {
            statusMessage = connectionSummary
        }
    }

    func savePreferences(_ updated: AppPreferences) throws {
        guard (1...65_535).contains(updated.controlPort),
              (1...65_535).contains(updated.audioPort)
        else {
            throw BridgeError.message("Ports must be between 1 and 65535")
        }
        guard !updated.virtualAudioDevice.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw BridgeError.message("Virtual audio device name cannot be empty")
        }

        var saved = updated
        if Bundle.main.bundleURL.pathExtension == "app",
           updated.launchAtLogin != preferences.launchAtLogin {
            if updated.launchAtLogin {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
            saved.launchAtLogin = SMAppService.mainApp.status == .enabled
        }

        restartRequired = runtimeSettingsChanged(from: preferences, to: saved)
        preferences = saved
        preferences.save(to: defaults)
        refreshReadiness()
    }

    func completeSetup() {
        preferences.setupCompleted = true
        preferences.save(to: defaults)
    }

    func requestAccessibilityPermission() {
        let options = [
            kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true
        ] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
    }

    func openAccessibilitySettings() {
        guard let url = URL(
            string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        ) else {
            return
        }
        NSWorkspace.shared.open(url)
    }

    func revealLog() {
        let directory = AppLog.fileURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        if !FileManager.default.fileExists(atPath: AppLog.fileURL.path) {
            FileManager.default.createFile(atPath: AppLog.fileURL.path, contents: nil)
        }
        NSWorkspace.shared.activateFileViewerSelecting([AppLog.fileURL])
    }

    func restartApplication() throws {
        let bundleURL = Bundle.main.bundleURL
        guard bundleURL.pathExtension == "app" else {
            throw BridgeError.message("Restart is available when running the packaged app")
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = [
            "-c",
            "sleep 0.8; /usr/bin/open -n \"$1\"",
            "doubao-bridge-restart",
            bundleURL.path
        ]
        try process.run()
        NSApp.terminate(nil)
    }

    private func runtimeSettingsChanged(
        from current: AppPreferences,
        to updated: AppPreferences
    ) -> Bool {
        current.controlPort != updated.controlPort
            || current.audioPort != updated.audioPort
            || current.virtualAudioDevice != updated.virtualAudioDevice
            || current.restoreDefaultInput != updated.restoreDefaultInput
            || current.showCaptureWindow != updated.showCaptureWindow
    }

    private static func executableIsAvailable(_ executable: String) -> Bool {
        if executable.contains("/") {
            return FileManager.default.isExecutableFile(atPath: executable)
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/which")
        process.arguments = [executable]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }
}
