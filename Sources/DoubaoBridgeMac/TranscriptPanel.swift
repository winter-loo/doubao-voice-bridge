import AppKit
import Combine
import QuartzCore
import SwiftUI

struct TranscriptPanelMetrics: Equatable {
    let size: NSSize
    let cornerRadius: CGFloat
    let horizontalPadding: CGFloat
    let verticalPadding: CGFloat
    let statusOpacity: CGFloat
}

enum TranscriptPanelStyle {
    static let compact = TranscriptPanelMetrics(
        size: NSSize(width: 180, height: 38),
        cornerRadius: 36,
        horizontalPadding: 14,
        verticalPadding: 10,
        statusOpacity: 0.92
    )
    static let expanded = TranscriptPanelMetrics(
        size: NSSize(width: 420, height: 104),
        cornerRadius: 24,
        horizontalPadding: 16,
        verticalPadding: 8,
        statusOpacity: 0.62
    )
    static let anchorGap: CGFloat = 0
    static let iconSize: CGFloat = 26
    static let contentGap: CGFloat = 10
    static let statusFontSize: CGFloat = 12
    static let transcriptFontSize: CGFloat = 16
    static let surfaceOpacity: CGFloat = 0.97
    static let outlineOpacity: CGFloat = 0.10
}

enum TranscriptPanelLayout: Equatable {
    case compact
    case expanded

    var metrics: TranscriptPanelMetrics {
        switch self {
        case .compact:
            return TranscriptPanelStyle.compact
        case .expanded:
            return TranscriptPanelStyle.expanded
        }
    }

    var size: NSSize {
        metrics.size
    }

    static func resolve(transcriptText: String) -> Self {
        transcriptText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ? .compact
            : .expanded
    }
}

struct TranscriptPanelGeometry {
    static let edgeInset: CGFloat = 8
    static let anchorGap = TranscriptPanelStyle.anchorGap

    static func frame(anchor: NSRect, visibleFrame: NSRect, size: NSSize) -> NSRect {
        let preferredX = anchor.midX - size.width / 2
        let minimumX = visibleFrame.minX + edgeInset
        let maximumX = visibleFrame.maxX - size.width - edgeInset
        let x = min(max(preferredX, minimumX), max(minimumX, maximumX))
        let y = anchor.minY - anchorGap - size.height
        return NSRect(origin: NSPoint(x: x, y: y), size: size)
    }
}

@MainActor
final class TranscriptPanelPresentation: ObservableObject {
    @Published var layout: TranscriptPanelLayout

    init(layout: TranscriptPanelLayout) {
        self.layout = layout
    }
}

final class TranscriptPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

@MainActor
final class TranscriptPanelController {
    private let panel: TranscriptPanel
    private let presentation: TranscriptPanelPresentation
    private weak var statusButton: NSStatusBarButton?
    private var observations = Set<AnyCancellable>()

    init(model: BridgeAppModel, statusButton: NSStatusBarButton?) {
        self.statusButton = statusButton

        let initialLayout = TranscriptPanelLayout.resolve(transcriptText: model.transcriptText)
        presentation = TranscriptPanelPresentation(layout: initialLayout)
        panel = TranscriptPanel(
            contentRect: NSRect(origin: .zero, size: initialLayout.size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.contentViewController = NSHostingController(
            rootView: TranscriptIslandView(model: model, presentation: presentation)
        )
        panel.backgroundColor = .clear
        panel.isOpaque = false
        // NSWindow shadows follow the rectangular window bounds, not the
        // SwiftUI island shape, so they expose a box around transparent panels.
        panel.hasShadow = false
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient, .ignoresCycle]
        panel.hidesOnDeactivate = false
        panel.ignoresMouseEvents = true
        panel.animationBehavior = .none
        panel.isReleasedWhenClosed = false

        model.$transcriptText
            .map(TranscriptPanelLayout.resolve)
            .removeDuplicates()
            .sink { [weak self] layout in
                self?.updateLayout(layout)
            }
            .store(in: &observations)
    }

    func show() {
        positionBelowStatusItem(layout: presentation.layout, animated: false)
        panel.orderFrontRegardless()
    }

    func close() {
        observations.removeAll()
        panel.orderOut(nil)
        panel.close()
        panel.contentViewController = nil
    }

    private func updateLayout(_ layout: TranscriptPanelLayout) {
        guard presentation.layout != layout else {
            return
        }

        presentation.layout = layout
        positionBelowStatusItem(layout: layout, animated: panel.isVisible)
    }

    private func positionBelowStatusItem(layout: TranscriptPanelLayout, animated: Bool) {
        guard let button = statusButton,
              let buttonWindow = button.window
        else {
            return
        }

        let anchorRect = buttonWindow.convertToScreen(button.convert(button.bounds, to: nil))
        let screen = buttonWindow.screen
            ?? NSScreen.screens.first(where: { $0.frame.intersects(anchorRect) })
            ?? NSScreen.main
        guard let screen else {
            return
        }

        let frame = TranscriptPanelGeometry.frame(
            anchor: anchorRect,
            visibleFrame: screen.visibleFrame,
            size: layout.size
        )

        if animated {
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.24
                context.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
                panel.animator().setFrame(frame, display: true)
            }
        } else {
            panel.setFrame(frame, display: true)
        }
    }
}

