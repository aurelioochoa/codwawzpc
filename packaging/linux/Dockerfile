# Ubuntu 22.04 x86_64: glibc 2.35 baseline.
FROM ubuntu:22.04@sha256:e0f80c829509ef4df9e76bd8944eb7b8ec25cbb379b112a7f9448b2c77a77c9b
ENV DEBIAN_FRONTEND=noninteractive
# Bootstrap CA certificates from signed Ubuntu snapshot indexes. Signature and
# package hash verification remain enabled while the TLS trust store is absent.
RUN sed -i 's|http://archive.ubuntu.com/ubuntu|https://snapshot.ubuntu.com/ubuntu/20261026T000000Z|g; s|http://security.ubuntu.com/ubuntu|https://snapshot.ubuntu.com/ubuntu/20261026T000000Z|g' /etc/apt/sources.list && \
    echo 'Acquire::Check-Valid-Until "false";' > /etc/apt/apt.conf.d/99snapshot && \
    echo 'Acquire::https::Verify-Peer "false";' > /etc/apt/apt.conf.d/99bootstrap && \
    apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm /etc/apt/apt.conf.d/99bootstrap
RUN apt-get install -y --no-install-recommends \
    curl build-essential cmake git pkg-config python3 python3-tk \
    python3-pil ffmpeg libx11-dev libxext-dev libxrandr-dev libxcursor-dev \
    libxi-dev libxinerama-dev libxss-dev libasound2-dev libpulse-dev \
    libwayland-dev libxkbcommon-dev libgl1-mesa-dev libudev-dev libboost-dev pulseaudio-utils && \
    rm -rf /var/lib/apt/lists/*
RUN apt-get update && apt-get install -y --no-install-recommends libsndio-dev python3-pil.imagetk && rm -rf /var/lib/apt/lists/*
# Pillow 10 is required by the launcher; the wheel and checksum are pinned.
RUN curl -fsSL https://files.pythonhosted.org/packages/b5/5b/6651c288b08df3b8c1e2f8c1152201e0b25d240e22ddade0f1e242fc9fa0/pillow-10.4.0-cp310-cp310-manylinux_2_28_x86_64.whl -o /tmp/pillow.whl && \
    echo 'a985e028fc183bf12a77a8bbf36318db4238a3ded7fa9df1b9a133f1cb79f8fc  /tmp/pillow.whl' | sha256sum -c - && \
    python3 -m zipfile -e /tmp/pillow.whl /opt/pillow && rm /tmp/pillow.whl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.94.1
ENV PATH=/root/.cargo/bin:$PATH
WORKDIR /src
