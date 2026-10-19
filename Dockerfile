FROM rust:1.98-slim-bookworm AS builder

WORKDIR /build

COPY . .

RUN rustup component add rustfmt
RUN apt-get update && apt-get install -y --no-install-recommends \
    libsoapysdr-dev \
    libsoapysdr0.8 \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN cargo build --release --package descrambler

FROM rust:1.98-slim-bookworm AS main

WORKDIR /app

RUN rustup component add rustfmt
RUN cargo install rust-script
RUN apt-get update && apt-get install -y --no-install-recommends \
    libsoapysdr-dev \
    libsoapysdr0.8 \
    soapysdr-tools \
    soapysdr0.8-module-airspy \
    soapysdr0.8-module-bladerf \
    soapysdr0.8-module-hackrf \
    soapysdr0.8-module-lms7 \
    soapysdr0.8-module-mirisdr \
    soapysdr0.8-module-osmosdr \
    soapysdr0.8-module-redpitaya \
    soapysdr0.8-module-remote \
    soapysdr0.8-module-rfspace \
    soapysdr0.8-module-rtlsdr \
    soapysdr0.8-module-uhd \
    libuhd-dev \
    uhd-host \
    bash-completion \
    dbus \
    avahi-daemon \
    pkg-config \
    libfontconfig1-dev \
    && rm -rf /var/lib/apt/lists/*

RUN python3 /usr/libexec/uhd/utils/uhd_images_downloader.py

COPY --from=builder /build/target/release/descrambler /usr/local/bin/descrambler

RUN mkdir -p /etc/bash_completion.d  \
    && descrambler completions bash > /etc/bash_completion.d/descrambler  \
    && echo 'source /etc/bash_completion.d/descrambler' >> /root/.bashrc \
    && echo "PS1='\[\[\e[1;36;47m\] 5GDescrambler \[\e[1;37;46m\]\[\e[1;37;46m\] \w \[\e[0;36;49m\]\[\e[0m\] '" >> /root/.bashrc

COPY docker-entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]