/*
 * SPDX-License-Identifier: LGPL-2.1-or-later
 */
#ifndef _DOUBAO_VOICEBRIDGE_VOICEBRIDGE_H_
#define _DOUBAO_VOICEBRIDGE_VOICEBRIDGE_H_

#include <memory>
#include <string>

#include <fcitx-utils/eventloopinterface.h>
#include <fcitx-utils/handlertable.h>
#include <fcitx-utils/trackableobject.h>
#include <fcitx/addoninstance.h>
#include <fcitx/event.h>
#include <fcitx/addonmanager.h>
#include <fcitx/inputcontext.h>
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

    /// Shows `text` in the focused application as provisional preedit, the
    /// same way an input method shows what is still being typed. An empty
    /// `text` withdraws the preedit instead. Returns false when no application
    /// is focused.
    bool updatePreedit(const std::string &text);

    /// Withdraws any outstanding preedit and delivers `text`. Returns
    /// "committed" when it reached the focused application, or "held" when
    /// nothing was focused, in which case the text is kept and written into
    /// the next application to take the input focus. Losing a whole dictation
    /// because the focus moved is the one outcome worth engineering against.
    std::string commitString(const std::string &text);

    /// Names the application that currently owns the input focus, as its
    /// frontend reported it, or an empty string when nothing is focused. This
    /// is what turns "the text went nowhere" into a diagnosable answer.
    std::string focusedProgram();

private:
    FCITX_ADDON_DEPENDENCY_LOADER(dbus, instance_->addonManager());

    /// Withdraws the preedit from whichever input context is still showing
    /// one, so moving the focus mid-dictation never strands provisional text.
    void withdrawPreedit();

    /// Keeps `text` until an application takes the input focus, and expires it
    /// so a forgotten dictation cannot surface much later out of context.
    void holdText(const std::string &text);

    /// Discards any text still waiting for an application to focus.
    void dropHeldText();

    fcitx::Instance *instance_;
    std::unique_ptr<VoiceBridgeService> service_;
    /// The input context the preedit was last shown in. Weak, because an
    /// application can disappear between two recognition updates.
    fcitx::TrackableObjectReference<fcitx::InputContext> preeditContext_;
    /// Text that had nowhere to go when the session ended.
    std::string heldText_;
    std::unique_ptr<fcitx::HandlerTableEntry<fcitx::EventHandler>> focusWatcher_;
    std::unique_ptr<fcitx::EventSourceTime> heldTextExpiry_;
};

} // namespace doubao

#endif // _DOUBAO_VOICEBRIDGE_VOICEBRIDGE_H_
