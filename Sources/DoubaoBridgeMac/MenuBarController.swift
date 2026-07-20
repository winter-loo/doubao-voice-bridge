import AppKit
import Combine
import SwiftUI

@MainActor
final class MenuBarController: NSObject {
    private let model: BridgeAppModel
    private let statusItem: NSStatusItem
    private var observation: AnyCancellable?
    private var setupWindow: NSWindow?
    private var settingsWindow: NSWindow?

    init(model: BridgeAppModel) {
        self.model = model
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        super.init()

        observation = model.objectWillChange.sink { [weak self] _ in
            DispatchQueue.main.async {
                self?.refreshMenu()
            }
        }
        refreshMenu()

        if model.shouldPresentSetup {
            DispatchQueue.main.async { [weak self] in
                self?.showSetupAssistant()
            }
        }
    }

    private func refreshMenu() {
        let image = NSImage(systemSymbolName: model.phase.symbolName, accessibilityDescription: model.phase.title)
        image?.isTemplate = true
        statusItem.button?.image = image
        statusItem.button?.contentTintColor = statusColor
        statusItem.button?.toolTip = "Doubao Voice Bridge: \(model.phase.title)"

        let menu = NSMenu()
        let title = NSMenuItem(title: "Doubao Voice Bridge", action: nil, keyEquivalent: "")
        title.isEnabled = false
        menu.addItem(title)

        let status = NSMenuItem(title: model.phase.title, action: nil, keyEquivalent: "")
        status.image = NSImage(systemSymbolName: model.phase.symbolName, accessibilityDescription: nil)
        status.isEnabled = false
        menu.addItem(status)

        let clients = NSMenuItem(title: model.connectionSummary, action: nil, keyEquivalent: "")
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
        launchAtLogin.state = model.preferences.launchAtLogin ? .on : .off
        menu.addItem(launchAtLogin)

        menu.addItem(.separator())
        menu.addItem(menuItem("Quit", symbol: "power", action: #selector(quit)))
        statusItem.menu = menu
    }

    private var statusColor: NSColor {
        switch model.phase {
        case .starting:
            return .secondaryLabelColor
        case .ready:
            return .systemGreen
        case .activating, .optimizing:
            return .systemOrange
        case .listening:
            return .systemBlue
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
