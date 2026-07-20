# Doubao Voice Bridge — Landing Page Design System

## Product context

Doubao Voice Bridge is an open-source developer tool that lets a Windows or Linux machine use Doubao IME speech recognition running on a Mac. Audio travels from the remote client to the Mac bridge, is played into a virtual audio device, and recognized text streams back to the client.

Primary audience: technical early adopters who use multiple computers, are comfortable with terminals, and want Doubao-quality voice input outside macOS.

Primary job to be done: understand the bridge in seconds, trust that the unusual signal path is real and inspectable, then open GitHub or follow the installation guide.

Primary CTA: `View on GitHub`.
Secondary CTA: `Read installation guide`.
Language: English.

## Page architecture

1. Floating navigation: compact wordmark, `How it works`, `Setup`, `Diagnostics`, GitHub CTA.
2. Hero: concise promise, playful live waveform/signal visual, two CTAs, small open-source/platform proof line.
3. Signal-path explainer: three large connected beats — remote microphone, Mac bridge + Doubao, streamed text — with a packet visibly moving through the path.
4. Terminal proof: a polished command/output composition showing `phase=recording`, partial text, and final text rather than generic product screenshots.
5. Feature rhythm: remote mic, real-time partials, focus safety, TCP transport, fixture replay, diagnostics.
6. Setup strip: prerequisites and three short install/run steps; link to the full guide rather than exposing every CLI option.
7. Final CTA and compact footer.

## Visual direction

Style name: Playful Signal Lab.

Adapt the Kinetic Orange style into a friendly audio-tool identity. Preserve its strong typography, orange/black/white contrast, visible borders, and energetic movement. Avoid an aggressive poster aesthetic: use lowercase/sentence-case body copy, rounded waveform capsules, and generous breathing room.

The page should feel like an audio signal is traveling through it. Repeated dots, waveform bars, small protocol labels, and moving packets form the visual motif. Do not use stock photography, 3D blobs, glassmorphism, generic dashboard screenshots, or decorative gradients.

## Color tokens

- `signal-orange`: `#FF4D00` — primary brand field, active waveform, focus states.
- `signal-coral`: `#FF7448` — secondary waveform steps and hover highlights.
- `ink`: `#090909` — primary text, dark sections, borders.
- `paper`: `#FFF9F2` — warm main background; use instead of clinical pure white for large fields.
- `white`: `#FFFFFF` — text and controls on ink.
- `mint`: `#BDFCC9` — sparse success/final-text state only.
- `muted-ink`: `#6C625B` — secondary body copy on paper.
- `line-light`: `rgba(9, 9, 9, 0.16)` — quiet dividers.

Never introduce purple, blue, pink, neon rainbow palettes, or gradients. Orange is the only dominant accent; mint is reserved for successful final transcription.

## Typography

- Display: `Archivo Black`, fallback `Arial Black, sans-serif`. Use for hero and section statements. Hero may be uppercase; other headings may use sentence case to keep the tone friendly. Tight tracking `-0.045em`, line-height `0.88–0.96`.
- Technical labels and commands: `Space Mono`, fallback `ui-monospace, SFMono-Regular, monospace`. Uppercase labels, tracking `0.02em`.
- Body and navigation: `Inter`, fallback `system-ui, sans-serif`. Line-height `1.5–1.65`.
- Hero size: `clamp(4rem, 9vw, 9rem)`.
- Section statement: `clamp(2.5rem, 5.5vw, 5.75rem)`.
- Body large: `1.125–1.375rem`; normal: `1rem`; labels: `0.6875–0.8125rem`.

## Spacing and layout

- Desktop canvas: 1440px target; responsive down to 320px.
- Content max width: 1280px with `24px / 48px / 72px` responsive side padding.
- Section spacing: `96–160px` desktop, `72–96px` mobile.
- Base spacing rhythm: 4, 8, 12, 16, 24, 32, 48, 64, 96, 128, 160.
- Use full-width color fields interrupted by strong 2px ink borders.
- Use asymmetric but legible grids: hero `7/5`, signal path `4/4/4`, proof `5/7`, features `6/6`.
- On mobile, stack content in narrative order and preserve the visible signal connection as a vertical line.

