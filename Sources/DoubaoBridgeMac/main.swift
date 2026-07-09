import AppKit
import ApplicationServices
import Carbon
import CoreAudio
import Foundation
import Network

struct Config {
    var port: UInt16 = 4387
    var udpPort: UInt16 = 5004
    var audioTransport = "udp"
    var audioDeviceIndex: Int = 4
    var ffmpegPath = "ffmpeg"
    var token: String?
    var voiceShortcut = "cmd+shift+d"
    var voiceShortcutMode = "toggle"
    var audioSourceCommand: String?
    var inputSourceID = "com.bytedance.inputmethod.doubaoime.pinyin"
    var remoteInputDeviceName: String?
    var restoreDefaultInput = true
    var listAudioDevices = false
    var startupDelay: TimeInterval = 0.6
    var voiceActivationCheckDelay: TimeInterval = 1.0
    var voiceActivationRetries = 2
    var voiceActivationRetryDelay: TimeInterval = 0.25
    var finalDelay: TimeInterval = 1.8
    var showWindow = true

    static func parse() throws -> Config {
        var config = Config()
        var args = Array(CommandLine.arguments.dropFirst())

        func takeValue(after option: String) throws -> String {
            guard !args.isEmpty else {
                throw BridgeError.message("Missing value for \(option)")
            }
            return args.removeFirst()
        }

        while !args.isEmpty {
            let option = args.removeFirst()
            switch option {
            case "--port":
                config.port = UInt16(try takeValue(after: option)) ?? config.port
            case "--udp-port":
                config.udpPort = UInt16(try takeValue(after: option)) ?? config.udpPort
            case "--audio-transport":
                config.audioTransport = try takeValue(after: option)
            case "--audio-device-index":
                config.audioDeviceIndex = Int(try takeValue(after: option)) ?? config.audioDeviceIndex
            case "--ffmpeg":
                config.ffmpegPath = try takeValue(after: option)
            case "--token":
                config.token = try takeValue(after: option)
            case "--voice-shortcut":
                config.voiceShortcut = try takeValue(after: option)
            case "--voice-shortcut-mode":
                config.voiceShortcutMode = try takeValue(after: option)
            case "--audio-source-command":
                config.audioSourceCommand = try takeValue(after: option)
            case "--input-source-id":
                config.inputSourceID = try takeValue(after: option)
            case "--remote-input-device":
                config.remoteInputDeviceName = try takeValue(after: option)
            case "--no-restore-default-input":
                config.restoreDefaultInput = false
            case "--list-audio-devices":
                config.listAudioDevices = true
            case "--startup-delay":
                config.startupDelay = Double(try takeValue(after: option)) ?? config.startupDelay
            case "--voice-activation-check-delay":
                config.voiceActivationCheckDelay = Double(try takeValue(after: option)) ?? config.voiceActivationCheckDelay
            case "--voice-activation-retries":
                config.voiceActivationRetries = Int(try takeValue(after: option)) ?? config.voiceActivationRetries
            case "--voice-activation-retry-delay":
                config.voiceActivationRetryDelay = Double(try takeValue(after: option)) ?? config.voiceActivationRetryDelay
            case "--final-delay":
                config.finalDelay = Double(try takeValue(after: option)) ?? config.finalDelay
            case "--hide-window":
                config.showWindow = false
            case "--help", "-h":
                printUsage()
                exit(0)
            default:
                throw BridgeError.message("Unknown option: \(option)")
            }
        }

        return config
    }
}

enum BridgeError: Error, CustomStringConvertible {
    case message(String)

    var description: String {
        switch self {
        case .message(let value):
            return value
        }
    }
}

func printUsage() {
    print("""
    Usage:
      doubao-bridge-mac [options]

    Options:
      --port <port>                  TCP control port. Default: 4387
      --udp-port <port>              UDP raw PCM audio port. Default: 5004
      --audio-transport <udp|tcp>    Raw PCM push transport. Default: udp
      --audio-device-index <index>   AudioToolbox output index for the virtual device. Default: 4
      --ffmpeg <path>                ffmpeg executable. Default: ffmpeg from PATH
      --token <token>                Optional TCP auth token
      --voice-shortcut <shortcut>    Doubao voice shortcut. Default: cmd+shift+d
      --voice-shortcut-mode <mode>   toggle or hold. Default: toggle
      --audio-source-command <cmd>   Command that writes raw s16le 48kHz mono PCM to stdout
      --input-source-id <id>         Doubao input source id
      --remote-input-device <name>   Temporarily set macOS default input to this device on start
      --no-restore-default-input     Do not restore the previous default input device on stop
      --list-audio-devices           List CoreAudio devices and exit
      --startup-delay <seconds>      Delay before toggling Doubao voice input. Default: 0.6
      --voice-activation-check-delay <seconds>
                                      Delay before checking Doubao voice UI. Default: 1.0
      --voice-activation-retries <n> Retry voice shortcut when Doubao voice UI is not detected. Default: 2
      --voice-activation-retry-delay <seconds>
                                      Delay between voice shortcut retries. Default: 0.25
      --final-delay <seconds>        Delay after stop before final text emit. Default: 1.8
      --hide-window                  Create the capture window off-screen
    """)
}

struct HotKey {
    let keyCode: CGKeyCode
    let flags: CGEventFlags
    let name: String
}

func parseHotKey(_ value: String) throws -> HotKey {
    let parts = value
        .lowercased()
        .split(separator: "+")
        .map { String($0.trimmingCharacters(in: .whitespacesAndNewlines)) }

    var flags = CGEventFlags()
    var keyName: String?

    for part in parts {
        switch part {
        case "cmd", "command", "meta":
            flags.insert(.maskCommand)
        case "shift":
            flags.insert(.maskShift)
        case "ctrl", "control":
            flags.insert(.maskControl)
        case "opt", "option", "alt":
            flags.insert(.maskAlternate)
        default:
            keyName = part
        }
    }

    guard let keyName, let keyCode = keyCodeMap[keyName] else {
        throw BridgeError.message("Unsupported voice shortcut key: \(value)")
    }

    return HotKey(keyCode: keyCode, flags: flags, name: value)
}

let keyCodeMap: [String: CGKeyCode] = [
    "a": 0x00, "s": 0x01, "d": 0x02, "f": 0x03, "h": 0x04, "g": 0x05,
    "z": 0x06, "x": 0x07, "c": 0x08, "v": 0x09, "b": 0x0B,
    "q": 0x0C, "w": 0x0D, "e": 0x0E, "r": 0x0F, "y": 0x10, "t": 0x11,
    "1": 0x12, "2": 0x13, "3": 0x14, "4": 0x15, "6": 0x16, "5": 0x17,
    "=": 0x18, "9": 0x19, "7": 0x1A, "-": 0x1B, "8": 0x1C, "0": 0x1D,
    "]": 0x1E, "o": 0x1F, "u": 0x20, "[": 0x21, "i": 0x22, "p": 0x23,
    "l": 0x25, "j": 0x26, "'": 0x27, "k": 0x28, ";": 0x29, "\\": 0x2A,
    ",": 0x2B, "/": 0x2C, "n": 0x2D, "m": 0x2E, ".": 0x2F,
    "tab": 0x30, "space": 0x31, "`": 0x32, "escape": 0x35, "esc": 0x35,
    "return": 0x24, "enter": 0x24,
    "left-option": 0x3A, "left-alt": 0x3A,
    "right-option": 0x3D, "right-alt": 0x3D,
    "fn": 0x3F, "function": 0x3F
]

