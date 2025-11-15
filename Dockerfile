FROM rust:alpine3.22 AS builder

WORKDIR /code

RUN apk upgrade --update-cache --available && apk add pkgconfig make musl-dev perl openssl-dev openssl-libs-static

COPY . .

RUN cargo fetch && cargo build --release

FROM alpine:3.22

WORKDIR /etc/github-webhook-rust

RUN apk upgrade --update-cache --available && apk add bash

COPY --from=builder /code/target/release/github-webhook-rust /usr/local/bin/
COPY --from=builder /code/run.sh /opt/github-webhook-rust/

RUN chmod +x /usr/local/bin/github-webhook-rust && chmod +x /opt/github-webhook-rust/run.sh

ENV GWR_HOSTNAME=0.0.0.0
ENV GWR_PORT=9527
ENV GWR_TLS=false
ENV GWR_WORKERS=0

EXPOSE $GWR_PORT

VOLUME [ "/etc/github-webhook-rust" ]

ENTRYPOINT [ "/opt/github-webhook-rust/run.sh" ]
