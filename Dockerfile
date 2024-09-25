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
        inotify-tools \
        openrc \
        openssh \
        syslog-ng \
    && mkdir -p /run/openrc \
    && touch /run/openrc/softlevel \
    && addgroup newbs

ENV RUST_LOG=info

WORKDIR /usr/app

# Copy admin helper scripts
COPY --from=builder /usr/app/pp_admin/create_user.sh ./create_user
COPY --from=builder /usr/app/pp_admin/delete_user.sh ./delete_user
COPY --from=builder /usr/app/pp_admin/enable_user.sh /usr/local/bin/enable_user
COPY --from=builder /usr/app/pp_admin/sshd_config /etc/ssh/sshd_config

# Copy poker binaries
COPY --from=builder /usr/app/target/release/pp_client /usr/local/bin/pp_client
COPY --from=builder /usr/app/target/release/pp_server ./pp_server

RUN chmod +x ./create_user \
    && chmod +x ./delete_user \
    && chmod +x /usr/local/bin/enable_user

CMD ["sh", "-c", "rc-status; rc-service sshd start; rc-service syslog-ng start; ./pp_server"]