final class TextCaptureWindow: NSObject, NSTextViewDelegate {
    private let window: NSWindow
    private let textView: NSTextView
    private var lastText = ""
    var onChange: ((String, String) -> Void)?

    init(showWindow: Bool) {
        let rect = showWindow
            ? NSRect(x: 120, y: 120, width: 680, height: 320)
            : NSRect(x: -10000, y: -10000, width: 640, height: 240)

        window = NSWindow(
            contentRect: rect,
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Doubao Voice Bridge Capture"
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        let scrollView = NSScrollView(frame: NSRect(x: 0, y: 0, width: rect.width, height: rect.height))
        scrollView.hasVerticalScroller = true
        scrollView.autoresizingMask = [.width, .height]

        textView = NSTextView(frame: scrollView.bounds)
        textView.autoresizingMask = [.width, .height]
        textView.font = NSFont.systemFont(ofSize: 18)
        textView.isRichText = false
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.allowsUndo = true

        scrollView.documentView = textView
        window.contentView = scrollView

        super.init()
        textView.delegate = self
        window.makeKeyAndOrderFront(nil)
    }

    func focus() {
        NSRunningApplication.current.activate(options: [.activateAllWindows, .activateIgnoringOtherApps])
        NSApp.activate(ignoringOtherApps: true)
        window.orderFrontRegardless()
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(textView)
    }

    func diagnosticSnapshot() -> [String: Any] {
        let responderDescription = window.firstResponder.map { String(describing: type(of: $0)) } ?? "nil"
        let selectedRange = textView.selectedRange()
        let markedRange = textView.markedRange()
        return [
            "isMainWindow": window.isMainWindow,
            "isKeyWindow": window.isKeyWindow,
            "firstResponder": responderDescription,
            "textViewIsFirstResponder": window.firstResponder === textView,
            "textLength": textView.string.count,
            "textPreview": String(textView.string.prefix(80)),
            "hasMarkedText": textView.hasMarkedText(),
            "markedRange": NSStringFromRange(markedRange),
            "selectedRange": NSStringFromRange(selectedRange),
            "activeApp": NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? ""
        ]
    }

    func clear() {
        lastText = ""
        textView.string = ""
        onChange?("", "")
    }

    func currentText() -> String {
        textView.string
    }

    func textDidChange(_ notification: Notification) {
        let text = textView.string
        let delta: String
        if text.hasPrefix(lastText) {
            delta = String(text.dropFirst(lastText.count))
        } else {
            delta = text
        }
        lastText = text
        print("[capture] textDidChange textLength=\(text.count) deltaLength=\(delta.count) deltaPreview=\(String(delta.prefix(80)))")
        onChange?(text, delta)
    }
}

final class DoubaoController {
    private let inputSourceID: String
    private let hotKey: HotKey
    private let leftAlternateMask = CGEventFlags(rawValue: 0x00080000 | 0x00000020)
    private let rightAlternateMask = CGEventFlags(rawValue: 0x00080000 | 0x00000040)
    private let secondaryFnMask = CGEventFlags(rawValue: 0x00800000)

    init(inputSourceID: String, hotKey: HotKey) {
        self.inputSourceID = inputSourceID
        self.hotKey = hotKey
    }

    func switchToDoubao() throws {
        let sources = TISCreateInputSourceList(nil, false).takeRetainedValue() as! [TISInputSource]
        guard let source = sources.first(where: {
            inputSourceString($0, kTISPropertyInputSourceID) == inputSourceID
        }) else {
            throw BridgeError.message("Doubao input source not found: \(inputSourceID)")
        }

        let status = TISSelectInputSource(source)
        if status != noErr {
            throw BridgeError.message("TISSelectInputSource failed: \(status)")
        }
    }

    func selectedInputSourceID() -> String {
        guard let source = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue() else {
            return ""
        }
        return inputSourceString(source, kTISPropertyInputSourceID)
    }

    func selectedInputSourceMatchesDoubao() -> Bool {
        selectedInputSourceID() == inputSourceID
    }

    func pressVoiceShortcut() {
        print("[hotkey] pressing \(hotKey.name) keyCode=\(hotKey.keyCode) flags=\(hotKey.flags.rawValue)")
        if let modifierFlag = modifierFlag(for: hotKey.keyCode), hotKey.flags.isEmpty {
            pressModifierShortcut(keyCode: hotKey.keyCode, flag: modifierFlag)
            return
        }

        let source = CGEventSource(stateID: .hidSystemState)

        let down = CGEvent(keyboardEventSource: source, virtualKey: hotKey.keyCode, keyDown: true)!
        down.flags = hotKey.flags
        down.post(tap: .cghidEventTap)

        let up = CGEvent(keyboardEventSource: source, virtualKey: hotKey.keyCode, keyDown: false)!
        up.flags = hotKey.flags
        up.post(tap: .cghidEventTap)
    }

    func holdVoiceShortcutDown() {
        print("[hotkey] hold-down \(hotKey.name) keyCode=\(hotKey.keyCode) flags=\(hotKey.flags.rawValue)")
        if let modifierFlag = modifierFlag(for: hotKey.keyCode), hotKey.flags.isEmpty {
            postModifierState(keyCode: hotKey.keyCode, flags: modifierFlag, keyDown: true)
            return
        }

        let source = CGEventSource(stateID: .hidSystemState)
        let down = CGEvent(keyboardEventSource: source, virtualKey: hotKey.keyCode, keyDown: true)!
        down.flags = hotKey.flags
        down.post(tap: .cghidEventTap)
    }

    func releaseVoiceShortcut() {
        print("[hotkey] release \(hotKey.name) keyCode=\(hotKey.keyCode) flags=\(hotKey.flags.rawValue)")
        if modifierFlag(for: hotKey.keyCode) != nil, hotKey.flags.isEmpty {
            postModifierState(keyCode: hotKey.keyCode, flags: [], keyDown: false)
            return
        }

        let source = CGEventSource(stateID: .hidSystemState)
        let up = CGEvent(keyboardEventSource: source, virtualKey: hotKey.keyCode, keyDown: false)!
        up.flags = []
        up.post(tap: .cghidEventTap)
    }

    private func pressModifierShortcut(keyCode: CGKeyCode, flag: CGEventFlags) {
        postModifierState(keyCode: keyCode, flags: flag, keyDown: true)
        usleep(120_000)
        postModifierState(keyCode: keyCode, flags: [], keyDown: false)
    }

    private func postModifierState(keyCode: CGKeyCode, flags: CGEventFlags, keyDown: Bool) {
        let source = CGEventSource(stateID: .hidSystemState)

        let event = CGEvent(keyboardEventSource: source, virtualKey: keyCode, keyDown: keyDown)!
        event.type = .flagsChanged
        event.flags = flags
        event.post(tap: .cghidEventTap)
    }

    private func modifierFlag(for keyCode: CGKeyCode) -> CGEventFlags? {
        switch keyCode {
        case 0x37, 0x36:
            return .maskCommand
        case 0x38, 0x3C:
            return .maskShift
        case 0x3A:
            return leftAlternateMask
        case 0x3D:
            return rightAlternateMask
        case 0x3B, 0x3E:
            return .maskControl
        case 0x3F:
            return secondaryFnMask
        default:
            return nil
        }
    }

