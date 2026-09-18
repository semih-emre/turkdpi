cmake_minimum_required(VERSION 3.21)
project(turkdpi-gui VERSION 0.5.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

# DBus bilerek listelenmiyor: yalnızca Linux'ta bulunuyor ve arayüz onu
# kullanmıyor. Zorunlu bileşen olarak istemek, Windows ve macOS derlemelerini
# yapılandırma aşamasında durduruyordu.
find_package(Qt6 6.4 REQUIRED COMPONENTS Core Gui Network Qml Quick QuickControls2)
qt_standard_project_setup(REQUIRES 6.4)

qt_add_executable(turkdpi-gui
    WIN32       # Windows'ta arkada konsol penceresi açılmasın
    MACOSX_BUNDLE
    main.cpp backend.cpp backend.h
)
qt_add_qml_module(turkdpi-gui URI TurkDPI VERSION 1.0 QML_FILES Main.qml)
target_compile_definitions(turkdpi-gui PRIVATE TURKDPI_VERSION="${PROJECT_VERSION}")
target_link_libraries(turkdpi-gui PRIVATE
    Qt6::Core Qt6::Gui Qt6::Network Qt6::Qml Qt6::Quick Qt6::QuickControls2)

install(TARGETS turkdpi-gui
    RUNTIME DESTINATION bin
    BUNDLE DESTINATION .
)

# Masaüstü girişi ve kaynaktan güncelleme betiği yalnızca Linux'ta anlamlı.
# Windows ve macOS'ta güncelleme, sürüm sayfasındaki kurulum paketiyle yapılıyor.
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    install(FILES turkdpi.desktop DESTINATION share/applications)
    install(PROGRAMS ../scripts/turkdpi-update DESTINATION lib/turkdpi)
endif()
