FROM rust:bookworm AS rust-builder
RUN apt-get update && apt-get install -y libasound2-dev libopus-dev cmake
RUN mkdir /build
ADD . /build/
WORKDIR /build
RUN --mount=type=cache,target=/build/.cargo/registry \
    --mount=type=cache,target=/build/target/release/incremental\
    --mount=type=cache,target=/build/target/release/build\
    cargo build --release

FROM debian:bookworm
LABEL maintainer="shenjindi@miuda.ai"
RUN --mount=type=cache,target=/var/apt apt-get update && apt-get install -y ca-certificates tzdata libopus0
ENV DEBIAN_FRONTEND=noninteractive
ENV LANG=C.UTF-8

WORKDIR /app
COPY --from=rust-builder /build/static /app/static

COPY --from=rust-builder /build/target/release/active-call /app/active-call

ENTRYPOINT ["/app/active-call"]
