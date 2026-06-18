ARG UBUNTU_VERSION=22.04
FROM ubuntu:${UBUNTU_VERSION}

RUN apt-get update && apt-get install -y \
    build-essential \
    cargo \
    pkg-config \
    libcairo2-dev \
    libx11-dev \
    libxext-dev \
    libxrender-dev \
    libfreetype-dev \
    libpng-dev \
    libavcodec-dev \
    libavformat-dev \
    libswscale-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .

RUN cargo build --release

CMD ["bash"]
