import AudioToolbox
import CoreAudio
import Foundation
import os.lock

final class RealtimePCMBuffer {
    private var samples: [Float32]
    private var readIndex = 0
    private var writeIndex = 0
    private var availableFrames = 0
    private var pendingLowByte: UInt8?
    private var lock = os_unfair_lock_s()

    init(capacityFrames: Int) {
        samples = Array(repeating: 0, count: max(1, capacityFrames))
    }

    func enqueueS16LE(_ data: Data) -> UInt64 {
        var droppedFrames: UInt64 = 0
        os_unfair_lock_lock(&lock)
        data.withUnsafeBytes { rawBytes in
            let bytes = rawBytes.bindMemory(to: UInt8.self)
            var offset = 0
            if let pendingLowByte, !bytes.isEmpty {
                let bits = UInt16(pendingLowByte) | UInt16(bytes[0]) << 8
                if enqueueLocked(bits: bits) {
                    droppedFrames += 1
                }
                self.pendingLowByte = nil
                offset = 1
            }
            while offset + 1 < bytes.count {
                let bits = UInt16(bytes[offset]) | UInt16(bytes[offset + 1]) << 8
                if enqueueLocked(bits: bits) {
                    droppedFrames += 1
                }
                offset += 2
            }
            if offset < bytes.count {
                pendingLowByte = bytes[offset]
            }
        }
        os_unfair_lock_unlock(&lock)
        return droppedFrames * 2
    }

    private func enqueueLocked(bits: UInt16) -> Bool {
        var droppedFrame = false
        if availableFrames == samples.count {
            readIndex = (readIndex + 1) % samples.count
            availableFrames -= 1
            droppedFrame = true
        }
        samples[writeIndex] = Float32(Int16(bitPattern: bits)) / 32_768
        writeIndex = (writeIndex + 1) % samples.count
        availableFrames += 1
        return droppedFrame
    }

    func dequeue(frameCount: Int) -> [Float32] {
        var output = Array(repeating: Float32(0), count: frameCount)
        os_unfair_lock_lock(&lock)
        for index in output.indices where availableFrames > 0 {
            output[index] = samples[readIndex]
            readIndex = (readIndex + 1) % samples.count
            availableFrames -= 1
        }
        os_unfair_lock_unlock(&lock)
        return output
    }

    func render(into audioBufferList: UnsafeMutablePointer<AudioBufferList>, frameCount: UInt32) {
        let buffers = UnsafeMutableAudioBufferListPointer(audioBufferList)
        os_unfair_lock_lock(&lock)
        for frame in 0..<Int(frameCount) {
            let sample: Float32
            if availableFrames > 0 {
                sample = samples[readIndex]
                readIndex = (readIndex + 1) % samples.count
                availableFrames -= 1
            } else {
                sample = 0
            }
            for buffer in buffers {
                guard let data = buffer.mData else { continue }
                let channels = max(1, Int(buffer.mNumberChannels))
                let output = data.assumingMemoryBound(to: Float32.self)
                for channel in 0..<channels {
                    output[frame * channels + channel] = sample
                }
            }
        }
        os_unfair_lock_unlock(&lock)
    }
}

private let realtimeAudioRenderCallback: AURenderCallback = {
    reference,
    _,
    _,
    _,
    frameCount,
    audioBufferList in
    guard let audioBufferList else {
        return noErr
    }
    let output = Unmanaged<RealtimeAudioOutput>
        .fromOpaque(reference)
        .takeUnretainedValue()
    output.render(into: audioBufferList, frameCount: frameCount)
    return noErr
}

final class RealtimeAudioOutput {
    private static let transportSampleRate: Float64 = 48_000
    private let pcmBuffer = RealtimePCMBuffer(capacityFrames: 12_000)
    private var audioUnit: AudioUnit?

