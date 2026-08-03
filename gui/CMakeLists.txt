cmake_minimum_required(VERSION 3.21)
project(turkdpi-gui VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.6 REQUIRED COMPONENTS Core DBus Gui Qml Quick QuickControls2)
qt_standard_project_setup(REQUIRES 6.6)

qt_add_executable(turkdpi-gui main.cpp backend.cpp backend.h)
qt_add_qml_module(turkdpi-gui URI TurkDPI VERSION 1.0 QML_FILES Main.qml)
target_link_libraries(turkdpi-gui PRIVATE Qt6::Core Qt6::DBus Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2)

install(TARGETS turkdpi-gui RUNTIME DESTINATION bin)
install(FILES turkdpi.desktop DESTINATION share/applications)
