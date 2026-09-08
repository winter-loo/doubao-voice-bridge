cmake_minimum_required(VERSION 3.16)

project(doubao-voice-bridge-fcitx5 VERSION 0.1.0 LANGUAGES CXX)

find_package(Fcitx5Utils REQUIRED)
find_package(Fcitx5Core REQUIRED)
find_package(Fcitx5Module REQUIRED COMPONENTS DBus)

include("${FCITX_INSTALL_CMAKECONFIG_DIR}/Fcitx5Utils/Fcitx5CompilerSettings.cmake")

add_fcitx5_addon(doubaovoicebridge src/voicebridge.cpp)
target_link_libraries(doubaovoicebridge Fcitx5::Core Fcitx5::Module::DBus)

configure_file(data/doubaovoicebridge.conf.in
               "${CMAKE_CURRENT_BINARY_DIR}/doubaovoicebridge.conf" @ONLY)

install(TARGETS doubaovoicebridge DESTINATION "${FCITX_INSTALL_ADDONDIR}")
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/doubaovoicebridge.conf"
        DESTINATION "${FCITX_INSTALL_PKGDATADIR}/addon")
