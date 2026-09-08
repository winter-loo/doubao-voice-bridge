/*
 * SPDX-License-Identifier: LGPL-2.1-or-later
 */
#include "voicebridge.h"

#include <chrono>
#include <memory>
#include <string>
#include <utility>

#include <fcitx-utils/dbus/bus.h>
#include <fcitx-utils/event.h>
#include <fcitx-utils/dbus/objectvtable.h>
#include <fcitx/addonfactory.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

namespace doubao {

namespace {

constexpr char VoiceBridgeObjectPath[] = "/voicebridge";
constexpr char VoiceBridgeInterface[] = "local.doubao.VoiceBridge1";

/// Long enough to click back into the application the dictation was meant
/// for, short enough that a forgotten dictation cannot surface out of context.
constexpr auto HeldTextLifetime = std::chrono::minutes(2);

/// Applications that render preedit themselves get it inline; the rest get it
/// in fcitx5's own input panel, which is the only feedback they can show.
void showPreedit(fcitx::InputContext *inputContext, const std::string &text) {
    fcitx::Text preedit;
    preedit.append(text, fcitx::TextFormatFlag::Underline);
    preedit.setCursor(static_cast<int>(text.size()));
    if (inputContext->capabilityFlags().test(fcitx::CapabilityFlag::Preedit)) {
        inputContext->inputPanel().setClientPreedit(preedit);
    } else {
        inputContext->inputPanel().setPreedit(preedit);
    }
    inputContext->updatePreedit();
    inputContext->updateUserInterface(
        fcitx::UserInterfaceComponent::InputPanel);
}

void clearPreedit(fcitx::InputContext *inputContext) {
    inputContext->inputPanel().reset();
    inputContext->updatePreedit();
    inputContext->updateUserInterface(
        fcitx::UserInterfaceComponent::InputPanel);
}

} // namespace

/// The bus object is published on fcitx5's own connection, so callers address
/// it as org.fcitx.Fcitx5 without a second well-known name to request.
class VoiceBridgeService
    : public fcitx::dbus::ObjectVTable<VoiceBridgeService> {
public:
    explicit VoiceBridgeService(VoiceBridge *parent) : parent_(parent) {}

    bool updatePreedit(const std::string &text) {
        return parent_->updatePreedit(text);
    }

    std::string commitString(const std::string &text) {
        return parent_->commitString(text);
    }

    std::string focusedProgram() { return parent_->focusedProgram(); }

private:
    FCITX_OBJECT_VTABLE_METHOD(updatePreedit, "UpdatePreedit", "s", "b");
    FCITX_OBJECT_VTABLE_METHOD(commitString, "CommitString", "s", "s");
    FCITX_OBJECT_VTABLE_METHOD(focusedProgram, "FocusedProgram", "", "s");

    VoiceBridge *parent_;
};

VoiceBridge::VoiceBridge(fcitx::Instance *instance)
    : instance_(instance),
      service_(std::make_unique<VoiceBridgeService>(this)) {
    auto *bus = dbus()->call<fcitx::IDBusModule::bus>();
    bus->addObjectVTable(VoiceBridgeObjectPath, VoiceBridgeInterface,
                         *service_);
    bus->flush();

    focusWatcher_ = instance_->watchEvent(
        fcitx::EventType::InputContextFocusIn,
        fcitx::EventWatcherPhase::Default, [this](fcitx::Event &event) {
            if (heldText_.empty()) {
                return;
            }
            // Taken before committing, because the commit reaches the
            // application and the text must go out exactly once.
            const std::string text = std::exchange(heldText_, {});
            heldTextExpiry_.reset();
            static_cast<fcitx::InputContextEvent &>(event)
                .inputContext()
                ->commitString(text);
        });
}

VoiceBridge::~VoiceBridge() = default;

void VoiceBridge::withdrawPreedit() {
    if (auto *shownIn = preeditContext_.get()) {
        clearPreedit(shownIn);
    }
    preeditContext_.unwatch();
}

bool VoiceBridge::updatePreedit(const std::string &text) {
    // Withdrawing is addressed to whichever context still shows the preedit,
    // not to whatever happens to be focused now.
    if (text.empty()) {
        withdrawPreedit();
        return true;
    }
    auto *inputContext = instance_->lastFocusedInputContext();
    if (inputContext == nullptr || !inputContext->hasFocus()) {
        return false;
    }
    if (preeditContext_.get() != inputContext) {
        withdrawPreedit();
        preeditContext_ = inputContext->watch();
    }
    showPreedit(inputContext, text);
    return true;
}

std::string VoiceBridge::commitString(const std::string &text) {
    withdrawPreedit();
    auto *inputContext = instance_->lastFocusedInputContext();
    if (inputContext != nullptr && inputContext->hasFocus()) {
        if (!text.empty()) {
            inputContext->commitString(text);
        }
        return "committed";
    }
    if (text.empty()) {
        return "committed";
    }
    holdText(text);
    return "held";
}

void VoiceBridge::holdText(const std::string &text) {
    heldText_ = text;
    // The expiry is absolute, so it fires once and never repeats.
    heldTextExpiry_ = instance_->eventLoop().addTimeEvent(
        CLOCK_MONOTONIC,
        fcitx::now(CLOCK_MONOTONIC) +
            std::chrono::microseconds(HeldTextLifetime).count(),
        0, [this](fcitx::EventSourceTime *, uint64_t) {
            heldText_.clear();
            return true;
        });
}

void VoiceBridge::dropHeldText() {
    heldText_.clear();
    heldTextExpiry_.reset();
}

std::string VoiceBridge::focusedProgram() {
    auto *inputContext = instance_->lastFocusedInputContext();
    if (inputContext == nullptr || !inputContext->hasFocus()) {
        return {};
    }
    return inputContext->program();
}

class VoiceBridgeFactory : public fcitx::AddonFactory {
    fcitx::AddonInstance *create(fcitx::AddonManager *manager) override {
        return new VoiceBridge(manager->instance());
    }
};

} // namespace doubao

FCITX_ADDON_FACTORY_V2(doubaovoicebridge, doubao::VoiceBridgeFactory)
