import SwiftUI

private enum SetupStep: Int, CaseIterable, Identifiable {
    case accessibility
    case doubao
    case audio
    case pairing
    case test

    var id: Int { rawValue }

    var title: String {
        switch self {
        case .accessibility: return "Permission"
        case .doubao: return "Doubao"
        case .audio: return "Audio"
        case .pairing: return "Windows"
        case .test: return "Test"
        }
    }

    var symbol: String {
        switch self {
        case .accessibility: return "hand.raised"
        case .doubao: return "character.cursor.ibeam"
        case .audio: return "waveform"
        case .pairing: return "desktopcomputer"
        case .test: return "checkmark.circle"
        }
    }
}

struct SetupAssistantView: View {
    @ObservedObject var model: BridgeAppModel
    let onFinish: () -> Void

    @State private var step = SetupStep.accessibility

    var body: some View {
        HStack(spacing: 0) {
            setupSidebar
            Divider()
            VStack(alignment: .leading, spacing: 0) {
                stepContent
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .padding(28)
                Divider()
                navigationBar
                    .padding()
            }
        }
        .frame(minWidth: 680, minHeight: 460)
        .onAppear {
            model.refreshReadiness()
        }
    }

    private var setupSidebar: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Set Up Bridge", systemImage: "waveform.badge.mic")
                .font(.headline)
                .padding(.bottom, 12)

            ForEach(SetupStep.allCases) { item in
                Label(item.title, systemImage: item.symbol)
                    .foregroundStyle(item == step ? Color.accentColor : .secondary)
                    .padding(.vertical, 6)
            }

            Spacer()
            Text("Your Mac performs recognition. Windows supplies microphone audio and receives the final text.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding()
        .frame(width: 190, alignment: .leading)
        .background(.quaternary)
    }

