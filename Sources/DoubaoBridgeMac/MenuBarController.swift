import AppKit
import Combine
import SwiftUI

struct MenuBarSnapshot: Equatable {
    let phase: BridgeAppPhase
    let connectedClientCount: Int
    let launchAtLogin: Bool

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
}

@MainActor
final class MenuBarController: NSObject {
    private let model: BridgeAppModel
    private let statusItem: NSStatusItem
    private var observations = Set<AnyCancellable>()
    private var transcriptPanelController: TranscriptPanelController?
    private var setupWindow: NSWindow?
    private var settingsWindow: NSWindow?

    init(model: BridgeAppModel) {
        self.model = model
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        super.init()

        model.$phase
            .combineLatest(model.$connectedClientCount, model.$preferences)
            .sink { [weak self] output in
                let (phase, connectedClientCount, preferences) = output
                self?.refreshMenu(
                    snapshot: MenuBarSnapshot(
                        phase: phase,
                        connectedClientCount: connectedClientCount,
                        launchAtLogin: preferences.launchAtLogin
                    )
                )
            }
            .store(in: &observations)

        model.$transcriptPanelSession
            .removeDuplicates()
            .sink { [weak self] session in
                self?.replaceTranscriptPanel(for: session)
            }
            .store(in: &observations)

        if model.shouldPresentSetup {
            DispatchQueue.main.async { [weak self] in
                self?.showSetupAssistant()
            }
        }
    }

    private func replaceTranscriptPanel(for session: TranscriptPanelSession?) {
        transcriptPanelController?.close()
        transcriptPanelController = nil

        guard session != nil else {
            return
        }

        let controller = TranscriptPanelController(
            model: model,
            statusButton: statusItem.button
        )
        transcriptPanelController = controller
        controller.show()
    }

    private func refreshMenu(snapshot: MenuBarSnapshot) {
        let image = BrandAssets.menuBarTemplateImage() ?? NSImage(
            systemSymbolName: snapshot.phase.symbolName,
            accessibilityDescription: snapshot.phase.title
        )
        image?.isTemplate = true
        statusItem.button?.image = image
        statusItem.button?.contentTintColor = statusColor(for: snapshot.phase)
        statusItem.button?.toolTip = "Doubao Voice Bridge: \(snapshot.phase.title)"
        statusItem.button?.setAccessibilityLabel("Doubao Voice Bridge: \(snapshot.phase.title)")

        let menu = NSMenu()
        let title = NSMenuItem(title: "Doubao Voice Bridge", action: nil, keyEquivalent: "")
        title.isEnabled = false
        menu.addItem(title)

        let status = NSMenuItem(title: snapshot.phase.title, action: nil, keyEquivalent: "")
        status.image = NSImage(systemSymbolName: snapshot.phase.symbolName, accessibilityDescription: nil)
        status.isEnabled = false
        menu.addItem(status)

        let clients = NSMenuItem(title: snapshot.connectionSummary, action: nil, keyEquivalent: "")
        clients.image = NSImage(systemSymbolName: "desktopcomputer", accessibilityDescription: nil)
        clients.isEnabled = false
        menu.addItem(clients)

        menu.addItem(.separator())
        menu.addItem(menuItem("Setup Assistant...", symbol: "checklist", action: #selector(openSetup)))
        menu.addItem(menuItem("Settings...", symbol: "gear", action: #selector(openSettings)))
        menu.addItem(menuItem("Diagnostics...", symbol: "stethoscope", action: #selector(openDiagnostics)))

        let launchAtLogin = menuItem(
            "Start at Login",
            symbol: "power",
            action: #selector(toggleLaunchAtLogin)
        )
        launchAtLogin.state = snapshot.launchAtLogin ? .on : .off
        menu.addItem(launchAtLogin)

        menu.addItem(.separator())
        menu.addItem(menuItem("Quit", symbol: "power", action: #selector(quit)))
        statusItem.menu = menu
    }

    private func statusColor(for phase: BridgeAppPhase) -> NSColor {
        switch phase {
        case .starting:
            return .secondaryLabelColor
        case .ready:
            return .labelColor
        case .activating, .optimizing:
            return .systemOrange
        case .listening:
            return BrandPalette.coreBlue
        case .error:
            return .systemRed
        }
    }

    private func menuItem(_ title: String, symbol: String, action: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.target = self
        item.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
        return item
    }

    @objc private func openSetup() {
        showSetupAssistant()
    }

    @objc private func openSettings() {
        showSettings(tab: .general)
    }

    @objc private func openDiagnostics() {
        showSettings(tab: .diagnostics)
    }

    @objc private func toggleLaunchAtLogin() {
        var updated = model.preferences
        updated.launchAtLogin.toggle()
        do {
            try model.savePreferences(updated)
        } catch {
            showError(error)
        }
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    private func showSetupAssistant() {
        if let setupWindow {
            present(setupWindow)
            return
        }

        let view = SetupAssistantView(model: model) { [weak self] in
            self?.setupWindow?.close()
        }
        let window = makeWindow(
            title: "Set Up Doubao Voice Bridge",
            content: view,
            size: NSSize(width: 720, height: 500)
        )
        setupWindow = window
        present(window)
    }

    private func showSettings(tab: SettingsTab) {
        settingsWindow?.close()
        let view = BridgeSettingsView(model: model, initialTab: tab) { [weak self] in
            self?.settingsWindow?.close()
        }
        let window = makeWindow(
            title: "Doubao Voice Bridge Settings",
            content: view,
            size: NSSize(width: 620, height: 480)
        )
        settingsWindow = window
        present(window)
    }

    private func makeWindow<Content: View>(
        title: String,
        content: Content,
        size: NSSize
    ) -> NSWindow {
        let controller = NSHostingController(rootView: content)
        let window = NSWindow(contentViewController: controller)
        window.title = title
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable]
        window.setContentSize(size)
        window.center()
        window.isReleasedWhenClosed = false
        return window
    }

    private func present(_ window: NSWindow) {
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    private func showError(_ error: Error) {
        let alert = NSAlert()
        alert.messageText = "Doubao Voice Bridge"
        alert.informativeText = String(describing: error)
        alert.alertStyle = .warning
        alert.runModal()
    }
}
