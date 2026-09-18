FROM docker.io/rust:1.93

WORKDIR /usr/src/myapp

COPY . .

RUN cargo build

ENTRYPOINT ./target/debug/rb
