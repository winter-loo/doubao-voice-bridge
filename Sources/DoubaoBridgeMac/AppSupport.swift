import AppKit
import Darwin
import Foundation

enum AppLog {
    static let fileURL: URL = {
        let base = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask)[0]
        return base
            .appendingPathComponent("Logs", isDirectory: true)
            .appendingPathComponent("DoubaoVoiceBridge", isDirectory: true)
            .appendingPathComponent("bridge.log", isDirectory: false)
    }()

    static func redirectWhenDetached() {
        guard CommandLine.arguments.count == 1,
              Bundle.main.bundleURL.pathExtension == "app",
              isatty(STDOUT_FILENO) == 0
        else {
            return
        }

        let directory = fileURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
        freopen(fileURL.path, "a", stdout)
        freopen(fileURL.path, "a", stderr)
        setbuf(stdout, nil)
        setbuf(stderr, nil)
        print("\n[app] launched \(Date())")
    }
}

enum NetworkAddressProvider {
    static func ipv4Addresses() -> [String] {
        Host.current().addresses
            .filter { !$0.contains(":") && $0 != "127.0.0.1" }
            .sorted()
    }
}

enum ExecutableResolver {
    static func resolve(_ executable: String) -> String {
        if executable.contains("/") {
            return executable
        }

        let pathDirectories = (ProcessInfo.processInfo.environment["PATH"] ?? "")
            .split(separator: ":")
            .map(String.init)
        let commonDirectories = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        for directory in pathDirectories + commonDirectories {
            let candidate = URL(fileURLWithPath: directory)
                .appendingPathComponent(executable)
                .path
            if FileManager.default.isExecutableFile(atPath: candidate) {
                return candidate
            }
        }
        return executable
    }
}

enum AppFailurePresenter {
    static func present(_ error: Error) {
        guard CommandLine.arguments.count == 1,
              Bundle.main.bundleURL.pathExtension == "app"
        else {
            return
        }
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        app.activate(ignoringOtherApps: true)

        let alert = NSAlert()
        alert.messageText = "Doubao Voice Bridge Could Not Start"
        alert.informativeText = String(describing: error)
        alert.alertStyle = .critical
        alert.addButton(withTitle: "Quit")
        alert.runModal()
    }
}
