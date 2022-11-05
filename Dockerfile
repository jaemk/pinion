FROM rust:1.58.1-bullseye as builder

# install migration manager
RUN cargo install migrant --features postgres

# create a new empty shell
RUN mkdir -p /app
WORKDIR /app

RUN USER=root cargo new --bin pinion
WORKDIR /app/pinion

# copy over your manifests
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock

# this build step will cache your dependencies
RUN cargo build --release
RUN rm src/*.rs

# copy all source/static/resource files
COPY ./src ./src
COPY ./static ./static
COPY ./sqlx-data.json ./sqlx-data.json
# COPY ./templates ./templates

# build for release
RUN rm ./target/release/deps/pinion*
# use the sqlx-data.json file instead of introspecting a live db
ENV SQLX_OFFLINE=true
RUN cargo build --release

# copy over git dir and embed latest commit hash
COPY ./.git ./.git
# make sure there's no trailing newline
RUN git rev-parse HEAD | awk '{ printf "%s", substr($0, 0, 7)>"commit_hash.txt" }'
RUN rm -rf ./.git

COPY ./Migrant.toml ./Migrant.toml
COPY ./migrations ./migrations

# copy out the binary, static assets, and commit_hash
FROM debian:bullseye-slim
WORKDIR /app/pinion
COPY --from=builder /usr/local/cargo/bin/migrant /usr/bin/migrant
COPY --from=builder /app/pinion/commit_hash.txt ./commit_hash.txt
COPY --from=builder /app/pinion/static ./static
COPY --from=builder /app/pinion/sqlx-data.json ./sqlx-data.json
COPY --from=builder /app/pinion/Migrant.toml ./Migrant.toml
COPY --from=builder /app/pinion/migrations ./migrations
COPY --from=builder /app/pinion/target/release/pinion ./pinion

CMD migrant setup && migrant list && migrant apply -a || true && ./pinion