    func openSettings() {
        let url = URL(fileURLWithPath: "/Library/Input Methods/DoubaoIme.app/Contents/DoubaoImeSettings.app")
        NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
    }

    private func inputSourceString(_ source: TISInputSource, _ key: CFString) -> String {
        guard let pointer = TISGetInputSourceProperty(source, key) else {
            return ""
        }
        return Unmanaged<CFString>.fromOpaque(pointer).takeUnretainedValue() as String
    }
}

final class VoiceStateDetector {
    private let controller: DoubaoController
    private let voiceKeywords = [
        "语音", "麦克风", "录音", "识别", "聆听", "正在听", "按住", "松开", "取消",
        "voice", "mic", "microphone", "record", "listening", "dictation"
    ]

    init(controller: DoubaoController) {
        self.controller = controller
    }

    func snapshot(label: String, capture: [String: Any]) -> [String: Any] {
        let apps = doubaoApplications()
        let appPIDs = Set(apps.map { $0.processIdentifier })
        let windows = doubaoWindows(appPIDs: appPIDs)
        let ax = accessibilitySnapshot(for: apps)
        let samples = (ax["samples"] as? [String] ?? []) + windows.compactMap { $0["name"] as? String }
        let keywordHits = voiceKeywords.filter { keyword in
            samples.contains { $0.localizedCaseInsensitiveContains(keyword) }
        }
        let selectedInputSourceID = controller.selectedInputSourceID()
        let likelyVoiceUIActive = !keywordHits.isEmpty || windows.contains { window in
            let name = window["name"] as? String ?? ""
            return !name.isEmpty || (window["layer"] as? Int ?? 0) > 0
        }

        return [
            "type": "voice_state",
            "label": label,
            "selectedInputSourceID": selectedInputSourceID,
            "selectedInputSourceMatchesDoubao": controller.selectedInputSourceMatchesDoubao(),
            "accessibilityTrusted": AXIsProcessTrusted(),
            "doubaoApps": apps.map { app in
                [
                    "pid": app.processIdentifier,
                    "bundleIdentifier": app.bundleIdentifier ?? "",
                    "localizedName": app.localizedName ?? "",
                    "isActive": app.isActive
                ]
            },
            "doubaoWindowCount": windows.count,
            "doubaoWindows": windows,
            "ax": ax,
            "keywordHits": keywordHits,
            "likelyVoiceUIActive": likelyVoiceUIActive,
            "capture": capture
        ]
    }

    private func doubaoApplications() -> [NSRunningApplication] {
        NSWorkspace.shared.runningApplications.filter { app in
            if app.bundleIdentifier == "com.bytedance.inputmethod.doubaoime" {
                return true
            }
            return app.executableURL?.path == "/Library/Input Methods/DoubaoIme.app/Contents/MacOS/DoubaoIme"
        }
    }

    private func doubaoWindows(appPIDs: Set<pid_t>) -> [[String: Any]] {
        guard let rawWindows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
            return []
        }

        return rawWindows.compactMap { info in
            let ownerName = info[kCGWindowOwnerName as String] as? String ?? ""
            let ownerPID = info[kCGWindowOwnerPID as String] as? Int ?? 0
            guard appPIDs.contains(pid_t(ownerPID)) || ownerName == "豆包输入法" else {
                return nil
            }

            let boundsDict = info[kCGWindowBounds as String] as? [String: Any] ?? [:]
            return [
                "ownerName": ownerName,
                "ownerPID": ownerPID,
                "name": info[kCGWindowName as String] as? String ?? "",
                "layer": info[kCGWindowLayer as String] as? Int ?? 0,
                "alpha": info[kCGWindowAlpha as String] as? Double ?? 0,
                "bounds": boundsDict
            ]
        }
    }

    private func accessibilitySnapshot(for apps: [NSRunningApplication]) -> [String: Any] {
        guard AXIsProcessTrusted() else {
            return ["trusted": false, "samples": [], "windows": []]
        }

        var allSamples: [String] = []
        var appWindows: [[String: Any]] = []
        for app in apps {
            let element = AXUIElementCreateApplication(app.processIdentifier)
            var windowsValue: CFTypeRef?
            let status = AXUIElementCopyAttributeValue(element, kAXWindowsAttribute as CFString, &windowsValue)
            guard status == .success, let windows = windowsValue as? [AXUIElement] else {
                continue
            }

            for window in windows.prefix(8) {
                let summary = summarizeAXElement(window, maxDepth: 3, sampleLimit: 30)
                let samples = summary["samples"] as? [String] ?? []
                allSamples.append(contentsOf: samples)
                appWindows.append([
                    "pid": app.processIdentifier,
                    "bundleIdentifier": app.bundleIdentifier ?? "",
                    "localizedName": app.localizedName ?? "",
                    "summary": summary
                ])
            }
        }

        return [
            "trusted": true,
            "samples": Array(allSamples.prefix(80)),
            "windows": appWindows
        ]
    }

    private func summarizeAXElement(_ element: AXUIElement, maxDepth: Int, sampleLimit: Int) -> [String: Any] {
        var samples: [String] = []
        var visited = 0

        func visit(_ element: AXUIElement, depth: Int) {
            guard depth <= maxDepth, samples.count < sampleLimit, visited < 120 else {
                return
            }
            visited += 1

            let parts = [
                axString(element, kAXRoleAttribute),
                axString(element, kAXSubroleAttribute),
                axString(element, kAXTitleAttribute),
                axString(element, kAXDescriptionAttribute),
                axString(element, kAXValueAttribute),
                axString(element, kAXHelpAttribute),
                axString(element, kAXIdentifierAttribute)
            ].filter { !$0.isEmpty }

            if !parts.isEmpty {
                samples.append(parts.joined(separator: " | "))
            }

            var childrenValue: CFTypeRef?
            let status = AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &childrenValue)
            guard status == .success, let children = childrenValue as? [AXUIElement] else {
                return
            }
            for child in children.prefix(25) {
                visit(child, depth: depth + 1)
            }
        }

        visit(element, depth: 0)
        return [
            "visited": visited,
            "samples": samples
        ]
    }

    private func axString(_ element: AXUIElement, _ attribute: String) -> String {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success,
              let value
        else {
            return ""
        }
        return String(describing: value)
    }
}

final class FocusStateDetector {
    func snapshot(label: String, capture: [String: Any]) -> [String: Any] {
        var object: [String: Any] = [
            "type": "focus_state",
            "label": label,
            "accessibilityTrusted": AXIsProcessTrusted(),
            "frontmostApp": appDictionary(NSWorkspace.shared.frontmostApplication),
            "capture": capture
        ]

        guard AXIsProcessTrusted() else {
            object["focusedApplication"] = [:]
            object["focusedElement"] = [:]
            return object
        }

        let system = AXUIElementCreateSystemWide()
        object["focusedApplication"] = focusedApplicationSnapshot(system)
        object["focusedElement"] = focusedElementSnapshot(system)
        return object
    }

