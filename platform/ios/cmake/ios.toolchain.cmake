# CMake toolchain for the iOS core's C++ dependencies (Dynarmic, OpenAL Soft).
set(CMAKE_SYSTEM_NAME iOS)
set(CMAKE_OSX_ARCHITECTURES arm64)
set(CMAKE_OSX_DEPLOYMENT_TARGET 15.0)
# The cmake crate passes -DCMAKE_OSX_SYSROOT=/ for iOS targets; use the SDK.
if(DEFINED ENV{SDKROOT} AND NOT "$ENV{SDKROOT}" STREQUAL "")
    set(CMAKE_OSX_SYSROOT "$ENV{SDKROOT}" CACHE PATH "" FORCE)
else()
    set(CMAKE_OSX_SYSROOT iphoneos CACHE STRING "" FORCE)
endif()
# try_compile can't link executables without signing; static libraries suffice.
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
