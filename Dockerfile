# ═══════════════════════════════════════════════════════════════════════════════
# Cogent Docker Image
# Multi-stage build for minimal final image size
# ═══════════════════════════════════════════════════════════════════════════════

# Stage 1: Build
FROM rust:slim-bookworm AS builder

WORKDIR /app

# Copy everything and build in one shot
COPY . .
RUN cargo build --release --workspace

# Stage 2: Runtime
FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="Cogent"
LABEL org.opencontainers.image.description="Unified security audit & compliance platform"
LABEL org.opencontainers.image.source="https://github.com/KidIkaros/cogent"
LABEL org.opencontainers.image.licenses="Apache-2.0 OR OPL-1.1"

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy all binaries from builder
COPY --from=builder /app/target/release/cogent /usr/local/bin/
COPY --from=builder /app/target/release/cogent-server /usr/local/bin/
COPY --from=builder /app/target/release/access-control /usr/local/bin/
COPY --from=builder /app/target/release/cohesion /usr/local/bin/
COPY --from=builder /app/target/release/comments /usr/local/bin/
COPY --from=builder /app/target/release/coupling /usr/local/bin/
COPY --from=builder /app/target/release/crap /usr/local/bin/
COPY --from=builder /app/target/release/cryptocheck /usr/local/bin/
COPY --from=builder /app/target/release/deadcode /usr/local/bin/
COPY --from=builder /app/target/release/debt /usr/local/bin/
COPY --from=builder /app/target/release/doccov /usr/local/bin/
COPY --from=builder /app/target/release/dupfind /usr/local/bin/
COPY --from=builder /app/target/release/errhandle /usr/local/bin/
COPY --from=builder /app/target/release/fuzz /usr/local/bin/
COPY --from=builder /app/target/release/halstead /usr/local/bin/
COPY --from=builder /app/target/release/licenses /usr/local/bin/
COPY --from=builder /app/target/release/linelen /usr/local/bin/
COPY --from=builder /app/target/release/mutate /usr/local/bin/
COPY --from=builder /app/target/release/propcov /usr/local/bin/
COPY --from=builder /app/target/release/riskmap /usr/local/bin/
COPY --from=builder /app/target/release/sast /usr/local/bin/
COPY --from=builder /app/target/release/sbom /usr/local/bin/
COPY --from=builder /app/target/release/secrets /usr/local/bin/
COPY --from=builder /app/target/release/supply-chain /usr/local/bin/
COPY --from=builder /app/target/release/taint /usr/local/bin/
COPY --from=builder /app/target/release/typecov /usr/local/bin/
COPY --from=builder /app/target/release/vulnscan /usr/local/bin/

# Verify installation
RUN cogent --version

# Default: run cogent check on mounted workspace
WORKDIR /workspace
ENTRYPOINT ["cogent"]
CMD ["check", "."]