    private func focusedApplicationSnapshot(_ system: AXUIElement) -> [String: Any] {
        var appValue: CFTypeRef?
        guard AXUIElementCopyAttributeValue(system, kAXFocusedApplicationAttribute as CFString, &appValue) == .success,
              let appElement = appValue
        else {
            return [:]
        }

        let element = appElement as! AXUIElement
        var pid = pid_t(0)
        AXUIElementGetPid(element, &pid)
        var object = appDictionary(NSRunningApplication(processIdentifier: pid))
        object["pid"] = Int(pid)
        object["ax"] = axElementSummary(element, includeChildren: false)
        return object
    }

    private func focusedElementSnapshot(_ system: AXUIElement) -> [String: Any] {
        var elementValue: CFTypeRef?
        guard AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute as CFString, &elementValue) == .success,
              let elementValue
        else {
            return [:]
        }

        let element = elementValue as! AXUIElement
        var pid = pid_t(0)
        AXUIElementGetPid(element, &pid)
        return [
            "pid": Int(pid),
            "app": appDictionary(NSRunningApplication(processIdentifier: pid)),
            "ax": axElementSummary(element, includeChildren: true)
        ]
    }

    private func appDictionary(_ app: NSRunningApplication?) -> [String: Any] {
        guard let app else {
            return [:]
        }
        return [
            "pid": app.processIdentifier,
            "bundleIdentifier": app.bundleIdentifier ?? "",
            "localizedName": app.localizedName ?? "",
            "isActive": app.isActive,
            "executablePath": app.executableURL?.path ?? ""
        ]
    }

    private func axElementSummary(_ element: AXUIElement, includeChildren: Bool) -> [String: Any] {
        var object: [String: Any] = [
            "role": axString(element, kAXRoleAttribute),
            "subrole": axString(element, kAXSubroleAttribute),
            "title": axString(element, kAXTitleAttribute),
            "description": axString(element, kAXDescriptionAttribute),
            "value": truncated(axString(element, kAXValueAttribute), limit: 200),
            "selectedText": truncated(axString(element, kAXSelectedTextAttribute), limit: 200),
            "help": axString(element, kAXHelpAttribute),
            "identifier": axString(element, kAXIdentifierAttribute),
            "focused": axString(element, kAXFocusedAttribute),
            "enabled": axString(element, kAXEnabledAttribute)
        ]

        if includeChildren {
            var childrenValue: CFTypeRef?
            if AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &childrenValue) == .success,
               let children = childrenValue as? [AXUIElement] {
                object["children"] = children.prefix(12).map { child in
                    [
                        "role": axString(child, kAXRoleAttribute),
                        "title": axString(child, kAXTitleAttribute),
                        "description": axString(child, kAXDescriptionAttribute),
                        "value": truncated(axString(child, kAXValueAttribute), limit: 120)
                    ]
                }
            }
        }

        return object
    }

    private func axString(_ element: AXUIElement, _ attribute: String) -> String {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success,
              let value
        else {
            return ""
        }
        return String(describing: value)
    }

    private func truncated(_ value: String, limit: Int) -> String {
        if value.count <= limit {
            return value
        }
        return String(value.prefix(limit))
    }
}

struct AudioDeviceInfo {
    let id: AudioDeviceID
    let name: String
    let uid: String
    let inputChannels: Int
    let outputChannels: Int
}

final class CoreAudioDeviceManager {
    func devices() throws -> [AudioDeviceInfo] {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var size: UInt32 = 0
        var status = AudioObjectGetPropertyDataSize(
            AudioObjectID(kAudioObjectSystemObject),
            &address,
            0,
            nil,
            &size
        )
        guard status == noErr else {
            throw BridgeError.message("AudioObjectGetPropertyDataSize(devices) failed: \(status)")
        }

        let count = Int(size) / MemoryLayout<AudioDeviceID>.size
        var ids = Array(repeating: AudioDeviceID(0), count: count)
        status = ids.withUnsafeMutableBufferPointer { buffer in
            AudioObjectGetPropertyData(
                AudioObjectID(kAudioObjectSystemObject),
                &address,
                0,
                nil,
                &size,
                buffer.baseAddress!
            )
        }
        guard status == noErr else {
            throw BridgeError.message("AudioObjectGetPropertyData(devices) failed: \(status)")
        }

        return ids.map { id in
            AudioDeviceInfo(
                id: id,
                name: stringProperty(kAudioObjectPropertyName, deviceID: id),
                uid: stringProperty(kAudioDevicePropertyDeviceUID, deviceID: id),
                inputChannels: channelCount(deviceID: id, scope: kAudioDevicePropertyScopeInput),
                outputChannels: channelCount(deviceID: id, scope: kAudioDevicePropertyScopeOutput)
            )
        }
    }

    func defaultInputDevice() throws -> AudioDeviceID {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultInputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        var deviceID = AudioDeviceID(0)
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject),
            &address,
            0,
            nil,
            &size,
            &deviceID
        )
        guard status == noErr else {
            throw BridgeError.message("Read default input device failed: \(status)")
        }
        return deviceID
    }

    func setDefaultInputDevice(_ deviceID: AudioDeviceID) throws {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultInputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var value = deviceID
        let size = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = AudioObjectSetPropertyData(
            AudioObjectID(kAudioObjectSystemObject),
            &address,
            0,
            nil,
            size,
            &value
        )
        guard status == noErr else {
            throw BridgeError.message("Set default input device failed: \(status)")
        }
    }

    func findInputDevice(matching query: String) throws -> AudioDeviceInfo {
        let normalized = query.lowercased()
        let inputDevices = try devices().filter { $0.inputChannels > 0 }
        if let exact = inputDevices.first(where: {
            $0.name.lowercased() == normalized || $0.uid.lowercased() == normalized
        }) {
            return exact
        }
        if let partial = inputDevices.first(where: {
            $0.name.lowercased().contains(normalized) || $0.uid.lowercased().contains(normalized)
        }) {
            return partial
        }
        throw BridgeError.message("Input device not found: \(query)")
    }

    func describe(deviceID: AudioDeviceID) -> String {
        let name = stringProperty(kAudioObjectPropertyName, deviceID: deviceID)
        let uid = stringProperty(kAudioDevicePropertyDeviceUID, deviceID: deviceID)
        return "\(name) [\(uid)] id=\(deviceID)"
    }

    func printDevices() throws {
        let defaultInput = try? defaultInputDevice()
        for device in try devices().filter({ $0.inputChannels > 0 || $0.outputChannels > 0 }) {
            let marker = device.id == defaultInput ? " default-input" : ""
            print("id=\(device.id) input=\(device.inputChannels) output=\(device.outputChannels)\(marker) name=\(device.name) uid=\(device.uid)")
        }
    }

    private func stringProperty(_ selector: AudioObjectPropertySelector, deviceID: AudioDeviceID) -> String {
        var address = AudioObjectPropertyAddress(
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: MemoryLayout<CFString?>.size,
            alignment: MemoryLayout<CFString?>.alignment
        )
        defer {
            raw.deallocate()
        }

        var size = UInt32(MemoryLayout<CFString?>.size)
        let status = AudioObjectGetPropertyData(deviceID, &address, 0, nil, &size, raw)
        guard status == noErr else {
            return ""
        }
        let value = raw.load(as: CFString?.self)
        return value as String? ?? ""
    }

    private func channelCount(deviceID: AudioDeviceID, scope: AudioObjectPropertyScope) -> Int {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyStreamConfiguration,
            mScope: scope,
            mElement: kAudioObjectPropertyElementMain
        )
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(deviceID, &address, 0, nil, &size) == noErr else {
            return 0
        }

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: Int(size),
            alignment: MemoryLayout<AudioBufferList>.alignment
        )
        defer {
            raw.deallocate()
        }

        guard AudioObjectGetPropertyData(deviceID, &address, 0, nil, &size, raw) == noErr else {
            return 0
        }

        let list = raw.assumingMemoryBound(to: AudioBufferList.self)
        return UnsafeMutableAudioBufferListPointer(list).reduce(0) { count, buffer in
            count + Int(buffer.mNumberChannels)
        }
    }
}

