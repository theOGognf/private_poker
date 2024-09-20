FROM rust:alpine AS chef

RUN apk update \
    && apk add --no-cache \
        gcc \
        musl-dev \
    && cargo install cargo-chef

WORKDIR /usr/app

FROM chef AS planner

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /usr/app/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN cargo build --release

FROM alpine:latest AS runtime

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
COPY --from=builder /usr/app/pp_admin/delete_user.sh ./bin/delete_user.sh
COPY --from=builder /usr/app/pp_admin/sshd_config /etc/ssh/sshd_config
COPY --from=builder /usr/app/target/release/pp_client /usr/local/bin/pp_client
COPY --from=builder /usr/app/target/release/pp_server ./bin/pp_server

RUN chmod +x ./bin/create_user.sh \
    && chmod +x ./bin/delete_user.sh

CMD ["sh", "-c", "rc-status; rc-service sshd start; rc-service syslog-ng start; ./bin/pp_server"]
