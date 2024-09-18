FROM rust:alpine AS builder

RUN apk update \
    && apk add --no-cache \
        gcc \
        musl-dev

WORKDIR /usr/app

COPY . .

RUN cargo build --release

FROM alpine:latest

RUN apk update \
    && apk add --no-cache \
        openrc \
        openssh \
        syslog-ng \
    && mkdir -p /run/openrc \
    && touch /run/openrc/softlevel

ENV RUST_LOG=info

WORKDIR /usr/app

COPY --from=builder /usr/app/pp_admin/create_user.sh ./bin/create_user.sh
COPY --from=builder /usr/app/pp_admin/sshd_config /etc/ssh/sshd_config
COPY --from=builder /usr/app/target/release/pp_client /usr/local/bin/pp_client
COPY --from=builder /usr/app/target/release/pp_server ./bin/pp_server

RUN chmod +x ./bin/create_user.sh

CMD ["sh", "-c", "rc-status; rc-service sshd start; rc-service syslog-ng start; ./bin/pp_server"]