final class AudioReceiver {
    struct AudioLevelSnapshot {
        let bytes: UInt64
        let packets: UInt64
        let peak: Int16
        let rmsDBFS: Double
        let peakDBFS: Double

        var dictionary: [String: Any] {
            [
                "bytes": bytes,
                "packets": packets,
                "peak": peak,
                "rmsDBFS": rmsDBFS,
                "peakDBFS": peakDBFS
            ]
        }
    }

    private let ffmpegPath: String
    private let udpPort: UInt16
    private let audioTransport: String
    private let audioDeviceIndex: Int
    private let audioSourceCommand: String?
    private var process: Process?
    private var sourceProcess: Process?
    private var processErrorPipe: Pipe?
    private var sourceErrorPipe: Pipe?
    private var tcpInputPipe: Pipe?
    private var tcpListener: NWListener?
    private var tcpConnections: [NWConnection] = []
    private let tcpQueue = DispatchQueue(label: "doubao.bridge.audio.tcp")
    private let tcpQueueKey = DispatchSpecificKey<Bool>()
    private var tcpIsStopping = false
    private var levelBytes: UInt64 = 0
    private var levelPackets: UInt64 = 0
    private var levelSumSquares: Double = 0
    private var levelSamples: UInt64 = 0
    private var levelPeak: Int16 = 0

    init(
        ffmpegPath: String,
        udpPort: UInt16,
        audioTransport: String,
        audioDeviceIndex: Int,
        audioSourceCommand: String?
    ) {
        self.ffmpegPath = ffmpegPath
        self.udpPort = udpPort
        self.audioTransport = audioTransport
        self.audioDeviceIndex = audioDeviceIndex
        self.audioSourceCommand = audioSourceCommand
        tcpQueue.setSpecific(key: tcpQueueKey, value: true)
    }

    func resetAudioLevel() {
        tcpQueue.async { [weak self] in
            self?.resetAudioLevelOnQueue()
        }
    }

    func audioLevelSnapshot(reset: Bool = false) -> AudioLevelSnapshot {
        if DispatchQueue.getSpecific(key: tcpQueueKey) == true {
            return audioLevelSnapshotOnQueue(reset: reset)
        }
        return tcpQueue.sync {
            audioLevelSnapshotOnQueue(reset: reset)
        }
    }

    func start() throws {
        if process?.isRunning == true {
            return
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        var arguments = [
            ffmpegPath,
            "-hide_banner",
            "-loglevel", "warning",
        ]

        if let audioSourceCommand {
            print("[audio] starting source command: \(audioSourceCommand)")
            let sourceProcess = Process()
            sourceProcess.executableURL = URL(fileURLWithPath: "/bin/sh")
            sourceProcess.arguments = ["-lc", audioSourceCommand]
            let pipe = Pipe()
            sourceProcess.standardOutput = pipe
            let sourceErrorPipe = Pipe()
            sourceProcess.standardError = sourceErrorPipe
            sourceErrorPipe.fileHandleForReading.readabilityHandler = { handle in
                let data = handle.availableData
                guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
                print("[audio-source stderr] \(text.trimmingCharacters(in: .whitespacesAndNewlines))")
            }
            try sourceProcess.run()
            print("[audio] source pid=\(sourceProcess.processIdentifier)")
            self.sourceProcess = sourceProcess
            self.sourceErrorPipe = sourceErrorPipe
            process.standardInput = pipe.fileHandleForReading

            arguments += [
                "-f", "s16le",
                "-ar", "48000",
                "-ac", "1",
                "-i", "-"
            ]
        } else if audioTransport == "tcp" {
            let pipe = Pipe()
            tcpInputPipe = pipe
            process.standardInput = pipe.fileHandleForReading

            arguments += [
                "-f", "s16le",
                "-ar", "48000",
                "-ac", "1",
                "-i", "-"
            ]
        } else {
            let extraInputArguments: [String]
            let inputURL: String
            switch audioTransport {
            case "udp":
                inputURL = "udp://0.0.0.0:\(udpPort)?listen=1&fifo_size=1000000&overrun_nonfatal=1"
                extraInputArguments = []
            default:
                throw BridgeError.message("Unsupported audio transport: \(audioTransport)")
            }

            arguments += [
            "-fflags", "nobuffer",
            "-flags", "low_delay",
            "-probesize", "32",
            "-analyzeduration", "0",
            "-f", "s16le",
            "-ar", "48000",
            "-ac", "1",
            ] + extraInputArguments + [
            "-i", inputURL,
            ]
        }

        arguments += [
            "-ac", "2",
            "-ar", "48000",
            "-f", "audiotoolbox",
            "-audio_device_index", "\(audioDeviceIndex)",
            "-"
        ]
        process.arguments = arguments
        print("[audio] starting ffmpeg: /usr/bin/env \(arguments.joined(separator: " "))")

        let errorPipe = Pipe()
        process.standardError = errorPipe
        process.standardOutput = Pipe()
        errorPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            print("[audio-ffmpeg stderr] \(text.trimmingCharacters(in: .whitespacesAndNewlines))")
        }
        process.terminationHandler = { _ in
            errorPipe.fileHandleForReading.readabilityHandler = nil
            print("[audio] ffmpeg exited status=\(process.terminationStatus)")
        }

        try process.run()
        print("[audio] ffmpeg pid=\(process.processIdentifier)")
        self.process = process
        self.processErrorPipe = errorPipe

        if audioSourceCommand == nil && audioTransport == "tcp" {
            try startTCPAudioServer()
        }
    }

    func stop() {
        stopTCPAudioServer()
        guard let process else {
            return
        }
        if process.isRunning {
            process.terminate()
        }
        processErrorPipe?.fileHandleForReading.readabilityHandler = nil
        processErrorPipe = nil
        self.process = nil
        if let sourceProcess {
            if sourceProcess.isRunning {
                sourceProcess.terminate()
            }
            sourceErrorPipe?.fileHandleForReading.readabilityHandler = nil
            sourceErrorPipe = nil
            self.sourceProcess = nil
        }
    }

    private func startTCPAudioServer() throws {
        guard let port = NWEndpoint.Port(rawValue: udpPort) else {
            throw BridgeError.message("Invalid TCP audio port: \(udpPort)")
        }

        let listener = try NWListener(using: .tcp, on: port)
        tcpIsStopping = false
        listener.newConnectionHandler = { [weak self] connection in
            self?.acceptTCPAudio(connection)
        }
        listener.stateUpdateHandler = { state in
            print("[audio-tcp] listener state: \(state)")
        }
        listener.start(queue: tcpQueue)
        tcpListener = listener
        print("[audio-tcp] listening on 0.0.0.0:\(udpPort)")
    }

