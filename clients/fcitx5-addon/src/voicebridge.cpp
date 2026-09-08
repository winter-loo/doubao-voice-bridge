/*
 * SPDX-License-Identifier: LGPL-2.1-or-later
 */
#include "voicebridge.h"

#include <memory>
#include <string>

#include <fcitx-utils/dbus/bus.h>
#include <fcitx-utils/dbus/objectvtable.h>
#include <fcitx/addonfactory.h>
#include <fcitx/inputcontext.h>

namespace doubao {

namespace {

constexpr char VoiceBridgeObjectPath[] = "/voicebridge";
constexpr char VoiceBridgeInterface[] = "local.doubao.VoiceBridge1";

} // namespace

/// The bus object is published on fcitx5's own connection, so callers address
/// it as org.fcitx.Fcitx5 without a second well-known name to request.
class VoiceBridgeService
    : public fcitx::dbus::ObjectVTable<VoiceBridgeService> {
public:
    explicit VoiceBridgeService(VoiceBridge *parent) : parent_(parent) {}

    bool commitString(const std::string &text) {
        return parent_->commitString(text);
    }

    std::string focusedProgram() { return parent_->focusedProgram(); }

private:
    FCITX_OBJECT_VTABLE_METHOD(commitString, "CommitString", "s", "b");
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
}

VoiceBridge::~VoiceBridge() = default;

bool VoiceBridge::commitString(const std::string &text) {
    auto *inputContext = instance_->lastFocusedInputContext();
    if (inputContext == nullptr || !inputContext->hasFocus()) {
        return false;
    }
    if (!text.empty()) {
        inputContext->commitString(text);
    }
    return true;
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
