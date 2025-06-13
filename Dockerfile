FROM rust:alpine3.22 AS builder

WORKDIR /code

COPY . .

RUN apk upgrade --update-cache --available && apk add pkgconfig make musl-dev perl openssl-dev openssl-libs-static && cargo fetch && cargo build --release

FROM alpine:3.22

WORKDIR /etc/github-webhook-rust

COPY --from=builder /code/target/release/github-webhook-rust /usr/local/bin/
COPY --from=builder /code/channels/ ./channels/
COPY --from=builder /code/run.sh /opt/github-webhook-rust/

RUN apk upgrade --update-cache --available && apk add bash && chmod u+x /usr/local/bin/github-webhook-rust && chmod u+x /opt/github-webhook-rust/run.sh

ENV GWR_HOSTNAME=0.0.0.0
ENV GWR_PORT=9527
ENV GWR_TLS=false
ENV GWR_WORKERS=0

EXPOSE $GWR_PORT

VOLUME [ "/etc/github-webhook-rust" ]

ENTRYPOINT [ "/opt/github-webhook-rust/run.sh" ]