    func start(deviceID: AudioDeviceID) throws {
        stop()
        try configureSampleRate(deviceID: deviceID)

        var description = AudioComponentDescription(
            componentType: kAudioUnitType_Output,
            componentSubType: kAudioUnitSubType_HALOutput,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0
        )
        guard let component = AudioComponentFindNext(nil, &description) else {
            throw BridgeError.message("Could not find the CoreAudio HAL output component")
        }

        var unit: AudioUnit?
        try check(
            AudioComponentInstanceNew(component, &unit),
            operation: "Create CoreAudio output"
        )
        guard let unit else {
            throw BridgeError.message("CoreAudio did not create an output unit")
        }

        do {
            var enabled: UInt32 = 1
            try check(
                AudioUnitSetProperty(
                    unit,
                    kAudioOutputUnitProperty_EnableIO,
                    kAudioUnitScope_Output,
                    0,
                    &enabled,
                    UInt32(MemoryLayout<UInt32>.size)
                ),
                operation: "Enable CoreAudio output"
            )

            var selectedDevice = deviceID
            try check(
                AudioUnitSetProperty(
                    unit,
                    kAudioOutputUnitProperty_CurrentDevice,
                    kAudioUnitScope_Global,
                    0,
                    &selectedDevice,
                    UInt32(MemoryLayout<AudioDeviceID>.size)
                ),
                operation: "Select CoreAudio output device"
            )

            var format = AudioStreamBasicDescription()
            var formatSize = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
            try check(
                AudioUnitGetProperty(
                    unit,
                    kAudioUnitProperty_StreamFormat,
                    kAudioUnitScope_Input,
                    0,
                    &format,
                    &formatSize
                ),
                operation: "Read CoreAudio stream format"
            )
            format.mSampleRate = Self.transportSampleRate
            try check(
                AudioUnitSetProperty(
                    unit,
                    kAudioUnitProperty_StreamFormat,
                    kAudioUnitScope_Input,
                    0,
                    &format,
                    UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
                ),
                operation: "Set CoreAudio client stream to 48 kHz"
            )

            format = AudioStreamBasicDescription()
            formatSize = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
            try check(
                AudioUnitGetProperty(
                    unit,
                    kAudioUnitProperty_StreamFormat,
                    kAudioUnitScope_Input,
                    0,
                    &format,
                    &formatSize
                ),
                operation: "Verify CoreAudio client stream format"
            )
            guard format.mFormatID == kAudioFormatLinearPCM,
                  format.mFormatFlags & kAudioFormatFlagIsFloat != 0,
                  format.mBitsPerChannel == 32,
                  abs(format.mSampleRate - Self.transportSampleRate) < 1
            else {
                throw BridgeError.message("CoreAudio output does not provide 48 kHz Float32 PCM")
            }
            print(
                "[audio] CoreAudio format rate=\(format.mSampleRate) " +
                "channels=\(format.mChannelsPerFrame) flags=\(format.mFormatFlags)"
            )

            var callback = AURenderCallbackStruct(
                inputProc: realtimeAudioRenderCallback,
                inputProcRefCon: Unmanaged.passUnretained(self).toOpaque()
            )
            try check(
                AudioUnitSetProperty(
                    unit,
                    kAudioUnitProperty_SetRenderCallback,
                    kAudioUnitScope_Input,
                    0,
                    &callback,
                    UInt32(MemoryLayout<AURenderCallbackStruct>.size)
                ),
                operation: "Install CoreAudio render callback"
            )
            try check(AudioUnitInitialize(unit), operation: "Initialize CoreAudio output")
            try check(AudioOutputUnitStart(unit), operation: "Start CoreAudio output")
            audioUnit = unit
        } catch {
            AudioUnitUninitialize(unit)
            AudioComponentInstanceDispose(unit)
            throw error
        }
    }

    func stop() {
        guard let audioUnit else { return }
        AudioOutputUnitStop(audioUnit)
        AudioUnitUninitialize(audioUnit)
        AudioComponentInstanceDispose(audioUnit)
        self.audioUnit = nil
    }

    func enqueueS16LE(_ data: Data) -> UInt64 {
        pcmBuffer.enqueueS16LE(data)
    }

    fileprivate func render(
        into audioBufferList: UnsafeMutablePointer<AudioBufferList>,
        frameCount: UInt32
    ) {
        pcmBuffer.render(into: audioBufferList, frameCount: frameCount)
    }

    private func check(_ status: OSStatus, operation: String) throws {
        guard status == noErr else {
            let raw = UInt32(bitPattern: status)
            let code = String(bytes: [
                UInt8((raw >> 24) & 0xFF),
                UInt8((raw >> 16) & 0xFF),
                UInt8((raw >> 8) & 0xFF),
                UInt8(raw & 0xFF),
            ], encoding: .ascii) ?? "????"
            throw BridgeError.message("\(operation) failed: \(status) (\(code))")
        }
    }

    private func configureSampleRate(deviceID: AudioDeviceID) throws {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyNominalSampleRate,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var sampleRate = Self.transportSampleRate
        try check(
            AudioObjectSetPropertyData(
                deviceID,
                &address,
                0,
                nil,
                UInt32(MemoryLayout<Float64>.size),
                &sampleRate
            ),
            operation: "Set virtual audio device to 48 kHz"
        )

        var actualRate: Float64 = 0
        var size = UInt32(MemoryLayout<Float64>.size)
        try check(
            AudioObjectGetPropertyData(
                deviceID,
                &address,
                0,
                nil,
                &size,
                &actualRate
            ),
            operation: "Read virtual audio device sample rate"
        )
        guard abs(actualRate - Self.transportSampleRate) < 1 else {
            throw BridgeError.message(
                "Virtual audio device stayed at \(actualRate) Hz; 48000 Hz is required"
            )
        }
    }

    deinit {
        stop()
    }
}