private struct TranscriptIslandView: View {
    @ObservedObject var model: BridgeAppModel
    @ObservedObject var presentation: TranscriptPanelPresentation

    private var displayedText: String {
        model.transcriptText.isEmpty ? "识别到的文字会显示在这里" : model.transcriptText
    }

    private var metrics: TranscriptPanelMetrics {
        presentation.layout.metrics
    }

    var body: some View {
        HStack(
            alignment: presentation.layout == .expanded ? .top : .center,
            spacing: TranscriptPanelStyle.contentGap
        ) {
            TranscriptActivityGlyph(phase: model.phase)

            VStack(alignment: .leading, spacing: presentation.layout == .expanded ? 6 : 0) {
                Text(model.transcriptStatus)
                    .font(.system(size: TranscriptPanelStyle.statusFontSize, weight: .semibold))
                    .foregroundStyle(.white.opacity(metrics.statusOpacity))

                if presentation.layout == .expanded {
                    Text(displayedText)
                        .font(
                            .system(
                                size: TranscriptPanelStyle.transcriptFontSize,
                                design: .rounded
                            )
                        )
                        .foregroundStyle(.white)
                        .lineLimit(2)
                        .truncationMode(.head)
                        .frame(maxWidth: .infinity, alignment: .topLeading)
                        .textSelection(.disabled)
                        .transition(.opacity)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, metrics.horizontalPadding)
        .padding(.vertical, metrics.verticalPadding)
        .frame(
            width: metrics.size.width,
            height: metrics.size.height,
            alignment: presentation.layout == .expanded ? .topLeading : .leading
        )
        .background {
            RoundedRectangle(cornerRadius: metrics.cornerRadius, style: .continuous)
                .fill(
                    Color(nsColor: .windowBackgroundColor)
                        .opacity(TranscriptPanelStyle.surfaceOpacity)
                )
                .overlay {
                    RoundedRectangle(cornerRadius: metrics.cornerRadius, style: .continuous)
                        .strokeBorder(
                            Color.primary.opacity(TranscriptPanelStyle.outlineOpacity),
                            lineWidth: 0.5
                        )
                }
        }
        .clipShape(RoundedRectangle(cornerRadius: metrics.cornerRadius, style: .continuous))
        .animation(.easeInOut(duration: 0.22), value: presentation.layout)
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("豆包语音输入")
        .accessibilityValue(displayedText)
    }
}

private struct TranscriptActivityGlyph: View {
    let phase: BridgeAppPhase

    private var color: Color {
        switch phase {
        case .starting, .activating:
            return .orange
        case .listening:
            return Color(red: 0.25, green: 0.72, blue: 1)
        case .optimizing:
            return Color(red: 1, green: 0.69, blue: 0.18)
        case .ready:
            return Color(red: 0.38, green: 0.88, blue: 0.50)
        case .error:
            return .red
        }
    }

    private var isAnimated: Bool {
        switch phase {
        case .activating, .listening, .optimizing:
            return true
        case .starting, .ready, .error:
            return false
        }
    }

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 24, paused: !isAnimated)) { context in
            let time = context.date.timeIntervalSinceReferenceDate

            HStack(alignment: .center, spacing: 2.5) {
                ForEach(0..<5, id: \.self) { index in
                    Capsule(style: .continuous)
                        .fill(color)
                        .frame(width: 2.5, height: barHeight(index: index, time: time))
                }
            }
            .frame(
                width: TranscriptPanelStyle.iconSize,
                height: TranscriptPanelStyle.iconSize
            )
            .background(color.opacity(0.13), in: Circle())
        }
        .accessibilityHidden(true)
    }

    private func barHeight(index: Int, time: TimeInterval) -> CGFloat {
        guard isAnimated else {
            return [6, 10, 14, 10, 6][index]
        }

        let speed = phase == .optimizing ? 3.2 : 5.4
        let wave = sin(time * speed + Double(index) * 1.18)
        return 6 + CGFloat((wave + 1) / 2) * 12
    }
}