    private func acceptTCPAudio(_ connection: NWConnection) {
        print("[audio-tcp] accepted connection")
        tcpConnections.append(connection)
        connection.stateUpdateHandler = { [weak self, weak connection] state in
            guard let self, let connection else { return }
            switch state {
            case .cancelled, .failed:
                self.tcpConnections.removeAll { $0 === connection }
            default:
                break
            }
        }
        connection.start(queue: tcpQueue)
        receiveTCPAudio(from: connection)
    }

    private func receiveTCPAudio(from connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self, weak connection] data, _, isComplete, error in
            guard let self, let connection else { return }
            guard !self.tcpIsStopping else {
                connection.cancel()
                return
            }
            if let data, !data.isEmpty {
                self.ingestAudioLevel(data)
                self.tcpInputPipe?.fileHandleForWriting.write(data)
            }
            if isComplete || error != nil {
                connection.cancel()
                self.tcpConnections.removeAll { $0 === connection }
                return
            }
            self.receiveTCPAudio(from: connection)
        }
    }

    private func stopTCPAudioServer() {
        if DispatchQueue.getSpecific(key: tcpQueueKey) == true {
            stopTCPAudioServerOnQueue()
        } else {
            tcpQueue.sync {
                stopTCPAudioServerOnQueue()
            }
        }
    }

    private func stopTCPAudioServerOnQueue() {
        tcpIsStopping = true
        tcpListener?.cancel()
        tcpListener = nil
        for connection in tcpConnections {
            connection.cancel()
        }
        tcpConnections.removeAll()
        tcpInputPipe?.fileHandleForWriting.closeFile()
        tcpInputPipe = nil
    }

    private func resetAudioLevelOnQueue() {
        levelBytes = 0
        levelPackets = 0
        levelSumSquares = 0
        levelSamples = 0
        levelPeak = 0
    }

    private func audioLevelSnapshotOnQueue(reset: Bool) -> AudioLevelSnapshot {
        let rms: Double
        if levelSamples > 0 {
            rms = sqrt(levelSumSquares / Double(levelSamples))
        } else {
            rms = 0
        }
        let peak = max(1, Int(abs(Int(levelPeak))))
        let rmsDBFS = rms > 0 ? 20 * log10(rms / 32768.0) : -120.0
        let peakDBFS = 20 * log10(Double(peak) / 32768.0)
        let snapshot = AudioLevelSnapshot(
            bytes: levelBytes,
            packets: levelPackets,
            peak: levelPeak,
            rmsDBFS: max(rmsDBFS, -120.0),
            peakDBFS: max(peakDBFS, -120.0)
        )
        if reset {
            resetAudioLevelOnQueue()
        }
        return snapshot
    }

    private func ingestAudioLevel(_ data: Data) {
        levelBytes += UInt64(data.count)
        levelPackets += 1
        var index = data.startIndex
        while index + 1 < data.endIndex {
            let low = UInt16(data[index])
            let high = UInt16(data[data.index(after: index)]) << 8
            let sample = Int16(bitPattern: high | low)
            let magnitude = abs(Int(sample))
            if magnitude > abs(Int(levelPeak)) {
                levelPeak = sample
            }
            let normalized = Double(sample)
            levelSumSquares += normalized * normalized
            levelSamples += 1
            index = data.index(index, offsetBy: 2)
        }
    }
}

final class Bridge {
    private let config: Config
    private let captureWindow: TextCaptureWindow
    private let controller: DoubaoController
    private let voiceStateDetector: VoiceStateDetector
    private let focusStateDetector: FocusStateDetector
    private let audioReceiver: AudioReceiver
    private let audioDeviceManager: CoreAudioDeviceManager
    private var savedDefaultInputDevice: AudioDeviceID?
    private var voiceActivationGeneration = 0
    private var voiceActivationAttempt = 0
    private var audioLevelGeneration = 0
    private(set) var isRecording = false
    var emit: (([String: Any]) -> Void)?

    init(
        config: Config,
        captureWindow: TextCaptureWindow,
        controller: DoubaoController,
        voiceStateDetector: VoiceStateDetector,
        focusStateDetector: FocusStateDetector,
        audioReceiver: AudioReceiver,
        audioDeviceManager: CoreAudioDeviceManager
    ) {
        self.config = config
        self.captureWindow = captureWindow
        self.controller = controller
        self.voiceStateDetector = voiceStateDetector
        self.focusStateDetector = focusStateDetector
        self.audioReceiver = audioReceiver
        self.audioDeviceManager = audioDeviceManager
    }

    func startSession() {
        do {
            print("[session] start requested")
            voiceActivationGeneration += 1
            audioLevelGeneration += 1
            voiceActivationAttempt = 0
            try switchDefaultInputForRemoteSession()
            try audioReceiver.start()
            audioReceiver.resetAudioLevel()
            captureWindow.clear()
            captureWindow.focus()
            logDiagnostics("after initial focus")
            try controller.switchToDoubao()
            logDiagnostics("after switchToDoubao")
            emitVoiceState("after switchToDoubao")
            emit?(["type": "status", "recording": true, "phase": "arming"])

            let generation = voiceActivationGeneration
            DispatchQueue.main.asyncAfter(deadline: .now() + config.startupDelay) { [weak self] in
                self?.beginVoiceActivation(generation: generation)
            }
        } catch {
            restoreDefaultInputIfNeeded()
            emit?(["type": "error", "message": "\(error)"])
        }
    }

    func stopSession() {
        print("[session] stop requested")
        voiceActivationGeneration += 1
        audioLevelGeneration += 1
        logDiagnostics("before stop shortcut")
        emitVoiceState("before stop shortcut")
        emitFocusState("before stop shortcut")
        emitAudioLevel(label: "before stop shortcut", reset: false)
        if isRecording {
            if config.voiceShortcutMode == "hold" {
                controller.releaseVoiceShortcut()
            } else {
                controller.pressVoiceShortcut()
            }
            isRecording = false
            emitFocusState("after stop shortcut")
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + config.finalDelay) { [weak self] in
            guard let self else { return }
            self.logDiagnostics("before final emit")
            self.emitVoiceState("before final emit")
            self.emitFocusState("before final emit")
            self.emitAudioLevel(label: "before final emit", reset: false)
            self.audioReceiver.stop()
            self.restoreDefaultInputIfNeeded()
            self.emit?(["type": "final", "text": self.captureWindow.currentText()])
            self.emit?(["type": "status", "recording": false, "phase": "idle"])
        }
    }

    func toggleSession() {
        isRecording ? stopSession() : startSession()
    }

    func clear() {
        captureWindow.clear()
    }

    func status() {
        emit?(["type": "status", "recording": isRecording, "phase": isRecording ? "recording" : "idle"])
    }

    func diagnose() {
        let snapshot = captureWindow.diagnosticSnapshot()
        print("[diagnose] \(snapshot)")
        emit?(["type": "diagnose", "capture": snapshot])
        emitVoiceState("diagnose")
    }

    func voiceState() {
        emitVoiceState("manual")
    }

    func focusState() {
        emitFocusState("manual")
    }

    func openSettings() {
        controller.openSettings()
    }