## Shape and borders

- Structural panels: sharp or subtly rounded (`0–12px`) with 2px solid ink borders.
- Buttons and protocol tags: pill radius (`999px`).
- Waveform capsules: rounded (`999px`) to contrast with the structural grid.
- Shadows: no soft drop shadows. A navigation pill may use a hard offset shadow `4px 4px 0 #090909` on light fields.

## Core components

### Floating navigation

Paper background, 2px ink border, pill shape, hard offset shadow. Wordmark uses a small orange waveform mark plus `DOUBAO / BRIDGE`. GitHub CTA is ink-filled with white text. Collapse to wordmark + GitHub icon/button on mobile.

### Buttons

Primary: ink background, white text, 2px ink border, pill shape, arrow icon. Hover translates `2px -2px` and reveals orange arrow movement.

Secondary: transparent/paper background, ink text, 2px ink border. Hover fills signal-orange.

Focus: 3px signal-orange outline with 3px offset. Minimum target height 48px.

### Signal path cards

Three numbered blocks: `01 SPEAK`, `02 BRIDGE`, `03 TYPE`. Each combines a bold headline, short description, technical mono label, and a bespoke line icon. A 2px cable connects them; an orange packet animates along the cable. On mobile, the cable becomes vertical.

### Terminal proof

Ink panel with paper/white terminal text. Header uses three simple circles and label `LIVE SESSION`. Commands are paper; protocol metadata is muted; partial recognition is orange; final recognition is mint. Keep output realistic and based on the documented JSON events.

### Feature rows

Bordered list rather than floating cards. Each row has an orange mono index, a bold title, a concise explanation, and a technical tag. Hover nudges the title horizontally and animates a waveform mark.

## Content voice

Playful, concise, technically honest. Prefer vivid verbs and short lines. Avoid unsubstantiated claims such as “best,” “instant,” or “zero latency.” Explain that the tool is a prototype and requires a Mac, Doubao IME, Soundflower, FFmpeg, and Accessibility permission.

Suggested hero copy:

- Eyebrow: `OPEN-SOURCE VOICE ROUTING FOR MAC + WINDOWS + LINUX`
- Headline: `Speak over here. Let Doubao type over there.`
- Body: `Stream your microphone to a Mac, route it through Doubao IME, and send live transcription back to Windows or Linux.`
- Proof: `TCP audio · live partials · inspectable JSON events`

## Motion

- Primary ambient motion: waveform bars scale vertically with staggered timing, 1.6–2.4 seconds, ease-in-out.
- Signal packet: travels across the three-step cable in 4 seconds, linear with a short pause at each node.
- Marquee: one restrained proof strip such as `MIC → TCP → MAC → DOUBAO → TEXT`, 24 seconds linear. Never use multiple competing marquees.
- Section reveal: 16–24px upward motion with opacity over 400–550ms.
- Hover: horizontal translations no larger than 8px; button lift no larger than 2px.
- Respect `prefers-reduced-motion`: remove continuous movement and show a static packet at the final node.

## Accessibility and responsive requirements

- Maintain WCAG AA contrast for all functional text.
- Never communicate recording/final status by color alone; pair color with labels and icons.
- Provide visible focus states and semantic heading order.
- Decorative waveform SVGs are hidden from assistive technology; meaningful diagrams have concise accessible labels.
- Preserve installation commands as selectable text.
- Mobile typography must avoid clipping; terminal blocks scroll horizontally only when commands cannot wrap safely.

## Prohibited patterns

- No gradient backgrounds.
- No stock microphone photos or generic AI imagery.
- No glass cards or blurred translucent layers.
- No invented cloud service, account system, pricing, customer logos, download count, or testimonials.
- No faux macOS application screenshot; show the real architecture and terminal behavior instead.
