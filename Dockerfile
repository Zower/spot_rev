FROM rust:1.70 as builder
WORKDIR /usr/src/spot_rev_r
COPY . .
RUN cargo install --path .

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y pkg-config ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/spot_rev_r /usr/local/bin/spot_rev_r
CMD ["spot_rev_r"]