    func testHotkey() {
        do {
            print("[test-hotkey] requested")
            captureWindow.focus()
            logDiagnostics("test-hotkey after focus")
            try controller.switchToDoubao()
            logDiagnostics("test-hotkey after switchToDoubao")
            emitVoiceState("test-hotkey before press")
            if config.voiceShortcutMode == "hold" {
                controller.holdVoiceShortcutDown()
                emitVoiceState("test-hotkey after hold")
                scheduleVoiceStateChecks(prefix: "test-hotkey after hold")
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
                    self?.controller.releaseVoiceShortcut()
                    self?.emitVoiceState("test-hotkey after release")
                }
            } else {
                controller.pressVoiceShortcut()
                emitVoiceState("test-hotkey after press")
                scheduleVoiceStateChecks(prefix: "test-hotkey after press")
            }
            logDiagnostics("test-hotkey after press")
            emit?(["type": "ack", "command": "test-hotkey"])
        } catch {
            emit?(["type": "error", "message": "\(error)"])
        }
    }

    private func switchDefaultInputForRemoteSession() throws {
        guard let remoteInputDeviceName = config.remoteInputDeviceName else {
            return
        }

        let previous = try audioDeviceManager.defaultInputDevice()
        let remote = try audioDeviceManager.findInputDevice(matching: remoteInputDeviceName)
        savedDefaultInputDevice = previous

        if previous != remote.id {
            try audioDeviceManager.setDefaultInputDevice(remote.id)
        }

        emit?([
            "type": "audio",
            "event": "default_input_set",
            "device": audioDeviceManager.describe(deviceID: remote.id),
            "previous": audioDeviceManager.describe(deviceID: previous)
        ])
    }

    private func restoreDefaultInputIfNeeded() {
        guard config.restoreDefaultInput, let savedDefaultInputDevice else {
            return
        }

        do {
            try audioDeviceManager.setDefaultInputDevice(savedDefaultInputDevice)
            emit?([
                "type": "audio",
                "event": "default_input_restored",
                "device": audioDeviceManager.describe(deviceID: savedDefaultInputDevice)
            ])
        } catch {
            emit?(["type": "error", "message": "Failed to restore default input: \(error)"])
        }

        self.savedDefaultInputDevice = nil
    }

    private func logDiagnostics(_ label: String) {
        print("[diagnose] \(label): \(captureWindow.diagnosticSnapshot())")
    }

    private func beginVoiceActivation(generation: Int) {
        guard generation == voiceActivationGeneration else {
            return
        }

        captureWindow.focus()
        logDiagnostics("before voice shortcut")
        emitVoiceState("before voice shortcut")
        if config.voiceShortcutMode == "hold" {
            controller.holdVoiceShortcutDown()
        } else {
            controller.pressVoiceShortcut()
        }
        isRecording = true
        logDiagnostics("after voice shortcut")
        emitVoiceState("after voice shortcut")
        scheduleVoiceStateChecks(prefix: "after voice shortcut")

        DispatchQueue.main.asyncAfter(deadline: .now() + config.voiceActivationCheckDelay) { [weak self] in
            self?.checkVoiceActivation(generation: generation)
        }
    }

    private func checkVoiceActivation(generation: Int) {
        guard generation == voiceActivationGeneration else {
            return
        }

        let snapshot = emitVoiceState("voice activation check attempt \(voiceActivationAttempt + 1)")
        if snapshot["likelyVoiceUIActive"] as? Bool == true {
            emit?(["type": "status", "recording": true, "phase": "recording"])
            startAudioLevelReporting(generation: generation)
            return
        }

        if voiceActivationAttempt < config.voiceActivationRetries {
            voiceActivationAttempt += 1
            print("[voice-state] voice UI not active; retry \(voiceActivationAttempt)/\(config.voiceActivationRetries)")
            emit?([
                "type": "status",
                "recording": true,
                "phase": "voice_retry",
                "attempt": voiceActivationAttempt,
                "maxAttempts": config.voiceActivationRetries
            ])
            releaseVoiceShortcutIfNeeded()

            DispatchQueue.main.asyncAfter(deadline: .now() + config.voiceActivationRetryDelay) { [weak self] in
                self?.beginVoiceActivation(generation: generation)
            }
            return
        }

        print("[voice-state] Doubao voice UI did not become active after \(config.voiceActivationRetries + 1) attempts")
        releaseVoiceShortcutIfNeeded()
        audioReceiver.stop()
        restoreDefaultInputIfNeeded()
        emit?([
            "type": "error",
            "message": "Doubao voice UI did not become active",
            "phase": "voice_activation_failed"
        ])
        emit?(["type": "status", "recording": false, "phase": "idle"])
    }

    private func releaseVoiceShortcutIfNeeded() {
        guard isRecording else {
            return
        }
        if config.voiceShortcutMode == "hold" {
            controller.releaseVoiceShortcut()
        } else {
            controller.pressVoiceShortcut()
        }
        isRecording = false
    }

    private func startAudioLevelReporting(generation: Int) {
        audioLevelGeneration += 1
        let levelGeneration = audioLevelGeneration
        emitAudioLevel(label: "recording start", reset: true)
        scheduleAudioLevelReport(voiceGeneration: generation, levelGeneration: levelGeneration)
    }

    private func scheduleAudioLevelReport(voiceGeneration: Int, levelGeneration: Int) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
            guard let self,
                  voiceGeneration == self.voiceActivationGeneration,
                  levelGeneration == self.audioLevelGeneration,
                  self.isRecording
            else {
                return
            }
            self.emitAudioLevel(label: "recording interval", reset: true)
            self.scheduleAudioLevelReport(voiceGeneration: voiceGeneration, levelGeneration: levelGeneration)
        }
    }

    private func emitAudioLevel(label: String, reset: Bool) {
        var object = audioReceiver.audioLevelSnapshot(reset: reset).dictionary
        object["type"] = "audio_level"
        object["label"] = label
        print("[audio-level] \(label): bytes=\(object["bytes"] ?? 0) rmsDBFS=\(object["rmsDBFS"] ?? 0) peakDBFS=\(object["peakDBFS"] ?? 0)")
        emit?(object)
    }

    private func emitFocusState(_ label: String) {
        let snapshot = focusStateDetector.snapshot(label: label, capture: captureWindow.diagnosticSnapshot())
        let focused = snapshot["focusedElement"] as? [String: Any]
        let app = focused?["app"] as? [String: Any]
        let ax = focused?["ax"] as? [String: Any]
        print("[focus-state] \(label): app=\(app?["bundleIdentifier"] ?? "") role=\(ax?["role"] ?? "") value=\(ax?["value"] ?? "") captureTextLength=\(captureWindow.currentText().count)")
        emit?(snapshot)
    }

    @discardableResult
    private func emitVoiceState(_ label: String) -> [String: Any] {
        let snapshot = voiceStateDetector.snapshot(label: label, capture: captureWindow.diagnosticSnapshot())
        print("[voice-state] \(label): input=\(snapshot["selectedInputSourceID"] ?? "") windows=\(snapshot["doubaoWindowCount"] ?? 0) keywords=\(snapshot["keywordHits"] ?? []) likely=\(snapshot["likelyVoiceUIActive"] ?? false)")
        emit?(snapshot)
        return snapshot
    }

    private func scheduleVoiceStateChecks(prefix: String) {
        for delay in [0.25, 1.0] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
                self?.emitVoiceState("\(prefix) +\(delay)s")
            }
        }
    }
}

