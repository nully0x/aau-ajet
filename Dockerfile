FROM rust:1.96.0 AS builder

# Create a new empty shell project
WORKDIR /usr/src/aau-ajet
COPY . .

RUN mkdir -p data/uploads

# build dependencies
RUN cargo build --release

# Final stage
FROM debian:bookworm-slim

# Install necessary runtime dependencies
RUN apt-get update && apt-get install -y \
    libsqlite3-0 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app directory structure
WORKDIR /usr/src/aau-ajet
RUN mkdir -p src/static

RUN mkdir -p data/uploads


# Copy the build artifact from the builder stage
COPY --from=builder /usr/src/aau-ajet/target/release/journal-site /usr/src/aau-ajet/journal-site

# Copy static files and templates maintaining the original structure
COPY --from=builder /usr/src/aau-ajet/src/static /usr/src/aau-ajet/src/static
COPY --from=builder /usr/src/aau-ajet/templates /usr/src/aau-ajet/templates

ENV RUST_LOG=info

# Expose the port the app runs on
EXPOSE 8080

# Run the binary
CMD ["./journal-site"]
