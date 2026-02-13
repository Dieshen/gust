FROM rust:1.86
WORKDIR /app
COPY . .
RUN cargo build --release
CMD ["cargo", "run", "--release"]