final class BridgeServer {
    private final class Client {
        let id = UUID()
        let connection: NWConnection
        var buffer = Data()
        var authorized: Bool

        init(connection: NWConnection, authorized: Bool) {
            self.connection = connection
            self.authorized = authorized
        }
    }

    private let listener: NWListener
    private let queue = DispatchQueue(label: "doubao.bridge.server")
    private let token: String?
    private let bridge: Bridge
    private var clients: [UUID: Client] = [:]

    init(port: UInt16, token: String?, bridge: Bridge) throws {
        guard let nwPort = NWEndpoint.Port(rawValue: port) else {
            throw BridgeError.message("Invalid TCP port: \(port)")
        }
        listener = try NWListener(using: .tcp, on: nwPort)
        self.token = token
        self.bridge = bridge
    }

    func start() {
        listener.newConnectionHandler = { [weak self] connection in
            self?.accept(connection)
        }
        listener.stateUpdateHandler = { state in
            print("Bridge server state: \(state)")
        }
        listener.start(queue: queue)
    }

    func broadcast(_ object: [String: Any]) {
        guard JSONSerialization.isValidJSONObject(object),
              let data = try? JSONSerialization.data(withJSONObject: object),
              var line = String(data: data, encoding: .utf8)
        else {
            return
        }
        line.append("\n")
        let bytes = Data(line.utf8)

        queue.async { [weak self] in
            guard let self else { return }
            for client in self.clients.values where client.authorized {
                client.connection.send(content: bytes, completion: .contentProcessed { _ in })
            }
        }
    }

    private func accept(_ connection: NWConnection) {
        let client = Client(connection: connection, authorized: token == nil)
        clients[client.id] = client

        connection.stateUpdateHandler = { [weak self, weak client] state in
            guard let self, let client else { return }
            if case .cancelled = state {
                self.clients.removeValue(forKey: client.id)
            }
            if case .failed = state {
                self.clients.removeValue(forKey: client.id)
            }
        }

        connection.start(queue: queue)
        send(["type": "hello", "authRequired": token != nil], to: client)
        receive(from: client)
    }

    private func receive(from client: Client) {
        client.connection.receive(minimumIncompleteLength: 1, maximumLength: 4096) { [weak self, weak client] data, _, isComplete, error in
            guard let self, let client else { return }
            if let data, !data.isEmpty {
                client.buffer.append(data)
                self.consumeLines(from: client)
            }
            if isComplete || error != nil {
                client.connection.cancel()
                self.clients.removeValue(forKey: client.id)
                return
            }
            self.receive(from: client)
        }
    }

    private func consumeLines(from client: Client) {
        while let newline = client.buffer.firstIndex(of: 0x0A) {
            let lineData = client.buffer[..<newline]
            client.buffer.removeSubrange(...newline)

            guard let line = String(data: lineData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines),
                !line.isEmpty
            else {
                continue
            }

            handle(line, from: client)
        }
    }

    private func handle(_ line: String, from client: Client) {
        let parts = line.split(separator: " ", maxSplits: 1).map(String.init)
        let command = parts[0].lowercased()
        let argument = parts.count > 1 ? parts[1] : ""

        if !client.authorized {
            if command == "token", argument == token {
                client.authorized = true
                send(["type": "auth", "ok": true], to: client)
            } else {
                send(["type": "auth", "ok": false], to: client)
            }
            return
        }

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            switch command {
            case "start":
                self.bridge.startSession()
                self.send(["type": "ack", "command": "start"], to: client)
            case "stop":
                self.bridge.stopSession()
                self.send(["type": "ack", "command": "stop"], to: client)
            case "toggle":
                self.bridge.toggleSession()
                self.send(["type": "ack", "command": "toggle"], to: client)
            case "clear":
                self.bridge.clear()
                self.send(["type": "ack", "command": "clear"], to: client)
            case "status":
                self.bridge.status()
                self.send(["type": "ack", "command": "status"], to: client)
            case "diagnose":
                self.bridge.diagnose()
                self.send(["type": "ack", "command": "diagnose"], to: client)
            case "voice-state":
                self.bridge.voiceState()
                self.send(["type": "ack", "command": "voice-state"], to: client)
            case "focus-state":
                self.bridge.focusState()
                self.send(["type": "ack", "command": "focus-state"], to: client)
            case "test-hotkey":
                self.bridge.testHotkey()
                self.send(["type": "ack", "command": "test-hotkey"], to: client)
            case "settings":
                self.bridge.openSettings()
                self.send(["type": "ack", "command": "settings"], to: client)
            default:
                self.send(["type": "error", "message": "Unknown command: \(command)"], to: client)
            }
        }
    }

    private func send(_ object: [String: Any], to client: Client) {
        guard JSONSerialization.isValidJSONObject(object),
              let data = try? JSONSerialization.data(withJSONObject: object),
              var line = String(data: data, encoding: .utf8)
        else {
            return
        }
        line.append("\n")
        client.connection.send(content: Data(line.utf8), completion: .contentProcessed { _ in })
    }
}

do {
    let config = try Config.parse()
    let audioDeviceManager = CoreAudioDeviceManager()
    if config.listAudioDevices {
        try audioDeviceManager.printDevices()
        exit(0)
    }

    let hotKey = try parseHotKey(config.voiceShortcut)

    let app = NSApplication.shared
    app.setActivationPolicy(.regular)

    let captureWindow = TextCaptureWindow(showWindow: config.showWindow)
    let controller = DoubaoController(inputSourceID: config.inputSourceID, hotKey: hotKey)
    let voiceStateDetector = VoiceStateDetector(controller: controller)
    let focusStateDetector = FocusStateDetector()
    let audioReceiver = AudioReceiver(
        ffmpegPath: config.ffmpegPath,
        udpPort: config.udpPort,
        audioTransport: config.audioTransport,
        audioDeviceIndex: config.audioDeviceIndex,
        audioSourceCommand: config.audioSourceCommand
    )
    let bridge = Bridge(
        config: config,
        captureWindow: captureWindow,
        controller: controller,
        voiceStateDetector: voiceStateDetector,
        focusStateDetector: focusStateDetector,
        audioReceiver: audioReceiver,
        audioDeviceManager: audioDeviceManager
    )
    let server = try BridgeServer(port: config.port, token: config.token, bridge: bridge)

    bridge.emit = { object in
        server.broadcast(object)
    }
    captureWindow.onChange = { text, delta in
        server.broadcast(["type": "text", "text": text, "delta": delta])
    }

    server.start()
    print("Doubao bridge listening on TCP \(config.port), audio \(config.udpPort)")
    print("Audio transport: \(config.audioTransport)")
    print("Bundle identifier: \(Bundle.main.bundleIdentifier ?? "(none)")")
    print("AudioToolbox output index: \(config.audioDeviceIndex)")
    print("Doubao input source: \(config.inputSourceID)")
    if let remoteInputDeviceName = config.remoteInputDeviceName {
        print("Remote session default input override: \(remoteInputDeviceName)")
    }
    if let audioSourceCommand = config.audioSourceCommand {
        print("Audio source command: \(audioSourceCommand)")
    }
    app.run()
} catch {
    fputs("doubao-bridge-mac: \(error)\n", stderr)
    printUsage()
    exit(1)
}
