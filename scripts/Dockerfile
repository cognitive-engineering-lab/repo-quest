FROM ubuntu:latest
RUN apt update -y && \
  apt install -y curl wget build-essential libssl-dev librsvg2-dev file && \
  apt install -y libgtk-3-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev libwebkit2gtk-4.1-dev libxdo-dev
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install dioxus-cli --version 0.5.7 --locked