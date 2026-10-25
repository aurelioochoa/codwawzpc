# Pinned SDL2 predates the modern PipeWire C API's typed node wrappers.
# SDL's PulseAudio and ALSA backends remain available on Linux.
set(SDL_PIPEWIRE OFF CACHE BOOL "Disable incompatible legacy PipeWire backend" FORCE)
# GCC 16 defaults to C23, where SDL2's legacy `typedef bool` is invalid.
set(CMAKE_C_STANDARD 17)
set(CMAKE_C_STANDARD_REQUIRED ON)
