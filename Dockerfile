FROM rust:1.93 AS comp

ENV PATH=/:$PATH
WORKDIR /gen-rp-rs
COPY . .

RUN cargo build --bin poll --release

FROM fedora:rawhide

COPY --from=comp /gen-rp-rs/target/release/poll /rp-poll

ENV INTERVAL=1day

ENTRYPOINT ["/rp-poll"]