    @ViewBuilder
    private var stepContent: some View {
        switch step {
        case .accessibility:
            SetupPage(
                title: "Allow Accessibility",
                subtitle: "The bridge needs permission to activate Doubao and keep recognized text in its private capture target.",
                symbol: "hand.raised.fill"
            ) {
                ReadinessRow(
                    title: "Accessibility permission",
                    detail: model.readiness.accessibilityGranted ? "Granted" : "Required",
                    isReady: model.readiness.accessibilityGranted
                )
                HStack {
                    Button("Request Permission", systemImage: "lock.open") {
                        model.requestAccessibilityPermission()
                    }
                    Button("Open System Settings", systemImage: "gear") {
                        model.openAccessibilitySettings()
                    }
                    Button("Check Again", systemImage: "arrow.clockwise") {
                        model.refreshReadiness()
                    }
                }
            }

        case .doubao:
            SetupPage(
                title: "Configure Doubao",
                subtitle: "Set Doubao's microphone to 自动检测 and its voice shortcut to long-press Fn.",
                symbol: "character.cursor.ibeam"
            ) {
                ReadinessRow(
                    title: "Doubao input source",
                    detail: model.readiness.doubaoInputAvailable ? "Detected" : "Not detected",
                    isReady: model.readiness.doubaoInputAvailable
                )
                Text("Doubao must also have Microphone permission in System Settings.")
                    .foregroundStyle(.secondary)
                HStack {
                    Button("Open Doubao Settings", systemImage: "slider.horizontal.3") {
                        model.openDoubaoSettingsAction?()
                    }
                    Button("Check Again", systemImage: "arrow.clockwise") {
                        model.refreshReadiness()
                    }
                }
            }

        case .audio:
            SetupPage(
                title: "Check Audio Routing",
                subtitle: "Remote microphone audio is played into the virtual device selected below.",
                symbol: "waveform"
            ) {
                ReadinessRow(
                    title: model.preferences.virtualAudioDevice,
                    detail: model.readiness.virtualAudioAvailable ? "Available" : "Not found",
                    isReady: model.readiness.virtualAudioAvailable
                )
                ReadinessRow(
                    title: "FFmpeg",
                    detail: model.readiness.ffmpegAvailable ? "Available" : "Not found in PATH",
                    isReady: model.readiness.ffmpegAvailable
                )
                Text("The bridge switches the default Mac input only while a remote session is active, then restores the physical microphone.")
                    .foregroundStyle(.secondary)
                Button("Check Again", systemImage: "arrow.clockwise") {
                    model.refreshReadiness()
                }
            }

        case .pairing:
            SetupPage(
                title: "Connect Windows",
                subtitle: "Use one of these addresses as DOUBAO_BRIDGE_SERVER on the Windows client.",
                symbol: "desktopcomputer"
            ) {
                if model.networkAddresses.isEmpty {
                    Text("No non-loopback IPv4 address is currently available.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(model.networkAddresses, id: \.self) { address in
                        LabeledContent("Server address") {
                            Text(verbatim: "\(address):\(model.preferences.controlPort)")
                                .monospaced()
                                .textSelection(.enabled)
                        }
                    }
                }
                ReadinessRow(
                    title: "Windows client",
                    detail: model.connectionSummary,
                    isReady: model.connectedClientCount > 0
                )
                Text("The client may be paired later; the Mac bridge remains ready in the menu bar.")
                    .foregroundStyle(.secondary)
            }

        case .test:
            SetupPage(
                title: "Run a Voice Test",
                subtitle: "Verify that the bridge can select Doubao and activate its Fn voice shortcut.",
                symbol: "checkmark.circle.fill"
            ) {
                ReadinessRow(
                    title: "Mac setup",
                    detail: model.readiness.isReady ? "Ready" : "Some checks still need attention",
                    isReady: model.readiness.isReady
                )
                LabeledContent("Bridge status") {
                    Text(model.phase.title)
                }
                Button("Test Doubao Activation", systemImage: "mic.badge.plus") {
                    model.testDoubaoAction?()
                }
                Text("The test briefly activates Doubao on this Mac. Full text return is tested from the Windows client.")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var navigationBar: some View {
        HStack {
            Button("Back", systemImage: "chevron.left") {
                move(by: -1)
            }
            .disabled(step == SetupStep.allCases.first)

            Spacer()

            if step == SetupStep.allCases.last {
                Button("Finish", systemImage: "checkmark") {
                    model.completeSetup()
                    onFinish()
                }
                .buttonStyle(.borderedProminent)
                .disabled(!model.readiness.isReady)
            } else {
                Button("Continue", systemImage: "chevron.right") {
                    model.refreshReadiness()
                    move(by: 1)
                }
                .buttonStyle(.borderedProminent)
            }
        }
    }

    private func move(by offset: Int) {
        let next = step.rawValue + offset
        if let value = SetupStep(rawValue: next) {
            step = value
        }
    }
}

private struct SetupPage<Content: View>: View {
    let title: String
    let subtitle: String
    let symbol: String
    @ViewBuilder let content: Content

    init(
        title: String,
        subtitle: String,
        symbol: String,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.subtitle = subtitle
        self.symbol = symbol
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Image(systemName: symbol)
                .font(.largeTitle)
                .foregroundStyle(Color.accentColor)
                .accessibilityHidden(true)
            Text(title)
                .font(.title)
                .bold()
            Text(subtitle)
                .font(.body)
                .foregroundStyle(.secondary)
            Divider()
            VStack(alignment: .leading, spacing: 14) {
                content
            }
        }
    }
}

struct ReadinessRow: View {
    let title: String
    let detail: String
    let isReady: Bool

    var body: some View {
        HStack {
            Label(title, systemImage: isReady ? "checkmark.circle.fill" : "exclamationmark.circle.fill")
                .foregroundStyle(isReady ? Color.green : Color.orange)
            Spacer()
            Text(detail)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}

enum SettingsTab: Hashable {
    case general
    case connection
    case diagnostics
}

private struct PresentedError: Identifiable {
    let id = UUID()
    let message: String
}

struct BridgeSettingsView: View {
    @ObservedObject var model: BridgeAppModel
    let onClose: () -> Void

    @State private var selectedTab: SettingsTab
    @State private var draft: AppPreferences
    @State private var presentedError: PresentedError?

    init(
        model: BridgeAppModel,
        initialTab: SettingsTab,
        onClose: @escaping () -> Void
    ) {
        self.model = model
        self.onClose = onClose
        _selectedTab = State(initialValue: initialTab)
        _draft = State(initialValue: model.preferences)
    }

    var body: some View {
        VStack(spacing: 0) {
            TabView(selection: $selectedTab) {
                generalSettings
                    .tabItem { Label("General", systemImage: "gear") }
                    .tag(SettingsTab.general)
                connectionSettings
                    .tabItem { Label("Connection", systemImage: "network") }
                    .tag(SettingsTab.connection)
                diagnostics
                    .tabItem { Label("Diagnostics", systemImage: "stethoscope") }
                    .tag(SettingsTab.diagnostics)
            }
            Divider()
            HStack {
                if model.restartRequired {
                    Label("Restart required to apply runtime changes", systemImage: "arrow.clockwise")
                        .foregroundStyle(.orange)
                    Button("Restart App", systemImage: "arrow.clockwise") {
                        do {
                            try model.restartApplication()
                        } catch {
                            presentedError = PresentedError(message: String(describing: error))
                        }
                    }
                }
                Spacer()
                Button("Cancel", action: onClose)
                Button("Save") {
                    do {
                        try model.savePreferences(draft)
                    } catch {
                        presentedError = PresentedError(message: String(describing: error))
                    }
                }
                .buttonStyle(.borderedProminent)
            }
            .padding()
        }
        .frame(minWidth: 560, minHeight: 420)
        .alert(item: $presentedError) { error in
            Alert(
                title: Text("Could Not Apply Settings"),
                message: Text(error.message),
                dismissButton: .default(Text("OK"))
            )
        }
    }

    private var generalSettings: some View {
        Form {
            Section("Application") {
                Toggle("Open Doubao Voice Bridge at login", isOn: $draft.launchAtLogin)
                Toggle("Show developer capture window", isOn: $draft.showCaptureWindow)
                Text("The capture window is normally kept off-screen and appears only as an internal text target for Doubao.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Section("Audio Safety") {
                Toggle("Restore the previous Mac microphone after each session", isOn: $draft.restoreDefaultInput)
            }
        }
        .formStyle(.grouped)
        .padding()
    }

    private var connectionSettings: some View {
        Form {
            Section("Network") {
                LabeledContent("Control port") {
                    TextField("Control port", value: $draft.controlPort, format: .number)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 100)
                }
                LabeledContent("Audio port") {
                    TextField("Audio port", value: $draft.audioPort, format: .number)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 100)
                }
            }
            Section("Virtual Audio") {
                TextField("Device name", text: $draft.virtualAudioDevice)
                Text("TCP transport and the Fn hold shortcut are selected automatically for the consumer app.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .padding()
    }

    private var diagnostics: some View {
        Form {
            Section("Current Status") {
                LabeledContent("Bridge") { Text(model.phase.title) }
                LabeledContent("Windows clients") { Text("\(model.connectedClientCount)") }
                ReadinessRow(
                    title: "Accessibility",
                    detail: model.readiness.accessibilityGranted ? "Granted" : "Required",
                    isReady: model.readiness.accessibilityGranted
                )
                ReadinessRow(
                    title: "Doubao input source",
                    detail: model.readiness.doubaoInputAvailable ? "Detected" : "Missing",
                    isReady: model.readiness.doubaoInputAvailable
                )
                ReadinessRow(
                    title: draft.virtualAudioDevice,
                    detail: model.readiness.virtualAudioAvailable ? "Detected" : "Missing",
                    isReady: model.readiness.virtualAudioAvailable
                )
                ReadinessRow(
                    title: "FFmpeg",
                    detail: model.readiness.ffmpegAvailable ? "Available" : "Missing",
                    isReady: model.readiness.ffmpegAvailable
                )
            }
            Section("Actions") {
                HStack {
                    Button("Refresh", systemImage: "arrow.clockwise") {
                        model.refreshReadiness()
                    }
                    Button("Reveal Log", systemImage: "doc.text.magnifyingglass") {
                        model.revealLog()
                    }
                    Button("Run Doubao Test", systemImage: "mic.badge.plus") {
                        model.testDoubaoAction?()
                    }
                }
            }
        }
        .formStyle(.grouped)
        .padding()
    }
}
