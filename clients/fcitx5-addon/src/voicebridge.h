/*
 * SPDX-License-Identifier: LGPL-2.1-or-later
 */
#ifndef _DOUBAO_VOICEBRIDGE_VOICEBRIDGE_H_
#define _DOUBAO_VOICEBRIDGE_VOICEBRIDGE_H_

#include <memory>
#include <string>

#include <fcitx/addoninstance.h>
#include <fcitx/addonmanager.h>
#include <fcitx/instance.h>

#include "dbus_public.h"

namespace doubao {

class VoiceBridgeService;

/// Delivers the speech the Doubao voice client recognized to whichever
/// application currently owns the input focus, the same way an input method
/// delivers what a user typed.
class VoiceBridge : public fcitx::AddonInstance {
public:
    explicit VoiceBridge(fcitx::Instance *instance);
    ~VoiceBridge() override;

    /// Commits `text` to the focused application. Returns false when no
    /// application is focused, which leaves the text with the caller.
    bool commitString(const std::string &text);

    /// Names the application that currently owns the input focus, as its
    /// frontend reported it, or an empty string when nothing is focused. This
    /// is what turns "the text went nowhere" into a diagnosable answer.
    std::string focusedProgram();

private:
    FCITX_ADDON_DEPENDENCY_LOADER(dbus, instance_->addonManager());

    fcitx::Instance *instance_;
    std::unique_ptr<VoiceBridgeService> service_;
};

} // namespace doubao

#endif // _DOUBAO_VOICEBRIDGE_VOICEBRIDGE_H_
